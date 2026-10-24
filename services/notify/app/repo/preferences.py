"""NotificationPreference: one row per (user, kind, channel) the user has set; no row = off."""

from sqlalchemy import Boolean, Column, DateTime, MetaData, String, Table, select
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.ext.asyncio import AsyncSession

metadata = MetaData()

preference = Table(
    "NotificationPreference",
    metadata,
    Column("UserRef", String(26), primary_key=True),
    Column("Kind", String(48), primary_key=True),
    Column("Channel", String(8), primary_key=True),
    Column("Enabled", Boolean, nullable=False),
    Column("UpdatedAt", DateTime),
)


async def for_user(session: AsyncSession, user_ref: str) -> dict[tuple[str, str], bool]:
    rows = await session.execute(select(preference).where(preference.c.UserRef == user_ref))
    return {(r.Kind, r.Channel): bool(r.Enabled) for r in rows}


async def set_pref(session: AsyncSession, user_ref: str, kind: str, channel: str, enabled: bool) -> None:
    stmt = insert(preference).values(UserRef=user_ref, Kind=kind, Channel=channel, Enabled=enabled)
    await session.execute(stmt.on_duplicate_key_update(Enabled=stmt.inserted.Enabled))
