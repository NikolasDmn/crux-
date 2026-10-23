"""SSE tickets (spec 3.4). Core issues them; any service redeems them for its own streams.

Shared contract (core/crates/service/src/sse.rs): Redis key `crux:sse:<sha256(ticket)>`, value
`{"userRef", "role"}`, single use (GETDEL), 60 s.
"""

import hashlib
import json
from typing import Any

from redis.asyncio import Redis

from crux_common.auth import Principal
from crux_common.errors import Unauthenticated

PING_SECONDS = 20


def ticket_key(ticket: str) -> str:
    return "crux:sse:" + hashlib.sha256(ticket.encode()).hexdigest()


async def redeem(redis: Redis, ticket: str) -> Principal:
    raw = await redis.getdel(ticket_key(ticket))
    if raw is None:
        raise Unauthenticated()
    data = json.loads(raw)
    return Principal(user_ref=data["userRef"], role=data.get("role", "User"), tier="")


def event(name: str, data: Any) -> str:
    """One SSE frame. Streams send `ping` every 20 s and end with `done` or `error`."""
    return f"event: {name}\ndata: {json.dumps(data, separators=(',', ':'))}\n\n"
