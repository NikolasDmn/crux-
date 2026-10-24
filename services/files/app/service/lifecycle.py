"""Account export and deletion for files (spec 11, 9.4).

Export: core sends every service's JSON part; files adds its own (the file list) and the user's
original PDFs, zips it all into one `Export` StoredFile owned by the user (not counted in the
quota), downloadable through the usual signed URL for 24 hours.
"""

import hashlib
import json
import tempfile
import zipfile
from datetime import timedelta
from typing import Any

from sqlalchemy import select, update
from sqlalchemy.ext.asyncio import AsyncSession

from app.repo import files as repo
from app.repo import storage
from app.repo.files import stored_file
from app.service.deps import Deps
from crux_common.db import transaction
from crux_common.ids import new_id
from crux_common.messaging.bus import Delivery
from crux_common.messaging.contracts import Contracts
from crux_common.messaging.outbox import emit
from crux_common.timeutil import iso, utcnow

EXPORT_DAYS = timedelta(hours=24)


def _file_meta(r: Any) -> dict[str, Any]:
    return {
        "id": r.PublicId,
        "purpose": r.Purpose,
        "fileName": r.FileName,
        "mimeType": r.MimeType,
        "sizeBytes": r.SizeBytes,
        "sha256": r.Sha256,
        "status": r.Status,
        "courseId": r.CourseRef,
        "uploadedAt": iso(r.UploadedAt) if r.UploadedAt else None,
        "purgedAt": iso(r.PurgedAt) if r.PurgedAt else None,
    }


async def build_export(deps: Deps, user_ref: str, export_ref: str, parts: dict[str, Any]) -> dict[str, Any]:
    """Returns {fileRef, sizeBytes}. Idempotent per export ref (a retry returns the same file)."""
    name = f"crux-export-{export_ref}.zip"
    async with deps.sessions() as s:
        done = (
            await s.execute(
                select(stored_file).where(
                    stored_file.c.OwnerUserRef == user_ref,
                    stored_file.c.Purpose == "Export",
                    stored_file.c.FileName == name,
                )
            )
        ).first()
        if done is not None and done.Status == "Active":
            return {"fileRef": done.PublicId, "sizeBytes": done.SizeBytes}
        mine = (
            await s.execute(
                select(stored_file).where(
                    stored_file.c.OwnerUserRef == user_ref,
                    stored_file.c.Purpose.in_(["Original", "QuestionImage", "Thumbnail"]),
                )
            )
        ).all()
    with tempfile.SpooledTemporaryFile(max_size=32 * 1024 * 1024) as tmp:
        with zipfile.ZipFile(tmp, "w", compression=zipfile.ZIP_DEFLATED) as z:
            z.writestr(
                "README.txt",
                "Your Crux data. Each service's part is a JSON file; originals/ holds your uploaded PDFs.\n",
            )
            for part, data in parts.items():
                z.writestr(f"{part}.json", json.dumps(data, ensure_ascii=False, indent=1, default=str))
            z.writestr("files.json", json.dumps([_file_meta(r) for r in mine], ensure_ascii=False, indent=1))
            for r in mine:
                if r.Purpose == "Original" and r.Status in ("Active", "PendingPurge") and r.StorageKey:
                    with z.open(f"originals/{r.PublicId}-{r.FileName}", "w") as out:
                        async for chunk in deps.store.read(r.StorageKey):
                            out.write(chunk)
        size = tmp.tell()
        tmp.seek(0)
        h = hashlib.sha256()
        while block := tmp.read(1024 * 1024):
            h.update(block)
        tmp.seek(0)
        public_id = new_id()
        key = f"exports/{user_ref}/{public_id}.zip"
        await deps.store.put_file(key, tmp, "application/zip")
    async with transaction(deps.sessions) as s:
        await repo.insert_derived(
            s,
            PublicId=public_id,
            OwnerUserRef=user_ref,
            UploadedByUserRef=user_ref,
            CourseRef=None,
            Purpose="Export",
            StorageKey=key,
            FileName=name,
            MimeType="application/zip",
            SizeBytes=size,
            Sha256=h.hexdigest(),
            SourcePage=None,
            UploadedAt=utcnow(),
        )
    return {"fileRef": public_id, "sizeBytes": size}


async def expire_exports(deps: Deps) -> int:
    """Exports are downloadable for 24 h (spec 11); then the object goes."""
    n = 0
    async with transaction(deps.sessions) as s:
        rows = (
            await s.execute(
                select(stored_file)
                .where(
                    stored_file.c.Purpose == "Export",
                    stored_file.c.Status == "Active",
                    stored_file.c.UploadedAt < utcnow() - EXPORT_DAYS,
                )
                .with_for_update()
            )
        ).all()
        for r in rows:
            await deps.store.delete(r.StorageKey)
            await repo.set_status(s, r.Id, "Active", "Deleted", StorageKey=None)
            n += 1
    return n


class UserDeletedHandler:
    """`user.deleted`: every object the user owns goes (their courses were transferred or deleted
    by core before this), the quota row too; then `user.purged`."""

    topics = ("user.deleted",)

    def __init__(self, deps: Deps, contracts: Contracts) -> None:
        self._deps = deps
        self._contracts = contracts

    async def handle(self, s: AsyncSession, d: Delivery) -> None:
        user_ref = d.envelope.payload["userRef"]
        rows = (
            await s.execute(select(stored_file).where(stored_file.c.OwnerUserRef == user_ref).with_for_update())
        ).all()
        # Derived files first (they point at their originals).
        for r in sorted(rows, key=lambda r: r.DerivedFromFileId is None):
            if r.StorageKey:
                await self._deps.store.delete(r.StorageKey)
            if r.Status != "Deleted":
                await repo.set_status(s, r.Id, r.Status, "Deleted", StorageKey=None, PurgeAt=None)
        # Files uploaded into someone else's course stay (they belong to that course and its owner);
        # only the uploader reference is cleared.
        await s.execute(
            update(stored_file).where(stored_file.c.UploadedByUserRef == user_ref).values(UploadedByUserRef=None)
        )
        await storage.delete_user(s, user_ref)
        await emit(s, self._contracts, "user.purged", user_ref, {"userRef": user_ref, "service": "files"})
