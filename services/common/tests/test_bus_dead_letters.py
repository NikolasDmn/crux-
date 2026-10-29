"""Audit L4: malformed and repeatedly failing events go to a dead-letter stream (real Redis:
set CRUX_TEST_REDIS_URL, e.g. redis://:crux-redis-dev@127.0.0.1:6379/15)."""

import os
import uuid

import pytest
from redis.asyncio import Redis

from crux_common.messaging.bus import MAX_DELIVERIES, Envelope, RedisBus, dead_key, stream_key

URL = os.environ.get("CRUX_TEST_REDIS_URL")
pytestmark = pytest.mark.skipif(not URL, reason="CRUX_TEST_REDIS_URL not set")


async def test_dead_letters() -> None:
    r = Redis.from_url(URL or "")
    topic = f"test.dead.{uuid.uuid4().hex[:8]}"
    bus = RedisBus(r)
    try:
        await bus.ensure_group("g", [topic])
        env = Envelope("01EVT", topic, 1, "2026-10-29T00:00:00.000Z", "test", None, {"x": 1})
        await bus.publish(env)
        await r.xadd(stream_key(topic), {"envelope": "not json"})

        got = await bus.read("g", "c", [topic], 10, 10)
        assert [d.envelope.eventId for d in got] == ["01EVT"]
        dead = await r.xrange(dead_key(topic))
        assert len(dead) == 1 and dead[0][1][b"reason"] == b"malformed", "malformed: dead-lettered and acked at once"

        # The good one keeps failing (never acked): retried until MAX_DELIVERIES, then buried.
        for _ in range(MAX_DELIVERIES - 1):
            assert len(await bus.read_pending("g", "c", [topic], 10)) == 1
        assert await bus.read_pending("g", "c", [topic], 10) == []
        assert len(await r.xrange(dead_key(topic))) == 2
        assert (await r.xpending(stream_key(topic), "g"))["pending"] == 0
    finally:
        await r.delete(stream_key(topic), dead_key(topic))
        await r.aclose()
