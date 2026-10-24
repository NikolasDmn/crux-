import asyncio
import logging

from redis.asyncio import Redis
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.clients.smtp import SmtpMailer
from app.service.account import UserDeletedHandler
from app.service.requests import NotifyRequestedHandler, RedisPublisher, dispatch_once
from app.service.users import UserEventsHandler
from app.settings import get_settings
from crux_common.messaging.consumer import Handler
from crux_common.messaging.contracts import Contracts
from crux_common.settings import REDIS_SOCKET_TIMEOUT
from crux_common.worker import run_worker

log = logging.getLogger("crux.notify.dispatch")


async def dispatcher(sessions: async_sessionmaker[AsyncSession], stop: asyncio.Event) -> None:
    """Job `dispatch` (spec 10): sends due deliveries; idempotent, safe to run in several workers."""
    settings = get_settings()
    mailer = SmtpMailer(settings.smtp_url, settings.mail_from)
    redis = Redis.from_url(settings.redis_url, socket_timeout=REDIS_SOCKET_TIMEOUT)
    publisher = RedisPublisher(redis)
    while not stop.is_set():
        try:
            n = await dispatch_once(sessions, mailer, settings, publisher)
            if n:
                log.info("dispatched", extra={"count": n})
        except Exception as e:
            log.error("dispatch failed", extra={"error": repr(e), "alert": True})
        try:
            await asyncio.wait_for(stop.wait(), timeout=settings.dispatch_interval_seconds)
        except TimeoutError:
            pass
    await redis.aclose()


if __name__ == "__main__":
    settings = get_settings()
    handlers: list[Handler] = [
        UserEventsHandler(settings.unsubscribe_secret),
        NotifyRequestedHandler(),
        UserDeletedHandler(Contracts(settings.contracts_dir)),
    ]
    asyncio.run(run_worker(settings, handlers, extra=[dispatcher]))
