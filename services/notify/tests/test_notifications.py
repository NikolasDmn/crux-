"""Milestone 6: preferences, in-app inbox + push, email for every kind, reschedule/cancel by key,
unsubscribe, suppression (spec 8)."""

from datetime import datetime, timedelta
from pathlib import Path
from typing import Any

import pytest
from sqlalchemy import select

from app.domain import kinds, templates, unsubscribe
from app.repo import recipients
from app.repo.notifications import delivery, notification
from app.service import inbox as svc
from app.service.requests import NotifyRequestedHandler, dispatch_once
from app.service.users import UserEventsHandler
from crux_common.auth import Principal
from crux_common.errors import NotFound, ValidationFailed
from crux_common.messaging.bus import Delivery, Envelope
from crux_common.messaging.consumer import Outcome, process
from crux_common.messaging.contracts import Contracts
from crux_common.testing import migrated_db
from crux_common.timeutil import iso, utcnow

SERVICE_DIR = Path(__file__).resolve().parents[1]
CONTRACTS = SERVICE_DIR.parents[1] / "contracts"
USER = "01TEST00000000000000000001"
ME = Principal(user_ref=USER, role="User", tier="free")
SECRET = "test-secret"


class Config:
    app_base_url = "http://crux.test"
    unsubscribe_secret = SECRET


CONFIG = Config()


class FakeMailer:
    def __init__(self) -> None:
        self.sent: list[tuple[str, str, str, dict[str, str]]] = []

    async def send(self, to: str, subject: str, body: str, headers: dict[str, str]) -> str:
        self.sent.append((to, subject, body, headers))
        return "<m@test>"


class FakePublisher:
    def __init__(self) -> None:
        self.pushed: list[tuple[str, dict[str, Any]]] = []

    async def publish(self, user_ref: str, data: dict[str, Any]) -> None:
        self.pushed.append((user_ref, data))


_n = 0


def _id() -> str:
    global _n
    _n += 1
    return f"01EVT{_n:021d}"


def req(kind: str, key: str, due: datetime, payload: dict[str, Any] | None = None, cancel: bool = False) -> Delivery:
    body: dict[str, Any] = {
        "userRef": USER,
        "kind": kind,
        "dedupeKey": key,
        "dueAt": iso(due),
        "payload": payload or {},
    }
    if cancel:
        body["cancel"] = True
    return Delivery("1-0", "notify.requested", Envelope(_id(), "notify.requested", 1, "x", "core", USER, body))


def user_event(verified: bool) -> Delivery:
    p = {
        "userRef": USER,
        "email": "nina@example.com",
        "emailVerified": verified,
        "timeZone": "Europe/Amsterdam",
        "locale": "en",
        "tierCode": "free",
        "storageQuotaBytes": 1,
    }
    return Delivery("1-0", "user.updated", Envelope(_id(), "user.updated", 1, "x", "core", USER, p))


def test_channel_rules() -> None:
    assert kinds.channels_for({}, "exam.eve") == [], "everything is opt-in"
    assert kinds.channels_for({}, "course.shared") == ["InApp"], "needs an answer: always in-app"
    assert kinds.channels_for({("flash.due", "Email"): True}, "flash.due") == ["Email"]
    assert kinds.channels_for({("nope", "InApp"): True}, "nope") == []
    t = templates.inbox("exam.eve", {"courseCode": "BSA", "startsAt": "2026-11-03T08:00:00.000Z", "format": "MC, 2 h"})
    assert t.title == "Tomorrow: BSA exam" and "Tue 03 Nov, 09:00" in t.body, t
    e = templates.render("flash.due", {"dueCount": 3}, "http://u", app_url="http://crux.test")
    assert e.subject == "3 flashcards due today" and "http://crux.test/flash" in e.body and "http://u" in e.body


async def test_inbox_push_email_reschedule_cancel_unsubscribe() -> None:
    async with migrated_db(SERVICE_DIR, "notify") as sessions:
        c, h = Contracts(CONTRACTS), NotifyRequestedHandler()
        users = UserEventsHandler(SECRET)
        assert await process(sessions, c, users, user_event(verified=False)) == Outcome.APPLIED
        mailer, pub = FakeMailer(), FakePublisher()
        past = utcnow() - timedelta(minutes=1)

        # Nothing enabled: not stored. course.shared: always in-app.
        await process(sessions, c, h, req("exam.eve", "exam.eve:D1", past))
        await process(
            sessions,
            c,
            h,
            req(
                "course.shared", "course.shared:C1", past, {"inviterName": "Sam", "courseName": "BSA", "role": "Viewer"}
            ),
        )
        async with sessions() as s:
            assert [r.Kind for r in await s.execute(select(notification))] == ["course.shared"]

        # Turn on exam.eve in-app + email, flash.due email.
        with pytest.raises(ValidationFailed):
            await svc.put_preferences(
                sessions,
                ME,
                svc.PreferencesInput(preferences=[svc.PreferenceChange(kind="nope", channel="InApp", enabled=True)]),
            )
        view = await svc.put_preferences(
            sessions,
            ME,
            svc.PreferencesInput(
                preferences=[
                    svc.PreferenceChange(kind="exam.eve", channel="InApp", enabled=True),
                    svc.PreferenceChange(kind="exam.eve", channel="Email", enabled=True),
                    svc.PreferenceChange(kind="flash.due", channel="Email", enabled=True),
                ]
            ),
        )
        eve = next(k for k in view.kinds if k.kind == "exam.eve")
        assert (eve.in_app, eve.email) == (True, True)
        assert next(k for k in view.kinds if k.kind == "course.shared").in_app_always

        future = utcnow() + timedelta(hours=5)
        await process(sessions, c, h, req("exam.eve", "exam.eve:D2", future, {"courseCode": "BSA"}))
        assert await dispatch_once(sessions, mailer, CONFIG, pub) == 1, "only the course share is due"
        assert pub.pushed[0][0] == USER and pub.pushed[0][1]["kind"] == "course.shared"

        # Re-request with the same key before delivery = reschedule (no double).
        await process(sessions, c, h, req("exam.eve", "exam.eve:D2", past, {"courseCode": "BSA", "title": "Final"}))
        async with sessions() as s:
            rows = list(await s.execute(select(notification).where(notification.c.DedupeKey == "exam.eve:D2")))
        assert len(rows) == 1 and rows[0].Payload["title"] == "Final"
        await dispatch_once(sessions, mailer, CONFIG, pub)
        assert [p[1]["title"] for p in pub.pushed][-1] == "Tomorrow: BSA Final"
        # Email skipped: the address isn't verified.
        async with sessions() as s:
            statuses = {(r.Channel, r.Status, r.LastError) for r in await s.execute(select(delivery))}
        assert ("Email", "Skipped", "unverified") in statuses
        # After delivery the same key again is ignored.
        await process(sessions, c, h, req("exam.eve", "exam.eve:D2", past))
        assert await dispatch_once(sessions, mailer, CONFIG, pub) == 0

        # Verified: flash.due email goes out with one-click unsubscribe.
        await process(sessions, c, users, user_event(verified=True))
        await process(sessions, c, h, req("flash.due", "flash.due:2026-10-23", past, {"dueCount": 4}))
        assert await dispatch_once(sessions, mailer, CONFIG, pub) == 1
        to, subject, body, headers = mailer.sent[0]
        assert (to, subject) == ("nina@example.com", "4 flashcards due today")
        token = unsubscribe.token_for(SECRET, USER)
        assert (
            headers["List-Unsubscribe"] == f"<http://crux.test/api/v1/notify/unsubscribe?token={token}&kind=flash.due>"
        )
        assert headers["List-Unsubscribe-Post"] == "List-Unsubscribe=One-Click" and token in body

        # Cancel by key: gone from the inbox, never sent.
        await process(sessions, c, h, req("exam.eve", "exam.eve:D3", future, {"courseCode": "RK"}))
        await process(sessions, c, h, req("exam.eve", "exam.eve:D3", future, cancel=True))
        async with sessions() as s, s.begin():
            await s.execute(delivery.update().where(delivery.c.Status == "Pending").values(NextAttemptAt=past))
        assert await dispatch_once(sessions, mailer, CONFIG, pub) == 0

        # Inbox: newest first, read one, read all.
        box = await svc.inbox(sessions, ME, unread_only=False, limit=1, cursor=None)
        assert box.unread == 2 and box.items[0].kind == "exam.eve" and box.next_cursor
        older = await svc.inbox(sessions, ME, unread_only=False, limit=10, cursor=box.next_cursor)
        assert [i.kind for i in older.items] == ["course.shared"] and older.next_cursor is None
        assert await svc.mark_read(sessions, ME, box.items[0].id) == 1
        assert await svc.mark_read(sessions, ME, None) == 0
        with pytest.raises(NotFound):
            await svc.mark_read(
                sessions, Principal(user_ref="01TEST00000000000000000002", role="User", tier=""), box.items[0].id
            )

        # Unsubscribe from one kind, then from everything; bounces suppress too.
        assert await svc.unsubscribe_from(sessions, token, "flash.due") == '"Flashcards due" emails'
        assert not next(k for k in (await svc.get_preferences(sessions, ME)).kinds if k.kind == "flash.due").email
        with pytest.raises(NotFound):
            await svc.unsubscribe_from(sessions, "wrong", "flash.due")
        await svc.unsubscribe_from(sessions, token, "all")
        assert (await svc.get_preferences(sessions, ME)).email.suppressed
        await process(sessions, c, h, req("exam.eve", "exam.eve:D4", past, {"courseCode": "BSA"}))
        await dispatch_once(sessions, mailer, CONFIG, pub)
        async with sessions() as s:
            assert ("Email", "Skipped", "suppressed") in {
                (r.Channel, r.Status, r.LastError) for r in await s.execute(select(delivery))
            }
        n = await svc.record_bounces(
            sessions,
            svc.BounceBatch(
                events=[
                    svc.BounceEvent(type="bounce", email="Bob@Example.com"),
                    svc.BounceEvent(type="open", email="x@y.z"),
                ]
            ),
        )
        assert n == 1
        async with sessions() as s:
            assert await recipients.is_suppressed(s, "bob@example.com")
        assert len(mailer.sent) == 1


async def test_export_and_user_deletion() -> None:
    from app.repo.preferences import preference
    from app.service import account as lifecycle
    from crux_common.messaging.tables import outbox

    async with migrated_db(SERVICE_DIR, "notify") as sessions:
        c = Contracts(CONTRACTS)
        await process(sessions, c, UserEventsHandler(SECRET), user_event(verified=True))
        await svc.put_preferences(
            sessions,
            ME,
            svc.PreferencesInput(preferences=[svc.PreferenceChange(kind="exam.eve", channel="Email", enabled=True)]),
        )
        await process(
            sessions,
            c,
            NotifyRequestedHandler(),
            req("course.shared", "course.shared:X", utcnow(), {"courseName": "BSA"}),
        )
        async with sessions() as s:
            out = await lifecycle.export(s, USER)
        assert (
            out["recipient"]["email"] == "nina@example.com"
            and len(out["preferences"]) == 1
            and len(out["notifications"]) == 1
        )
        d = Delivery("1-0", "user.deleted", Envelope(_id(), "user.deleted", 1, "x", "core", USER, {"userRef": USER}))
        assert await process(sessions, c, lifecycle.UserDeletedHandler(c), d) == Outcome.APPLIED
        async with sessions() as s:
            assert await recipients.get(s, USER) is None
            assert (await s.execute(select(preference))).all() == [] and (
                await s.execute(select(notification))
            ).all() == []
            assert [r.Topic for r in await s.execute(select(outbox.c.Topic))] == ["user.purged"]
