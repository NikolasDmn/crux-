"""Async SQLAlchemy engine with the schema header's connection rules: UTC session zone and strict
mode on every pooled connection (review H10), utf8mb4."""

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

from sqlalchemy import event, text
from sqlalchemy.ext.asyncio import AsyncEngine, AsyncSession, async_sessionmaker, create_async_engine

SESSION_INIT = "SET time_zone = '+00:00', sql_mode = CONCAT(@@sql_mode, ',STRICT_TRANS_TABLES')"


def make_engine(url: str, pool_size: int = 10) -> AsyncEngine:
    sep = "&" if "?" in url else "?"
    engine = create_async_engine(
        f"{url}{sep}charset=utf8mb4", pool_size=pool_size, pool_pre_ping=True, pool_recycle=1800
    )

    @event.listens_for(engine.sync_engine, "connect")
    def _on_connect(dbapi_conn: Any, _record: Any) -> None:
        cur = dbapi_conn.cursor()
        cur.execute(SESSION_INIT)
        cur.close()

    return engine


def make_sessionmaker(engine: AsyncEngine) -> async_sessionmaker[AsyncSession]:
    return async_sessionmaker(engine, expire_on_commit=False)


@asynccontextmanager
async def transaction(sessions: async_sessionmaker[AsyncSession]) -> AsyncIterator[AsyncSession]:
    """One transaction per use case (spec 2.1): commits on success, rolls back on error."""
    async with sessions() as session, session.begin():
        yield session


async def ping(engine: AsyncEngine) -> bool:
    try:
        async with engine.connect() as conn:
            await conn.execute(text("SELECT 1"))
        return True
    except Exception:
        return False
