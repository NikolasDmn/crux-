"""Keeps Recipient in step with core (user.created / user.updated). Email only ever goes to a
verified address, so EmailVerified must follow core closely."""

from sqlalchemy.ext.asyncio import AsyncSession

from app.domain import unsubscribe
from app.repo import recipients
from crux_common.messaging.bus import Delivery


class UserEventsHandler:
    topics = ("user.created", "user.updated")

    def __init__(self, unsubscribe_secret: str) -> None:
        self._secret = unsubscribe_secret

    async def handle(self, session: AsyncSession, delivery: Delivery) -> None:
        p = delivery.envelope.payload
        token = unsubscribe.token_for(self._secret, p["userRef"])
        await recipients.upsert(
            session,
            user_ref=p["userRef"],
            email=p["email"],
            email_verified=bool(p["emailVerified"]),
            time_zone=p["timeZone"],
            locale=p["locale"],
            unsubscribe_token_hash=unsubscribe.token_hash(token),
        )
