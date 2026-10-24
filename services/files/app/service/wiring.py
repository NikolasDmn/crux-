"""Builds the real dependencies from settings (tests build their own Deps with doubles)."""

from redis.asyncio import Redis
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.clients.core import HttpCore
from app.clients.scanner import ClamdScanner, FakeScanner, Scanner
from app.clients.store import S3Store
from app.service.deps import Deps
from app.settings import Settings
from crux_common.messaging.contracts import Contracts
from crux_common.service_tokens import ServiceTokens


def build_deps(
    settings: Settings, sessions: async_sessionmaker[AsyncSession], redis: Redis | None, contracts: Contracts
) -> Deps:
    store = S3Store(
        settings.s3_endpoint,
        settings.s3_public_endpoint,
        settings.s3_access_key,
        settings.s3_secret_key,
        settings.s3_bucket,
        settings.s3_region,
    )
    tokens = ServiceTokens(settings.core_internal_url, settings.service_name, settings.service_secret)
    scanner: Scanner = (
        ClamdScanner(settings.clamd_host, settings.clamd_port) if settings.scanner == "clamd" else FakeScanner()
    )
    return Deps(
        settings=settings,
        sessions=sessions,
        store=store,
        core=HttpCore(settings.core_internal_url, tokens),
        scanner=scanner,
        contracts=contracts,
        redis=redis,
    )
