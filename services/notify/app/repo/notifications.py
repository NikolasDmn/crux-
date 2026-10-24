"""Notification + NotificationDelivery (schema-v5-notify.sql)."""

from datetime import datetime
from typing import Any, cast

from sqlalchemy import JSON, BigInteger, Column, DateTime, Integer, MetaData, String, Table, func, select, update
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


async def by_dedupe(session: AsyncSession, user_ref: str, dedupe_key: str) -> Any | None:
    stmt = select(notification).where(notification.c.UserRef == user_ref, notification.c.DedupeKey == dedupe_key)
    return (await session.execute(stmt)).first()


async def queue(session: AsyncSession, notification_id: int, channel: str, due_at: datetime) -> None:
    await session.execute(
        insert(delivery)
        .prefix_with("IGNORE")
        .values(NotificationId=notification_id, Channel=channel, Status="Pending", Attempts=0, NextAttemptAt=due_at)
    )


async def pending_channels(session: AsyncSession, notification_id: int) -> list[str]:
    rows = await session.execute(
        select(delivery.c.Channel).where(delivery.c.NotificationId == notification_id, delivery.c.Status == "Pending")
    )
    return [r.Channel for r in rows]


async def reschedule(session: AsyncSession, notification_id: int, due_at: datetime, payload: dict[str, Any]) -> None:
    """A re-request with the same key before delivery moves it (spec 8: no doubles)."""
    await session.execute(
        update(notification)
        .where(notification.c.Id == notification_id)
        .values(DueAt=due_at, Payload=payload, CancelledAt=None)
    )
    await session.execute(
        update(delivery)
        .where(delivery.c.NotificationId == notification_id, delivery.c.Status == "Pending")
        .values(NextAttemptAt=due_at)
    )


async def cancel(session: AsyncSession, notification_id: int, now: datetime) -> None:
    await session.execute(update(notification).where(notification.c.Id == notification_id).values(CancelledAt=now))
    await session.execute(
        update(delivery)
        .where(delivery.c.NotificationId == notification_id, delivery.c.Status == "Pending")
        .values(Status="Skipped", NextAttemptAt=None)
    )


async def lock_due_in_app(session: AsyncSession, now: datetime, limit: int) -> list[Any]:
    stmt = (
        select(
            delivery.c.NotificationId,
            notification.c.PublicId,
            notification.c.UserRef,
            notification.c.Kind,
            notification.c.Payload,
            notification.c.DueAt,
        )
        .join(notification, notification.c.Id == delivery.c.NotificationId)
        .where(delivery.c.Channel == "InApp", delivery.c.Status == "Pending", delivery.c.NextAttemptAt <= now)
        .order_by(delivery.c.NextAttemptAt)
        .limit(limit)
        .with_for_update(skip_locked=True, of=delivery)
    )
    return list((await session.execute(stmt)).all())


async def mark_channel(session: AsyncSession, notification_id: int, channel: str, **values: Any) -> None:
    await session.execute(
        update(delivery)
        .where(delivery.c.NotificationId == notification_id, delivery.c.Channel == channel)
        .values(**values)
    )


def _inbox_query(user_ref: str) -> Any:
    """The inbox: notifications delivered in-app (their InApp delivery is Sent), not cancelled."""
    return (
        select(notification)
        .join(delivery, (delivery.c.NotificationId == notification.c.Id) & (delivery.c.Channel == "InApp"))
        .where(notification.c.UserRef == user_ref, delivery.c.Status == "Sent", notification.c.CancelledAt.is_(None))
    )


async def inbox(
    session: AsyncSession, user_ref: str, unread_only: bool, before_id: int | None, limit: int
) -> list[Any]:
    stmt = _inbox_query(user_ref)
    if unread_only:
        stmt = stmt.where(notification.c.ReadAt.is_(None))
    if before_id is not None:
        stmt = stmt.where(notification.c.Id < before_id)
    return list((await session.execute(stmt.order_by(notification.c.Id.desc()).limit(limit))).all())


async def unread_count(session: AsyncSession, user_ref: str) -> int:
    stmt = select(func.count()).select_from(_inbox_query(user_ref).where(notification.c.ReadAt.is_(None)).subquery())
    return int((await session.execute(stmt)).scalar_one())


async def by_public(session: AsyncSession, user_ref: str, public_id: str) -> Any | None:
    stmt = select(notification).where(notification.c.UserRef == user_ref, notification.c.PublicId == public_id)
    return (await session.execute(stmt)).first()


async def mark_read(session: AsyncSession, user_ref: str, notification_id: int | None, now: datetime) -> int:
    stmt = update(notification).where(notification.c.UserRef == user_ref, notification.c.ReadAt.is_(None))
    if notification_id is not None:
        stmt = stmt.where(notification.c.Id == notification_id)
    result = cast(CursorResult[Any], await session.execute(stmt.values(ReadAt=now)))
    return int(result.rowcount)
