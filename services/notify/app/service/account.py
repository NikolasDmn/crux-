"""Account export and deletion for notify (spec 11, 9.4)."""

from typing import Any

from sqlalchemy import delete, select
from sqlalchemy.ext.asyncio import AsyncSession

from app.repo.notifications import notification
from app.repo.preferences import preference
from app.repo.recipients import recipient
from crux_common.messaging.bus import Delivery
from crux_common.messaging.contracts import Contracts
from crux_common.messaging.outbox import emit
from crux_common.timeutil import iso


async def export(session: AsyncSession, user_ref: str) -> dict[str, Any]:
    r = (await session.execute(select(recipient).where(recipient.c.UserRef == user_ref))).first()
    prefs = await session.execute(select(preference).where(preference.c.UserRef == user_ref))
    notes = await session.execute(
        select(notification).where(notification.c.UserRef == user_ref).order_by(notification.c.Id)
    )
    return {
        "recipient": {
            "email": r.Email,
            "emailVerified": bool(r.EmailVerified),
            "timeZone": r.TimeZone,
            "locale": r.Locale,
        }
        if r
        else None,
        "preferences": [{"kind": p.Kind, "channel": p.Channel, "enabled": bool(p.Enabled)} for p in prefs],
        "notifications": [
            {
                "id": n.PublicId,
                "kind": n.Kind,
                "dueAt": iso(n.DueAt),
                "readAt": iso(n.ReadAt) if n.ReadAt else None,
                "payload": n.Payload,
            }
            for n in notes
        ],
    }


class UserDeletedHandler:
    """`user.deleted`: everything of the user goes (the suppression list keeps the address, so
    nothing is ever sent to it again), then `user.purged`."""

    topics = ("user.deleted",)

    def __init__(self, contracts: Contracts) -> None:
        self._contracts = contracts

    async def handle(self, session: AsyncSession, d: Delivery) -> None:
        user_ref = d.envelope.payload["userRef"]
        await session.execute(delete(notification).where(notification.c.UserRef == user_ref))
        await session.execute(delete(preference).where(preference.c.UserRef == user_ref))
        await session.execute(delete(recipient).where(recipient.c.UserRef == user_ref))
        await emit(session, self._contracts, "user.purged", user_ref, {"userRef": user_ref, "service": "notify"})
