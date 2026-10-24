from fastapi import FastAPI

from app.http.routes import internal, router
from app.service.wiring import build_deps
from app.settings import get_settings
from crux_common.app import create_app


async def _startup(app: FastAPI) -> None:
    # The bucket is created by the worker (retried until the store is up), not here: the API must
    # start even while the object store is still booting.
    app.state.deps = build_deps(get_settings(), app.state.sessions, app.state.redis, app.state.contracts)


app = create_app(get_settings(), "Crux files", [internal, router], on_startup=_startup)
