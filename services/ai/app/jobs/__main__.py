"""ai-worker: outbox relay, consumers, and the ai jobs (spec 10)."""

import asyncio
import logging

from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.service import chat
from app.service.wiring import build_deps
from app.settings import get_settings
from crux_common.messaging.consumer import Handler
from crux_common.messaging.contracts import Contracts
from crux_common.worker import run_worker

log = logging.getLogger("crux.ai.worker")
DAY = 24 * 3600.0


async def daily_jobs(sessions: async_sessionmaker[AsyncSession], stop: asyncio.Event) -> None:
    """`transcripts.purge` (daily)."""
    s = get_settings()
    deps = build_deps(s, sessions, Contracts(s.contracts_dir))
    while not stop.is_set():
        try:
            n = await chat.purge_transcripts(deps)
            log.info("job done", extra={"job": "transcripts.purge", "count": n})
        except Exception as e:
            log.error("job failed", extra={"job": "transcripts.purge", "error": repr(e), "alert": True})
        try:
            await asyncio.wait_for(stop.wait(), timeout=DAY)
        except TimeoutError:
            pass


if __name__ == "__main__":
    handlers: list[Handler] = []
    asyncio.run(run_worker(get_settings(), handlers, extra=[daily_jobs]))
