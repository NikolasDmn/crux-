"""Notification kinds and channel rules (spec 8). Pure.

Everything is opt-in: no NotificationPreference row = that kind/channel is off. Some kinds always
show in the inbox: a course shared with you and the exam check-in need an answer, and a blocked
(infected) upload must not go unnoticed.
"""

from collections.abc import Mapping
from dataclasses import dataclass

IN_APP = "InApp"
EMAIL = "Email"
CHANNELS = (IN_APP, EMAIL)


@dataclass(frozen=True)
class Kind:
    code: str
    label: str
    description: str


KINDS: dict[str, Kind] = {
    k.code: k
    for k in (
        Kind("exam.eve", "Night before an exam", "18:00 the day before, with the time, place and format."),
        Kind("exam.checkin", "Exam check-in", "17:00 on exam day: how did it go?"),
        Kind("flash.due", "Flashcards due", "08:00, only on days with cards due."),
        Kind("plan.today", "Today's plan", "07:30, what's planned for today."),
        Kind("mock.graded", "Mock graded", "When grading of a mock finishes."),
        Kind("import.ready", "Import ready", "When imported questions are ready to review."),
        Kind("generation.done", "Questions generated", "When generated questions are ready to review."),
        Kind("course.shared", "Course shared with you", "Someone shared a course; accept or decline."),
        Kind("file.infected", "Upload blocked", "An upload failed the virus scan and was removed."),
        Kind("storage.purged", "Originals removed", "The storage saver removed original files (their text stays)."),
    )
}

#: Shown in the inbox whatever the preferences (spec 8): they need an answer.
ALWAYS_IN_APP = frozenset({"course.shared", "exam.checkin", "file.infected"})

Prefs = Mapping[tuple[str, str], bool]


def is_known(kind: str) -> bool:
    return kind in KINDS


def enabled(prefs: Prefs, kind: str, channel: str) -> bool:
    if channel == IN_APP and kind in ALWAYS_IN_APP:
        return True
    return prefs.get((kind, channel), False)


def channels_for(prefs: Prefs, kind: str) -> list[str]:
    """The channels a new notification of `kind` is delivered on (empty = don't store it)."""
    if not is_known(kind):
        return []
    return [c for c in CHANNELS if enabled(prefs, kind, c)]
