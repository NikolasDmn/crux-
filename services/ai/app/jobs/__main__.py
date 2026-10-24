"""ai-worker: outbox relay, consumers (AI grading, grade decisions) and the ai jobs (spec 10)."""

import asyncio
import logging

from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.service import chat, generation, grading, imports, lifecycle
from app.service.deps import Deps
from app.service.wiring import build_deps
from app.settings import Settings, get_settings
from crux_common import db
from crux_common.messaging.consumer import Handler
from crux_common.messaging.contracts import Contracts
from crux_common.worker import run_worker

log = logging.getLogger("crux.ai.worker")
DAY = 24 * 3600.0


def worker_deps(settings: Settings) -> Deps:
    """The handlers' own engine: they're built before run_worker opens its sessions."""
    sessions = db.make_sessionmaker(db.make_engine(settings.database_url, pool_size=5))
    return build_deps(settings, sessions, Contracts(settings.contracts_dir))


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


async def generation_loop(sessions: async_sessionmaker[AsyncSession], stop: asyncio.Event) -> None:
    """The import and generation queues (one job at a time per worker) and the hourly nightly top-up."""
    s = get_settings()
    deps = build_deps(s, sessions, Contracts(s.contracts_dir))
    last_nightly = 0.0
    loop = asyncio.get_running_loop()
    while not stop.is_set():
        worked = False
        try:
            if loop.time() - last_nightly > 3600:
                last_nightly = loop.time()
                n = await generation.nightly(deps)
                log.info("job done", extra={"job": "topup.nightly", "count": n})
            worked = await imports.run_one(deps) or await generation.run_one(deps)
        except Exception as e:
            log.error("job failed", extra={"job": "generation", "error": repr(e), "alert": True})
        if not worked:
            try:
                await asyncio.wait_for(stop.wait(), timeout=2.0)
            except TimeoutError:
                pass


if __name__ == "__main__":
    settings = get_settings()
    deps = worker_deps(settings)
    handlers: list[Handler] = [
        grading.MockSubmittedHandler(deps),
        grading.GradeDecidedHandler(),
        lifecycle.UserDeletedHandler(deps),
    ]
    asyncio.run(run_worker(settings, handlers, extra=[daily_jobs, generation_loop]))
