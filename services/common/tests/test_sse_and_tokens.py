import hashlib
import json
from typing import Any

import httpx
import pytest

from crux_common import sse
from crux_common.errors import Unauthenticated
from crux_common.service_tokens import ServiceTokens


class FakeRedis:
    def __init__(self) -> None:
        self.data: dict[str, str] = {}

    async def getdel(self, key: str) -> str | None:
        return self.data.pop(key, None)


async def test_ticket_key_matches_core_and_is_single_use() -> None:
    r = FakeRedis()
    assert sse.ticket_key("abc") == "crux:sse:" + hashlib.sha256(b"abc").hexdigest()
    r.data[sse.ticket_key("abc")] = json.dumps({"userRef": "01TEST00000000000000000001", "role": "User"})
    p = await sse.redeem(r, "abc")  # type: ignore[arg-type]
    assert p.user_ref == "01TEST00000000000000000001"
    with pytest.raises(Unauthenticated):
        await sse.redeem(r, "abc")  # type: ignore[arg-type]


def test_sse_frame_format() -> None:
    assert sse.event("ping", {}) == "event: ping\ndata: {}\n\n"


async def test_service_tokens_are_cached_per_audience() -> None:
    calls: list[dict[str, Any]] = []

    def handler(request: httpx.Request) -> httpx.Response:
        body = json.loads(request.content)
        calls.append(body)
        assert request.headers["authorization"] == "Bearer s3cret"
        return httpx.Response(200, json={"token": f"tok-{body['audience']}", "expiresIn": 300})

    client = httpx.AsyncClient(transport=httpx.MockTransport(handler))
    tokens = ServiceTokens("http://core:8080", "files", "s3cret", client)
    assert await tokens.token("core") == "tok-core"
    assert await tokens.token("core") == "tok-core"
    h = await tokens.headers("ai", on_behalf_of="01TEST00000000000000000001")
    assert h == {"authorization": "Bearer tok-ai", "x-on-behalf-of": "01TEST00000000000000000001"}
    assert calls == [{"service": "files", "audience": "core"}, {"service": "files", "audience": "ai"}]
    await client.aclose()
