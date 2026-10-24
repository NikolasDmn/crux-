"""Prometheus metrics for every Python service (spec 12): request counts and latency per route,
and the outbox backlog. Served on `/internal/metrics` (the gateway never exposes `/internal/*`)."""

import time
from collections.abc import Awaitable, Callable

from fastapi import FastAPI, Request, Response
from sqlalchemy import text

_series: dict[tuple[str, str, int], list[float]] = {}


def _route(request: Request) -> str:
    route = request.scope.get("route")
    return str(getattr(route, "path", "unmatched"))


async def middleware(request: Request, call_next: Callable[[Request], Awaitable[Response]]) -> Response:
    started = time.perf_counter()
    response = await call_next(request)
    key = (request.method, _route(request), response.status_code)
    s = _series.setdefault(key, [0.0, 0.0])
    s[0] += 1
    s[1] += time.perf_counter() - started
    return response


def _esc(v: str) -> str:
    return v.replace("\\", "\\\\").replace('"', '\\"')


def install(app: FastAPI, service: str) -> None:
    app.middleware("http")(middleware)

    @app.get("/internal/metrics", include_in_schema=False)
    async def metrics(request: Request) -> Response:
        lines = ["# TYPE crux_http_requests_total counter", "# TYPE crux_http_request_duration_seconds summary"]
        for (method, route, status), (count, total) in sorted(_series.items()):
            labels = f'service="{service}",method="{_esc(method)}",route="{_esc(route)}",status="{status}"'
            lines.append(f"crux_http_requests_total{{{labels}}} {int(count)}")
            lines.append(f"crux_http_request_duration_seconds_sum{{{labels}}} {total:.6f}")
            lines.append(f"crux_http_request_duration_seconds_count{{{labels}}} {int(count)}")
        try:
            async with request.app.state.sessions() as s:
                backlog = (await s.execute(text("SELECT COUNT(*) FROM Outbox WHERE PublishedAt IS NULL"))).scalar_one()
        except Exception:
            backlog = -1
        lines += ["# TYPE crux_outbox_unpublished gauge", f'crux_outbox_unpublished{{service="{service}"}} {backlog}']
        return Response("\n".join(lines) + "\n", media_type="text/plain; version=0.0.4")
