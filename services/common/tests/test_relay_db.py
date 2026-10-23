"""Python outbox: emit in a transaction, relay publishes in order exactly once (spec 3.3)."""

from pathlib import Path

from crux_common.db import transaction
from crux_common.messaging.bus import MemoryBus
from crux_common.messaging.contracts import Contracts
from crux_common.messaging.outbox import emit
from crux_common.messaging.relay import publish_batch
from crux_common.testing import migrated_db

ROOT = Path(__file__).resolve().parents[3]
FILES_DIR = ROOT / "services" / "files"


async def test_emit_and_relay() -> None:
    async with migrated_db(FILES_DIR, "common") as sessions:
        c = Contracts(ROOT / "contracts")
        async with transaction(sessions) as s:
            for i in range(3):
                await emit(s, c, "user.deleted", None, {"userRef": f"01TEST0000000000000000000{i}"})
        bus = MemoryBus()
        assert await publish_batch(sessions, bus, "files") == 3
        assert await publish_batch(sessions, bus, "files") == 0
        assert [e.payload["userRef"][-1] for e in bus.published] == ["0", "1", "2"]
        assert all(e.producer == "files" and e.occurredAt.endswith("Z") for e in bus.published)
