"""Builds the real dependencies from settings (tests build their own Deps with doubles)."""

from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.clients.core import HttpCore, HttpFiles
from app.clients.llm import FakeProvider, registry
from app.clients.transcripts import S3Transcripts
from app.domain import crypto
from app.service.deps import Deps
from app.settings import Settings
from crux_common.messaging.contracts import Contracts
from crux_common.service_tokens import ServiceTokens


def build_deps(settings: Settings, sessions: async_sessionmaker[AsyncSession], contracts: Contracts) -> Deps:
    tokens = ServiceTokens(settings.core_internal_url, settings.service_name, settings.service_secret)
    return Deps(
        settings=settings,
        sessions=sessions,
        providers=registry(FakeProvider() if settings.fake_allowed else None),
        core=HttpCore(settings.core_internal_url, tokens),
        files=HttpFiles(settings.files_internal_url, tokens),
        contracts=contracts,
        master_key=crypto.master_key(settings.kms_master_key),
        store=S3Transcripts(settings),
    )
