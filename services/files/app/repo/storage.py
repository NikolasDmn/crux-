"""UserStorage: the per-user quota (schema-v5-files.sql). Reserve on upload start, settle on
complete, release on reject/delete, all as single conditional UPDATEs so they can't race."""

from typing import Any, cast

from sqlalchemy import BigInteger, Boolean, Column, DateTime, Integer, MetaData, String, Table, select, update
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.engine import CursorResult
from sqlalchemy.ext.asyncio import AsyncSession

metadata = MetaData()

user_storage = Table(
    "UserStorage",
    metadata,
    Column("UserRef", String(26), primary_key=True),
    Column("QuotaBytes", BigInteger, nullable=False),
    Column("TierCode", String(32)),
    Column("UsedBytes", BigInteger, nullable=False),
    Column("ReservedBytes", BigInteger, nullable=False),
    Column("AutoPurgeOriginals", Boolean, nullable=False),
    Column("KeepThumbnails", Boolean, nullable=False),
    Column("UpdatedAt", DateTime),
    Column("RowVersion", Integer, nullable=False),
)

_NEW = {"UsedBytes": 0, "ReservedBytes": 0, "AutoPurgeOriginals": False, "KeepThumbnails": False, "RowVersion": 0}


async def upsert_quota(session: AsyncSession, user_ref: str, quota_bytes: int, tier_code: str | None = None) -> None:
    """From core's user events (spec 4.3): creates the row on first sight, then only the quota changes."""
    stmt = insert(user_storage).values(UserRef=user_ref, QuotaBytes=quota_bytes, TierCode=tier_code, **_NEW)
    await session.execute(
        stmt.on_duplicate_key_update(QuotaBytes=stmt.inserted.QuotaBytes, TierCode=stmt.inserted.TierCode)
    )


async def ensure(session: AsyncSession, user_ref: str, default_quota: int) -> None:
    """A user whose user.created hasn't arrived yet gets the default quota until it does."""
    await session.execute(
        insert(user_storage).prefix_with("IGNORE").values(UserRef=user_ref, QuotaBytes=default_quota, **_NEW)
    )


async def get(session: AsyncSession, user_ref: str) -> Any | None:
    return (await session.execute(select(user_storage).where(user_storage.c.UserRef == user_ref))).first()


def _rows(result: Any) -> int:
    return int(cast(CursorResult[Any], result).rowcount)


async def reserve(session: AsyncSession, user_ref: str, size: int) -> bool:
    c = user_storage.c
    result = await session.execute(
        update(user_storage)
        .where(c.UserRef == user_ref, c.UsedBytes + c.ReservedBytes + size <= c.QuotaBytes)
        .values(ReservedBytes=c.ReservedBytes + size)
    )
    return _rows(result) == 1


async def settle(session: AsyncSession, user_ref: str, size: int) -> None:
    """Reserved → used (upload completed)."""
    c = user_storage.c
    await session.execute(
        update(user_storage)
        .where(c.UserRef == user_ref)
        .values(ReservedBytes=c.ReservedBytes - size, UsedBytes=c.UsedBytes + size)
    )


async def release_reserved(session: AsyncSession, user_ref: str, size: int) -> None:
    c = user_storage.c
    await session.execute(
        update(user_storage).where(c.UserRef == user_ref).values(ReservedBytes=c.ReservedBytes - size)
    )


async def release_used(session: AsyncSession, user_ref: str, size: int) -> None:
    c = user_storage.c
    await session.execute(update(user_storage).where(c.UserRef == user_ref).values(UsedBytes=c.UsedBytes - size))


async def set_settings(session: AsyncSession, user_ref: str, **values: bool) -> None:
    c = user_storage.c
    await session.execute(
        update(user_storage).where(c.UserRef == user_ref).values(RowVersion=c.RowVersion + 1, **values)
    )
