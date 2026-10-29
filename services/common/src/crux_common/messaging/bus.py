"""Redis Streams transport: one stream per topic, a consumer group per consuming service."""

import json
import logging
from dataclasses import asdict, dataclass, field
from typing import Any, Protocol, cast

from redis.asyncio import Redis
from redis.exceptions import ResponseError

log = logging.getLogger("crux.bus")

MAX_LEN = 100_000
#: Audit L4: after this many deliveries an entry moves to the dead-letter stream, so a handful of
#: events that always fail can't keep the retry sweep from reaching the rest.
MAX_DELIVERIES = 10


def stream_key(topic: str) -> str:
    return f"crux:events:{topic}"


def dead_key(topic: str) -> str:
    return f"crux:events:dead:{topic}"


@dataclass(frozen=True)
class Envelope:
    eventId: str
    topic: str
    schemaVersion: int
    occurredAt: str
    producer: str
    aggregateRef: str | None
    payload: dict[str, Any]

    def to_json(self) -> str:
        return json.dumps(asdict(self), separators=(",", ":"))

    @classmethod
    def from_json(cls, raw: str | bytes) -> "Envelope":
        d = json.loads(raw)
        return cls(
            eventId=d["eventId"],
            topic=d["topic"],
            schemaVersion=int(d["schemaVersion"]),
            occurredAt=d["occurredAt"],
            producer=d["producer"],
            aggregateRef=d.get("aggregateRef"),
            payload=d["payload"],
        )


@dataclass(frozen=True)
class Delivery:
    stream_id: str
    topic: str
    envelope: Envelope


class EventBus(Protocol):
    async def publish(self, envelope: Envelope) -> None: ...
    async def read(self, group: str, consumer: str, topics: list[str], count: int, block_ms: int) -> list[Delivery]: ...
    async def read_pending(self, group: str, consumer: str, topics: list[str], count: int) -> list[Delivery]: ...
    async def ack(self, group: str, delivery: Delivery) -> None: ...


class RedisBus:
    def __init__(self, redis: Redis) -> None:
        self._r = redis

    async def ensure_group(self, group: str, topics: list[str]) -> None:
        for topic in topics:
            try:
                await self._r.xgroup_create(stream_key(topic), group, id="0", mkstream=True)
            except ResponseError as e:
                if "BUSYGROUP" not in str(e):
                    raise

    async def publish(self, envelope: Envelope) -> None:
        await self._r.xadd(
            stream_key(envelope.topic), {"envelope": envelope.to_json()}, maxlen=MAX_LEN, approximate=True
        )

    async def _read(
        self, group: str, consumer: str, topics: list[str], count: int, block_ms: int | None, start: str
    ) -> list[Delivery]:
        streams = {stream_key(t): start for t in topics}
        raw_reply = await self._r.xreadgroup(group, consumer, streams, count=count, block=block_ms)  # type: ignore[arg-type]
        reply = cast(list[tuple[Any, list[tuple[Any, dict[Any, Any]]]]], raw_reply)
        out: list[Delivery] = []
        for key, entries in reply or []:
            key_s = key.decode() if isinstance(key, bytes) else key
            topic = key_s.removeprefix("crux:events:")
            for entry_id, fields in entries:
                eid = entry_id.decode() if isinstance(entry_id, bytes) else entry_id
                raw = fields.get(b"envelope") or fields.get("envelope")
                try:
                    if raw is None:
                        raise KeyError("envelope")
                    out.append(Delivery(stream_id=eid, topic=topic, envelope=Envelope.from_json(raw)))
                except (TypeError, ValueError, KeyError):
                    # Never parseable: dead-letter and ack it now, or it would be retried forever.
                    log.warning("malformed event dead-lettered", extra={"stream": key_s, "id": eid})
                    await self._bury(group, topic, eid, fields, "malformed")
        return out

    async def _bury(self, group: str, topic: str, entry_id: str, fields: dict[Any, Any], reason: str) -> None:
        dead = {**fields, "reason": reason, "group": group, "id": entry_id}
        await self._r.xadd(dead_key(topic), dead, maxlen=MAX_LEN, approximate=True)
        await self._r.xack(stream_key(topic), group, entry_id)

    async def _bury_exhausted(self, group: str, topics: list[str]) -> None:
        for topic in topics:
            pending = await self._r.xpending_range(stream_key(topic), group, min="-", max="+", count=100)
            for p in pending:
                if int(p["times_delivered"]) < MAX_DELIVERIES:
                    continue
                eid = str(p["message_id"].decode() if isinstance(p["message_id"], bytes) else p["message_id"])
                rows = await self._r.xrange(stream_key(topic), min=eid, max=eid)
                fields: dict[Any, Any] = dict(rows[0][1] or {}) if rows else {}
                log.error(
                    "event dead-lettered after repeated failures",
                    extra={"topic": topic, "id": eid, "group": group, "alert": True},
                )
                await self._bury(group, topic, eid, fields, f"failed {MAX_DELIVERIES} times")

    async def read(self, group: str, consumer: str, topics: list[str], count: int, block_ms: int) -> list[Delivery]:
        return await self._read(group, consumer, topics, count, block_ms, ">")

    async def read_pending(self, group: str, consumer: str, topics: list[str], count: int) -> list[Delivery]:
        await self._bury_exhausted(group, topics)
        return await self._read(group, consumer, topics, count, None, "0")

    async def ack(self, group: str, delivery: Delivery) -> None:
        await self._r.xack(stream_key(delivery.topic), group, delivery.stream_id)


@dataclass
class MemoryBus:
    """Test double: records what was published and hands it back on read."""

    published: list[Envelope] = field(default_factory=list)
    acked: dict[str, list[str]] = field(default_factory=dict)

    async def publish(self, envelope: Envelope) -> None:
        self.published.append(envelope)

    async def read(self, group: str, consumer: str, topics: list[str], count: int, block_ms: int) -> list[Delivery]:
        hits = [(i, e) for i, e in enumerate(self.published) if e.topic in topics][:count]
        return [Delivery(stream_id=f"{i}-0", topic=e.topic, envelope=e) for i, e in hits]

    async def read_pending(self, group: str, consumer: str, topics: list[str], count: int) -> list[Delivery]:
        return []

    async def ack(self, group: str, delivery: Delivery) -> None:
        self.acked.setdefault(group, []).append(delivery.stream_id)
