"""files-worker: outbox relay, consumers, the scan/extract queue, and the daily sweeps (spec 10)."""

import asyncio
import logging
import time
from typing import Any

from redis.asyncio import Redis
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.clients.store import S3Store
from app.service import jobs, lifecycle, saver
from app.service.deps import Deps
from app.service.users import UserEventsHandler
from app.service.wiring import build_deps
from app.settings import get_settings
from crux_common.messaging.consumer import Handler
from crux_common.messaging.contracts import Contracts
from crux_common.settings import REDIS_SOCKET_TIMEOUT
from crux_common.worker import run_worker

log = logging.getLogger("crux.files.worker")
SWEEP_EVERY = 3600.0


def _deps(sessions: async_sessionmaker[AsyncSession]) -> Deps:
    s = get_settings()
    return build_deps(
        s, sessions, Redis.from_url(s.redis_url, socket_timeout=REDIS_SOCKET_TIMEOUT), Contracts(s.contracts_dir)
    )


STORE_WAIT_SECONDS = 120


async def wait_for_store(deps: Deps) -> None:
    """The object store may still be booting when the worker starts (compose starts them together)."""
    for attempt in range(STORE_WAIT_SECONDS):
        try:
            await deps.store.ensure_bucket()
            return
        except Exception as e:
            if attempt == 0:
                log.info("waiting for the object store", extra={"error": repr(e)})
            await asyncio.sleep(1)
    await deps.store.ensure_bucket()  # last try: let it raise


async def job_loop(sessions: async_sessionmaker[AsyncSession], stop: asyncio.Event) -> None:
    deps = _deps(sessions)
    await wait_for_store(deps)
    last_sweep = 0.0
    while not stop.is_set():
        try:
            if time.monotonic() - last_sweep > SWEEP_EVERY:
                last_sweep = time.monotonic()
                stuck = await jobs.recover_stuck(deps)
                abandoned = await jobs.sweep_abandoned(deps)
                purged = await saver.purge_due(deps)
                expired = await lifecycle.expire_exports(deps)
                log.info(
                    "sweeps done",
                    extra={
                        "stuck_jobs": stuck,
                        "abandoned_uploads": abandoned,
                        "purged": purged,
                        "expired_exports": expired,
                    },
                )
            worked = await jobs.run_one(deps)
        except Exception as e:
            log.error("job loop error", extra={"error": repr(e), "alert": True})
            worked = False
        if not worked:
            try:
                await asyncio.wait_for(stop.wait(), timeout=1.0)
            except TimeoutError:
                pass


def _deps_for_handlers(settings: Any, store: S3Store) -> Deps:
    """Handlers are built before run_worker opens its sessions: they get their own engine."""
    from crux_common import db

    sessions = db.make_sessionmaker(db.make_engine(settings.database_url, pool_size=2))
    d = _deps(sessions)
    d.store = store
    return d


if __name__ == "__main__":
    settings = get_settings()
    store = S3Store(
        settings.s3_endpoint,
        settings.s3_public_endpoint,
        settings.s3_access_key,
        settings.s3_secret_key,
        settings.s3_bucket,
        settings.s3_region,
    )
    handlers: list[Handler] = [
        UserEventsHandler(),
        jobs.CourseEventsHandler(store),
        saver.PurgeRequestsHandler(Contracts(settings.contracts_dir)),
        lifecycle.UserDeletedHandler(_deps_for_handlers(settings, store), Contracts(settings.contracts_dir)),
    ]
    asyncio.run(run_worker(get_settings(), handlers, extra=[job_loop]))
