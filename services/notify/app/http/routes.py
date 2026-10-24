"""Public routes under /api/v1/notify (spec 15). The gateway strips the prefix."""

import asyncio
import json
from collections.abc import AsyncIterator
from html import escape
from typing import Any

from fastapi import APIRouter, Query, Request
from fastapi.responses import HTMLResponse, StreamingResponse

from app.service import account
from app.service import inbox as svc
from app.service.requests import stream_channel
from crux_common.auth import Caller, User
from crux_common.errors import NotFound
from crux_common.sse import PING_SECONDS, event, redeem

router = APIRouter(tags=["notify"])
internal = APIRouter(prefix="/internal/v1", tags=["internal"])


def sessions(request: Request):  # type: ignore[no-untyped-def]
    return request.app.state.sessions


@router.get("/preferences", response_model=svc.PreferencesView, response_model_by_alias=True)
async def get_preferences(user: User, request: Request) -> svc.PreferencesView:
    """Kind by channel. Everything is off until you turn it on."""
    return await svc.get_preferences(sessions(request), user)


@router.put("/preferences", response_model=svc.PreferencesView, response_model_by_alias=True)
async def put_preferences(body: svc.PreferencesInput, user: User, request: Request) -> svc.PreferencesView:
    """Sets the listed (kind, channel) switches; others stay as they are."""
    return await svc.put_preferences(sessions(request), user, body)


@router.get("/inbox", response_model=svc.InboxView, response_model_by_alias=True)
async def get_inbox(
    user: User,
    request: Request,
    unread: bool = False,
    limit: int = Query(30),
    cursor: str | None = None,
) -> svc.InboxView:
    return await svc.inbox(sessions(request), user, unread, limit, cursor)


@router.post("/read-all")
async def read_all(user: User, request: Request) -> dict[str, int]:
    return {"unread": await svc.mark_read(sessions(request), user, None)}


@router.post("/{notification_id}/read")
async def read_one(notification_id: str, user: User, request: Request) -> dict[str, int]:
    return {"unread": await svc.mark_read(sessions(request), user, notification_id)}


@router.get("/stream")
async def stream(ticket: str, request: Request) -> StreamingResponse:
    """SSE (spec 3.4): `notification` for each new inbox item, `ping` every 20 s. Auth: `?ticket=`
    from `POST /api/v1/sse-ticket`."""
    redis = request.app.state.redis
    user = await redeem(redis, ticket)

    async def frames() -> AsyncIterator[str]:
        pubsub = redis.pubsub()
        await pubsub.subscribe(stream_channel(user.user_ref))
        loop = asyncio.get_running_loop()
        last_ping = loop.time()
        try:
            yield event("ready", {})
            while not await request.is_disconnected():
                msg = await pubsub.get_message(ignore_subscribe_messages=True, timeout=1.0)
                if msg and msg.get("type") == "message":
                    yield event("notification", json.loads(msg["data"]))
                if loop.time() - last_ping >= PING_SECONDS:
                    last_ping = loop.time()
                    yield event("ping", {})
        finally:
            await pubsub.unsubscribe()
            await pubsub.aclose()

    return StreamingResponse(frames(), media_type="text/event-stream", headers={"cache-control": "no-cache"})


_STYLE = "font-family:system-ui;max-width:32rem;margin:3rem auto;padding:0 1rem"


def _page(title: str, body: str) -> HTMLResponse:
    return HTMLResponse(
        "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'>"
        f"<title>{escape(title)}</title><body style='{_STYLE}'>"
        f"<h1 style='font-size:1.4rem'>{escape(title)}</h1>{body}</body>"
    )


@router.get("/unsubscribe", response_class=HTMLResponse)
async def unsubscribe_page(token: str, kind: str = "all") -> HTMLResponse:
    """The link in every email. GET only asks (link scanners open links); the button POSTs."""
    fields = (
        f"<input type=hidden name=token value='{escape(token)}'><input type=hidden name=kind value='{escape(kind)}'>"
    )
    return _page(
        "Stop these emails?",
        f"<form method=post>{fields}<button style='font-size:1rem;padding:.5rem 1rem'>Unsubscribe</button></form>",
    )


@router.post("/unsubscribe", response_class=HTMLResponse)
async def unsubscribe(request: Request, token: str | None = None, kind: str | None = None) -> HTMLResponse:
    """One-click (RFC 8058 `List-Unsubscribe-Post`) or the confirm button: token and kind come in the
    query or a form body."""
    if token is None or kind is None:
        form = await request.form()
        token = token or str(form.get("token", ""))
        kind = kind or str(form.get("kind", "all"))
    what = await svc.unsubscribe_from(sessions(request), token, kind or "all")
    return _page(
        "Unsubscribed",
        f"<p>You won't get {escape(what)} any more. Change this any time in Settings → Notifications.</p>",
    )


@router.post("/webhooks/email")
async def email_webhook(body: svc.BounceBatch, request: Request) -> dict[str, int]:
    """Bounces and complaints from the email provider (spec 8) → suppression list. Authenticated
    with the shared secret in `X-Crux-Webhook-Secret`; 404 when unset or wrong."""
    if not _webhook_ok(request):
        raise NotFound()
    return {"suppressed": await svc.record_bounces(sessions(request), body)}


@router.post("/webhooks/postmark")
async def postmark_webhook(body: dict[str, Any], request: Request) -> dict[str, int]:
    """Postmark's bounce and spam-complaint webhooks (B6-8). Set the webhook URL with HTTP Basic
    auth whose password is `CRUX_EMAIL_WEBHOOK_SECRET` (or send it in `X-Crux-Webhook-Secret`);
    404 when unset or wrong. Soft bounces are acknowledged and ignored."""
    if not _webhook_ok(request):
        raise NotFound()
    event = svc.postmark_event(body)
    if event is None:
        return {"suppressed": 0}
    return {"suppressed": await svc.record_bounces(sessions(request), svc.BounceBatch(events=[event]))}


def _webhook_ok(request: Request) -> bool:
    return svc.webhook_authorized(
        request.app.state.settings.email_webhook_secret,
        request.headers.get("x-crux-webhook-secret", ""),
        request.headers.get("authorization", ""),
    )


@internal.post("/export")
async def internal_export(request: Request, caller: Caller, body: dict[str, str]) -> dict[str, object]:
    """Spec 11: this service's part of an account export. Service token required."""
    async with sessions(request)() as s:
        return await account.export(s, body["userRef"])
