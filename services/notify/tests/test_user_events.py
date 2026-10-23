"""notify keeps Recipient in step with core's user events (spec 8)."""

from pathlib import Path

from sqlalchemy import select

from app.domain import unsubscribe
from app.repo.recipients import recipient
from app.service.users import UserEventsHandler
from crux_common.messaging.bus import Delivery, Envelope
from crux_common.messaging.consumer import Outcome, process
from crux_common.messaging.contracts import Contracts
from crux_common.testing import migrated_db

SERVICE_DIR = Path(__file__).resolve().parents[1]
CONTRACTS = SERVICE_DIR.parents[1] / "contracts"
USER = "01TEST00000000000000000001"


def payload(verified: bool, email: str = "a@b.nl") -> dict:
    return {
        "userRef": USER,
        "email": email,
        "emailVerified": verified,
        "timeZone": "Europe/Amsterdam",
        "locale": "en",
        "tierCode": "free",
        "storageQuotaBytes": 1,
    }


def test_unsubscribe_token_is_stable_per_user_and_secret() -> None:
    a = unsubscribe.token_for("s", USER)
    assert a == unsubscribe.token_for("s", USER)
    assert a != unsubscribe.token_for("t", USER)
    assert len(unsubscribe.token_hash(a)) == 64


async def test_recipient_tracks_email_verification() -> None:
    async with migrated_db(SERVICE_DIR, "notify") as sessions:
        h, c = UserEventsHandler("secret"), Contracts(CONTRACTS)
        e1 = Envelope(
            "01TEST0000000000000000000A", "user.created", 1, "2026-10-23T12:00:00.000Z", "core", USER, payload(False)
        )
        e2 = Envelope(
            "01TEST0000000000000000000B",
            "user.updated",
            1,
            "2026-10-23T12:01:00.000Z",
            "core",
            USER,
            payload(True, "new@b.nl"),
        )
        assert await process(sessions, c, h, Delivery("1-0", e1.topic, e1)) == Outcome.APPLIED
        assert await process(sessions, c, h, Delivery("2-0", e2.topic, e2)) == Outcome.APPLIED
        async with sessions() as s:
            row = (await s.execute(select(recipient))).one()
        assert (row.Email, row.EmailVerified) == ("new@b.nl", True)
        assert row.UnsubscribeTokenHash == unsubscribe.token_hash(unsubscribe.token_for("secret", USER))
