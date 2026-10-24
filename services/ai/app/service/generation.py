"""Question generation (spec 7.5): `POST /ai/generate` queues a GenerationJob; the worker runs it,
validates every question and has core create the valid ones (ai never writes catalog rows). The
nightly top-up queues jobs at 03:00 user-local for users or courses with NightlyGeneration on."""

import json
import logging
from datetime import timedelta
from typing import Any
from urllib.parse import quote
from zoneinfo import ZoneInfo

from pydantic import Field
from sqlalchemy import select, update
from sqlalchemy.dialects.mysql import insert

from app.clients.core import CoreError
from app.clients.llm import LlmError
from app.domain import generation as rules
from app.repo.tables import course_setting, generation_job, settings
from app.service import runtime
from app.service.account import Camel
from app.service.deps import Deps
from crux_common.auth import Principal
from crux_common.db import transaction
from crux_common.errors import AiBudgetExceeded, AiDisabled, NotFound
from crux_common.ids import new_id
from crux_common.messaging.outbox import emit
from crux_common.timeutil import iso, utcnow

log = logging.getLogger("crux.ai.generation")
PROMPT_VERSION = "gen-1"
NIGHTLY_HOUR = 3


class GenerateInput(Camel):
    course_id: str
    component_id: str
    concept_id: str | None = None
    n: int = Field(default=10, ge=1, le=rules.MAX_N)
    seed_question_id: str | None = None


class JobView(Camel):
    id: str
    course_id: str
    #: Queued, Running, Done, Failed, Blocked
    status: str
    triggered_by: str
    request: dict[str, Any]
    created_count: int
    rejected_count: int
    created_at: str | None
    finished_at: str | None


def _view(j: Any) -> JobView:
    return JobView(
        id=j.PublicId,
        course_id=j.CourseRef,
        status=j.Status,
        triggered_by=j.TriggeredBy,
        request=j.Request,
        created_count=j.CreatedCount,
        rejected_count=j.RejectedCount,
        created_at=iso(j.CreatedAt) if j.CreatedAt else None,
        finished_at=iso(j.FinishedAt) if j.FinishedAt else None,
    )


async def _queue(deps: Deps, user_ref: str, course_ref: str, request: dict[str, Any], triggered_by: str) -> JobView:
    async with transaction(deps.sessions) as s:
        public_id = new_id()
        await s.execute(
            insert(generation_job).values(
                PublicId=public_id,
                UserRef=user_ref,
                CourseRef=course_ref,
                TriggeredBy=triggered_by,
                Request=request,
                Status="Queued",
                CreatedCount=0,
                RejectedCount=0,
                CreatedAt=utcnow(),
            )
        )
        j = (await s.execute(select(generation_job).where(generation_job.c.PublicId == public_id))).one()
        return _view(j)


async def start(deps: Deps, user: Principal, inp: GenerateInput) -> JobView:
    access = await deps.core.course_access(user.user_ref, inp.course_id)
    if access is None or access.role is None:
        raise NotFound()
    await runtime.check(deps, user.user_ref, "Generation", inp.course_id)
    request = {
        "componentRef": inp.component_id,
        "conceptRef": inp.concept_id,
        "n": inp.n,
        "seedQuestionRef": inp.seed_question_id,
    }
    return await _queue(deps, user.user_ref, inp.course_id, request, "Button")


async def get(deps: Deps, user: Principal, job_ref: str) -> JobView:
    async with deps.sessions() as s:
        j = (
            await s.execute(
                select(generation_job).where(
                    generation_job.c.PublicId == job_ref, generation_job.c.UserRef == user.user_ref
                )
            )
        ).first()
    if j is None:
        raise NotFound()
    return _view(j)


async def _finish(
    deps: Deps, job_id: int, status: str, created: int = 0, rejected: int = 0, run_id: int | None = None
) -> None:
    async with transaction(deps.sessions) as s:
        await s.execute(
            update(generation_job)
            .where(generation_job.c.Id == job_id)
            .values(Status=status, CreatedCount=created, RejectedCount=rejected, AiRunId=run_id, FinishedAt=utcnow())
        )


async def run_one(deps: Deps) -> bool:
    """The queue worker: one Queued job. False when there was none."""
    async with transaction(deps.sessions) as s:
        j = (
            await s.execute(
                select(generation_job)
                .where(generation_job.c.Status == "Queued")
                .order_by(generation_job.c.Id)
                .limit(1)
                .with_for_update(skip_locked=True)
            )
        ).first()
        if j is None:
            return False
        await s.execute(update(generation_job).where(generation_job.c.Id == j.Id).values(Status="Running"))
    req = j.Request
    try:
        g = await runtime.check(deps, j.UserRef, "Generation", j.CourseRef)
    except AiDisabled:
        await _finish(deps, j.Id, "Blocked")
        return True
    params = f"user={j.UserRef}&course={j.CourseRef}&component={quote(req['componentRef'])}"
    if req.get("seedQuestionRef"):
        params += f"&seed={quote(req['seedQuestionRef'])}"
    try:
        ctx = await deps.core.call("GET", f"/internal/v1/ai/generation-context?{params}", j.UserRef)
    except CoreError:
        await _finish(deps, j.Id, "Failed")
        return True
    existing: list[str] = list(ctx.get("existingPrompts") or [])

    def block(tag: str, value: Any) -> str:
        # Data can't close its own block.
        return f"<{tag}>\n" + json.dumps(value, ensure_ascii=False).replace("</", "<\\/") + f"\n</{tag}>"

    parts = [block("course", {"course": ctx.get("course"), "lecture": ctx.get("component")})]
    if ctx.get("seed"):
        parts.append(block("seed", ctx["seed"]))
    parts.append(block("existing", [p[:200] for p in existing[:60]]))
    parts.append(f"Write {req['n']} new questions for this lecture.")
    user_msg = "\n\n".join(parts)
    try:
        run_id = await runtime.start_run(
            deps,
            g,
            prompt_version=PROMPT_VERSION,
            estimate_input_tokens=len(user_msg) // 4 + 600,
            max_output_tokens=600 * req["n"],
        )
    except AiBudgetExceeded:
        await _finish(deps, j.Id, "Blocked")
        return True
    provider, key = runtime.provider(deps, g)
    try:
        raw, used = await provider.complete_json(
            key,
            g.model.id,
            rules.SYSTEM,
            [{"role": "user", "content": user_msg}],
            rules.SCHEMA,
            min(16000, 600 * req["n"] + 500),
        )
    except LlmError as e:
        await runtime.finish_run(deps, run_id, None, error=str(e))
        await _finish(deps, j.Id, "Failed", run_id=run_id)
        return True
    await runtime.finish_run(deps, run_id, used)
    accepted: list[dict[str, Any]] = []
    rejected = 0
    for q in (raw.get("questions") or [])[: req["n"]]:
        if rules.check(q, existing) is None:
            accepted.append({"type": q["type"], "content": rules.to_content(q)})
            existing.append(q["prompt"])
        else:
            rejected += 1
    created = 0
    if accepted:
        try:
            result = await deps.core.call(
                "POST",
                "/internal/v1/questions/generated",
                j.UserRef,
                {
                    "user": j.UserRef,
                    "course": j.CourseRef,
                    "component": req["componentRef"],
                    "concept": req.get("conceptRef"),
                    "seedQuestion": req.get("seedQuestionRef"),
                    "autoApprove": bool(g.settings.AutoApproveGenerated),
                    "questions": accepted,
                },
            )
            created = len(result.get("created", []))
            rejected += len(result.get("rejected", []))
        except CoreError as e:
            log.warning("core refused generated questions", extra={"status": e.status})
            await _finish(deps, j.Id, "Failed", 0, rejected + len(accepted), run_id)
            return True
    await _finish(deps, j.Id, "Done", created, rejected, run_id)
    if created:
        async with transaction(deps.sessions) as s:
            await emit(
                s,
                deps.contracts,
                "notify.requested",
                j.UserRef,
                {
                    "userRef": j.UserRef,
                    "kind": "generation.done",
                    "dedupeKey": f"generation.done:{j.PublicId}",
                    "dueAt": iso(utcnow()),
                    "payload": {"courseId": j.CourseRef, "count": created, "jobId": j.PublicId},
                },
            )
    return True


async def nightly(deps: Deps) -> int:
    """Hourly: users whose local time is 03:00 and who (or whose courses) have NightlyGeneration on."""
    now = utcnow()
    async with deps.sessions() as s:
        users = {
            r.UserRef
            for r in await s.execute(
                select(settings.c.UserRef).where(settings.c.Enabled.is_(True), settings.c.NightlyGeneration.is_(True))
            )
        }
        course_on: dict[str, set[str]] = {}
        course_off: dict[str, set[str]] = {}
        for r in await s.execute(select(course_setting).where(course_setting.c.NightlyGeneration.is_not(None))):
            (course_on if r.NightlyGeneration else course_off).setdefault(r.UserRef, set()).add(r.CourseRef)
        recent = {
            r.UserRef
            for r in await s.execute(
                select(generation_job.c.UserRef).where(
                    generation_job.c.TriggeredBy == "Nightly", generation_job.c.CreatedAt > now - timedelta(hours=20)
                )
            )
        }
    queued = 0
    for user_ref in (users | set(course_on)) - recent:
        try:
            plan = await deps.core.call("GET", f"/internal/v1/ai/topup-plan?user={user_ref}", user_ref)
        except CoreError:
            continue
        try:
            local_hour = (
                now.replace(tzinfo=ZoneInfo("UTC"))
                .astimezone(ZoneInfo(plan.get("timeZone") or "Europe/Amsterdam"))
                .hour
            )
        except Exception:
            local_hour = now.hour
        if local_hour != NIGHTLY_HOUR:
            continue
        courses = [
            c
            for c in plan.get("courses", [])
            if (user_ref in users and c["courseId"] not in course_off.get(user_ref, set()))
            or c["courseId"] in course_on.get(user_ref, set())
        ]
        if (await runtime.decide(deps, user_ref, "Generation"))[0] is not None:
            continue
        for course_ref, component_ref, n in rules.topup_needs(courses):
            await _queue(
                deps,
                user_ref,
                course_ref,
                {"componentRef": component_ref, "n": n, "conceptRef": None, "seedQuestionRef": None},
                "Nightly",
            )
            queued += 1
    return queued
