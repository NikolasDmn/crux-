"""Recipient: what notify needs about a user, synced from core's user events."""

from sqlalchemy import Boolean, Column, DateTime, MetaData, String, Table
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
