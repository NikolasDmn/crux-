"""files mirrors user quotas from core's events, once per event (spec 3.3, 9.5)."""

from pathlib import Path

from sqlalchemy import select

from app.repo.storage import user_storage
from app.service.users import UserEventsHandler
from crux_common.messaging.bus import Delivery, Envelope
from crux_common.messaging.consumer import Outcome, process
from crux_common.messaging.contracts import Contracts
from crux_common.testing import migrated_db

SERVICE_DIR = Path(__file__).resolve().parents[1]
CONTRACTS = SERVICE_DIR.parents[1] / "contracts"
USER = "01TEST00000000000000000001"


def delivery(event_id: str, topic: str, payload: dict) -> Delivery:
    return Delivery("1-0", topic, Envelope(event_id, topic, 1, "2026-10-23T12:00:00.000Z", "core", USER, payload))


def user_payload(quota: int) -> dict:
    return {
        "userRef": USER,
        "email": "a@b.nl",
        "emailVerified": False,
        "timeZone": "UTC",
        "locale": "en",
        "tierCode": "free",
        "storageQuotaBytes": quota,
    }


async def test_quota_follows_user_and_tier_events_with_dedupe() -> None:
    async with migrated_db(SERVICE_DIR, "files") as sessions:
        h, c = UserEventsHandler(), Contracts(CONTRACTS)
        created = delivery("01TEST0000000000000000000A", "user.created", user_payload(1 << 30))
        assert await process(sessions, c, h, created) == Outcome.APPLIED
        assert await process(sessions, c, h, created) == Outcome.DUPLICATE

        tier = delivery(
            "01TEST0000000000000000000B",
            "user.tier_changed",
            {"userRef": USER, "tierCode": "plus", "storageQuotaBytes": 5 << 30},
        )
        assert await process(sessions, c, h, tier) == Outcome.APPLIED

        async with sessions() as s:
            row = (await s.execute(select(user_storage).where(user_storage.c.UserRef == USER))).one()
        assert (row.QuotaBytes, row.UsedBytes) == (5 << 30, 0)

        bad = delivery("01TEST0000000000000000000C", "user.created", {"userRef": USER})
        assert await process(sessions, c, h, bad) == Outcome.REJECTED
