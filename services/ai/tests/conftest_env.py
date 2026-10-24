"""Shared test environment for the ai service: a migrated DB, the fake provider, fake core/files."""

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path

from app.clients.core import CourseAccess, FakeCore, FakeFiles
from app.clients.llm import FakeProvider, registry
from app.clients.transcripts import MemoryTranscripts
from app.domain import crypto
from app.service.deps import Deps
from app.settings import DEV_MASTER_KEY, Settings
from crux_common.auth import Principal
from crux_common.messaging.contracts import Contracts
from crux_common.testing import migrated_db

SERVICE_DIR = Path(__file__).resolve().parents[1]
CONTRACTS = SERVICE_DIR.parents[1] / "contracts"
USER = Principal("01TESTUSER0000000000000001", "User", "free")
OTHER = Principal("01TESTUSER0000000000000002", "User", "free")
COURSE = "01TESTCOURSE00000000000001"


@asynccontextmanager
async def env() -> AsyncIterator[tuple[Deps, FakeProvider, FakeCore, FakeFiles]]:
    async with migrated_db(SERVICE_DIR, "ai") as sessions:
        fake, core, files = FakeProvider(), FakeCore(), FakeFiles()
        core.access[(USER.user_ref, COURSE)] = CourseAccess(USER.user_ref, "Owner")
        deps = Deps(
            settings=Settings(database_url="unused"),
            sessions=sessions,
            providers=registry(fake),
            core=core,
            files=files,
            contracts=Contracts(CONTRACTS),
            master_key=crypto.master_key(DEV_MASTER_KEY),
            store=MemoryTranscripts(),
        )
        yield deps, fake, core, files
