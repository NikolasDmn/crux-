"""Public routes under /api/v1/files (spec 15). The gateway strips the prefix."""

import asyncio
import json
from collections.abc import AsyncIterator

from fastapi import APIRouter, Request
from fastapi.responses import StreamingResponse

from app.repo import files as repo
from app.service import files as svc
from app.service.deps import Deps
from crux_common.auth import User
from crux_common.errors import NotFound
from crux_common.sse import PING_SECONDS, event, redeem

router = APIRouter(tags=["files"])


def deps(request: Request) -> Deps:
    d: Deps = request.app.state.deps
    return d


@router.post("/uploads", response_model=svc.UploadStarted, response_model_by_alias=True)
async def start_upload(body: svc.UploadInput, user: User, request: Request) -> svc.UploadStarted:
    """Reserve quota and get a pre-signed PUT URL (15 min). If the same file is already stored,
    `alreadyPresent` is true and there's nothing to upload."""
    return await svc.start_upload(deps(request), user, body)


@router.get("/storage", response_model=svc.StorageView, response_model_by_alias=True)
async def storage_view(user: User, request: Request) -> svc.StorageView:
    """Used vs quota, per course (spec 6.2 quota view)."""
    return await svc.storage_view(deps(request), user)


@router.patch("/storage/settings", response_model=svc.StorageView, response_model_by_alias=True)
async def storage_settings(body: svc.StorageSettings, user: User, request: Request) -> svc.StorageView:
    return await svc.patch_storage_settings(deps(request), user, body)


@router.post("/{file_id}/complete", response_model=svc.FileView, response_model_by_alias=True)
async def complete(file_id: str, user: User, request: Request) -> svc.FileView:
    """After the PUT: verify size, sha256 and type, then queue the virus scan."""
    return await svc.complete_upload(deps(request), user, file_id)


@router.get("/{file_id}", response_model=svc.FileView, response_model_by_alias=True)
async def get_file(file_id: str, user: User, request: Request) -> svc.FileView:
    return await svc.get(deps(request), user, file_id)


@router.get("/{file_id}/url", response_model=svc.SignedUrl, response_model_by_alias=True)
async def download_url(file_id: str, user: User, request: Request) -> svc.SignedUrl:
    """A signed GET URL, valid 5 minutes."""
    return await svc.download_url(deps(request), user, file_id)


@router.get("/{file_id}/pages/{page}/text", response_model=svc.PageText, response_model_by_alias=True)
async def page_text(file_id: str, page: int, user: User, request: Request) -> svc.PageText:
    """Extracted text of one page; works for purged originals too (spec 6.2)."""
    return await svc.page_text(deps(request), user, file_id, page)


@router.get("/{file_id}/pages/{page}/thumbnail", response_model=svc.SignedUrl, response_model_by_alias=True)
async def page_thumbnail(file_id: str, page: int, user: User, request: Request) -> svc.SignedUrl:
    """A signed URL for the page's thumbnail (only when thumbnails are kept)."""
    return await svc.thumbnail_url(deps(request), user, file_id, page)


@router.get("/{file_id}/events")
async def file_events(file_id: str, ticket: str, request: Request) -> StreamingResponse:
    """SSE (spec 3.4): `scanned`, `extracted`, `failed`, then `done`. Auth: `?ticket=` from
    `POST /api/v1/sse-ticket`."""
    d = deps(request)
    user = await redeem(request.app.state.redis, ticket)
    row = await svc.readable(d, user, file_id)

    async def stream() -> AsyncIterator[str]:
        seen_scan, last_ping = "Pending", asyncio.get_running_loop().time()
        while True:
            if await request.is_disconnected():
                return
            async with d.sessions() as s:
                cur = await repo.by_id(s, row.Id)
            if cur is None:
                yield event("error", {"code": "not_found"})
                return
            if cur.ScanStatus != seen_scan and cur.ScanStatus != "Pending":
                seen_scan = cur.ScanStatus
                yield event("scanned", {"status": cur.ScanStatus})
                if cur.ScanStatus in ("Infected", "Failed"):
                    yield event("failed", {"reason": cur.ScanStatus.lower()})
                    yield event("done", {})
                    return
            if cur.ExtractStatus in ("Done", "Failed", "NotNeeded") and seen_scan == "Clean":
                yield event(
                    "extracted" if cur.ExtractStatus == "Done" else "failed",
                    {"pageCount": cur.PageCount, "status": cur.ExtractStatus},
                )
                yield event("done", {})
                return
            now = asyncio.get_running_loop().time()
            if now - last_ping >= PING_SECONDS:
                last_ping = now
                yield event("ping", {})
            await asyncio.sleep(1)

    return StreamingResponse(stream(), media_type="text/event-stream", headers={"cache-control": "no-cache"})


internal = APIRouter(prefix="/internal/v1", tags=["internal"])


@internal.post("/files/{file_id}/crops")
async def internal_crop(file_id: str, body: svc.CropInput, request: Request) -> dict[str, object]:
    """For core: render a question image from a page region (spec 6.1 step 6). Service token required."""
    from crux_common.auth import service_caller

    await service_caller(request)
    return await svc.create_crop(deps(request), file_id, body)


@internal.get("/files/{file_id}")
async def internal_file(file_id: str, request: Request) -> dict[str, object]:
    """For core (spec 15 internal table): file state, ownership and course. Service token required."""
    from crux_common.auth import service_caller

    await service_caller(request)
    async with deps(request).sessions() as s:
        row = await repo.by_public_id(s, file_id)
    if row is None or row.Status == "Deleted":
        raise NotFound()
    v = svc.view(row)
    return {
        **json.loads(v.model_dump_json(by_alias=True)),
        "ownerRef": row.OwnerUserRef,
        "uploadedByRef": row.UploadedByUserRef,
        "courseRef": row.CourseRef,
    }


@internal.get("/chunks/search")
async def internal_search(request: Request, files: str, q: str, k: int = 8) -> list[dict[str, object]]:
    """For ai (spec 7.4): the top-k chunks of these files for a query (FULLTEXT). Service token."""
    from crux_common.auth import service_caller

    await service_caller(request)
    refs = [f for f in files.split(",") if f][:200]
    async with deps(request).sessions() as s:
        rows = await repo.search_chunks(s, refs, q, max(1, min(k, 50)))
    return [{"fileRef": r.FileRef, "chunkNo": r.ChunkNo, "pageNo": r.PageNo, "text": r.Text} for r in rows]


@internal.get("/files/{file_id}/chunks")
async def internal_chunks(file_id: str, request: Request, pages: str = "") -> list[dict[str, object]]:
    """For ai: a file's chunks (all, or some pages). Service token."""
    from crux_common.auth import service_caller

    await service_caller(request)
    wanted = [int(p) for p in pages.split(",") if p.isdigit()] or None
    async with deps(request).sessions() as s:
        row = await repo.by_public_id(s, file_id)
        if row is None:
            raise NotFound()
        rows = await repo.chunks_of(s, row.Id, wanted)
    return [{"fileRef": file_id, "chunkNo": r.ChunkNo, "pageNo": r.PageNo, "text": r.Text} for r in rows]
