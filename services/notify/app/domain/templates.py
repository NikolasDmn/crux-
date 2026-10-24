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
    elif kind == "invite.created":
        inviter = payload.get("inviterName") or "Someone"
        email = Email(
            f"{inviter} invited you to Crux",
            f"Hi,\n\n{inviter} invited you to Crux, a study and exam-prep app.\n\n"
            f"Create your account here:\n\n{link}\n\n"
            "The invite works only for this email address. If you weren't expecting it, ignore this email.\n",
        )
    elif kind == "invite.course_shared":
        inviter = payload.get("inviterName") or "Someone"
        course = payload.get("courseName") or "a course"
        email = Email(
            f'{inviter} shared "{course}" with you on Crux',
            f'Hi,\n\n{inviter} shared the course "{course}" with you on Crux.\n\n'
            f"Create your account to open it:\n\n{link}\n\n"
            "The course will be in your list as soon as you've signed up. "
            "If you weren't expecting this, ignore this email.\n",
        )
    else:
        raise ValueError(f"no email template for {kind}")
    if unsubscribe_url:
        email = Email(email.subject, email.body + f"\nStop these emails: {unsubscribe_url}\n")
    return email
