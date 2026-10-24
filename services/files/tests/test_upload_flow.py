"""Upload → complete → scan → extract on real MySQL, with an in-memory store, fake core and the
EICAR-aware fake scanner (spec 6.1, decisions D2-4, D2-7)."""

import hashlib
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from datetime import timedelta
from pathlib import Path

import pymupdf
import pytest
from sqlalchemy import select, update

from app.clients.core import CourseAccess, FakeCore
from app.clients.scanner import EICAR, FakeScanner
from app.clients.store import MemoryStore
from app.repo import files as repo
from app.repo import storage
from app.service import files as svc
from app.service import jobs
from app.service.deps import Deps
from app.settings import Settings
from crux_common.auth import Principal
from crux_common.errors import FileRejected, NotFound, QuotaExceeded
from crux_common.messaging.bus import Delivery, Envelope
from crux_common.messaging.contracts import Contracts
from crux_common.messaging.tables import outbox
from crux_common.testing import migrated_db

SERVICE_DIR = Path(__file__).resolve().parents[1]
CONTRACTS = SERVICE_DIR.parents[1] / "contracts"
OWNER = Principal("01TEST0000000000000000OWNR", "User", "free")
EDITOR = Principal("01TEST000000000000000EDITR", "User", "free")
STRANGER = Principal("01TEST00000000000000STRNGR", "User", "free")
COURSE = "01TEST000000000000000COURS"


def make_pdf(pages: list[str]) -> bytes:
    doc = pymupdf.open()
    for text in pages:
        page = doc.new_page()
        page.insert_text((72, 72), text)
    data: bytes = doc.tobytes()
    doc.close()
    return data


@asynccontextmanager
async def env(quota: int = 10_000_000) -> AsyncIterator[tuple[Deps, MemoryStore, FakeCore]]:
    async with migrated_db(SERVICE_DIR, "files") as sessions:
        store, core = MemoryStore(), FakeCore()
        core.courses[(OWNER.user_ref, COURSE)] = CourseAccess(OWNER.user_ref, "Owner")
        core.courses[(EDITOR.user_ref, COURSE)] = CourseAccess(OWNER.user_ref, "Editor")
        settings = Settings(database_url="unused", max_upload_bytes=5_000_000)
        deps = Deps(
            settings=settings,
            sessions=sessions,
            store=store,
            core=core,
            scanner=FakeScanner(),
            contracts=Contracts(CONTRACTS),
        )
        async with sessions() as s, s.begin():
            await storage.upsert_quota(s, OWNER.user_ref, quota, "free")
        yield deps, store, core


async def upload(
    deps: Deps, store: MemoryStore, user: Principal, data: bytes, name: str = "slides.pdf", course: str | None = None
) -> svc.FileView:
    started = await svc.start_upload(
        deps,
        user,
        svc.UploadInput(
            file_name=name, size_bytes=len(data), sha256=hashlib.sha256(data).hexdigest(), course_id=course
        ),
    )
    if started.already_present:
        return started.file
    async with deps.sessions() as s:
        row = await repo.by_public_id(s, started.file.id)
    store.put(row.StorageKey, data)
    return await svc.complete_upload(deps, user, started.file.id)


async def drain(deps: Deps) -> None:
    while await jobs.run_one(deps):
        pass


async def topics(deps: Deps) -> list[str]:
    async with deps.sessions() as s:
        return list((await s.execute(select(outbox.c.Topic).order_by(outbox.c.Id))).scalars())


async def test_upload_scan_extract_and_read() -> None:
    async with env() as (deps, store, core):
        data = make_pdf(["Regression is about fitting lines to data.", "Page two talks about residuals at length."])
        f = await upload(deps, store, EDITOR, data, course=COURSE)
        assert (f.status, f.scan_status) == ("Active", "Pending")
        await drain(deps)

        got = await svc.get(deps, OWNER, f.id)
        assert (got.scan_status, got.extract_status, got.page_count) == ("Clean", "Done", 2)
        assert "residuals" in (await svc.page_text(deps, OWNER, f.id, 2)).text
        assert await topics(deps) == ["file.scanned", "file.extracted"]

        # D2-4: counted against the course owner, not the editor who uploaded.
        view = await svc.storage_view(deps, OWNER)
        assert (view.used_bytes, view.reserved_bytes, view.by_course[0].course_id) == (len(data), 0, COURSE)
        assert (await svc.storage_view(deps, EDITOR)).used_bytes == 0

        url = await svc.download_url(deps, EDITOR, f.id)
        assert url.url.startswith("memory://get/")
        with pytest.raises(NotFound):
            await svc.get(deps, STRANGER, f.id)
        core.file_access.add((STRANGER.user_ref, f.id))
        assert (await svc.get(deps, STRANGER, f.id)).id == f.id, "readers of a course that uses the file"


async def test_dedupe_quota_and_rejections() -> None:
    async with env(quota=2_000) as (deps, store, _):
        small = make_pdf(["tiny"])
        assert len(small) < 2_000
        first = await upload(deps, store, OWNER, small)
        again = await svc.start_upload(
            deps,
            OWNER,
            svc.UploadInput(file_name="copy.pdf", size_bytes=len(small), sha256=hashlib.sha256(small).hexdigest()),
        )
        assert again.already_present and again.file.id == first.id, "same owner + sha256 = same file"

        big = b"%PDF-" + b"x" * 1_500
        with pytest.raises(QuotaExceeded) as e:
            await svc.start_upload(
                deps,
                OWNER,
                svc.UploadInput(file_name="big.pdf", size_bytes=len(big), sha256=hashlib.sha256(big).hexdigest()),
            )
        assert e.value.details == {"usedBytes": len(small), "quotaBytes": 2_000}

        with pytest.raises(FileRejected) as e:
            await svc.start_upload(deps, OWNER, svc.UploadInput(file_name="a.docx", size_bytes=10, sha256="a" * 64))
        assert e.value.details == {"reason": "type"}

        # Declared a PDF, uploaded something else: rejected, reservation returned.
        fake = b"PK\x03\x04 not a pdf"
        with pytest.raises(FileRejected) as e:
            await upload(deps, store, OWNER, fake, name="sneaky.pdf")
        assert e.value.details == {"reason": "type"}
        async with deps.sessions() as s:
            q = await storage.get(s, OWNER.user_ref)
        assert (q.UsedBytes, q.ReservedBytes) == (len(small), 0)

        with pytest.raises(NotFound):
            await svc.start_upload(
                deps, STRANGER, svc.UploadInput(file_name="a.pdf", size_bytes=10, sha256="a" * 64, course_id=COURSE)
            )


async def test_infected_upload_is_deleted_and_reported() -> None:
    async with env() as (deps, store, _):
        data = b"%PDF-1.4\n" + EICAR + b"\n%%EOF"
        f = await upload(deps, store, OWNER, data, name="eicar.pdf")
        await drain(deps)
        async with deps.sessions() as s:
            row = await repo.by_public_id(s, f.id)
            q = await storage.get(s, OWNER.user_ref)
        assert (row.Status, row.ScanStatus, row.StorageKey) == ("Deleted", "Infected", None)
        assert q.UsedBytes == 0, "quota released"
        assert store.objects == {}, "object deleted"
        assert "file.scanned" in await topics(deps) and "notify.requested" in await topics(deps)
        with pytest.raises(NotFound):
            await svc.get(deps, OWNER, f.id)


async def test_course_deletion_and_abandoned_uploads_free_space() -> None:
    async with env() as (deps, store, _):
        f = await upload(deps, store, OWNER, make_pdf(["for the course"]), course=COURSE)
        handler = jobs.CourseEventsHandler(store)
        env_ = Envelope(
            "01TEST0000000000000000000E",
            "course.deleted",
            1,
            "x",
            "core",
            COURSE,
            {"courseRef": COURSE, "ownerRef": OWNER.user_ref, "memberRefs": [], "color": "#000000"},
        )
        async with deps.sessions() as s, s.begin():
            await handler.handle(s, Delivery("1-0", "course.deleted", env_))
        async with deps.sessions() as s:
            row = await repo.by_public_id(s, f.id)
            q = await storage.get(s, OWNER.user_ref)
        assert (row.Status, q.UsedBytes) == ("Deleted", 0)

        started = await svc.start_upload(
            deps, OWNER, svc.UploadInput(file_name="later.pdf", size_bytes=500, sha256="b" * 64)
        )
        async with deps.sessions() as s, s.begin():
            await s.execute(
                update(repo.stored_file)
                .where(repo.stored_file.c.PublicId == started.file.id)
                .values(UploadedAt=row.UploadedAt - timedelta(days=2))
            )
        assert await jobs.sweep_abandoned(deps) == 1
        async with deps.sessions() as s:
            q = await storage.get(s, OWNER.user_ref)
        assert q.ReservedBytes == 0


async def test_failed_scans_retry_then_fail() -> None:
    class Broken:
        async def scan(self, chunks: AsyncIterator[bytes]) -> str:
            raise ConnectionError("clamd down")

    async with env() as (deps, store, _):
        deps.scanner = Broken()
        f = await upload(deps, store, OWNER, make_pdf(["x"]))
        for _ in range(3):
            async with deps.sessions() as s, s.begin():
                await s.execute(update(repo.file_job).values(NextAttemptAt=row_time()))
            await jobs.run_one(deps)
        async with deps.sessions() as s:
            row = await repo.by_public_id(s, f.id)
            job = (await s.execute(select(repo.file_job))).one()
        assert (job.Status, job.Attempts, row.ScanStatus) == ("Failed", 3, "Failed")


def row_time():  # type: ignore[no-untyped-def]
    from datetime import datetime

    return datetime(2000, 1, 1)


async def test_crop_makes_an_independent_question_image() -> None:
    async with env() as (deps, store, _):
        f = await upload(deps, store, OWNER, make_pdf(["A diagram would be here."]), course=COURSE)
        await drain(deps)
        crop = await svc.create_crop(
            deps, f.id, svc.CropInput(page=1, box=svc.CropBox(x=50, y=40, w=200, h=60), course_ref=COURSE)
        )
        assert (crop["purpose"], crop["mimeType"], crop["courseRef"], crop["scanStatus"]) == (
            "QuestionImage",
            "image/png",
            COURSE,
            "Clean",
        )
        png_key = next(k for k in store.objects if k.startswith("crops/"))
        assert store.objects[png_key].startswith(b"\x89PNG")
        assert "file.crop_created" in await topics(deps)
        with pytest.raises(FileRejected):
            await svc.create_crop(deps, f.id, svc.CropInput(page=9, box=svc.CropBox(x=0, y=0, w=10, h=10)))
