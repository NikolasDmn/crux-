"""The gate before every model call (spec 7.1), and runs with reserve-then-settle budgeting (7.2)."""

from dataclasses import dataclass
from datetime import timedelta
from typing import Any, cast

from sqlalchemy import select, update
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.engine import CursorResult
from sqlalchemy.ext.asyncio import AsyncSession

from app.clients.llm import Provider, Usage
from app.domain import gate as rules
from app.domain import models
from app.repo.tables import course_setting, provider_key, run, run_context, settings, usage_monthly
from app.service.account import plaintext
from app.service.deps import Deps
from crux_common.db import transaction
from crux_common.errors import AiBudgetExceeded, AiDisabled
from crux_common.ids import new_id
from crux_common.timeutil import utcnow


@dataclass
class Gate:
    user_ref: str
    feature: str
    course_ref: str | None
    key: Any
    model: models.Model
    settings: Any

    @property
    def provider_name(self) -> str:
        return str(self.key.Provider)


async def decide(
    deps: Deps, user_ref: str, feature: str, course_ref: str | None = None, material_refs: list[str] | None = None
) -> tuple[str | None, Gate | None]:
    """(reason, gate): reason is None when allowed."""
    async with deps.sessions() as s:
        st = (await s.execute(select(settings).where(settings.c.UserRef == user_ref))).first()
        key = None
        if st is not None and st.DefaultProviderKeyId is not None:
            key = (await s.execute(select(provider_key).where(provider_key.c.Id == st.DefaultProviderKeyId))).first()
        cs = None
        if course_ref:
            cs = (
                await s.execute(
                    select(course_setting.c.AiEnabled).where(
                        course_setting.c.UserRef == user_ref, course_setting.c.CourseRef == course_ref
                    )
                )
            ).first()
    checks: list[rules.MaterialCheck] = []
    refs = material_refs or []
    if refs:
        rows = await deps.core.materials(user_ref, refs)
        found = {r["materialRef"]: r for r in rows}
        for ref in refs:
            r = found.get(ref)
            if r is None:
                checks.append(rules.MaterialCheck(False, True, None))
                continue
            clean = None
            if r.get("fileRef"):
                f = await deps.files.file(r["fileRef"])
                clean = f is not None and f.get("scanStatus") == "Clean"
            checks.append(rules.MaterialCheck(bool(r.get("aiAllowed")), bool(r.get("excluded")), clean))
    reason = rules.decide(
        bool(st and st.Enabled),
        bool(st and getattr(st, rules.ALLOW_COLUMN[feature])),
        key.Status if key else None,
        None if cs is None or cs.AiEnabled is None else bool(cs.AiEnabled),
        checks,
    )
    if reason is None and key is not None and key.Provider == "fake" and not deps.settings.fake_allowed:
        reason = "no_key"
    if reason is not None:
        return reason, None
    assert st is not None and key is not None
    model = (models.find(key.Provider, st.DefaultModel) if st.DefaultModel else None) or models.default_for(
        key.Provider
    )
    if model is None:
        return "no_key", None
    return None, Gate(user_ref, feature, course_ref, key, model, st)


async def check(
    deps: Deps, user_ref: str, feature: str, course_ref: str | None = None, material_refs: list[str] | None = None
) -> Gate:
    reason, g = await decide(deps, user_ref, feature, course_ref, material_refs)
    if reason is not None or g is None:
        raise AiDisabled(details={"reason": reason or "service_unavailable"})
    return g


def provider(deps: Deps, g: Gate) -> tuple[Provider, str]:
    """The adapter and the plaintext key (in memory for this call only)."""
    return deps.providers[g.provider_name], plaintext(deps, g.key)


async def start_run(
    deps: Deps,
    g: Gate,
    *,
    prompt_version: str,
    estimate_input_tokens: int,
    max_output_tokens: int,
    tutor_id: int | None = None,
    parent_run_id: int | None = None,
    idempotency_key: str | None = None,
) -> int:
    """Reserves the estimate on this month's usage; over the cap the run is Blocked (402)."""
    now = utcnow()
    month = rules.month_of(now)
    estimate = models.cost_micro_usd(g.model, estimate_input_tokens, max_output_tokens)
    async with transaction(deps.sessions) as s:
        await s.execute(
            insert(usage_monthly)
            .prefix_with("IGNORE")
            .values(UserRef=g.user_ref, Month=month, SpentMicroUsd=0, ReservedMicroUsd=0, Runs=0)
        )
        u = (
            await s.execute(
                select(usage_monthly)
                .where(usage_monthly.c.UserRef == g.user_ref, usage_monthly.c.Month == month)
                .with_for_update()
            )
        ).one()
        blocked = rules.over_cap(g.settings.MonthlyCapMicroUsd, int(u.SpentMicroUsd), int(u.ReservedMicroUsd), estimate)
        values = {
            "PublicId": new_id(),
            "UserRef": g.user_ref,
            "CourseRef": g.course_ref,
            "ParentRunId": parent_run_id,
            "Feature": g.feature,
            "ProviderKeyId": g.key.Id,
            "Provider": g.provider_name,
            "Model": g.model.id,
            "PromptVersion": prompt_version,
            "TutorTemplateId": tutor_id,
            "IdempotencyKey": idempotency_key,
            "Status": "Blocked" if blocked else "Running",
            "CostMicroUsd": estimate,
            "CreatedAt": now,
            "StartedAt": None if blocked else now,
        }
        result = await s.execute(insert(run).values(**values))
        run_id = int(cast(CursorResult[Any], result).inserted_primary_key[0])  # type: ignore[index]
        if not blocked:
            await s.execute(
                update(usage_monthly)
                .where(usage_monthly.c.UserRef == g.user_ref, usage_monthly.c.Month == month)
                .values(ReservedMicroUsd=usage_monthly.c.ReservedMicroUsd + estimate)
            )
    if blocked:
        raise AiBudgetExceeded(details={"capMicroUsd": g.settings.MonthlyCapMicroUsd})
    return run_id


async def finish_run(
    deps: Deps,
    run_id: int,
    used: Usage | None,
    *,
    error: str | None = None,
    transcript_ref: str | None = None,
    retain_days: int = 90,
) -> int:
    """Settles: the reservation goes, the actual cost is spent. Returns the cost."""
    now = utcnow()
    async with transaction(deps.sessions) as s:
        r = (await s.execute(select(run).where(run.c.Id == run_id).with_for_update())).one()
        # A fallback reply bills at the fallback model's rates (when the price table has it).
        model = (used and used.served_by and models.find(r.Provider, used.served_by)) or models.find(
            r.Provider, r.Model
        )
        actual = models.cost_micro_usd(model, used.input_tokens, used.output_tokens) if model and used else 0
        month = rules.month_of(r.CreatedAt)
        await s.execute(
            update(run)
            .where(run.c.Id == run_id)
            .values(
                Status="Failed" if error else "Succeeded",
                InputTokens=used.input_tokens if used else None,
                OutputTokens=used.output_tokens if used else None,
                CostMicroUsd=actual,
                Error=error,
                FinishedAt=now,
                TranscriptRef=transcript_ref,
                RetainUntil=now + timedelta(days=retain_days) if transcript_ref else None,
            )
        )
        await s.execute(
            update(usage_monthly)
            .where(usage_monthly.c.UserRef == r.UserRef, usage_monthly.c.Month == month)
            .values(
                ReservedMicroUsd=usage_monthly.c.ReservedMicroUsd - r.CostMicroUsd,
                SpentMicroUsd=usage_monthly.c.SpentMicroUsd + actual,
                Runs=usage_monthly.c.Runs + 1,
            )
        )
    return actual


async def log_context(s: AsyncSession, run_id: int, entries: list[tuple[str, str]]) -> None:
    """Spec 7.4: every record that went into the prompt."""
    for entity_type, ref in {(t, r[:40]) for t, r in entries}:
        await s.execute(
            insert(run_context).prefix_with("IGNORE").values(AiRunId=run_id, EntityType=entity_type, EntityRef=ref)
        )
