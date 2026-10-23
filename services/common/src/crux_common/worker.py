"""Worker process entry (`python -m app.jobs`): the outbox relay plus this service's consumers."""

import asyncio
import logging
import signal
import socket
from collections.abc import Callable, Coroutine, Sequence
from typing import Any

from redis.asyncio import Redis

from crux_common import db
from crux_common import logging as crux_logging
from crux_common.messaging import relay
from crux_common.messaging.bus import RedisBus
from crux_common.messaging.consumer import Handler
from crux_common.messaging.consumer import run as run_consumer
from crux_common.messaging.contracts import Contracts
from crux_common.settings import CommonSettings

log = logging.getLogger("crux.worker")

ExtraTask = Callable[[asyncio.Event], Coroutine[Any, Any, None]]


async def run_worker(settings: CommonSettings, handlers: Sequence[Handler], extra: Sequence[ExtraTask] = ()) -> None:
    crux_logging.setup(settings.service_name)
    engine = db.make_engine(settings.database_url, pool_size=5)
    sessions = db.make_sessionmaker(engine)
    redis = Redis.from_url(settings.redis_url)
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

    tasks = [asyncio.create_task(relay.run(sessions, bus, settings.service_name, stop))]
    tasks += [
        asyncio.create_task(run_consumer(sessions, bus, contracts, group, consumer_name, h, stop)) for h in handlers
    ]
    tasks += [asyncio.create_task(t(stop)) for t in extra]
    log.info("worker started", extra={"handlers": [type(h).__name__ for h in handlers]})
    await asyncio.gather(*tasks)
    await redis.aclose()
    await engine.dispose()
