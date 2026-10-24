"""Account export and deletion for ai (spec 11, 9.4). Key material never leaves: keys are listed
by provider, label and last 4 characters only."""

from datetime import datetime
from decimal import Decimal
from typing import Any

from sqlalchemy import delete, select
from sqlalchemy.ext.asyncio import AsyncSession

from app.repo.tables import (
    citation,
    consent_log,
    conversation,
    course_import,
    course_setting,
    generation_job,
    grade,
    message,
    proposal,
    provider_key,
    run,
    settings,
    tutor,
    usage_monthly,
)
from app.service.deps import Deps
from crux_common.messaging.bus import Delivery
from crux_common.messaging.outbox import emit
from crux_common.timeutil import iso


def _row(r: Any, skip: tuple[str, ...] = ()) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for k, v in r._mapping.items():
        if k in skip:
            continue
        out[k] = iso(v) if isinstance(v, datetime) else float(v) if isinstance(v, Decimal) else v
    return out


async def export(s: AsyncSession, user_ref: str) -> dict[str, Any]:
    async def rows(table: Any, *where: Any, skip: tuple[str, ...] = ()) -> list[dict[str, Any]]:
        return [_row(r, skip) for r in await s.execute(select(table).where(*where))]

    convs = (await s.execute(select(conversation).where(conversation.c.UserRef == user_ref))).all()
    conv_ids = [c.Id for c in convs]
    msgs = (await s.execute(select(message).where(message.c.ConversationId.in_(conv_ids)))).all() if conv_ids else []
    msg_ids = [m.Id for m in msgs]
    imports = (await s.execute(select(course_import).where(course_import.c.UserRef == user_ref))).all()
    import_ids = [i.Id for i in imports]
    return {
        "settings": await rows(settings, settings.c.UserRef == user_ref),
        "keys": await rows(provider_key, provider_key.c.UserRef == user_ref, skip=("Ciphertext", "KmsKeyId")),
        "consentLog": await rows(consent_log, consent_log.c.UserRef == user_ref),
        "tutors": await rows(tutor, tutor.c.OwnerUserRef == user_ref),
        "courseSettings": await rows(course_setting, course_setting.c.UserRef == user_ref),
        "usage": await rows(usage_monthly, usage_monthly.c.UserRef == user_ref),
        "runs": await rows(run, run.c.UserRef == user_ref, skip=("TranscriptRef",)),
        "conversations": [_row(c) for c in convs],
        "messages": [_row(m) for m in msgs],
        "citations": await rows(citation, citation.c.MessageId.in_(msg_ids)) if msg_ids else [],
        "grades": await rows(grade, grade.c.UserRef == user_ref),
        "imports": [_row(i) for i in imports],
        "proposals": await rows(proposal, proposal.c.ImportId.in_(import_ids)) if import_ids else [],
        "generationJobs": await rows(generation_job, generation_job.c.UserRef == user_ref),
    }


class UserDeletedHandler:
    """`user.deleted`: keys, transcripts, conversations, runs, grades, imports, tutors, settings
    go; then `user.purged`."""

    topics = ("user.deleted",)

    def __init__(self, deps: Deps) -> None:
        self._deps = deps

    async def handle(self, s: AsyncSession, d: Delivery) -> None:
        user_ref = d.envelope.payload["userRef"]
        if self._deps.store is not None:
            for r in await s.execute(
                select(run.c.TranscriptRef).where(run.c.UserRef == user_ref, run.c.TranscriptRef.is_not(None))
            ):
                await self._deps.store.delete(r.TranscriptRef)
        # Children first where there is no cascade from the user (refs aren't foreign keys).
        await s.execute(delete(grade).where(grade.c.UserRef == user_ref))
        await s.execute(delete(conversation).where(conversation.c.UserRef == user_ref))
        await s.execute(delete(course_import).where(course_import.c.UserRef == user_ref))
        await s.execute(delete(generation_job).where(generation_job.c.UserRef == user_ref))
        await s.execute(delete(run).where(run.c.UserRef == user_ref))
        await s.execute(delete(course_setting).where(course_setting.c.UserRef == user_ref))
        await s.execute(delete(usage_monthly).where(usage_monthly.c.UserRef == user_ref))
        await s.execute(delete(consent_log).where(consent_log.c.UserRef == user_ref))
        await s.execute(delete(settings).where(settings.c.UserRef == user_ref))
        await s.execute(delete(provider_key).where(provider_key.c.UserRef == user_ref))
        await s.execute(delete(tutor).where(tutor.c.OwnerUserRef == user_ref))
        await emit(s, self._deps.contracts, "user.purged", user_ref, {"userRef": user_ref, "service": "ai"})
