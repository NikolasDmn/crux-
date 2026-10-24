"""ai → core and files internal APIs (spec 3.2). Core decides access; ai never trusts itself for
authorization (spec 9.2) and never writes core tables (9.3). Access answers are cached ≤ 30 s."""

import time
from dataclasses import dataclass, field
from typing import Any, Protocol

import httpx

from crux_common.service_tokens import ServiceTokens

CACHE_SECONDS = 30.0


@dataclass(frozen=True)
class CourseAccess:
    owner_ref: str
    role: str | None  # Owner, Editor, Viewer


class CoreApi(Protocol):
    async def course_access(self, user_ref: str, course_ref: str) -> CourseAccess | None: ...
    async def materials(self, user_ref: str, refs: list[str]) -> list[dict[str, Any]]: ...
    async def context(
        self, user_ref: str, course_ref: str, scopes: list[str], anchor: dict[str, Any]
    ) -> dict[str, Any]: ...
    async def call(
        self, method: str, path: str, user_ref: str, body: dict[str, Any] | None = None
    ) -> dict[str, Any]: ...


class FilesApi(Protocol):
    async def file(self, file_ref: str) -> dict[str, Any] | None: ...
    async def search(self, file_refs: list[str], query: str, k: int) -> list[dict[str, Any]]: ...
    async def chunks(self, file_ref: str, pages: list[int] | None = None) -> list[dict[str, Any]]: ...


class CoreError(Exception):
    def __init__(self, status: int, body: Any) -> None:
        super().__init__(f"core answered {status}")
        self.status = status
        self.body = body


class HttpCore:
    def __init__(self, base: str, tokens: ServiceTokens) -> None:
        self._base = base.rstrip("/")
        self._tokens = tokens
        self._client = httpx.AsyncClient(timeout=10.0)
        self._cache: dict[tuple[str, str], tuple[float, CourseAccess | None]] = {}

    async def _headers(self, user_ref: str) -> dict[str, str]:
        return await self._tokens.headers("core", on_behalf_of=user_ref)

    async def course_access(self, user_ref: str, course_ref: str) -> CourseAccess | None:
        key = (user_ref, course_ref)
        hit = self._cache.get(key)
        if hit and hit[0] > time.monotonic():
            return hit[1]
        r = await self._client.get(
            f"{self._base}/internal/v1/access",
            params={"user": user_ref, "course": course_ref},
            headers=await self._headers(user_ref),
        )
        result = None
        if r.status_code != 404:
            r.raise_for_status()
            d = r.json()
            result = CourseAccess(owner_ref=d["ownerRef"], role=d.get("role"))
        self._cache[key] = (time.monotonic() + CACHE_SECONDS, result)
        return result

    async def materials(self, user_ref: str, refs: list[str]) -> list[dict[str, Any]]:
        if not refs:
            return []
        r = await self._client.get(
            f"{self._base}/internal/v1/ai/materials",
            params={"user": user_ref, "refs": ",".join(refs)},
            headers=await self._headers(user_ref),
        )
        r.raise_for_status()
        rows: list[dict[str, Any]] = r.json()
        return rows

    async def context(
        self, user_ref: str, course_ref: str, scopes: list[str], anchor: dict[str, Any]
    ) -> dict[str, Any]:
        return await self.call(
            "POST",
            "/internal/v1/context/user",
            user_ref,
            {"user": user_ref, "course": course_ref, "scopes": scopes, "anchor": anchor},
        )

    async def call(self, method: str, path: str, user_ref: str, body: dict[str, Any] | None = None) -> dict[str, Any]:
        r = await self._client.request(method, f"{self._base}{path}", json=body, headers=await self._headers(user_ref))
        if r.status_code >= 400:
            raise CoreError(r.status_code, r.json() if r.content else None)
        data: dict[str, Any] = r.json() if r.content else {}
        return data


class HttpFiles:
    def __init__(self, base: str, tokens: ServiceTokens) -> None:
        self._base = base.rstrip("/")
        self._tokens = tokens
        self._client = httpx.AsyncClient(timeout=10.0)

    async def _get(self, path: str, params: dict[str, Any] | None = None) -> httpx.Response:
        return await self._client.get(f"{self._base}{path}", params=params, headers=await self._tokens.headers("files"))

    async def file(self, file_ref: str) -> dict[str, Any] | None:
        r = await self._get(f"/internal/v1/files/{file_ref}")
        if r.status_code == 404:
            return None
        r.raise_for_status()
        d: dict[str, Any] = r.json()
        return d

    async def search(self, file_refs: list[str], query: str, k: int) -> list[dict[str, Any]]:
        if not file_refs:
            return []
        r = await self._get("/internal/v1/chunks/search", {"files": ",".join(file_refs), "q": query, "k": k})
        r.raise_for_status()
        rows: list[dict[str, Any]] = r.json()
        return rows

    async def chunks(self, file_ref: str, pages: list[int] | None = None) -> list[dict[str, Any]]:
        params = {"pages": ",".join(str(p) for p in pages)} if pages else None
        r = await self._get(f"/internal/v1/files/{file_ref}/chunks", params)
        if r.status_code == 404:
            return []
        r.raise_for_status()
        rows: list[dict[str, Any]] = r.json()
        return rows


@dataclass
class FakeCore:
    """Test double. `access[(user, course)]`, `material_rows[ref]`, `context_reply`, `calls`."""

    access: dict[tuple[str, str], CourseAccess] = field(default_factory=dict)
    material_rows: dict[str, dict[str, Any]] = field(default_factory=dict)
    context_reply: dict[str, Any] = field(default_factory=dict)
    replies: dict[tuple[str, str], Any] = field(default_factory=dict)
    calls: list[tuple[str, str, str, dict[str, Any] | None]] = field(default_factory=list)

    async def course_access(self, user_ref: str, course_ref: str) -> CourseAccess | None:
        return self.access.get((user_ref, course_ref))

    async def materials(self, user_ref: str, refs: list[str]) -> list[dict[str, Any]]:
        return [self.material_rows[r] for r in refs if r in self.material_rows]

    async def context(
        self, user_ref: str, course_ref: str, scopes: list[str], anchor: dict[str, Any]
    ) -> dict[str, Any]:
        self.calls.append(("POST", "/internal/v1/context/user", user_ref, {"scopes": scopes, "anchor": anchor}))
        return self.context_reply

    async def call(self, method: str, path: str, user_ref: str, body: dict[str, Any] | None = None) -> dict[str, Any]:
        self.calls.append((method, path, user_ref, body))
        reply = self.replies.get((method, path))
        if callable(reply):
            result: dict[str, Any] = reply(body)
            return result
        if isinstance(reply, CoreError):
            raise reply
        return reply if isinstance(reply, dict) else {}


@dataclass
class FakeFiles:
    files: dict[str, dict[str, Any]] = field(default_factory=dict)
    chunk_rows: dict[str, list[dict[str, Any]]] = field(default_factory=dict)

    async def file(self, file_ref: str) -> dict[str, Any] | None:
        return self.files.get(file_ref)

    async def search(self, file_refs: list[str], query: str, k: int) -> list[dict[str, Any]]:
        words = {w for w in query.lower().split() if len(w) > 2}
        scored = [
            (sum(w in c["text"].lower() for w in words), c) for f in file_refs for c in self.chunk_rows.get(f, [])
        ]
        return [c for score, c in sorted(scored, key=lambda x: -x[0]) if score > 0][:k]

    async def chunks(self, file_ref: str, pages: list[int] | None = None) -> list[dict[str, Any]]:
        rows = self.chunk_rows.get(file_ref, [])
        return [c for c in rows if pages is None or c.get("pageNo") in pages]
