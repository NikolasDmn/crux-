"""What every ai use case needs, built once per process (wiring.py) or by tests."""

from dataclasses import dataclass
from typing import Any

from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.clients.core import CoreApi, FilesApi
from app.clients.llm import Provider
from app.settings import Settings
from crux_common.messaging.contracts import Contracts


@dataclass
class Deps:
    settings: Settings
    sessions: async_sessionmaker[AsyncSession]
    providers: dict[str, Provider]
    core: CoreApi
    files: FilesApi
    contracts: Contracts
    master_key: bytes
    #: Transcript store (put / delete); None = transcripts off (tests).
    store: Any | None = None
    #: Redis for rate limits; None = no limits (tests, the worker).
    redis: Any | None = None
