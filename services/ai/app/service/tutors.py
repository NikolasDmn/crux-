"""Tutor templates (spec 7.3): three built-ins, custom ones from 5 personalities by 4 styles plus free
text, versioned (an edit inserts Version + 1; runs record the exact version)."""

import re
from typing import Any

from pydantic import Field
from sqlalchemy import select, update
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.ext.asyncio import AsyncSession

from app.repo.tables import course_setting, settings, tutor
from app.service.account import Camel
from app.service.deps import Deps
from crux_common.auth import Principal
from crux_common.db import transaction
from crux_common.errors import NotFound, ValidationFailed
from crux_common.ids import new_id
from crux_common.timeutil import iso

PERSONALITIES = {
    "Direct": "Be direct and brief. No pep talk.",
    "Warm": "Be warm and encouraging, and patient with mistakes.",
    "Socratic": "Lead with questions that get the student to the idea themselves.",
    "Playful": "Keep it light and friendly; a little humour is fine, never at the student's expense.",
    "Formal": "Be precise and formal, like an examiner.",
}
STYLES = {
    "HintFirst": "Give hints before answers; only solve when the student is stuck or asks for the solution.",
    "ExplainFirst": "Explain the idea from zero first, then show it on a small example.",
    "QuizMe": "Ask one exam-style question at a time and explain only what was wrong.",
    "WorkedExamples": "Teach through fully worked examples from the course materials.",
}
DEFAULT_TEMPLATE_KEY = "friendly-explainer"
MAX_TEXT = 2000
RULE_LIMITS: dict[str, tuple[type, int, int]] = {
    "approachCheckYesNoHint": (bool, 0, 1),
    "oneQuestionAtATime": (bool, 0, 1),
    "hintsBeforeSolution": (int, 0, 5),
    "maxWords": (int, 50, 1000),
}

#: Free text may shape the tutor, never the safety block (spec 7.3/7.4).
_UNSAFE = re.compile(
    r"ignore (all |the )?(previous|above|prior|safety)|system prompt|developer message|reveal (the )?(answer )?key"
    r"|hidden (answer|key)|disregard (the )?(rules|instructions)|you are no longer|jailbreak",
    re.IGNORECASE,
)


class TutorView(Camel):
    id: str
    key: str
    version: int
    name: str
    description: str
    personality: str
    teaching_style: str
    rules: dict[str, Any]
    built_in: bool
    created_at: str | None


def view(t: Any) -> TutorView:
    return TutorView(
        id=t.PublicId,
        key=t.TemplateKey,
        version=t.Version,
        name=t.Name,
        description=t.Description,
        personality=t.Personality,
        teaching_style=t.TeachingStyle,
        rules=t.Rules or {},
        built_in=t.OwnerUserRef is None,
        created_at=iso(t.CreatedAt) if t.CreatedAt else None,
    )


async def latest(s: AsyncSession, owner_ref: str | None, template_key: str) -> Any | None:
    owner = tutor.c.OwnerUserRef.is_(None) if owner_ref is None else tutor.c.OwnerUserRef == owner_ref
    stmt = (
        select(tutor)
        .where(owner, tutor.c.TemplateKey == template_key, tutor.c.IsActive.is_(True))
        .order_by(tutor.c.Version.desc())
        .limit(1)
    )
    return (await s.execute(stmt)).first()


async def list_tutors(deps: Deps, user: Principal) -> list[TutorView]:
    async with deps.sessions() as s:
        rows = await s.execute(
            select(tutor)
            .where(
                ((tutor.c.OwnerUserRef.is_(None)) | (tutor.c.OwnerUserRef == user.user_ref)), tutor.c.IsActive.is_(True)
            )
            .order_by(tutor.c.OwnerUserRef.is_(None).desc(), tutor.c.Id)
        )
        newest: dict[tuple[str | None, str], Any] = {}
        for t in rows:
            k = (t.OwnerUserRef, t.TemplateKey)
            if k not in newest or newest[k].Version < t.Version:
                newest[k] = t
        return [view(t) for t in newest.values()]


class TutorInput(Camel):
    name: str = Field(min_length=1, max_length=64)
    description: str = Field(default="", max_length=255)
    personality: str
    teaching_style: str
    text: str = Field(default="", max_length=MAX_TEXT)
    rules: dict[str, Any] = Field(default_factory=dict)


def _check(inp: TutorInput) -> dict[str, Any]:
    fields = []
    if inp.personality not in PERSONALITIES:
        fields.append({"field": "personality", "code": "invalid", "message": f"Use one of {', '.join(PERSONALITIES)}."})
    if inp.teaching_style not in STYLES:
        fields.append({"field": "teachingStyle", "code": "invalid", "message": f"Use one of {', '.join(STYLES)}."})
    if _UNSAFE.search(inp.text):
        fields.append(
            {
                "field": "text",
                "code": "unsafe",
                "message": "The text can shape the tutor, not override its safety rules.",
            }
        )
    rules: dict[str, Any] = {
        "approachCheckYesNoHint": False,
        "oneQuestionAtATime": True,
        "hintsBeforeSolution": 1,
        "maxWords": 300,
    }
    for k, v in inp.rules.items():
        limit = RULE_LIMITS.get(k)
        ok = limit is not None and (
            isinstance(v, bool)
            if limit[0] is bool
            else isinstance(v, int) and not isinstance(v, bool) and limit[1] <= v <= limit[2]
        )
        if not ok:
            fields.append({"field": f"rules.{k}", "code": "invalid", "message": "Unknown rule or value out of range."})
        else:
            rules[k] = v
    if fields:
        raise ValidationFailed(details={"fields": fields})
    return rules


def compose_prompt(personality: str, style: str, text: str) -> str:
    parts = [PERSONALITIES[personality], STYLES[style]]
    if text.strip():
        parts.append("The student asked for this: " + text.strip())
    return " ".join(parts)


async def create(deps: Deps, user: Principal, inp: TutorInput) -> TutorView:
    rules = _check(inp)
    async with transaction(deps.sessions) as s:
        public_id = new_id()
        await s.execute(
            insert(tutor).values(
                PublicId=public_id,
                TemplateKey=f"custom-{public_id.lower()}",
                Version=1,
                OwnerUserRef=user.user_ref,
                Name=inp.name,
                Description=inp.description or f"{inp.personality}, {inp.teaching_style}",
                Personality=inp.personality,
                TeachingStyle=inp.teaching_style,
                Rules=rules,
                PromptText=compose_prompt(inp.personality, inp.teaching_style, inp.text),
                IsActive=True,
            )
        )
        row = (await s.execute(select(tutor).where(tutor.c.PublicId == public_id))).first()
        return view(row)


async def edit(deps: Deps, user: Principal, tutor_ref: str, inp: TutorInput) -> TutorView:
    rules = _check(inp)
    async with transaction(deps.sessions) as s:
        cur = (
            await s.execute(select(tutor).where(tutor.c.PublicId == tutor_ref, tutor.c.OwnerUserRef == user.user_ref))
        ).first()
        if cur is None:
            raise NotFound()  # built-ins and other people's tutors
        top = await latest(s, user.user_ref, cur.TemplateKey)
        assert top is not None
        public_id = new_id()
        await s.execute(update(tutor).where(tutor.c.Id == top.Id).values(IsActive=False))
        await s.execute(
            insert(tutor).values(
                PublicId=public_id,
                TemplateKey=cur.TemplateKey,
                Version=top.Version + 1,
                OwnerUserRef=user.user_ref,
                Name=inp.name,
                Description=inp.description or f"{inp.personality}, {inp.teaching_style}",
                Personality=inp.personality,
                TeachingStyle=inp.teaching_style,
                Rules=rules,
                PromptText=compose_prompt(inp.personality, inp.teaching_style, inp.text),
                IsActive=True,
            )
        )
        row = (await s.execute(select(tutor).where(tutor.c.PublicId == public_id))).first()
        return view(row)


async def resolve(s: AsyncSession, user_ref: str, course_ref: str | None, conversation_tutor_id: int | None) -> Any:
    """The most specific choice wins: conversation, course, user default, built-in default. Always
    the newest active version of that template."""
    chosen_id = conversation_tutor_id
    if chosen_id is None and course_ref:
        cs = (
            await s.execute(
                select(course_setting.c.TutorTemplateId).where(
                    course_setting.c.UserRef == user_ref, course_setting.c.CourseRef == course_ref
                )
            )
        ).first()
        chosen_id = cs.TutorTemplateId if cs else None
    if chosen_id is None:
        st = (await s.execute(select(settings.c.DefaultTutorTemplateId).where(settings.c.UserRef == user_ref))).first()
        chosen_id = st.DefaultTutorTemplateId if st else None
    if chosen_id is not None:
        t = (await s.execute(select(tutor).where(tutor.c.Id == chosen_id))).first()
        if t is not None:
            newest = await latest(s, t.OwnerUserRef, t.TemplateKey)
            if newest is not None:
                return newest
    t = await latest(s, None, DEFAULT_TEMPLATE_KEY)
    assert t is not None
    return t
