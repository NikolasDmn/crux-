"""Dev only (`make purge-now FILE=<id>`): remove these freed originals now instead of after the 7-day
undo window, so "re-upload restores" can be tried (and the web's e2e suite can test it)."""

import asyncio
import sys

from sqlalchemy import update

from app.jobs.__main__ import _deps
from app.repo.files import stored_file
from app.service import saver
from app.settings import get_settings
from crux_common.db import make_engine, make_sessionmaker
from crux_common.timeutil import utcnow


async def main(file_ids: list[str]) -> None:
    engine = make_engine(get_settings().database_url)
    sessions = make_sessionmaker(engine)
    async with sessions() as s, s.begin():
        await s.execute(
            update(stored_file)
            .where(stored_file.c.PublicId.in_(file_ids), stored_file.c.Status == "PendingPurge")
            .values(PurgeAt=utcnow())
        )
    print("purged", await saver.purge_due(_deps(sessions)))
    await engine.dispose()


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit("usage: python -m app.jobs.purge_now <file id>...")
    asyncio.run(main(sys.argv[1:]))
