"""Errors in the API format of backend-spec 3.1: {"error": {"code", "message", "details"}}."""

from typing import Any

from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from starlette.exceptions import HTTPException as StarletteHTTPException

from crux_common.logging import current_request_id, log


class AppError(Exception):
    status = 500
    code = "internal"
    message = "Something went wrong."

    def __init__(self, message: str | None = None, details: dict[str, Any] | None = None) -> None:
        super().__init__(message or self.message)
        self.message = message or self.message
        self.details = details


class ValidationFailed(AppError):
    status, code, message = 422, "validation_failed", "Some fields need attention."

    @classmethod
    def field(cls, field: str, code: str, message: str) -> "ValidationFailed":
        return cls(details={"fields": [{"field": field, "code": code, "message": message}]})


class Unauthenticated(AppError):
    status, code, message = 401, "unauthenticated", "Sign in to continue."


class NotFound(AppError):
    """Also for "exists but not yours" (spec 4.2)."""

    status, code, message = 404, "not_found", "Not found."


class Conflict(AppError):
    status, code, message = 409, "conflict", "Changed somewhere else."


class AiDisabled(AppError):
    status, code, message = 403, "ai_disabled", "AI is off here."


class AiBudgetExceeded(AppError):
    status, code, message = 402, "ai_budget_exceeded", "This month's AI spending cap is reached."


class QuotaExceeded(AppError):
    status, code, message = 413, "quota_exceeded", "Not enough storage left."


class FileRejected(AppError):
    status, code, message = 422, "file_rejected", "This file can't be accepted."


class RateLimited(AppError):
    status, code, message = 429, "rate_limited", "Too many attempts. Try again later."


def body(code: str, message: str, details: dict[str, Any] | None) -> dict[str, Any]:
    return {"error": {"code": code, "message": message, "details": details, "requestId": current_request_id()}}


def install(app: FastAPI) -> None:
    @app.exception_handler(AppError)
    async def _app_error(_: Request, exc: AppError) -> JSONResponse:
        if exc.status >= 500:
            log.error("internal error", extra={"error": repr(exc)})
        return JSONResponse(body(exc.code, exc.message, exc.details), status_code=exc.status)

    @app.exception_handler(RequestValidationError)
    async def _validation(_: Request, exc: RequestValidationError) -> JSONResponse:
        fields = [
            {"field": ".".join(str(p) for p in e["loc"] if p != "body"), "code": e["type"], "message": e["msg"]}
            for e in exc.errors()
        ]
        return JSONResponse(
            body("validation_failed", "Some fields need attention.", {"fields": fields}), status_code=422
        )

    @app.exception_handler(StarletteHTTPException)
    async def _http(_: Request, exc: StarletteHTTPException) -> JSONResponse:
        code = {401: "unauthenticated", 404: "not_found", 405: "not_found", 409: "conflict", 429: "rate_limited"}.get(
            exc.status_code, "internal" if exc.status_code >= 500 else "validation_failed"
        )
        status = 404 if exc.status_code == 405 else exc.status_code
        return JSONResponse(body(code, str(exc.detail), None), status_code=status)

    @app.exception_handler(Exception)
    async def _unhandled(_: Request, exc: Exception) -> JSONResponse:
        log.exception("unhandled error", exc_info=exc)
        return JSONResponse(body("internal", "Something went wrong.", None), status_code=500)
