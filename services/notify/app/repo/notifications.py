"""Notification + NotificationDelivery (schema-v5-notify.sql)."""

from datetime import datetime
from typing import Any, cast

from sqlalchemy import JSON, BigInteger, Column, DateTime, Integer, MetaData, String, Table, select, update
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.engine import CursorResult
from sqlalchemy.ext.asyncio import AsyncSession

metadata = MetaData()

notification = Table(
    "Notification",
    metadata,
    Column("Id", BigInteger, primary_key=True, autoincrement=True),
    Column("PublicId", String(26), nullable=False),
    Column("UserRef", String(26), nullable=False),
    Column("Kind", String(48), nullable=False),
    Column("DedupeKey", String(128), nullable=False),
    Column("DueAt", DateTime, nullable=False),
    Column("Payload", JSON),
    Column("CreatedAt", DateTime),
    Column("ReadAt", DateTime),
    Column("CancelledAt", DateTime),
)

delivery = Table(
    "NotificationDelivery",
    metadata,
    Column("NotificationId", BigInteger, primary_key=True),
    Column("Channel", String(8), primary_key=True),
    Column("Status", String(8), nullable=False),
    Column("Attempts", Integer, nullable=False),
    Column("ProviderMessageId", String(128)),
    Column("LastError", String(500)),
    Column("NextAttemptAt", DateTime),
    Column("SentAt", DateTime),
)


async def insert_notification(
    session: AsyncSession,
    *,
    public_id: str,
    user_ref: str,
    kind: str,
    dedupe_key: str,
    due_at: datetime,
    payload: dict[str, Any],
) -> int | None:
    """Returns the new id, or None when (UserRef, DedupeKey) already exists (no doubles; spec 8)."""
    result = await session.execute(
        insert(notification)
        .prefix_with("IGNORE")
        .values(PublicId=public_id, UserRef=user_ref, Kind=kind, DedupeKey=dedupe_key, DueAt=due_at, Payload=payload)
    )
    cursor = cast(CursorResult[Any], result)
    if cursor.rowcount == 0:
        return None
    pk = cursor.inserted_primary_key
    assert pk is not None
    return int(pk[0])


async def queue_email(session: AsyncSession, notification_id: int, due_at: datetime) -> None:
    await session.execute(
        insert(delivery).values(
            NotificationId=notification_id, Channel="Email", Status="Pending", Attempts=0, NextAttemptAt=due_at
        )
    )


async def lock_due_emails(session: AsyncSession, now: datetime, limit: int) -> list[Any]:
    """Due email deliveries joined with their notification, locked (FOR UPDATE SKIP LOCKED)."""
    stmt = (
        select(
            delivery.c.NotificationId,
            delivery.c.Attempts,
            notification.c.Kind,
            notification.c.UserRef,
            notification.c.Payload,
            notification.c.CancelledAt,
        )
        .join(notification, notification.c.Id == delivery.c.NotificationId)
        .where(delivery.c.Channel == "Email", delivery.c.Status == "Pending", delivery.c.NextAttemptAt <= now)
        .order_by(delivery.c.NextAttemptAt)
        .limit(limit)
        .with_for_update(skip_locked=True, of=delivery)
    )
    return list((await session.execute(stmt)).all())


async def mark(session: AsyncSession, notification_id: int, **values: Any) -> None:
    await session.execute(
        update(delivery)
        .where(delivery.c.NotificationId == notification_id, delivery.c.Channel == "Email")
        .values(**values)
    )
