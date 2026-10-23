"""`notify.requested` → Notification + queued deliveries; the dispatcher sends them (spec 8).

Milestone 1 handles the account emails (verify, reset). Preferences, in-app inbox and the other
kinds arrive with milestone 6; until then, non-transactional requests are recorded and not sent.
"""

import logging

from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker

from app.clients.smtp import Mailer
from app.domain import delivery as rules
from app.domain import templates
from app.repo import notifications
from crux_common.ids import new_id
from crux_common.messaging.bus import Delivery
from crux_common.timeutil import parse_iso, utcnow

log = logging.getLogger("crux.notify")


class NotifyRequestedHandler:
    topics = ("notify.requested",)

    async def handle(self, session: AsyncSession, d: Delivery) -> None:
        p = d.envelope.payload
        due_at = parse_iso(p["dueAt"])
        payload = p["payload"]
        nid = await notifications.insert_notification(
            session,
            public_id=new_id(),
            user_ref=p["userRef"],
            kind=p["kind"],
            dedupe_key=p["dedupeKey"],
            due_at=due_at,
            payload=payload,
        )
        if nid is None:
            return  # same dedupe key: already requested
        if rules.is_transactional(p["kind"], bool(p.get("transactional"))) and payload.get("to"):
            await notifications.queue_email(session, nid, due_at)
        else:
            log.info("notification recorded; channel delivery arrives with milestone 6", extra={"kind": p["kind"]})


async def dispatch_once(sessions: async_sessionmaker[AsyncSession], mailer: Mailer, batch: int = 50) -> int:
    """Sends due emails. Each row is locked while it is sent, so two dispatchers never double-send."""
    sent = 0
    async with sessions() as session, session.begin():
        now = utcnow()
        for row in await notifications.lock_due_emails(session, now, batch):
            if row.CancelledAt is not None:
                await notifications.mark(session, row.NotificationId, Status="Skipped", NextAttemptAt=None)
                continue
            attempts = row.Attempts + 1
            try:
                email = templates.render(row.Kind, row.Payload, unsubscribe_url=None)
                message_id = await mailer.send(row.Payload["to"], email.subject, email.body, {})
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
            sent += 1
    return sent
