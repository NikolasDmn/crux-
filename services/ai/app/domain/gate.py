"""The AI gate (spec 7.1) and the monthly budget (spec 7.2). Pure.

ai_allowed(user, feature, course?, materials?) =
    UserAiSettings.Enabled and Allow<feature>
    and a usable key (Valid or Unverified)
    and (no course, or AiCourseSetting.AiEnabled is not false)
    and every material: AiAllowed and its file is Clean
"""

from dataclasses import dataclass
from datetime import datetime

#: settings column per feature
ALLOW_COLUMN = {
    "MaterialChat": "AllowMaterialChat",
    "Grading": "AllowGrading",
    "CourseImport": "AllowCourseImport",
    "PastExamImport": "AllowPastExamImport",
    "Generation": "AllowGeneration",
}

USABLE_KEY = frozenset({"Valid", "Unverified"})


@dataclass(frozen=True)
class MaterialCheck:
    ai_allowed: bool
    excluded: bool
    file_clean: bool | None  # None = no file (markdown)


def decide(
    enabled: bool,
    feature_allowed: bool,
    key_status: str | None,
    course_ai_enabled: bool | None,
    materials: list[MaterialCheck],
) -> str | None:
    """None = allowed, else the `reason` for `403 ai_disabled` (spec 3.1)."""
    if not enabled:
        return "user_off"
    if not feature_allowed:
        return "feature_off"
    if key_status not in USABLE_KEY:
        return "no_key"
    if course_ai_enabled is False:
        return "course_off"
    for m in materials:
        if not m.ai_allowed or m.excluded or m.file_clean is False:
            return "material_blocked"
    return None


def month_of(t: datetime) -> str:
    return t.strftime("%Y-%m")


def over_cap(cap_micro_usd: int | None, spent: int, reserved: int, estimate: int) -> bool:
    """Reserve-then-settle (spec 7.2): a run that could take the month over the cap is Blocked."""
    return cap_micro_usd is not None and spent + reserved + estimate > cap_micro_usd
