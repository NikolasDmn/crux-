"""Provider and model registry with the price table (spec 7.2: a static list per provider, updated
by hand). Prices are USD per million tokens; costs are estimates (the user pays the provider).

Anthropic rows follow Anthropic's published list (2026-10). The other providers' rows are
placeholders to check against their price pages before launch (OPEN-QUESTIONS B8-3).
"""

from dataclasses import dataclass


@dataclass(frozen=True)
class Model:
    provider: str
    id: str
    context: int
    input_per_mtok: float
    output_per_mtok: float
    default: bool = False


MODELS: tuple[Model, ...] = (
    Model("anthropic", "claude-opus-5-5", 1_000_000, 4.00, 20.00, default=True),
    Model("anthropic", "claude-sonnet-5-5", 1_000_000, 2.00, 10.00),
    Model("anthropic", "claude-haiku-5-5", 1_000_000, 0.10, 0.50),
    Model("openai", "gpt-5", 400_000, 1.25, 10.00, default=True),
    Model("openai", "gpt-5-mini", 400_000, 0.25, 2.00),
    Model("google", "gemini-2.5-pro", 1_000_000, 1.25, 10.00, default=True),
    Model("google", "gemini-2.5-flash", 1_000_000, 0.30, 2.50),
    Model("mistral", "mistral-large-latest", 128_000, 2.00, 6.00, default=True),
    Model("mistral", "mistral-small-latest", 128_000, 0.20, 0.60),
    # Development/test only: no network, deterministic.
    Model("fake", "fake-1", 200_000, 1.00, 2.00, default=True),
)

PROVIDERS: tuple[str, ...] = ("anthropic", "openai", "google", "mistral", "fake")


def find(provider: str, model_id: str) -> Model | None:
    return next((m for m in MODELS if m.provider == provider and m.id == model_id), None)


def default_for(provider: str) -> Model | None:
    return next((m for m in MODELS if m.provider == provider and m.default), None)


def for_provider(provider: str) -> list[Model]:
    return [m for m in MODELS if m.provider == provider]


def cost_micro_usd(model: Model, input_tokens: int, output_tokens: int) -> int:
    """Price-table estimate, rounded up to a whole micro-dollar."""
    usd = (input_tokens * model.input_per_mtok + output_tokens * model.output_per_mtok) / 1_000_000
    return int(-(-usd * 1_000_000 // 1))


def estimate_tokens(text: str) -> int:
    """Rough count for reserving budget before a call (~4 characters per token)."""
    return max(1, len(text) // 4)
