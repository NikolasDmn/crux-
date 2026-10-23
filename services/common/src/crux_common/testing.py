"""Integration-test helpers (spec 13: real MySQL). Set CRUX_TEST_MYSQL_URL, e.g.
`mysql://crux_test:crux-test@127.0.0.1:3306` (the compose MySQL has that user). Each test gets a
fresh `crux_test_<service>_<ulid>` database migrated from empty with the service's Alembic chain,
dropped afterwards. Without the variable the tests skip, so laptops without Docker stay green.
"""

import asyncio
import os
import sys
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path

import pymysql
import pytest
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from crux_common import db
from crux_common.ids import new_id

ENV = "CRUX_TEST_MYSQL_URL"


def mysql_url_or_skip() -> str:
    url = os.environ.get(ENV, "")
    if not url:
        pytest.skip(f"set {ENV} to run DB integration tests")
    return url.rstrip("/")


def _admin(url: str) -> pymysql.connections.Connection:
    rest = url.split("://", 1)[1]
    creds, host = rest.rsplit("@", 1)
    user, password = creds.split(":", 1)
    hostname, _, port = host.partition(":")
    return pymysql.connect(host=hostname, port=int(port or 3306), user=user, password=password, autocommit=True)


@asynccontextmanager
async def migrated_db(service_dir: Path, service: str) -> AsyncIterator[async_sessionmaker[AsyncSession]]:
    """A fresh database with every migration of `service_dir` applied (proves 'migrations run from empty')."""
    base = mysql_url_or_skip()
    name = f"crux_test_{service}_{new_id().lower()}"
    admin = _admin(base)
    with admin.cursor() as cur:
        cur.execute(f"CREATE DATABASE `{name}` CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci")
    async_url = base.replace("mysql://", "mysql+aiomysql://") + f"/{name}"
    try:
        env = {**os.environ, "CRUX_DATABASE_URL": async_url}
        proc = await asyncio.create_subprocess_exec(
            sys.executable, "-m", "alembic", "upgrade", "head", cwd=service_dir, env=env
        )
        if await proc.wait() != 0:
            raise RuntimeError(f"alembic upgrade failed for {service}")
        engine = db.make_engine(async_url, pool_size=2)
        try:
            yield db.make_sessionmaker(engine)
        finally:
            await engine.dispose()
    finally:
        with admin.cursor() as cur:
            cur.execute(f"DROP DATABASE IF EXISTS `{name}`")
        admin.close()
