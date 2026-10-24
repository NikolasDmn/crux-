"""Text per kind: emails (plain text; the design thread can add HTML later) and inbox items. Pure."""

from dataclasses import dataclass
from datetime import datetime
from typing import Any
from zoneinfo import ZoneInfo

from app.domain.kinds import KINDS


@dataclass(frozen=True)
class Email:
    subject: str
    body: str


@dataclass(frozen=True)
class InboxText:
    title: str
    body: str
    #: A path in the web app (the frontend routes, frontend-spec 3).
    link: str | None


def _when(payload: dict[str, Any], tz: str) -> str:
    raw = payload.get("startsAt")
    if not raw:
        return ""
    try:
        t = datetime.fromisoformat(str(raw).replace("Z", "+00:00")).astimezone(ZoneInfo(tz))
    except (ValueError, KeyError):
        return str(raw)
    return t.strftime("%a %d %b, %H:%M")


def _exam_name(p: dict[str, Any]) -> str:
    code = p.get("courseCode") or "your course"
    title = p.get("title")
    kind = "resit" if p.get("type") == "Resit" else "exam"
    return f"{code} {title}" if title else f"{code} {kind}"


def inbox(kind: str, payload: dict[str, Any], tz: str = "Europe/Amsterdam") -> InboxText:
    p = payload
    course = p.get("courseId")
    course_link = f"/c/{course}" if course else "/"
    if kind == "exam.eve":
        details = ", ".join(x for x in (_when(p, tz), p.get("format")) if x)
        return InboxText(
            f"Tomorrow: {_exam_name(p)}",
            f"{details}. Sleep well, a short warm-up in the morning is enough.",
            course_link,
        )
    if kind == "exam.checkin":
        return InboxText(
            f"How did {_exam_name(p)} go?",
            "Record how it went (and the grade when you have it). A fail offers a resit plan.",
            f"{course_link}/exams?checkin={p.get('deadlineId', '')}" if course else "/",
        )
    if kind == "flash.due":
        n = int(p.get("dueCount", 0))
        return InboxText(f"{n} flashcard{'s' if n != 1 else ''} due today", "A few minutes keeps them fresh.", "/flash")
    if kind == "plan.today":
        items = p.get("items") or []
        lines = "; ".join(str(i.get("description", "")) for i in items[:5])
        return InboxText(f"Today's plan: {len(items)} item{'s' if len(items) != 1 else ''}", lines, "/plan")
    if kind == "mock.graded":
        return InboxText(
            f"{p.get('mockName', 'Your mock')} is graded",
            p.get("summary", "Open the review to see how you did."),
            f"/mocks/{p.get('mockId', '')}/review",
        )
    if kind == "import.ready":
        return InboxText(
            "Your import is ready",
            f"{p.get('count', 'Some')} questions are ready to review.",
            f"{course_link}/bank?review=import",
        )
    if kind == "generation.done":
        return InboxText(
            "New questions are ready",
            f"{p.get('count', 'Some')} generated questions are ready to review.",
            f"{course_link}/bank?review=generated",
        )
    if kind == "course.shared":
        who = p.get("inviterName") or "Someone"
        return InboxText(
            f'{who} shared "{p.get("courseName", "a course")}" with you',
            f"As {p.get('role', 'Viewer')}. Accept to add it to your courses.",
            "/courses?invitations=1",
        )
    if kind == "file.infected":
        return InboxText(
            f"Upload blocked: {p.get('fileName', 'a file')}",
            "It failed the virus scan and was removed. Nothing else was affected.",
            "/settings/storage",
        )
    if kind == "storage.purged":
        n = int(p.get("count", 1))
        return InboxText(
            f"{n} original file{'s' if n != 1 else ''} removed",
            "Their text, question images and thumbnails stay. Upload the same file again to bring one back.",
            "/settings/storage",
        )
    return InboxText(kind, "", None)


def render(
    kind: str, payload: dict[str, Any], unsubscribe_url: str | None, app_url: str = "", tz: str = "Europe/Amsterdam"
) -> Email:
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
        if kind not in KINDS:
            raise ValueError(f"no email template for {kind}")
        item = inbox(kind, payload, tz)
        target = f"{app_url}{item.link}" if item.link else app_url
        email = Email(item.title, f"Hi,\n\n{item.body}\n\nOpen Crux: {target}\n")
    if unsubscribe_url:
        email = Email(email.subject, email.body + f"\nStop these emails: {unsubscribe_url}\n")
    return email
