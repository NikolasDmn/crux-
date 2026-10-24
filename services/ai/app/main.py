from fastapi import FastAPI

from app.http.routes import internal, router
from app.service.wiring import build_deps
from app.settings import get_settings
from crux_common.app import create_app


async def _startup(app: FastAPI) -> None:
    app.state.deps = build_deps(get_settings(), app.state.sessions, app.state.contracts)


app = create_app(get_settings(), "Crux AI", [internal, router], on_startup=_startup)
