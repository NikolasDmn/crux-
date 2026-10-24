"""Recipient: what notify needs about a user, synced from core's user events."""

from typing import Any

from sqlalchemy import Boolean, Column, DateTime, MetaData, String, Table, select
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.ext.asyncio import AsyncSession

metadata = MetaData()

recipient = Table(
    "Recipient",
    metadata,
    Column("UserRef", String(26), primary_key=True),
    Column("Email", String(255), nullable=False),
    Column("EmailVerified", Boolean, nullable=False),
    Column("TimeZone", String(64), nullable=False),
    Column("Locale", String(16), nullable=False),
    Column("UnsubscribeTokenHash", String(64), nullable=False),
    Column("UpdatedAt", DateTime),
)


async def upsert(
    session: AsyncSession,
    *,
    user_ref: str,
    email: str,
    email_verified: bool,
    time_zone: str,
    locale: str,
    unsubscribe_token_hash: str,
) -> None:
    stmt = insert(recipient).values(
        UserRef=user_ref,
        Email=email,
        EmailVerified=email_verified,
        TimeZone=time_zone,
        Locale=locale,
        UnsubscribeTokenHash=unsubscribe_token_hash,
    )
    await session.execute(
        stmt.on_duplicate_key_update(
            Email=stmt.inserted.Email,
            EmailVerified=stmt.inserted.EmailVerified,
            TimeZone=stmt.inserted.TimeZone,
            Locale=stmt.inserted.Locale,
        )
    )


async def get(session: AsyncSession, user_ref: str) -> Any | None:
    return (await session.execute(select(recipient).where(recipient.c.UserRef == user_ref))).first()


async def by_unsubscribe_hash(session: AsyncSession, token_hash: str) -> Any | None:
    return (await session.execute(select(recipient).where(recipient.c.UnsubscribeTokenHash == token_hash))).first()


# ---------- EmailSuppression (bounces, complaints, unsubscribe-all) ----------

suppression = Table(
    "EmailSuppression",
    metadata,
    Column("Email", String(255), primary_key=True),
    Column("Reason", String(16), nullable=False),
    Column("CreatedAt", DateTime),
)


async def is_suppressed(session: AsyncSession, email: str) -> bool:
    row = (await session.execute(select(suppression.c.Email).where(suppression.c.Email == email.lower()))).first()
    return row is not None


async def suppress(session: AsyncSession, email: str, reason: str) -> None:
    await session.execute(insert(suppression).prefix_with("IGNORE").values(Email=email.lower(), Reason=reason))
