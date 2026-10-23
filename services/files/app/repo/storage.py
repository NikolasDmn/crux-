"""UserStorage: the per-user quota mirror (schema-v5-files.sql)."""

from sqlalchemy import BigInteger, Boolean, Column, DateTime, Integer, MetaData, String, Table
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.ext.asyncio import AsyncSession

metadata = MetaData()

user_storage = Table(
    "UserStorage",
    metadata,
    Column("UserRef", String(26), primary_key=True),
    Column("QuotaBytes", BigInteger, nullable=False),
    Column("UsedBytes", BigInteger, nullable=False),
    Column("ReservedBytes", BigInteger, nullable=False),
    Column("AutoPurgeOriginals", Boolean, nullable=False),
    Column("KeepThumbnails", Boolean, nullable=False),
    Column("UpdatedAt", DateTime),
    Column("RowVersion", Integer, nullable=False),
)


async def upsert_quota(session: AsyncSession, user_ref: str, quota_bytes: int) -> None:
    """Creates the row on first sight of a user; afterwards only the quota changes (spec 4.3)."""
    stmt = insert(user_storage).values(
        UserRef=user_ref,
        QuotaBytes=quota_bytes,
        UsedBytes=0,
        ReservedBytes=0,
        AutoPurgeOriginals=False,
        KeepThumbnails=False,
        RowVersion=0,
    )
    await session.execute(stmt.on_duplicate_key_update(QuotaBytes=stmt.inserted.QuotaBytes))
