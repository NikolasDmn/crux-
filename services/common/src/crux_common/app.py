"""FastAPI app factory: logging, request ids, error format, health, auth, DB and Redis wiring."""

from collections.abc import AsyncIterator, Awaitable, Callable
from contextlib import asynccontextmanager
from typing import Any

from fastapi import APIRouter, FastAPI
from fastapi.responses import JSONResponse
from redis.asyncio import Redis

from crux_common import db, errors, metrics
from crux_common import logging as crux_logging
from crux_common.auth import Authenticator, Jwks
from crux_common.messaging.contracts import Contracts
from crux_common.settings import REDIS_SOCKET_TIMEOUT, CommonSettings


def create_app(
    settings: CommonSettings,
    title: str,
    routers: list[APIRouter],
    on_startup: Callable[[FastAPI], Awaitable[None]] | None = None,
) -> FastAPI:
    crux_logging.setup(settings.service_name)

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        engine = db.make_engine(settings.database_url)
        app.state.settings = settings
        app.state.engine = engine
        app.state.sessions = db.make_sessionmaker(engine)
        app.state.redis = Redis.from_url(settings.redis_url, socket_timeout=REDIS_SOCKET_TIMEOUT)
        app.state.auth = Authenticator(Jwks(settings.jwks_url), settings.service_name)
        app.state.contracts = Contracts(settings.contracts_dir)
        if on_startup:
            await on_startup(app)
        yield
        await app.state.redis.aclose()
        await engine.dispose()

    # The gateway strips /api/v1/<service>; the merged document re-adds the prefix (core).
    app = FastAPI(
        title=title, version="0.1.0", lifespan=lifespan, openapi_url="/openapi.json", docs_url=None, redoc_url=None
    )
    metrics.install(app, settings.service_name)
    app.middleware("http")(crux_logging.request_log_middleware)
    errors.install(app)

    @app.get("/healthz", include_in_schema=False)
    async def healthz() -> dict[str, str]:
        return {"status": "ok"}

    @app.get("/readyz", include_in_schema=False)
    async def readyz() -> JSONResponse:
        db_ok = await db.ping(app.state.engine)
        try:
            redis_ok = bool(await app.state.redis.ping())
        except Exception:
            redis_ok = False
        checks: dict[str, Any] = {"db": db_ok, "redis": redis_ok}
        return JSONResponse(checks, status_code=200 if all(checks.values()) else 503)

    for r in routers:
        app.include_router(r)
    return app
