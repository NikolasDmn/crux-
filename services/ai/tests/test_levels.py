"""Smartness levels: defaults, routes per provider, and the free-plan cap (plans-and-billing.md)."""

from app.domain import levels, models


def test_defaults_and_choice() -> None:
    assert levels.chosen(None, "MaterialChat") == "Quick"
    assert levels.chosen({"Grading": "Genius"}, "Grading") == "Genius"
    assert levels.chosen({"Grading": "Nope"}, "Grading") == "Standard", "unknown values fall back"
    assert levels.problems({"Grading": "Smart", "Dance": "Quick", "Generation": "Huge"}) == [
        "Dance=Quick",
        "Generation=Huge",
    ]


def test_every_route_names_a_priced_model() -> None:
    for provider, routes in levels.ROUTES.items():
        assert set(routes) == set(levels.LEVELS), provider
        for level, r in routes.items():
            assert models.find(provider, r.model), (provider, level, r.model)


def test_claude_routes() -> None:
    assert levels.route("anthropic", "Quick") == levels.Route("claude-haiku-5-5", "low")
    assert levels.route("anthropic", "Standard") == levels.Route("claude-haiku-5-5", "high")
    assert levels.route("anthropic", "Smart").model == "claude-sonnet-5-5"  # type: ignore[union-attr]
    assert levels.route("anthropic", "Genius").model == "claude-opus-5-5"  # type: ignore[union-attr]


def test_free_plan_cap() -> None:
    assert levels.capped("Genius", free=True) == "Standard"
    assert levels.capped("Smart", free=True) == "Standard"
    assert levels.capped("Quick", free=True) == "Quick"
    assert levels.capped("Genius", free=False) == "Genius"
