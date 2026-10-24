"""What the use cases need, built once per process (API or worker)."""

from dataclasses import dataclass

from redis.asyncio import Redis
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.clients.core import CoreApi
from app.clients.scanner import Scanner
from app.clients.store import ObjectStore
from app.settings import Settings
from crux_common.messaging.contracts import Contracts


@dataclass
class Deps:
    settings: Settings
    sessions: async_sessionmaker[AsyncSession]
    store: ObjectStore
    core: CoreApi
    scanner: Scanner
    contracts: Contracts
    redis: Redis | None = None
