"""`notify.requested` → Notification + one delivery per enabled channel; the dispatcher sends them
when due (spec 8).

- Account emails (verify, reset, invites) are transactional: email only, whatever the preferences.
- Every other kind goes to the channels the user enabled (plus in-app for kinds that need an
  answer). With no channel on, nothing is stored.
- The same dedupe key again: before delivery it reschedules; after, it's ignored. `cancel: true`
  cancels by key (a deadline that was removed).
"""

import json
import logging
from typing import Any, Protocol

from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.clients.smtp import Mailer
from app.domain import delivery as rules
from app.domain import kinds, templates, unsubscribe
from app.repo import notifications, preferences, recipients
from crux_common.ids import new_id
from crux_common.messaging.bus import Delivery
from crux_common.timeutil import iso, parse_iso, utcnow

log = logging.getLogger("crux.notify")


class Publisher(Protocol):
    """Live push to open inbox streams (Redis pub/sub in production)."""

    async def publish(self, user_ref: str, data: dict[str, Any]) -> None: ...


def stream_channel(user_ref: str) -> str:
    return f"crux:notify:{user_ref}"


class RedisPublisher:
    def __init__(self, redis: Any) -> None:
        self._redis = redis

    async def publish(self, user_ref: str, data: dict[str, Any]) -> None:
        await self._redis.publish(stream_channel(user_ref), json.dumps(data, separators=(",", ":")))


class NotifyRequestedHandler:
    topics = ("notify.requested",)

    async def handle(self, session: AsyncSession, d: Delivery) -> None:
        p = d.envelope.payload
        user_ref, kind, key = p["userRef"], p["kind"], p["dedupeKey"]
        due_at = parse_iso(p["dueAt"])
        payload = p["payload"]
        existing = await notifications.by_dedupe(session, user_ref, key)
        if p.get("cancel"):
            if existing is not None and existing.CancelledAt is None:
                await notifications.cancel(session, existing.Id, utcnow())
            return
        if existing is not None:
            if await notifications.pending_channels(session, existing.Id):
                await notifications.reschedule(session, existing.Id, due_at, payload)
            return  # delivered already: never twice
        if rules.is_transactional(kind, bool(p.get("transactional"))):
            channels = [kinds.EMAIL] if payload.get("to") else []
        else:
            channels = kinds.channels_for(await preferences.for_user(session, user_ref), kind)
        if not channels:
            log.info("notification not stored: no channel enabled", extra={"kind": kind})
            return
        nid = await notifications.insert_notification(
            session, public_id=new_id(), user_ref=user_ref, kind=kind, dedupe_key=key, due_at=due_at, payload=payload
        )
        if nid is None:
            return
        for c in channels:
            await notifications.queue(session, nid, c, due_at)


def inbox_item(row: Any, tz: str) -> dict[str, Any]:
    text = templates.inbox(row.Kind, row.Payload or {}, tz)
    return {
        "id": row.PublicId,
        "kind": row.Kind,
        "title": text.title,
        "body": text.body,
        "link": text.link,
        "payload": row.Payload or {},
        "createdAt": iso(row.DueAt),
        "readAt": iso(row.ReadAt) if getattr(row, "ReadAt", None) else None,
    }


class Config(Protocol):
    app_base_url: str
    unsubscribe_secret: str


def unsubscribe_url(config: Config, user_ref: str, kind: str) -> str:
    token = unsubscribe.token_for(config.unsubscribe_secret, user_ref)
    return f"{config.app_base_url}/api/v1/notify/unsubscribe?token={token}&kind={kind}"


async def _skip(session: AsyncSession, nid: int, attempts: int, reason: str) -> None:
    await notifications.mark(session, nid, Status="Skipped", Attempts=attempts, NextAttemptAt=None, LastError=reason)


async def dispatch_once(
    sessions: async_sessionmaker[AsyncSession],
    mailer: Mailer,
    config: Config,
    publisher: Publisher | None = None,
    batch: int = 50,
) -> int:
    """Sends due deliveries. Each row is locked while it is handled (FOR UPDATE SKIP LOCKED), so two
    dispatchers never double-send."""
    done = 0
    pushed: list[tuple[str, dict[str, Any]]] = []
    async with sessions() as session, session.begin():
        now = utcnow()
        # In-app: due now = shows in the inbox, pushed to open streams after commit.
        for row in await notifications.lock_due_in_app(session, now, batch):
            await notifications.mark_channel(
                session, row.NotificationId, kinds.IN_APP, Status="Sent", SentAt=now, NextAttemptAt=None, Attempts=1
            )
            r = await recipients.get(session, row.UserRef)
            pushed.append((row.UserRef, inbox_item(row, r.TimeZone if r else "Europe/Amsterdam")))
            done += 1
        for row in await notifications.lock_due_emails(session, now, batch):
            if row.CancelledAt is not None:
                await notifications.mark(session, row.NotificationId, Status="Skipped", NextAttemptAt=None)
                continue
            attempts = row.Attempts + 1
            payload = row.Payload or {}
            headers: dict[str, str] = {}
            tz = "Europe/Amsterdam"
            if rules.is_transactional(row.Kind, True) and payload.get("to"):
                to = payload["to"]
                unsub = None
            else:
                r = await recipients.get(session, row.UserRef)
                # Checked at send time: the address may have changed, bounced, or the kind been turned off.
                if r is None or not r.EmailVerified:
                    await _skip(session, row.NotificationId, attempts, "unverified")
                    continue
                if await recipients.is_suppressed(session, r.Email):
                    await _skip(session, row.NotificationId, attempts, "suppressed")
                    continue
                prefs = await preferences.for_user(session, row.UserRef)
                if not kinds.enabled(prefs, row.Kind, kinds.EMAIL):
                    await _skip(session, row.NotificationId, attempts, "disabled")
                    continue
                to, tz = r.Email, r.TimeZone
                unsub = unsubscribe_url(config, row.UserRef, row.Kind)
                headers = {"List-Unsubscribe": f"<{unsub}>", "List-Unsubscribe-Post": "List-Unsubscribe=One-Click"}
            try:
                email = templates.render(row.Kind, payload, unsubscribe_url=unsub, app_url=config.app_base_url, tz=tz)
                message_id = await mailer.send(to, email.subject, email.body, headers)
            except Exception as e:
                retry_at = rules.next_attempt(attempts, now)
                await notifications.mark(
                    session,
                    row.NotificationId,
                    Attempts=attempts,
                    LastError=repr(e)[:500],
                    Status="Pending" if retry_at else "Failed",
                    NextAttemptAt=retry_at,
                )
                log.warning(
                    "email send failed", extra={"kind": row.Kind, "attempt": attempts, "will_retry": bool(retry_at)}
                )
                continue
            await notifications.mark(
                session,
                row.NotificationId,
                Attempts=attempts,
                Status="Sent",
                SentAt=now,
                NextAttemptAt=None,
                ProviderMessageId=message_id[:128] or None,
            )
            done += 1
    if publisher is not None:
        for user_ref, item in pushed:
            try:
                await publisher.publish(user_ref, item)
            except Exception as e:  # the inbox has it; a missed push only delays the badge
                log.warning("inbox push failed", extra={"error": repr(e)})
    return done
