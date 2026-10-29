"""AI settings + consent, provider keys, per-course settings and usage (spec 7.1-7.2, 15).
Everything AI is opt-in: with no settings row (or Enabled = false) every call is refused."""

import base64
from typing import Any, cast

from pydantic import BaseModel, ConfigDict, Field
from pydantic.alias_generators import to_camel
from sqlalchemy import func, select, update
from sqlalchemy.dialects.mysql import insert
from sqlalchemy.engine import CursorResult
from sqlalchemy.ext.asyncio import AsyncSession

from app.clients.llm import LlmError
from app.domain import crypto, models
from app.repo.tables import consent_log, course_setting, provider_key, run, settings, tutor, usage_monthly
from app.service.deps import Deps
from crux_common.auth import Principal
from crux_common.db import transaction
from crux_common.errors import NotFound, ValidationFailed
from crux_common.ids import new_id
from crux_common.timeutil import iso, utcnow


class Camel(BaseModel):
    model_config = ConfigDict(alias_generator=to_camel, populate_by_name=True)


DEFAULTS: dict[str, Any] = {
    "Enabled": False,
    "AllowMaterialChat": True,
    "AllowGrading": True,
    "AllowCourseImport": True,
    "AllowPastExamImport": True,
    "AllowGeneration": True,
    "NightlyGeneration": False,
    "ShareProgress": True,
    "ShareMistakes": True,
    "ShareAnswers": True,
    "AutoApproveGenerated": True,
}

#: API field -> column, for the switches.
SWITCHES = {
    "enabled": "Enabled",
    "allow_material_chat": "AllowMaterialChat",
    "allow_grading": "AllowGrading",
    "allow_course_import": "AllowCourseImport",
    "allow_past_exam_import": "AllowPastExamImport",
    "allow_generation": "AllowGeneration",
    "nightly_generation": "NightlyGeneration",
    "share_progress": "ShareProgress",
    "share_mistakes": "ShareMistakes",
    "share_answers": "ShareAnswers",
    "auto_approve_generated": "AutoApproveGenerated",
}


async def settings_row(s: AsyncSession, user_ref: str) -> Any | None:
    return (await s.execute(select(settings).where(settings.c.UserRef == user_ref))).first()


async def ensure_settings(s: AsyncSession, user_ref: str) -> Any:
    await s.execute(insert(settings).prefix_with("IGNORE").values(UserRef=user_ref, RowVersion=0, **DEFAULTS))
    row = await settings_row(s, user_ref)
    assert row is not None
    return row


async def _ref_of(s: AsyncSession, table: Any, row_id: int | None) -> str | None:
    if row_id is None:
        return None
    r = (await s.execute(select(table.c.PublicId).where(table.c.Id == row_id))).first()
    return r.PublicId if r else None


class SettingsView(Camel):
    enabled: bool
    allow_material_chat: bool
    allow_grading: bool
    allow_course_import: bool
    allow_past_exam_import: bool
    allow_generation: bool
    nightly_generation: bool
    share_progress: bool
    share_mistakes: bool
    share_answers: bool
    auto_approve_generated: bool
    default_key_id: str | None
    default_model: str | None
    default_tutor_id: str | None
    monthly_cap_micro_usd: int | None
    #: The policy version you accepted, and the one in force now (enable again if they differ).
    consent_policy_version: str | None
    current_policy_version: str
    consented_at: str | None


async def _view(deps: Deps, s: AsyncSession, user_ref: str) -> SettingsView:
    row = await settings_row(s, user_ref)
    values = {k: bool(getattr(row, c)) if row else DEFAULTS[c] for k, c in SWITCHES.items()}
    return SettingsView(
        **values,
        default_key_id=await _ref_of(s, provider_key, row.DefaultProviderKeyId) if row else None,
        default_model=row.DefaultModel if row else None,
        default_tutor_id=await _ref_of(s, tutor, row.DefaultTutorTemplateId) if row else None,
        monthly_cap_micro_usd=row.MonthlyCapMicroUsd if row else None,
        consent_policy_version=row.ConsentPolicyVersion if row else None,
        current_policy_version=deps.settings.consent_policy_version,
        consented_at=iso(row.ConsentedAt) if row and row.ConsentedAt else None,
    )


async def get_settings(deps: Deps, user: Principal) -> SettingsView:
    async with deps.sessions() as s:
        return await _view(deps, s, user.user_ref)


class SettingsPatch(Camel):
    enabled: bool | None = None
    allow_material_chat: bool | None = None
    allow_grading: bool | None = None
    allow_course_import: bool | None = None
    allow_past_exam_import: bool | None = None
    allow_generation: bool | None = None
    nightly_generation: bool | None = None
    share_progress: bool | None = None
    share_mistakes: bool | None = None
    share_answers: bool | None = None
    auto_approve_generated: bool | None = None
    default_key_id: str | None = None
    default_model: str | None = None
    default_tutor_id: str | None = None
    #: null or 0 removes the cap.
    monthly_cap_micro_usd: int | None = Field(default=None, ge=0)
    #: Required when turning AI on: the policy version shown to you (spec 15).
    consent_policy_version: str | None = None


async def patch_settings(deps: Deps, user: Principal, p: SettingsPatch) -> SettingsView:
    now = utcnow()
    async with transaction(deps.sessions) as s:
        row = await ensure_settings(s, user.user_ref)
        values: dict[str, Any] = {}
        delta: dict[str, Any] = {}
        for field, col in SWITCHES.items():
            v = getattr(p, field)
            if v is not None and bool(getattr(row, col)) != v:
                values[col] = v
                delta[field] = v
        turning_on = values.get("Enabled") is True
        policy = deps.settings.consent_policy_version
        if turning_on or (p.consent_policy_version is not None and row.Enabled):
            if p.consent_policy_version != policy:
                raise ValidationFailed.field(
                    "consentPolicyVersion", "consent_required", f"Accept the AI policy ({policy}) to turn AI on."
                )
            values.update(ConsentPolicyVersion=policy, ConsentedAt=now)
            delta["consentPolicyVersion"] = policy
        if p.default_key_id is not None:
            k = await key_row(s, user.user_ref, p.default_key_id)
            if k is None or k.Status == "Revoked":
                raise ValidationFailed.field("defaultKeyId", "unknown", "Pick one of your keys.")
            values["DefaultProviderKeyId"] = k.Id
            if p.default_model is None and row.DefaultModel and not models.find(k.Provider, row.DefaultModel):
                values["DefaultModel"] = None
        if p.default_model is not None:
            key_id = values.get("DefaultProviderKeyId", row.DefaultProviderKeyId)
            k = (await s.execute(select(provider_key).where(provider_key.c.Id == key_id))).first() if key_id else None
            if p.default_model and (k is None or not models.find(k.Provider, p.default_model)):
                raise ValidationFailed.field("defaultModel", "unknown", "Pick a model of your key's provider.")
            values["DefaultModel"] = p.default_model or None
        if p.default_tutor_id is not None:
            t = await visible_tutor(s, user.user_ref, p.default_tutor_id)
            if t is None:
                raise ValidationFailed.field("defaultTutorId", "unknown", "Pick a tutor.")
            values["DefaultTutorTemplateId"] = t.Id
        if "monthly_cap_micro_usd" in p.model_fields_set:
            values["MonthlyCapMicroUsd"] = p.monthly_cap_micro_usd or None
            delta["monthlyCapMicroUsd"] = p.monthly_cap_micro_usd
        if values:
            await s.execute(
                update(settings)
                .where(settings.c.UserRef == user.user_ref)
                .values(RowVersion=settings.c.RowVersion + 1, **values)
            )
        if delta:
            # Spec 15: every change to what AI may do or see is logged with the policy version.
            await s.execute(
                insert(consent_log).values(
                    UserRef=user.user_ref, ChangedAt=now, PolicyVersion=row.ConsentPolicyVersion or policy, Delta=delta
                )
            )
        return await _view(deps, s, user.user_ref)


# ---------- keys ----------


async def key_row(s: AsyncSession, user_ref: str, key_ref: str) -> Any | None:
    stmt = select(provider_key).where(provider_key.c.UserRef == user_ref, provider_key.c.PublicId == key_ref)
    return (await s.execute(stmt)).first()


class ModelPrice(Camel):
    id: str
    #: USD per million tokens today (standard prompts; long prompts can cost more).
    input_per_mtok: float
    output_per_mtok: float
    #: The provider's default, picked when the key is added.
    default: bool


class KeyView(Camel):
    id: str
    provider: str
    label: str
    last4: str
    #: Unverified, Valid, Invalid
    status: str
    last_verified_at: str | None
    created_at: str | None
    is_default: bool
    models: list[str]
    #: The same models with today's prices, for the model picker.
    prices: list[ModelPrice]


def _key_view(k: Any, default_id: int | None) -> KeyView:
    return KeyView(
        id=k.PublicId,
        provider=k.Provider,
        label=k.Label,
        last4=k.Last4,
        status=k.Status,
        last_verified_at=iso(k.LastVerifiedAt) if k.LastVerifiedAt else None,
        created_at=iso(k.CreatedAt) if k.CreatedAt else None,
        is_default=k.Id == default_id,
        models=[m.id for m in models.for_provider(k.Provider)],
        prices=[_price(m) for m in models.for_provider(k.Provider)],
    )


def _price(m: models.Model) -> ModelPrice:
    i, o = m.rates(0, utcnow().date())
    return ModelPrice(id=m.id, input_per_mtok=i, output_per_mtok=o, default=m.default)


async def list_keys(deps: Deps, user: Principal) -> list[KeyView]:
    async with deps.sessions() as s:
        row = await settings_row(s, user.user_ref)
        rows = await s.execute(
            select(provider_key)
            .where(provider_key.c.UserRef == user.user_ref, provider_key.c.Status != "Revoked")
            .order_by(provider_key.c.Id)
        )
        return [_key_view(k, row.DefaultProviderKeyId if row else None) for k in rows]


class KeyInput(Camel):
    provider: str
    label: str = Field(default="default", min_length=1, max_length=64)
    api_key: str = Field(min_length=8, max_length=400)


def plaintext(deps: Deps, k: Any) -> str:
    return crypto.open_sealed(deps.master_key, base64.b64decode(k.Ciphertext), k.PublicId)


async def _verify(deps: Deps, provider: str, api_key: str) -> str:
    """Valid / Invalid, or Unverified when the provider couldn't be asked."""
    try:
        return "Valid" if await deps.providers[provider].verify(api_key) else "Invalid"
    except LlmError:
        return "Unverified"


async def add_key(deps: Deps, user: Principal, inp: KeyInput) -> KeyView:
    allowed = [p for p in models.PROVIDERS if p in deps.providers and (p != "fake" or deps.settings.fake_allowed)]
    if inp.provider not in allowed:
        raise ValidationFailed.field("provider", "unknown", f"Use one of {', '.join(allowed)}.")
    status = await _verify(deps, inp.provider, inp.api_key.strip())
    public_id = new_id()
    # Stored base64 (aiomysql can't bind raw bytes with PyMySQL >= 2.2.8; OPEN-QUESTIONS B8-12).
    blob = base64.b64encode(crypto.seal(deps.master_key, inp.api_key.strip(), public_id)).decode()
    now = utcnow()
    async with transaction(deps.sessions) as s:
        row = await ensure_settings(s, user.user_ref)
        dup = await s.execute(
            select(provider_key.c.Id).where(
                provider_key.c.UserRef == user.user_ref,
                provider_key.c.Provider == inp.provider,
                provider_key.c.Label == inp.label,
                provider_key.c.Status != "Revoked",
            )
        )
        if dup.first():
            raise ValidationFailed.field("label", "taken", "You already have a key with this label for this provider.")
        # A revoked key with the same label frees the label (the unique key spans all statuses).
        await s.execute(
            update(provider_key)
            .where(
                provider_key.c.UserRef == user.user_ref,
                provider_key.c.Provider == inp.provider,
                provider_key.c.Label == inp.label,
            )
            .values(Label=func.concat("revoked-", provider_key.c.PublicId))
        )
        result = await s.execute(
            insert(provider_key).values(
                PublicId=public_id,
                UserRef=user.user_ref,
                Provider=inp.provider,
                Label=inp.label,
                Ciphertext=blob,
                KmsKeyId=deps.settings.kms_key_id,
                Last4=crypto.last4(inp.api_key),
                Status=status,
                LastVerifiedAt=now if status != "Unverified" else None,
                CreatedAt=now,
            )
        )
        key_id = int(cast(CursorResult[Any], result).inserted_primary_key[0])  # type: ignore[index]
        default_id = row.DefaultProviderKeyId
        if default_id is None and status != "Invalid":
            default = models.default_for(inp.provider)
            await s.execute(
                update(settings)
                .where(settings.c.UserRef == user.user_ref)
                .values(DefaultProviderKeyId=key_id, DefaultModel=default.id if default else None)
            )
            default_id = key_id
        k = await key_row(s, user.user_ref, public_id)
        return _key_view(k, default_id)


async def verify_key(deps: Deps, user: Principal, key_ref: str) -> KeyView:
    async with deps.sessions() as s:
        k = await key_row(s, user.user_ref, key_ref)
    if k is None or k.Status == "Revoked":
        raise NotFound()
    status = await _verify(deps, k.Provider, plaintext(deps, k))
    async with transaction(deps.sessions) as s:
        await s.execute(
            update(provider_key).where(provider_key.c.Id == k.Id).values(Status=status, LastVerifiedAt=utcnow())
        )
        row = await settings_row(s, user.user_ref)
        return _key_view(await key_row(s, user.user_ref, key_ref), row.DefaultProviderKeyId if row else None)


async def revoke_key(deps: Deps, user: Principal, key_ref: str) -> None:
    async with transaction(deps.sessions) as s:
        k = await key_row(s, user.user_ref, key_ref)
        if k is None or k.Status == "Revoked":
            raise NotFound()
        # The ciphertext goes: a revoked key can't be decrypted again.
        await s.execute(
            update(provider_key)
            .where(provider_key.c.Id == k.Id)
            .values(Status="Revoked", RevokedAt=utcnow(), Ciphertext="")
        )
        await s.execute(
            update(settings)
            .where(settings.c.UserRef == user.user_ref, settings.c.DefaultProviderKeyId == k.Id)
            .values(DefaultProviderKeyId=None, DefaultModel=None)
        )


# ---------- tutors (lookups used here; CRUD in tutors.py) ----------


async def visible_tutor(s: AsyncSession, user_ref: str, tutor_ref: str) -> Any | None:
    stmt = select(tutor).where(
        tutor.c.PublicId == tutor_ref, (tutor.c.OwnerUserRef.is_(None)) | (tutor.c.OwnerUserRef == user_ref)
    )
    return (await s.execute(stmt)).first()


# ---------- per-course settings ----------


class CourseSettingsView(Camel):
    course_id: str
    #: null = follow your settings; false = no AI in this course.
    ai_enabled: bool | None
    tutor_id: str | None
    nightly_generation: bool | None
    generation_policy: dict[str, Any] | None


class CourseSettingsInput(Camel):
    ai_enabled: bool | None = None
    tutor_id: str | None = None
    nightly_generation: bool | None = None
    generation_policy: dict[str, Any] | None = None


async def _readable(deps: Deps, user: Principal, course_ref: str) -> None:
    access = await deps.core.course_access(user.user_ref, course_ref)
    if access is None or access.role is None:
        raise NotFound()


async def course_settings_row(s: AsyncSession, user_ref: str, course_ref: str) -> Any | None:
    stmt = select(course_setting).where(course_setting.c.UserRef == user_ref, course_setting.c.CourseRef == course_ref)
    return (await s.execute(stmt)).first()


async def get_course_settings(deps: Deps, user: Principal, course_ref: str) -> CourseSettingsView:
    await _readable(deps, user, course_ref)
    async with deps.sessions() as s:
        row = await course_settings_row(s, user.user_ref, course_ref)
        return CourseSettingsView(
            course_id=course_ref,
            ai_enabled=None if row is None or row.AiEnabled is None else bool(row.AiEnabled),
            tutor_id=await _ref_of(s, tutor, row.TutorTemplateId) if row else None,
            nightly_generation=None if row is None or row.NightlyGeneration is None else bool(row.NightlyGeneration),
            generation_policy=row.GenerationPolicy if row else None,
        )


async def put_course_settings(
    deps: Deps, user: Principal, course_ref: str, inp: CourseSettingsInput
) -> CourseSettingsView:
    await _readable(deps, user, course_ref)
    async with transaction(deps.sessions) as s:
        tutor_id = None
        if inp.tutor_id:
            t = await visible_tutor(s, user.user_ref, inp.tutor_id)
            if t is None:
                raise ValidationFailed.field("tutorId", "unknown", "Pick a tutor.")
            tutor_id = t.Id
        values = {
            "AiEnabled": inp.ai_enabled,
            "TutorTemplateId": tutor_id,
            "NightlyGeneration": inp.nightly_generation,
            "GenerationPolicy": inp.generation_policy,
        }
        stmt = insert(course_setting).values(UserRef=user.user_ref, CourseRef=course_ref, **values)
        await s.execute(stmt.on_duplicate_key_update(**values))
    return await get_course_settings(deps, user, course_ref)


# ---------- usage ----------


class MonthUsage(Camel):
    month: str
    spent_micro_usd: int
    reserved_micro_usd: int
    runs: int
    by_feature: dict[str, int]
    cap_micro_usd: int | None


async def usage(deps: Deps, user: Principal, months: int = 6) -> list[MonthUsage]:
    async with deps.sessions() as s:
        row = await settings_row(s, user.user_ref)
        rows = (
            await s.execute(
                select(usage_monthly)
                .where(usage_monthly.c.UserRef == user.user_ref)
                .order_by(usage_monthly.c.Month.desc())
                .limit(months)
            )
        ).all()
        feats = await s.execute(
            select(
                func.date_format(run.c.CreatedAt, "%Y-%m").label("Month"),
                run.c.Feature,
                func.coalesce(func.sum(run.c.CostMicroUsd), 0).label("Cost"),
            )
            .where(run.c.UserRef == user.user_ref)
            .group_by("Month", run.c.Feature)
        )
        by: dict[str, dict[str, int]] = {}
        for f in feats:
            by.setdefault(f.Month, {})[f.Feature] = int(f.Cost)
    return [
        MonthUsage(
            month=r.Month,
            spent_micro_usd=int(r.SpentMicroUsd),
            reserved_micro_usd=int(r.ReservedMicroUsd),
            runs=int(r.Runs),
            by_feature=by.get(r.Month, {}),
            cap_micro_usd=row.MonthlyCapMicroUsd if row else None,
        )
        for r in rows
    ]
