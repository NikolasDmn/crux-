"""Storage saver (spec 6.2) and opt-in thumbnails (spec 6.1 step 5).

Only the original object is ever deleted: the StoredFile row (with its sha256), the extracted text,
question crops and thumbnails stay, so tutoring, search and citations keep working. Files owns the
purge state; core mirrors it on its materials through `file.purge_scheduled`, `file.purged` and
`file.restored`, and asks for purges with `material.purge_requested` / `material.purge_cancelled`.
"""

import asyncio
import hashlib
import io
import logging
from datetime import timedelta
from typing import Any

from sqlalchemy.ext.asyncio import AsyncSession

from app.repo import files as repo
from app.repo import storage
from app.service.deps import Deps
from crux_common.db import transaction
from crux_common.ids import new_id
from crux_common.messaging.bus import Delivery
from crux_common.messaging.contracts import Contracts
from crux_common.messaging.outbox import emit
from crux_common.timeutil import iso, parse_iso, utcnow

log = logging.getLogger("crux.files.saver")

#: Spec 6.2: the undo window.
UNDO_WINDOW = timedelta(days=7)
THUMBNAIL_WIDTH = 320


async def schedule(s: AsyncSession, contracts: Contracts, row: Any, purge_at: Any, reason: str) -> bool:
    """Active original → PendingPurge until `purge_at`. False when it isn't eligible."""
    if row.Purpose != "Original" or row.Status != "Active" or row.ScanStatus != "Clean":
        return False
    await repo.set_status(s, row.Id, "Active", "PendingPurge", PurgeAt=purge_at, PurgeRequestedAt=utcnow())
    if reason == "auto":
        # A purge core asked for is already shown there; an automatic one has to be announced.
        await emit(
            s,
            contracts,
            "file.purge_scheduled",
            row.PublicId,
            {"fileRef": row.PublicId, "ownerRef": row.OwnerUserRef, "purgeAt": iso(purge_at), "reason": reason},
        )
    return True


async def after_extract(deps: Deps, s: AsyncSession, file_id: int) -> None:
    """Spec 6.2 automatic purge (AutoPurgeOriginals) and 6.1 step 5 thumbnails (KeepThumbnails), in
    the extract job's transaction. The 7-day window leaves time to cut question images first."""
    row = await repo.by_id(s, file_id, for_update=True)
    if row is None or row.Purpose != "Original":
        return
    q = await storage.get(s, row.OwnerUserRef)
    if q is None:
        return
    if q.KeepThumbnails:
        await repo.enqueue(s, row.Id, "Thumbnail", utcnow())
    if q.AutoPurgeOriginals:
        await schedule(s, deps.contracts, row, utcnow() + UNDO_WINDOW, "auto")


class PurgeRequestsHandler:
    """core's `material.purge_requested` / `material.purge_cancelled` (the course owner's buttons)."""

    topics = ("material.purge_requested", "material.purge_cancelled")

    def __init__(self, contracts: Contracts) -> None:
        self._contracts = contracts

    async def handle(self, session: AsyncSession, d: Delivery) -> None:
        p = d.envelope.payload
        row = await repo.by_public_id(session, p["fileRef"], for_update=True)
        if row is None or row.OwnerUserRef != p["ownerRef"]:
            # Only the owner (whose quota the file uses, D2-4) can free its space.
            log.warning("purge request for a file the owner doesn't own; ignored", extra={"file": p["fileRef"]})
            return
        if d.topic == "material.purge_requested":
            await schedule(session, self._contracts, row, parse_iso(p["purgeAt"]), "requested")
        elif row.Status == "PendingPurge":
            await repo.set_status(session, row.Id, "PendingPurge", "Active", PurgeAt=None, PurgeRequestedAt=None)


async def purge_due(deps: Deps) -> int:
    """Job `purge.originals` (hourly): delete the object, keep the row and everything derived."""
    n = 0
    owners: dict[str, int] = {}
    async with transaction(deps.sessions) as s:
        now = utcnow()
        for row in await repo.due_purges(s, now):
            await deps.store.delete(row.StorageKey)
            await repo.set_status(s, row.Id, "PendingPurge", "Purged", StorageKey=None, PurgeAt=None, PurgedAt=now)
            await storage.release_used(s, row.OwnerUserRef, row.SizeBytes)
            await emit(
                s,
                deps.contracts,
                "file.purged",
                row.PublicId,
                {"fileRef": row.PublicId, "ownerRef": row.OwnerUserRef, "sha256": row.Sha256, "purgedAt": iso(now)},
            )
            owners[row.OwnerUserRef] = owners.get(row.OwnerUserRef, 0) + 1
            n += 1
        for owner, count in owners.items():
            await emit(
                s,
                deps.contracts,
                "notify.requested",
                owner,
                {
                    "userRef": owner,
                    "kind": "storage.purged",
                    "dedupeKey": f"storage.purged:{owner}:{now:%Y-%m-%dT%H}",
                    "dueAt": iso(now),
                    "payload": {"count": count},
                },
            )
    return n


# ---------- thumbnails ----------


def render_thumbnails(pdf: bytes) -> list[bytes]:
    """320 px wide WebP per page (spec 6.1 step 5)."""
    import pymupdf
    from PIL import Image

    out: list[bytes] = []
    with pymupdf.open(stream=pdf, filetype="pdf") as doc:  # type: ignore[no-untyped-call]
        for page in doc:
            zoom = THUMBNAIL_WIDTH / max(page.rect.width, 1)
            pix = page.get_pixmap(matrix=pymupdf.Matrix(zoom, zoom))  # type: ignore[no-untyped-call]
            image = Image.open(io.BytesIO(pix.tobytes("png")))
            buf = io.BytesIO()
            image.save(buf, format="WEBP", quality=70)
            out.append(buf.getvalue())
    return out


async def make_thumbnails(deps: Deps, file_id: int) -> None:
    """Thumbnail job. Skipped when the original is gone, the user turned thumbnails off, or they
    would go over quota (thumbnails count against it)."""
    async with deps.sessions() as s:
        row = await repo.by_id(s, file_id)
        q = await storage.get(s, row.OwnerUserRef) if row else None
        existing = await repo.derived(s, file_id, "Thumbnail") if row else []
    if row is None or q is None or not q.KeepThumbnails or row.StorageKey is None or existing:
        return
    data = b"".join([c async for c in deps.store.read(row.StorageKey)])
    images = await asyncio.to_thread(render_thumbnails, data)
    total = sum(len(i) for i in images)
    async with transaction(deps.sessions) as s:
        if not await storage.reserve(s, row.OwnerUserRef, total):
            log.info("thumbnails skipped: over quota", extra={"file_id": file_id, "bytes": total})
            return
        base = row.FileName.rsplit(".", 1)[0]
        for page, img in enumerate(images, start=1):
            public_id = new_id()
            key = f"thumbnails/{row.OwnerUserRef}/{row.PublicId}/{page}.webp"
            await deps.store.put_bytes(key, img, "image/webp")
            await repo.insert_derived(
                s,
                PublicId=public_id,
                OwnerUserRef=row.OwnerUserRef,
                UploadedByUserRef=row.UploadedByUserRef,
                CourseRef=row.CourseRef,
                Purpose="Thumbnail",
                DerivedFromFileId=row.Id,
                SourcePage=page,
                StorageKey=key,
                FileName=f"{base}-p{page}.webp"[:255],
                MimeType="image/webp",
                SizeBytes=len(img),
                Sha256=hashlib.sha256(img).hexdigest(),
                UploadedAt=utcnow(),
            )
        await storage.settle(s, row.OwnerUserRef, total)


async def thumbnails_setting_changed(deps: Deps, s: AsyncSession, owner_ref: str, keep: bool) -> int:
    """On: queue thumbnails for originals still present. Off: delete them and give the space back."""
    n = 0
    if keep:
        for row in await repo.originals_for_thumbnails(s, owner_ref):
            await repo.enqueue(s, row.Id, "Thumbnail", utcnow())
            n += 1
        return n
    for t in await repo.owner_thumbnails(s, owner_ref):
        await deps.store.delete(t.StorageKey)
        await repo.set_status(s, t.Id, "Active", "Deleted", StorageKey=None)
        await storage.release_used(s, owner_ref, t.SizeBytes)
        n += 1
    return n


async def restored(s: AsyncSession, contracts: Contracts, row: Any) -> None:
    await emit(
        s,
        contracts,
        "file.restored",
        row.PublicId,
        {"fileRef": row.PublicId, "ownerRef": row.OwnerUserRef, "sha256": row.Sha256},
    )
