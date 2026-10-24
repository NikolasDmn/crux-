"""Worker process entry (`python -m app.jobs`): the outbox relay plus this service's consumers."""

import asyncio
import logging
import signal
import socket
from collections.abc import Callable, Coroutine, Sequence
from functools import partial
from typing import Any

from redis.asyncio import Redis
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from crux_common import db
from crux_common import logging as crux_logging
from crux_common.messaging import relay
from crux_common.messaging.bus import RedisBus
from crux_common.messaging.consumer import Handler
from crux_common.messaging.consumer import run as run_consumer
from crux_common.messaging.contracts import Contracts
from crux_common.settings import REDIS_SOCKET_TIMEOUT, CommonSettings

log = logging.getLogger("crux.worker")

#: Extra loops a service runs in its worker (e.g. notify's dispatcher): get sessions + the stop event.
ExtraTask = Callable[[async_sessionmaker[AsyncSession], asyncio.Event], Coroutine[Any, Any, None]]


RESTART_DELAY_SECONDS = 5.0


async def supervise(
    name: str, start: Callable[[], Coroutine[Any, Any, None]], stop: asyncio.Event, delay: float = RESTART_DELAY_SECONDS
) -> None:
    """Runs a worker loop; if it crashes (a dependency not up yet, a bug), log and restart it after a
    pause, so one failing loop never stops the others (relay, consumers, jobs)."""
    while not stop.is_set():
        try:
            await start()
            return
        except Exception as e:
            log.error("worker loop crashed; restarting", extra={"loop": name, "error": repr(e), "alert": True})
            try:
                await asyncio.wait_for(stop.wait(), timeout=delay)
            except TimeoutError:
                pass


async def run_worker(settings: CommonSettings, handlers: Sequence[Handler], extra: Sequence[ExtraTask] = ()) -> None:
    crux_logging.setup(settings.service_name)
    engine = db.make_engine(settings.database_url, pool_size=5)
    sessions = db.make_sessionmaker(engine)
    redis = Redis.from_url(settings.redis_url, socket_timeout=REDIS_SOCKET_TIMEOUT)
    bus = RedisBus(redis)
    contracts = Contracts(settings.contracts_dir)
    group = settings.service_name
    consumer_name = f"{settings.service_name}-{socket.gethostname()}"
    for h in handlers:
        await bus.ensure_group(group, list(h.topics))

    stop = asyncio.Event()
    loop = asyncio.get_running_loop()
    for sig in (signal.SIGINT, signal.SIGTERM):
        try:
            loop.add_signal_handler(sig, stop.set)
        except NotImplementedError:  # Windows dev
            pass

    tasks = [
        asyncio.create_task(supervise("relay", partial(relay.run, sessions, bus, settings.service_name, stop), stop))
    ]
    tasks += [
        asyncio.create_task(
            supervise(
                type(h).__name__, partial(run_consumer, sessions, bus, contracts, group, consumer_name, h, stop), stop
            )
        )
        for h in handlers
    ]
    tasks += [
        asyncio.create_task(supervise(getattr(t, "__name__", "task"), partial(t, sessions, stop), stop)) for t in extra
    ]
    log.info("worker started", extra={"handlers": [type(h).__name__ for h in handlers]})
    await asyncio.gather(*tasks)
    await redis.aclose()
    await engine.dispose()
