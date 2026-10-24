"""Delivery rules (spec 8). Pure: `now` is passed in."""

from datetime import datetime, timedelta

#: Retries with backoff: 1 min, 10 min, 1 h; then Failed.
BACKOFF = (timedelta(minutes=1), timedelta(minutes=10), timedelta(hours=1))

#: Account emails: sent whatever the preferences and to unverified addresses, because they are
#: how an address gets verified or an account recovered (OPEN-QUESTIONS.md Q-auth-4).
TRANSACTIONAL_KINDS = frozenset(
    {
        "auth.verify_email",
        "auth.password_reset",
        # Invite links (D2-1, D2-9): the recipient has no account, so preferences can't apply.
        "invite.created",
        "invite.course_shared",
    }
)


def next_attempt(attempts_so_far: int, now: datetime) -> datetime | None:
    """When to retry after the `attempts_so_far`-th failure; None = give up (Failed)."""
    if attempts_so_far < 1 or attempts_so_far > len(BACKOFF):
        return None
    return now + BACKOFF[attempts_so_far - 1]


def is_transactional(kind: str, flagged: bool) -> bool:
    """Only known account-email kinds may skip preferences, even if a producer sets the flag."""
    return flagged and kind in TRANSACTIONAL_KINDS
