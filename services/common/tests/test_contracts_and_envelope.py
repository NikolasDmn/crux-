from pathlib import Path

import pytest

from crux_common.messaging.bus import Envelope, MemoryBus
from crux_common.messaging.contracts import ContractError, Contracts
from crux_common.timeutil import iso, parse_iso

CONTRACTS = Path(__file__).resolve().parents[3] / "contracts"

USER = {
    "userRef": "01TEST00000000000000000001",
    "email": "a@b.nl",
    "emailVerified": False,
    "timeZone": "Europe/Amsterdam",
    "locale": "en",
    "tierCode": "free",
    "storageQuotaBytes": 1073741824,
}


def test_valid_payload_passes_and_invalid_names_the_problem() -> None:
    c = Contracts(CONTRACTS)
    c.validate("user.created", 1, USER)
    with pytest.raises(ContractError, match="storageQuotaBytes"):
        c.validate("user.created", 1, {**USER, "storageQuotaBytes": -5})
    with pytest.raises(ContractError, match="no contract"):
        c.validate("user.created", 9, USER)


def test_envelope_round_trip_matches_core_field_names() -> None:
    e = Envelope(
        eventId="01TEST00000000000000000009",
        topic="user.created",
        schemaVersion=1,
        occurredAt="2026-10-23T12:00:00.000Z",
        producer="core",
        aggregateRef=USER["userRef"],
        payload=USER,
    )
    assert Envelope.from_json(e.to_json()) == e
    assert '"schemaVersion":1' in e.to_json()


async def test_memory_bus_reads_by_topic() -> None:
    bus = MemoryBus()
    for topic in ("a", "b", "a"):
        await bus.publish(Envelope("01TEST00000000000000000001", topic, 1, "x", "core", None, {}))
    assert [d.stream_id for d in await bus.read("g", "c", ["a"], 10, 0)] == ["0-0", "2-0"]


def test_iso_has_millis_and_z() -> None:
    dt = parse_iso("2026-10-23T09:05:00.123Z")
    assert iso(dt) == "2026-10-23T09:05:00.123Z"
    assert dt.tzinfo is None, "DB columns are naive UTC"
