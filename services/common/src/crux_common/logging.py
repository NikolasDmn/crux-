"""Structured JSON logs (spec 12): request id, user PublicId, route, latency, status.
Never tokens, passwords, API keys, prompt text, answer text or file contents."""

import json
import logging
import sys
import time
from collections.abc import Awaitable, Callable
from contextvars import ContextVar
from datetime import UTC, datetime

from starlette.requests import Request
from starlette.responses import Response
from ulid import ULID

_request_id: ContextVar[str | None] = ContextVar("request_id", default=None)
_user_ref: ContextVar[str | None] = ContextVar("user_ref", default=None)

log = logging.getLogger("crux")


def current_request_id() -> str | None:
    return _request_id.get()


def set_user(user_ref: str) -> None:
    _user_ref.set(user_ref)


class JsonFormatter(logging.Formatter):
    RESERVED = set(logging.LogRecord("", 0, "", 0, "", (), None).__dict__) | {"message", "asctime"}

    def format(self, record: logging.LogRecord) -> str:
        out = {
            "ts": datetime.fromtimestamp(record.created, UTC).isoformat(timespec="milliseconds").replace("+00:00", "Z"),
            "level": record.levelname.lower(),
            "msg": record.getMessage(),
            "logger": record.name,
            "request_id": _request_id.get(),
        }
        out.update({k: v for k, v in record.__dict__.items() if k not in self.RESERVED})
        if record.exc_info:
            out["exc"] = self.formatException(record.exc_info)
        return json.dumps(out, default=str)


def setup(service: str, level: str = "INFO") -> None:
    handler = logging.StreamHandler(sys.stdout)
    handler.setFormatter(JsonFormatter())
    root = logging.getLogger()
    root.handlers = [handler]
    root.setLevel(level)
    logging.getLogger("uvicorn.access").disabled = True
    log.info("logging ready", extra={"service": service})


def _clean_request_id(raw: str | None) -> str | None:
    if raw and 0 < len(raw) <= 64 and all(c.isalnum() or c in "-_" for c in raw):
        return raw
    return None


async def request_log_middleware(request: Request, call_next: Callable[[Request], Awaitable[Response]]) -> Response:
    rid = _clean_request_id(request.headers.get("x-request-id")) or str(ULID())
    token_rid = _request_id.set(rid)
    token_user = _user_ref.set(None)
    started = time.perf_counter()
    try:
        response = await call_next(request)
    finally:
        _request_id.reset(token_rid)
    response.headers["x-request-id"] = rid
    route = getattr(request.scope.get("route"), "path", "unmatched")
    if route not in ("/healthz", "/readyz"):
        log.info(
            "request",
            extra={
                "request_id": rid,
                "user": _user_ref.get() or "",
                "method": request.method,
                "route": route,
                "status": response.status_code,
                "latency_ms": round((time.perf_counter() - started) * 1000),
            },
        )
    _user_ref.reset(token_user)
    return response
