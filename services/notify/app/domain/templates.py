"""Email text per kind. Plain text for now; the design thread can add HTML later."""

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class Email:
    subject: str
    body: str


def render(kind: str, payload: dict[str, Any], unsubscribe_url: str | None) -> Email:
    name = payload.get("firstName") or "there"
    link = payload.get("link", "")
    if kind == "auth.verify_email":
        email = Email(
            "Confirm your email for Crux",
            f"Hi {name},\n\nConfirm this address for your Crux account:\n\n{link}\n\n"
            "The link works for 48 hours. If you didn't sign up, ignore this email.\n",
        )
    elif kind == "auth.password_reset":
        email = Email(
            "Reset your Crux password",
            f"Hi {name},\n\nSet a new password here:\n\n{link}\n\n"
            "The link works for 1 hour and signs you out everywhere. "
            "If you didn't ask for this, ignore this email; your password stays the same.\n",
        )
    else:
        raise ValueError(f"no email template for {kind}")
    if unsubscribe_url:
        email = Email(email.subject, email.body + f"\nStop these emails: {unsubscribe_url}\n")
    return email
