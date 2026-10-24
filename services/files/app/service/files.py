"""Uploads, downloads, page text, quota view (backend-spec 6.1, 6.2 quota view)."""

import logging
from datetime import timedelta
from typing import Any

from pydantic import BaseModel, ConfigDict
from pydantic.alias_generators import to_camel

from app.clients.store import sha256_and_head
from app.domain import uploads as rules
from app.domain.uploads import Rejection
from app.repo import files as repo
from app.repo import storage
from app.service.deps import Deps
from crux_common.auth import Principal
from crux_common.db import transaction
from crux_common.errors import FileRejected, NotFound, QuotaExceeded, RateLimited
from crux_common.ids import new_id
from crux_common.messaging.outbox import emit
from crux_common.timeutil import iso, utcnow

log = logging.getLogger("crux.files")

UPLOAD_URL_SECONDS = 15 * 60
DOWNLOAD_URL_SECONDS = 5 * 60
UPLOADS_PER_MINUTE = 20


class Camel(BaseModel):
    model_config = ConfigDict(alias_generator=to_camel, populate_by_name=True)


class FileView(Camel):
    id: str
    file_name: str
    mime_type: str
    size_bytes: int
    sha256: str
    purpose: str
    #: Uploading, Active, PendingPurge, Purged, Deleted.
    status: str
    #: Pending, Clean, Infected, Failed.
    scan_status: str
    #: Pending, Done, Failed, NotNeeded.
    extract_status: str
    page_count: int | None
    course_id: str | None
    uploaded_at: str


def view(row: Any) -> FileView:
    return FileView(
        id=row.PublicId,
        file_name=row.FileName,
        mime_type=row.MimeType,
        size_bytes=row.SizeBytes,
        sha256=row.Sha256,
        purpose=row.Purpose,
        status=row.Status,
        scan_status=row.ScanStatus,
        extract_status=row.ExtractStatus,
        page_count=row.PageCount,
        course_id=row.CourseRef,
        uploaded_at=iso(row.UploadedAt),
    )


class UploadInput(Camel):
    file_name: str
    size_bytes: int
    sha256: str
    #: Only Original for now (question crops and thumbnails are made by the service).
    purpose: str = "Original"
    #: Upload for a course: it counts against the course owner's quota (D2-4). Needs Editor.
    course_id: str | None = None


class UploadStarted(Camel):
    file: FileView
    #: Absent when the same file is already stored (dedupe): skip the PUT.
    upload_url: str | None
    #: Send these headers with the PUT.
    upload_headers: dict[str, str]
    already_present: bool
    expires_at: str | None


def _reject(r: Rejection) -> FileRejected:
    return FileRejected(r.message, {"reason": r.reason})


async def _rate_limit(deps: Deps, user_ref: str) -> None:
    """Spec 12: uploads 20 per minute per user."""
    if deps.redis is None:
        return
    key = f"crux:rl:uploads:{user_ref}"
    n = await deps.redis.incr(key)
    if n == 1:
        await deps.redis.expire(key, 60)
    if n > UPLOADS_PER_MINUTE:
        raise RateLimited(details={"retryAfterSecs": 60})


async def _quota_owner(deps: Deps, user: Principal, course_ref: str | None) -> str:
    """D2-4: course uploads count against the course owner; needs Owner or Editor (404 otherwise)."""
    if course_ref is None:
        return user.user_ref
    access = await deps.core.course_access(user.user_ref, course_ref)
    if access is None or access.role not in ("Owner", "Editor"):
        raise NotFound()
    return access.owner_ref


async def start_upload(deps: Deps, user: Principal, inp: UploadInput) -> UploadStarted:
    if inp.purpose != "Original":
        raise _reject(Rejection("type", "Only originals can be uploaded."))
    checked = rules.check_start(
        inp.file_name, inp.size_bytes, inp.sha256, deps.settings.allowed_types, deps.settings.max_upload_bytes
    )
    if isinstance(checked, Rejection):
        raise _reject(checked)
    mime = checked
    sha = inp.sha256.lower()
    await _rate_limit(deps, user.user_ref)
    owner = await _quota_owner(deps, user, inp.course_id)
    now = utcnow()
    async with transaction(deps.sessions) as s:
        await storage.ensure(s, owner, deps.settings.default_quota_bytes)
        existing = await repo.original_by_sha(s, owner, sha)
        if existing is not None and existing.Status in ("Active", "PendingPurge"):
            # Spec 6.1 dedupe: the same file is already stored for this owner.
            return UploadStarted(
                file=view(existing), upload_url=None, upload_headers={}, already_present=True, expires_at=None
            )
        if existing is not None and existing.Status == "Uploading":
            file_id, row = existing.Id, existing
        else:
            if not await storage.reserve(s, owner, inp.size_bytes):
                q = await storage.get(s, owner)
                raise QuotaExceeded(
                    details={"usedBytes": q.UsedBytes if q else 0, "quotaBytes": q.QuotaBytes if q else 0}
                )
            values: dict[str, Any] = {
                "UploadedByUserRef": user.user_ref,
                "CourseRef": inp.course_id,
                "FileName": rules.safe_file_name(inp.file_name),
                "MimeType": mime,
                "SizeBytes": inp.size_bytes,
                "UploadedAt": now,
            }
            if existing is not None:
                # Purged or Deleted original of the same file: the same row comes back (restore,
                # spec 6.1/6.2), so core can reattach it to its material.
                key = f"originals/{owner}/{existing.PublicId}/{new_id()}"
                await repo.set_status(
                    s, existing.Id, existing.Status, "Uploading", StorageKey=key, PurgedAt=None, PurgeAt=None, **values
                )
                await repo.set_fields(s, existing.Id, ScanStatus="Pending", ExtractStatus="Pending")
                file_id = existing.Id
            else:
                public_id = new_id()
                key = f"originals/{owner}/{public_id}/{new_id()}"
                file_id = await repo.insert_original(
                    s, PublicId=public_id, OwnerUserRef=owner, StorageKey=key, Sha256=sha, **values
                )
            row = await repo.by_id(s, file_id)
        assert row is not None
        url = deps.store.presign_put(row.StorageKey, row.MimeType, UPLOAD_URL_SECONDS)
        return UploadStarted(
            file=view(row),
            upload_url=url,
            upload_headers={"content-type": row.MimeType},
            already_present=False,
            expires_at=iso(now + timedelta(seconds=UPLOAD_URL_SECONDS)),
        )


async def _reject_upload(deps: Deps, s: Any, row: Any, rejection: Rejection) -> None:
    await deps.store.delete(row.StorageKey)
    await storage.release_reserved(s, row.OwnerUserRef, row.SizeBytes)
    await repo.set_status(s, row.Id, "Uploading", "Deleted", StorageKey=None)


async def complete_upload(deps: Deps, user: Principal, file_ref: str) -> FileView:
    """Spec 6.1 step 2: check size and sha256 from the stored object, sniff the type, settle the
    quota, set Active and queue the scan."""
    rejection: Rejection | None = None
    async with transaction(deps.sessions) as s:
        row = await repo.by_public_id(s, file_ref, for_update=True)
        if row is None or user.user_ref not in (row.UploadedByUserRef, row.OwnerUserRef):
            raise NotFound()
        if row.Status != "Uploading":
            return view(row)  # idempotent: a retried complete gets the current state
        size = await deps.store.size(row.StorageKey)
        if size is None:
            raise FileRejected("The upload hasn't arrived yet. PUT the file to uploadUrl first.", {"reason": "missing"})
        if size != row.SizeBytes:
            rejection = Rejection("size", "The uploaded file's size doesn't match.")
        else:
            sha, head, _ = await sha256_and_head(deps.store, row.StorageKey)
            if sha != row.Sha256:
                rejection = Rejection("checksum", "The uploaded file doesn't match its sha256.")
            elif not rules.sniff_ok(row.MimeType, head):
                rejection = Rejection("type", "That isn't a PDF.")
        if rejection is not None:
            await _reject_upload(deps, s, row, rejection)
        else:
            await storage.settle(s, row.OwnerUserRef, row.SizeBytes)
            await repo.set_status(s, row.Id, "Uploading", "Active")
            await repo.enqueue(s, row.Id, "Scan", utcnow())
            fresh = await repo.by_id(s, row.Id)
            assert fresh is not None
            return view(fresh)
    raise _reject(rejection)


async def can_read(deps: Deps, user: Principal, row: Any) -> bool:
    """Owner, uploader, or a reader of a course that references the file (spec 6.1 step 7)."""
    if user.user_ref in (row.OwnerUserRef, row.UploadedByUserRef):
        return True
    return await deps.core.file_ref_access(user.user_ref, row.PublicId)


async def readable(deps: Deps, user: Principal, file_ref: str) -> Any:
    async with deps.sessions() as s:
        row = await repo.by_public_id(s, file_ref)
    if row is None or row.Status == "Deleted" or not await can_read(deps, user, row):
        raise NotFound()
    return row


async def get(deps: Deps, user: Principal, file_ref: str) -> FileView:
    return view(await readable(deps, user, file_ref))


class SignedUrl(Camel):
    url: str
    expires_at: str


async def download_url(deps: Deps, user: Principal, file_ref: str) -> SignedUrl:
    row = await readable(deps, user, file_ref)
    if row.Status not in ("Active", "PendingPurge") or row.ScanStatus != "Clean" or row.StorageKey is None:
        # Purged originals: the viewer falls back to text (spec 6.2); unscanned files aren't served.
        raise NotFound("The original isn't available.")
    url = deps.store.presign_get(row.StorageKey, row.FileName, DOWNLOAD_URL_SECONDS)
    return SignedUrl(url=url, expires_at=iso(utcnow() + timedelta(seconds=DOWNLOAD_URL_SECONDS)))


class PageText(Camel):
    page: int
    text: str


async def page_text(deps: Deps, user: Principal, file_ref: str, page: int) -> PageText:
    row = await readable(deps, user, file_ref)
    async with deps.sessions() as s:
        text = await repo.page_text(s, row.Id, page)
    if text is None:
        raise NotFound()
    return PageText(page=page, text=text)


class CourseUsage(Camel):
    course_id: str | None
    bytes: int
    originals: int
    #: Bytes the storage saver could free (milestone 7: originals already extracted).
    purgeable: int


class StorageView(Camel):
    used_bytes: int
    reserved_bytes: int
    quota_bytes: int
    tier: str | None
    auto_purge_originals: bool
    keep_thumbnails: bool
    by_course: list[CourseUsage]


async def storage_view(deps: Deps, user: Principal) -> StorageView:
    async with transaction(deps.sessions) as s:
        await storage.ensure(s, user.user_ref, deps.settings.default_quota_bytes)
        q = await storage.get(s, user.user_ref)
        usage = await repo.usage_by_course(s, user.user_ref)
    assert q is not None
    return StorageView(
        used_bytes=q.UsedBytes,
        reserved_bytes=q.ReservedBytes,
        quota_bytes=q.QuotaBytes,
        tier=q.TierCode,
        auto_purge_originals=bool(q.AutoPurgeOriginals),
        keep_thumbnails=bool(q.KeepThumbnails),
        by_course=[
            CourseUsage(course_id=u.CourseRef, bytes=int(u.Bytes), originals=int(u.Originals), purgeable=int(u.Bytes))
            for u in usage
        ],
    )


class StorageSettings(Camel):
    auto_purge_originals: bool | None = None
    keep_thumbnails: bool | None = None


async def patch_storage_settings(deps: Deps, user: Principal, inp: StorageSettings) -> StorageView:
    values = {
        k: v
        for k, v in {"AutoPurgeOriginals": inp.auto_purge_originals, "KeepThumbnails": inp.keep_thumbnails}.items()
        if v is not None
    }
    async with transaction(deps.sessions) as s:
        await storage.ensure(s, user.user_ref, deps.settings.default_quota_bytes)
        if values:
            await storage.set_settings(s, user.user_ref, **values)
    return await storage_view(deps, user)


async def emit_event(deps: Deps, s: Any, topic: str, aggregate: str, payload: dict[str, Any]) -> None:
    await emit(s, deps.contracts, topic, aggregate, payload)
