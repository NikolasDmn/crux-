"""Mirrors core's user and tier events into UserStorage (spec 3.3: user.*, user.tier_changed)."""

from sqlalchemy.ext.asyncio import AsyncSession

from app.repo import storage
from crux_common.messaging.bus import Delivery


class UserEventsHandler:
    topics = ("user.created", "user.updated", "user.tier_changed")

    async def handle(self, session: AsyncSession, delivery: Delivery) -> None:
        p = delivery.envelope.payload
        await storage.upsert_quota(session, p["userRef"], int(p["storageQuotaBytes"]), p.get("tierCode"))
