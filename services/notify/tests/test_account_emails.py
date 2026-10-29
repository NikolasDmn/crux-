"""Account emails (verify, reset): notify.requested → queued delivery → dispatcher (spec 8)."""

from datetime import datetime, timedelta
from pathlib import Path

import pytest
from sqlalchemy import select

from app.domain import delivery as rules
from app.domain import templates
from app.repo.notifications import delivery
from app.service.requests import NotifyRequestedHandler, dispatch_once
from crux_common.messaging.bus import Delivery, Envelope
from crux_common.messaging.consumer import Outcome, process
from crux_common.messaging.contracts import Contracts
from crux_common.testing import migrated_db

SERVICE_DIR = Path(__file__).resolve().parents[1]
CONTRACTS = SERVICE_DIR.parents[1] / "contracts"
USER = "01TEST00000000000000000001"
NOW = datetime(2026, 10, 23, 12, 0, 0)


class Config:
    app_base_url = "http://crux.test"
    unsubscribe_secret = "test-secret"


CONFIG = Config()


def test_backoff_is_1m_10m_1h_then_failed() -> None:
    assert rules.next_attempt(1, NOW) == NOW + timedelta(minutes=1)
    assert rules.next_attempt(2, NOW) == NOW + timedelta(minutes=10)
    assert rules.next_attempt(3, NOW) == NOW + timedelta(hours=1)
    assert rules.next_attempt(4, NOW) is None


def test_only_known_account_kinds_skip_preferences() -> None:
    assert rules.is_transactional("auth.verify_email", True)
    assert not rules.is_transactional("auth.verify_email", False)
    assert not rules.is_transactional("mock.graded", True), "a producer can't make any kind transactional"


def test_templates_include_the_link() -> None:
    e = templates.render("auth.verify_email", {"firstName": "Nick", "link": "http://x/verify?token=t"}, None)
    assert "http://x/verify?token=t" in e.body and e.subject
    shared = templates.render(
        "invite.course_shared", {"inviterName": "Nick D", "courseName": "BSA", "link": "http://x/signup?invite=C"}, None
    )
    assert "Nick D" in shared.subject and "BSA" in shared.subject and "invite=C" in shared.body
    assert (
        "invite=C"
        in templates.render("invite.created", {"inviterName": "Nick D", "link": "http://x/signup?invite=C"}, None).body
    )
    assert rules.is_transactional("invite.course_shared", True)
    with pytest.raises(ValueError, match="no email template"):
        templates.render("nope", {}, None)


class FakeMailer:
    def __init__(self, fail_times: int = 0) -> None:
        self.sent: list[tuple[str, str, str]] = []
        self.fail_times = fail_times

    async def send(self, to: str, subject: str, body: str, headers: dict[str, str]) -> str:
        if self.fail_times:
            self.fail_times -= 1
            raise ConnectionError("smtp down")
        self.sent.append((to, subject, body))
        return "<msg@test>"


def request(event_id: str, kind: str, dedupe: str, transactional: bool = True) -> Delivery:
    payload = {
        "userRef": USER,
        "kind": kind,
        "dedupeKey": dedupe,
        "dueAt": "2020-01-01T00:00:00.000Z",
        "transactional": transactional,
        "payload": {"to": "a@b.nl", "firstName": "A", "link": "http://crux.test/verify?token=abc"},
    }
    return Delivery("1-0", "notify.requested", Envelope(event_id, "notify.requested", 1, "x", "core", USER, payload))


async def test_verify_email_is_sent_once_and_retried_on_failure() -> None:
    async with migrated_db(SERVICE_DIR, "notify") as sessions:
        h, c = NotifyRequestedHandler(), Contracts(CONTRACTS)
        assert (
            await process(sessions, c, h, request("01TEST0000000000000000000A", "auth.verify_email", "k1"))
            == Outcome.APPLIED
        )
        # Same dedupe key from a re-request: no second email.
        assert (
            await process(sessions, c, h, request("01TEST0000000000000000000B", "auth.verify_email", "k1"))
            == Outcome.APPLIED
        )

        flaky = FakeMailer(fail_times=1)
        assert await dispatch_once(sessions, flaky, CONFIG) == 0
        async with sessions() as s:
            row = (await s.execute(select(delivery))).one()
        assert (row.Status, row.Attempts) == ("Pending", 1)
        assert row.NextAttemptAt is not None and "smtp down" in row.LastError

        # Not due yet: nothing happens until the backoff passes.
        assert await dispatch_once(sessions, flaky, CONFIG) == 0
        async with sessions() as s, s.begin():
            await s.execute(delivery.update().values(NextAttemptAt=datetime(2020, 1, 1)))
        assert await dispatch_once(sessions, flaky, CONFIG) == 1
        assert flaky.sent[0][0] == "a@b.nl" and "token=abc" in flaky.sent[0][2]
        assert await dispatch_once(sessions, flaky, CONFIG) == 0, "sent once"


async def test_account_kinds_without_the_flag_are_not_sent() -> None:
    async with migrated_db(SERVICE_DIR, "notify") as sessions:
        h, c = NotifyRequestedHandler(), Contracts(CONTRACTS)
        await process(
            sessions, c, h, request("01TEST0000000000000000000C", "auth.verify_email", "k2", transactional=False)
        )
        mailer = FakeMailer()
        assert await dispatch_once(sessions, mailer, CONFIG) == 0
        assert mailer.sent == []


def test_mock_graded_links_to_the_mock_from_either_producer() -> None:
    """Core sends mockRef/name/score; the ai service sends mockId/summary. Both open the result."""
    core = templates.inbox("mock.graded", {"mockRef": "01MOCK", "name": "Dress rehearsal", "score": 7, "maxScore": 10})
    assert (core.title, core.link) == ("Dress rehearsal is graded", "/mock/01MOCK/result")
    assert "7 of 10" in core.body
    ai = templates.inbox("mock.graded", {"mockId": "01MOCK2", "summary": "AI suggested grades."})
    assert (ai.link, ai.body) == ("/mock/01MOCK2/result", "AI suggested grades.")
