"""M8 part 4: question generation, validation, nightly top-up (spec 7.5)."""

from datetime import datetime
from typing import Any

import pytest
from sqlalchemy import update

import app.service.generation as gen_module
from app.domain import generation as rules
from app.repo.tables import generation_job
from app.service import account, generation
from crux_common.errors import AiDisabled, NotFound
from tests.conftest_env import COURSE, OTHER, USER, env
from tests.test_chat import turn_on

LECTURE = "01TESTLECTURE0000000000001"


def mc(prompt: str, correct: int = 1, options: list[str] | None = None) -> dict[str, Any]:
    opts = options or ["4 Var(X)", "2 Var(X)", "Var(X)"]
    return {
        "type": "MC",
        "prompt": prompt,
        "explanation": "Square the constant.",
        "options": [{"text": t, "isCorrect": i < correct, "distractorNote": "factor of 2"} for i, t in enumerate(opts)],
        "numeric": {"value": 0, "tolerance": 0, "unit": ""},
        "modelAnswer": "",
    }


def test_rules() -> None:
    existing = ["What is Var(2X) when Var(X) = 1?"]
    assert rules.check(mc("What is Var(3X) in terms of Var(X)?"), existing) is None
    assert rules.check(mc("What is Var(2X) when Var(X) = 1 ?"), existing) == "duplicate"
    assert rules.check(mc("Which of these is Var(3X)?", correct=2), []) == "not_one_correct"
    assert rules.check(mc("Which of these is Var(3X)?", options=["a", "A "]), []) == "duplicate_options"
    assert (
        rules.check(
            {"type": "MC", "prompt": "Which of these is right?", "options": [{"text": "a", "isCorrect": True}]}, []
        )
        == "too_few_options"
    )
    assert rules.to_content(mc("What is Var(3X) in terms of Var(X)?"))["options"][0]["isCorrect"] is True
    assert [rules.k_for(d) for d in (10, 7, 5, 3, 2, 0, None)] == [3, 3, 2, 2, 3, 3, 3]
    plan = [
        {
            "courseId": "C1",
            "daysToExam": 5,
            "lectures": [
                {"componentId": "L1", "questionCount": 4, "untested": 3},
                {"componentId": "L2", "questionCount": None},
            ],
        },
        {"courseId": "C2", "daysToExam": 20, "lectures": [{"componentId": "L3", "questionCount": 30, "untested": 0}]},
    ]
    assert rules.topup_needs(plan) == [("C1", "L1", 5), ("C2", "L3", 20)]
    assert sum(n for *_, n in rules.topup_needs(plan, cap=10)) == 10


async def test_generate_job_validates_and_creates_through_core() -> None:
    async with env() as (deps, fake, core, _files):
        with pytest.raises(NotFound):
            await generation.start(deps, OTHER, generation.GenerateInput(course_id=COURSE, component_id=LECTURE, n=3))
        with pytest.raises(AiDisabled):
            await generation.start(deps, USER, generation.GenerateInput(course_id=COURSE, component_id=LECTURE, n=3))
        await turn_on(deps)
        await account.patch_settings(deps, USER, account.SettingsPatch(auto_approve_generated=False))
        job = await generation.start(deps, USER, generation.GenerateInput(course_id=COURSE, component_id=LECTURE, n=3))
        assert job.status == "Queued"

        core.replies[
            ("GET", f"/internal/v1/ai/generation-context?user={USER.user_ref}&course={COURSE}&component={LECTURE}")
        ] = {
            "course": {"code": "ST"},
            "component": {"code": "L1"},
            "existingPrompts": ["What is Var(2X) when Var(X) = 1?"],
        }
        sent: list[dict[str, Any]] = []

        def created(body: dict[str, Any]) -> dict[str, Any]:
            sent.append(body)
            return {"created": [f"01GENQ{i:020d}" for i in range(len(body["questions"]))], "rejected": []}

        core.replies[("POST", "/internal/v1/questions/generated")] = created
        fake.json_replies = [
            {
                "questions": [
                    mc("What is Var(3X) in terms of Var(X)?"),
                    mc("What is Var(2X) when Var(X) = 1?"),  # a duplicate of the bank
                    mc("Which expression equals Var(-X)?", correct=2),  # two correct
                ]
            }
        ]
        assert await generation.run_one(deps)
        done = await generation.get(deps, USER, job.id)
        assert (done.status, done.created_count, done.rejected_count) == ("Done", 1, 2)
        assert sent[0]["autoApprove"] is False and len(sent[0]["questions"]) == 1
        assert "<existing>" in fake.calls[-1]["messages"][0]["content"]
        assert not await generation.run_one(deps), "the queue is empty"
        with pytest.raises(NotFound):
            await generation.get(deps, OTHER, job.id)


async def test_nightly_topup_at_three_local() -> None:
    async with env() as (deps, _fake, core, _files):
        await turn_on(deps)
        await account.patch_settings(deps, USER, account.SettingsPatch(nightly_generation=True))
        plan = {
            "timeZone": "UTC",
            "courses": [
                {
                    "courseId": COURSE,
                    "daysToExam": 10,
                    "lectures": [{"componentId": LECTURE, "questionCount": 2, "untested": 1}],
                }
            ],
        }
        core.replies[("GET", f"/internal/v1/ai/topup-plan?user={USER.user_ref}")] = plan
        real = gen_module.utcnow
        gen_module.utcnow = lambda: datetime(2026, 10, 24, 14, 0)  # type: ignore[assignment]
        try:
            assert await generation.nightly(deps) == 0, "not 03:00 there"
            gen_module.utcnow = lambda: datetime(2026, 10, 24, 3, 10)  # type: ignore[assignment]
            assert await generation.nightly(deps) == 1
            assert await generation.nightly(deps) == 0, "once a night"
        finally:
            gen_module.utcnow = real
        async with deps.sessions() as s, s.begin():
            await s.execute(update(generation_job).values(Status="Done"))
