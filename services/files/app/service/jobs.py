"""Scan and extract jobs (spec 6.1 steps 3-4, spec 10: retries 3x with exponential backoff), the
orphan sweep, and reacting to course deletion. Every job is idempotent."""

import asyncio
import hashlib
import io
import logging
import shutil
from dataclasses import dataclass
from datetime import timedelta
from typing import Any

from sqlalchemy.ext.asyncio import AsyncSession

from app.clients.store import ObjectStore
from app.domain.file_state import counts_against_quota
from app.domain.uploads import needs_ocr
from app.repo import files as repo
from app.repo import storage
from app.service import saver
from app.service.deps import Deps
from crux_common.db import transaction
from crux_common.messaging.bus import Delivery
from crux_common.messaging.outbox import emit
from crux_common.timeutil import iso, utcnow

log = logging.getLogger("crux.files.jobs")

MAX_ATTEMPTS = 3
BASE_BACKOFF = timedelta(seconds=30)
EXTRACTOR_VERSION = "mupdf-1"
STUCK_AFTER = timedelta(minutes=15)
ABANDONED_AFTER = timedelta(hours=24)


def backoff(attempts: int) -> timedelta:
    """30 s, 60 s, 120 s."""
    return BASE_BACKOFF * int(2 ** max(0, attempts - 1))


async def run_one(deps: Deps) -> bool:
    """Claims and runs one due job. Returns False when there was nothing to do."""
    async with transaction(deps.sessions) as s:
        job = await repo.claim_due(s, utcnow())
    if job is None:
        return False
    try:
        if job.Kind == "Scan":
            await scan(deps, job.FileId)
        elif job.Kind == "Thumbnail":
            await saver.make_thumbnails(deps, job.FileId)
        else:
            await extract(deps, job.FileId)
    except Exception as e:
        attempts = job.Attempts + 1
        give_up = attempts >= MAX_ATTEMPTS
        log.warning(
            "file job failed", extra={"kind": job.Kind, "file_id": job.FileId, "attempt": attempts, "error": repr(e)}
        )
        async with transaction(deps.sessions) as s:
            await repo.finish_job(
                s,
                job.Id,
                "Failed" if give_up else "Pending",
                Attempts=attempts,
                LastError=repr(e)[:500],
                NextAttemptAt=utcnow() + backoff(attempts),
            )
            if give_up:
                await _mark_failed(deps, s, job.Kind, job.FileId)
        return True
    async with transaction(deps.sessions) as s:
        await repo.finish_job(s, job.Id, "Done", Attempts=job.Attempts + 1, LastError=None)
    return True


async def _mark_failed(deps: Deps, s: AsyncSession, kind: str, file_id: int) -> None:
    row = await repo.by_id(s, file_id)
    if row is None:
        return
    if kind == "Scan":
        await repo.set_fields(s, file_id, ScanStatus="Failed")
        await emit(
            s,
            deps.contracts,
            "file.scanned",
            row.PublicId,
            {"fileRef": row.PublicId, "ownerRef": row.OwnerUserRef, "status": "Failed"},
        )
    else:
        await repo.set_fields(s, file_id, ExtractStatus="Failed")


async def scan(deps: Deps, file_id: int) -> None:
    async with deps.sessions() as s:
        row = await repo.by_id(s, file_id)
    if row is None or row.Status != "Active" or row.ScanStatus != "Pending" or row.StorageKey is None:
        return  # already handled, or gone
    verdict = await deps.scanner.scan(deps.store.read(row.StorageKey))
    async with transaction(deps.sessions) as s:
        row = await repo.by_id(s, file_id, for_update=True)
        if row is None or row.Status != "Active" or row.ScanStatus != "Pending":
            return
        payload = {"fileRef": row.PublicId, "ownerRef": row.OwnerUserRef, "status": verdict}
        if verdict == "Infected":
            # Spec 6.1 step 3: delete the object, mark Deleted, release the quota, tell the user.
            await deps.store.delete(row.StorageKey)
            await storage.release_used(s, row.OwnerUserRef, row.SizeBytes)
            await repo.set_status(s, row.Id, "Active", "Deleted", StorageKey=None)
            await repo.set_fields(s, row.Id, ScanStatus="Infected", ExtractStatus="NotNeeded")
            for user_ref in {row.OwnerUserRef, row.UploadedByUserRef or row.OwnerUserRef}:
                await emit(
                    s,
                    deps.contracts,
                    "notify.requested",
                    user_ref,
                    {
                        "userRef": user_ref,
                        "kind": "file.infected",
                        "dedupeKey": f"file.infected:{row.PublicId}",
                        "dueAt": iso(utcnow()),
                        "payload": {"fileName": row.FileName, "fileRef": row.PublicId},
                    },
                )
        else:
            await repo.set_fields(s, row.Id, ScanStatus="Clean")
            if row.Purpose == "Original":
                await repo.enqueue(s, row.Id, "Extract", utcnow())
        await emit(s, deps.contracts, "file.scanned", row.PublicId, payload)


@dataclass(frozen=True)
class Page:
    number: int
    text: str
    method: str  # Text | OCR


def tesseract_available() -> bool:
    return shutil.which("tesseract") is not None


def extract_pages(pdf: bytes, ocr_languages: str) -> list[Page]:
    """PyMuPDF text layer per page; OCR (Tesseract) for pages with under 20 characters (spec 6.1)."""
    import pymupdf

    ocr = tesseract_available()
    pages: list[Page] = []
    with pymupdf.open(stream=pdf, filetype="pdf") as doc:  # type: ignore[no-untyped-call]
        for i, page in enumerate(doc, start=1):
            text = page.get_text("text") or ""
            if needs_ocr(text) and ocr:
                import pytesseract
                from PIL import Image

                pix = page.get_pixmap(dpi=300)
                image = Image.open(io.BytesIO(pix.tobytes("png")))
                ocr_text = pytesseract.image_to_string(image, lang=ocr_languages)
                if len(ocr_text.strip()) > len(text.strip()):
                    pages.append(Page(i, ocr_text, "OCR"))
                    continue
            pages.append(Page(i, text, "Text"))
    return pages


async def extract(deps: Deps, file_id: int) -> None:
    async with deps.sessions() as s:
        row = await repo.by_id(s, file_id)
    if row is None or row.Status != "Active" or row.ScanStatus != "Clean" or row.StorageKey is None:
        return
    data = b"".join([chunk async for chunk in deps.store.read(row.StorageKey)])
    pages = await asyncio.to_thread(extract_pages, data, deps.settings.ocr_languages)
    rows = [
        {
            "FileId": row.Id,
            "PageNo": p.number,
            "ChunkNo": p.number,
            "Text": p.text,
            "ContentSha": hashlib.sha256(p.text.encode()).hexdigest(),
            "ExtractMethod": p.method,
            "ExtractorVersion": EXTRACTOR_VERSION,
        }
        for p in pages
    ]
    async with transaction(deps.sessions) as s:
        await repo.replace_chunks(s, row.Id, rows)
        await repo.set_fields(s, row.Id, ExtractStatus="Done", ExtractorVersion=EXTRACTOR_VERSION, PageCount=len(pages))
        await saver.after_extract(deps, s, row.Id)
        await emit(
            s,
            deps.contracts,
            "file.extracted",
            row.PublicId,
            {
                "fileRef": row.PublicId,
                "pageCount": len(pages),
                "chunkCount": len(rows),
                "extractorVersion": EXTRACTOR_VERSION,
            },
        )


async def recover_stuck(deps: Deps) -> int:
    async with transaction(deps.sessions) as s:
        return await repo.reclaim_stuck(s, utcnow() - STUCK_AFTER)


async def sweep_abandoned(deps: Deps) -> int:
    """orphan.sweep (spec 10): uploads never completed within 24 h give their reservation back."""
    n = 0
    async with transaction(deps.sessions) as s:
        for row in await repo.abandoned_uploads(s, utcnow() - ABANDONED_AFTER):
            if row.StorageKey:
                await deps.store.delete(row.StorageKey)
            await storage.release_reserved(s, row.OwnerUserRef, row.SizeBytes)
            await repo.set_status(s, row.Id, "Uploading", "Deleted", StorageKey=None)
            n += 1
    return n


class CourseEventsHandler:
    """course.deleted (the trash purge in core, D2-5): the course's uploads go too."""

    def __init__(self, store: ObjectStore) -> None:
        self._store = store

    @property
    def topics(self) -> tuple[str, ...]:
        return ("course.deleted",)

    async def handle(self, session: AsyncSession, delivery: Delivery) -> None:
        course_ref = delivery.envelope.payload["courseRef"]
        for row in await repo.by_course(session, course_ref):
            await _delete_file(self._store, session, row)


async def _delete_file(store: ObjectStore, s: AsyncSession, row: Any) -> None:
    if row.Status == "Deleted":
        return
    if row.StorageKey:
        await store.delete(row.StorageKey)
    if counts_against_quota(row.Status):
        await storage.release_used(s, row.OwnerUserRef, row.SizeBytes)
    elif row.Status == "Uploading":
        await storage.release_reserved(s, row.OwnerUserRef, row.SizeBytes)
    await repo.set_status(s, row.Id, row.Status, "Deleted", StorageKey=None)
