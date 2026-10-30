"""Smartness levels (plans-and-billing.md, stage 1). Pure.

Each AI feature runs at a level; the level picks the model and effort for the key's provider.
Users can raise or lower a feature's level. Free plans stop at Standard once Crux pays for AI
(stage 2); with your own key every level is open.
"""

from dataclasses import dataclass

LEVELS: tuple[str, ...] = ("Quick", "Standard", "Smart", "Genius")

FEATURES: tuple[str, ...] = ("MaterialChat", "Grading", "CourseImport", "PastExamImport", "Generation")

#: Chat-like work is cheap and fast; marking, writing questions and reading files think harder.
DEFAULTS: dict[str, str] = {
    "MaterialChat": "Quick",
    "Grading": "Standard",
    "CourseImport": "Standard",
    "PastExamImport": "Standard",
    "Generation": "Standard",
}

#: Levels a free plan may use when Crux pays for the call.
FREE_LEVELS = frozenset({"Quick", "Standard"})


@dataclass(frozen=True)
class Route:
    model: str
    #: Anthropic `output_config.effort`; None leaves the model's default (other providers ignore it).
    effort: str | None = None


#: Per provider, per level. Levels above a provider's best model reuse it.
ROUTES: dict[str, dict[str, Route]] = {
    "anthropic": {
        "Quick": Route("claude-haiku-5-5", "low"),
        "Standard": Route("claude-haiku-5-5", "high"),
        "Smart": Route("claude-sonnet-5-5", "high"),
        "Genius": Route("claude-opus-5-5", "high"),
    },
    "openai": {
        "Quick": Route("gpt-6-luna"),
        "Standard": Route("gpt-6-luna"),
        "Smart": Route("gpt-6.1-sol"),
        "Genius": Route("gpt-6-astra"),
    },
    "google": {
        "Quick": Route("gemini-3.5-flash-lite"),
        "Standard": Route("gemini-3.8-flash"),
        "Smart": Route("gemini-4-argon"),
        "Genius": Route("gemini-4-argon"),
    },
    "mistral": {
        "Quick": Route("mistral-small-2603"),
        "Standard": Route("mistral-small-2603"),
        "Smart": Route("mistral-medium-3-5"),
        "Genius": Route("mistral-medium-3-5"),
    },
    "fake": {level: Route("fake-1") for level in LEVELS},
}


def chosen(levels: dict[str, str] | None, feature: str) -> str:
    """The user's level for a feature, or the feature's default."""
    v = (levels or {}).get(feature)
    return v if v in LEVELS else DEFAULTS[feature]


def capped(level: str, free: bool) -> str:
    """A free plan's level when Crux pays: the highest it may use, at most the one asked for."""
    if not free or level in FREE_LEVELS:
        return level
    return max(FREE_LEVELS, key=LEVELS.index)


def route(provider: str, level: str) -> Route | None:
    return ROUTES.get(provider, {}).get(level)


def problems(levels: dict[str, str]) -> list[str]:
    """Unknown features or levels in a settings patch."""
    return [f"{f}={lv}" for f, lv in levels.items() if f not in FEATURES or lv not in LEVELS]
