"""Dev only: the fake provider's JSON answers, shaped like a real model's, so a dev stack (and the
web's Playwright suite) can review an import, accept generated questions and see a grade without a
paid key. The answer depends only on the schema asked for and the text sent, so runs repeat."""

import json
import re
from datetime import UTC, datetime, timedelta
from typing import Any

from app.clients.llm import Message

_FILE = re.compile(r'<file ref="([^"]+)">')
_LECTURE = re.compile(r"^(?:lecture|week|chapter)\s+(\d{1,2})\s*[:.-]\s*(.{3,80})$", re.I | re.M)
_PROMPT = re.compile(r'"prompt":\s*"([^"]{1,200})"')
#: Distinct enough that generation's near-duplicate check (trigram similarity > 0.9) keeps them.
_STEMS = [
    "Which assumption does this method rely on most?",
    "What happens to the estimate when the sample doubles?",
    "Which of these is a valid interpretation of the result?",
    "Where does the usual shortcut break down?",
    "Which quantity stays the same under a change of units?",
    "What is the first step when the data are skewed?",
    "Which conclusion does the test actually support?",
    "What does a wider interval tell you?",
]


def demo_json(schema: dict[str, Any], messages: list[Message], system: str) -> dict[str, Any]:
    text = messages[-1]["content"] if messages else ""
    props = schema.get("properties") or {}
    if "proposals" in props:
        return _import(text)
    if "questions" in props:
        return _generate(text)
    if "outcome" in props:
        return {
            "outcome": "Partial",
            "grade": 0.5,
            "feedback": "Partly there: the idea is right, but the answer stops short of the conclusion.",
            "slips": [{"type": "Concept", "description": "Stopped before stating the conclusion"}],
        }
    return {}


def _prop(kind: str, key: str, payload: dict[str, Any], parent: str, ref: str, conf: str = "High") -> dict[str, Any]:
    return {
        "kind": kind,
        "tempKey": key,
        "parentTempKey": parent,
        "payloadJson": json.dumps(payload),
        "sourceFileRef": ref,
        "sourcePage": 1,
        "confidence": conf,
        "rationale": "Demo answer from the dev fake provider." if conf != "High" else "",
    }


def _import(text: str) -> dict[str, Any]:
    refs = _FILE.findall(text)
    ref = refs[0] if refs else ""
    if '"part": 1' not in text and "<proposed>" in text:
        return {"files": [], "proposals": []}  # later parts add nothing
    found = [(int(n), name.strip()) for n, name in _LECTURE.findall(text)][:6]
    lectures = found or [(1, "Introduction"), (2, "Core methods")]
    exam = (datetime.now(UTC) + timedelta(days=60)).replace(hour=9, minute=0, second=0, microsecond=0)
    out = [
        _prop("Course", "c", {"name": "Demo import", "code": "DEMOIMP", "term": str(exam.year)}, "", ref),
        _prop(
            "Deadline",
            "d",
            {"type": "Exam", "title": "Final exam", "startsAt": f"{exam:%Y-%m-%dT%H:%M:%SZ}", "durationMin": 120},
            "",
            ref,
        ),
        _prop("ExamAids", "aids", {"calculator": True, "cheatSheet": False, "other": ""}, "", ref, "Medium"),
    ]
    for n, name in lectures:
        key = f"l{n}"
        out.append(_prop("Component", key, {"code": f"L{n}", "name": name, "estHours": 3}, "c", ref))
        out.append(_prop("Flashcard", f"f{n}", {"front": f"Key idea of {name}?", "back": "See the slides."}, key, ref))
    first = f"l{lectures[0][0]}"
    question = {
        "type": "MC",
        "componentTempKey": first,
        "content": {
            "prompt": f"Which statement about {lectures[0][1]} is true?",
            "explanation": "Demo question.",
            "keyReliability": "Derived",
            "options": [{"text": "The first one", "isCorrect": True}, {"text": "The second one", "isCorrect": False}],
        },
    }
    out.append(_prop("Question", "q1", question, first, ref, "Low"))
    return {"files": [{"fileRef": r, "kind": "Slides"} for r in dict.fromkeys(refs)], "proposals": out}


def _generate(text: str) -> dict[str, Any]:
    # New prompts each run: number them after what the bank already has.
    n = len(_PROMPT.findall(text))
    return {
        "questions": [
            {
                "type": "MC",
                "prompt": _STEMS[(n + i) % len(_STEMS)],
                "explanation": "Demo question from the dev fake provider.",
                "options": [
                    {"text": f"Option A{n + i}", "isCorrect": True, "distractorNote": ""},
                    {"text": f"Option B{n + i}", "isCorrect": False, "distractorNote": "a common slip"},
                    {"text": f"Option C{n + i}", "isCorrect": False, "distractorNote": "a misread"},
                ],
                "numeric": {"value": 0, "tolerance": 0, "unit": ""},
                "modelAnswer": "",
            }
            for i in range(2)
        ]
    }
