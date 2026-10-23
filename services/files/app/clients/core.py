"""files → core internal API (spec 4.2): course roles and file access through courses.
Answers are cached ≤ 30 s (spec 4.2)."""

import time
from dataclasses import dataclass
from typing import Protocol

import httpx

from crux_common.service_tokens import ServiceTokens

CACHE_SECONDS = 30.0


@dataclass(frozen=True)
class CourseAccess:
    owner_ref: str
    role: str | None  # Owner, Editor, Viewer; None = no access


class CoreApi(Protocol):
    async def course_access(self, user_ref: str, course_ref: str) -> CourseAccess | None: ...
    async def file_ref_access(self, user_ref: str, file_ref: str) -> bool: ...


class HttpCore:
    def __init__(self, base: str, tokens: ServiceTokens, client: httpx.AsyncClient | None = None) -> None:
        self._base = base.rstrip("/")
        self._tokens = tokens
        self._client = client or httpx.AsyncClient(timeout=2.0)
        self._cache: dict[tuple[str, str, str], tuple[float, object]] = {}

    def _cached(self, key: tuple[str, str, str]) -> object | None:
        hit = self._cache.get(key)
        if hit and hit[0] > time.monotonic():
            return hit[1]
        return None

    async def _get(self, path: str, params: dict[str, str]) -> httpx.Response:
        headers = await self._tokens.headers("core", on_behalf_of=params.get("user"))
        return await self._client.get(f"{self._base}{path}", params=params, headers=headers)

    async def course_access(self, user_ref: str, course_ref: str) -> CourseAccess | None:
        key = ("course", user_ref, course_ref)
        if (hit := self._cached(key)) is not None:
            return hit if isinstance(hit, CourseAccess) else None
        r = await self._get("/internal/v1/access", {"user": user_ref, "course": course_ref})
        result: CourseAccess | None = None
        if r.status_code != 404:
            r.raise_for_status()
            d = r.json()
            result = CourseAccess(owner_ref=d["ownerRef"], role=d.get("role"))
        self._cache[key] = (time.monotonic() + CACHE_SECONDS, result if result else False)
        return result

    async def file_ref_access(self, user_ref: str, file_ref: str) -> bool:
        key = ("file", user_ref, file_ref)
        if (hit := self._cached(key)) is not None:
            return bool(hit)
        r = await self._get("/internal/v1/access/file-ref", {"user": user_ref, "ref": file_ref})
        r.raise_for_status()
        allowed = bool(r.json()["allowed"])
        self._cache[key] = (time.monotonic() + CACHE_SECONDS, allowed)
        return allowed


class FakeCore:
    """Test double: set `courses[(user, course)]` and `file_access[(user, ref)]`."""

    def __init__(self) -> None:
        self.courses: dict[tuple[str, str], CourseAccess] = {}
        self.file_access: set[tuple[str, str]] = set()

    async def course_access(self, user_ref: str, course_ref: str) -> CourseAccess | None:
        return self.courses.get((user_ref, course_ref))

    async def file_ref_access(self, user_ref: str, file_ref: str) -> bool:
        return (user_ref, file_ref) in self.file_access
