"""Provider and model registry with the price table (spec 7.2: a static list per provider, updated
by hand). Prices are USD per million tokens, standard tier; costs are estimates (the user pays the
provider).

Checked 2026-10-24 against each provider's own price page (OPEN-QUESTIONS B8-3):
- Anthropic: the Claude model catalogue (Opus 5.5 $4/$20, Sonnet 5.5 $2/$10, Haiku 5.5 $0.10/$0.50
  up to 100K prompt tokens, $0.50/$2.50 above).
- OpenAI: developers.openai.com/api/docs/pricing. Prompts above 272K tokens cost more, so the
  context used here stops at 272K.
- Google: ai.google.dev/gemini-api/docs/pricing, with the scheduled rises (Gemini 4 Argon from
  1 Dec 2026, Gemini 3.8 Flash from 1 Jan 2027). Context isn't stated there; 1M as for the family.
- Mistral: model cards (Medium 3.5, Small 4) and the pricing page's FAQ (Large).
"""

from dataclasses import dataclass, field
from datetime import UTC, date, datetime


@dataclass(frozen=True)
class Model:
    provider: str
    id: str
    context: int
    input_per_mtok: float
    output_per_mtok: float
    default: bool = False
    #: Prompts above this many tokens are billed at `long_rates` (input, output).
    long_from: int | None = None
    long_rates: tuple[float, float] | None = None
    #: Announced price changes: (first day, input, output), oldest first.
    changes: tuple[tuple[date, float, float], ...] = field(default=())

    def rates(self, input_tokens: int, on: date) -> tuple[float, float]:
        rates = (self.input_per_mtok, self.output_per_mtok)
        for day, i, o in self.changes:
            if on >= day:
                rates = (i, o)
        if self.long_from is not None and self.long_rates and input_tokens > self.long_from:
            rates = self.long_rates
        return rates


MODELS: tuple[Model, ...] = (
    Model("anthropic", "claude-opus-5-5", 1_000_000, 4.00, 20.00, default=True),
    Model("anthropic", "claude-sonnet-5-5", 1_000_000, 2.00, 10.00),
    Model("anthropic", "claude-haiku-5-5", 1_000_000, 0.10, 0.50, long_from=100_000, long_rates=(0.50, 2.50)),
    # The refusal fallback can answer on this one (B8-2); listed so its replies are priced right.
    Model("anthropic", "claude-opus-4-8", 1_000_000, 5.00, 25.00),
    Model("openai", "gpt-6.1-sol", 272_000, 2.00, 10.00, default=True),
    Model("openai", "gpt-6-astra", 272_000, 10.00, 50.00),
    Model("openai", "gpt-6-luna", 272_000, 0.10, 0.50),
    Model(
        "google", "gemini-4-argon", 1_000_000, 2.00, 10.00, default=True, changes=((date(2026, 12, 1), 4.00, 20.00),)
    ),
    Model("google", "gemini-3.8-flash", 1_000_000, 0.75, 3.75, changes=((date(2027, 1, 1), 1.50, 7.50),)),
    Model("google", "gemini-3.5-flash-lite", 1_000_000, 0.30, 2.50),
    Model("mistral", "mistral-medium-3-5", 256_000, 1.50, 7.50, default=True),
    Model("mistral", "mistral-large-latest", 256_000, 0.50, 1.50),
    Model("mistral", "mistral-small-2603", 256_000, 0.15, 0.60),
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


def cost_micro_usd(model: Model, input_tokens: int, output_tokens: int, on: date | None = None) -> int:
    """Price-table estimate on `on` (default today, UTC), rounded up to a whole micro-dollar."""
    i, o = model.rates(input_tokens, on or datetime.now(UTC).date())
    usd = (input_tokens * i + output_tokens * o) / 1_000_000
    return int(-(-usd * 1_000_000 // 1))


def estimate_tokens(text: str) -> int:
    """Rough count for reserving budget before a call (~4 characters per token)."""
    return max(1, len(text) // 4)
