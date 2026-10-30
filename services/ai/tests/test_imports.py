"""M8 part 5: course import and past exams -> questions (spec 7.5)."""

import json
from typing import Any

import pytest

from app.clients.core import CoreError
from app.service import imports
from crux_common.errors import AiDisabled, Conflict, NotFound, ValidationFailed
from tests.conftest_env import COURSE, OTHER, USER, env
from tests.test_chat import turn_on

FILE = "01TESTFILE0000000000000009"
PAPER = "01TESTPAPER000000000000001"


def prop(kind: str, key: str, payload: dict[str, Any], parent: str = "", conf: str = "High") -> dict[str, Any]:
    return {
        "kind": kind,
        "tempKey": key,
        "parentTempKey": parent,
        "payloadJson": json.dumps(payload),
        "sourceFileRef": FILE,
        "sourcePage": 1,
        "confidence": conf,
        "rationale": "From the syllabus.",
    }


def mc(official: bool = True, prompt: str = "Var(2X)?") -> dict[str, Any]:
    return {
        "type": "MC",
        "content": {
            "prompt": prompt,
            "keyReliability": "Official" if official else "Derived",
            "options": [{"text": "4 Var(X)", "isCorrect": True}, {"text": "2 Var(X)", "isCorrect": False}],
        },
    }


async def test_import_review_apply() -> None:
    async with env() as (deps, fake, core, files):
        files.files[FILE] = {"ownerRef": USER.user_ref, "scanStatus": "Clean", "extractStatus": "Done"}
        files.chunk_rows[FILE] = [
            {"fileRef": FILE, "chunkNo": 1, "pageNo": 1, "text": "Week 1: Variance. Exam: 1 Dec, 2 h."}
        ]
        inp = imports.ImportInput(file_ids=[FILE])
        with pytest.raises(AiDisabled):
            await imports.create(deps, USER, inp)
        await turn_on(deps)
        with pytest.raises(AiDisabled):
            await imports.create(deps, OTHER, inp)  # the gate comes before any file is looked at
        files.files["01NOTREADY0000000000000001"] = {
            "ownerRef": USER.user_ref,
            "scanStatus": "Clean",
            "extractStatus": "Pending",
        }
        with pytest.raises(ValidationFailed):
            await imports.create(deps, USER, imports.ImportInput(file_ids=["01NOTREADY0000000000000001"]))
        imp = await imports.create(deps, USER, inp)
        assert imp.status == "Uploading"
        assert not await imports.run_one(deps), "not started yet"
        await imports.start(deps, USER, imp.id)

        fake.json_replies = [
            {
                "files": [{"fileRef": FILE, "kind": "Syllabus"}],
                "proposals": [
                    prop("Course", "course", {"name": "Stats", "code": "ST"}),
                    prop("Component", "l1", {"code": "L1", "name": "Variance"}, "course"),
                    prop("Question", "q1", mc(), "l1"),  # dropped: past exams are off
                    prop("Deadline", "d1", {"type": "Exam", "startsAt": "2026-12-01T09:00:00Z", "durationMin": 120}),
                    prop("Component", "l1", {"code": "dup"}),  # duplicate tempKey: dropped
                    {**prop("Flashcard", "f1", {}), "payloadJson": "not json"},
                ],
            }
        ]
        assert await imports.run_one(deps)
        got = await imports.get(deps, USER, imp.id)
        assert (got.status, got.files[0].detected_kind, got.proposal_counts) == ("Proposed", "Syllabus", {"Pending": 3})
        grouped = await imports.proposals(deps, USER, imp.id)
        assert list(grouped) == ["Course", "Component", "Deadline"]
        assert "<file" in fake.calls[-1]["messages"][0]["content"]

        lecture = grouped["Component"][0]
        edited = await imports.patch_proposal(
            deps, USER, lecture.id, imports.ProposalPatch(edits={"name": "Variance rules"})
        )
        assert (edited.status, edited.user_edits) == ("Edited", {"name": "Variance rules"})
        ids = [grouped["Course"][0].id, grouped["Deadline"][0].id]
        assert await imports.bulk(deps, USER, imp.id, imports.BulkInput(proposal_ids=ids, action="accept")) == {
            "updated": 2
        }
        with pytest.raises(NotFound):
            await imports.patch_proposal(deps, OTHER, lecture.id, imports.ProposalPatch(status="Rejected"))

        sent: list[dict[str, Any]] = []

        def apply(body: dict[str, Any]) -> dict[str, Any]:
            sent.append(body)
            refs = [p["ref"] for p in body["proposals"]]
            return {
                "courseRef": COURSE,
                "created": [
                    {"proposalRef": r, "entityRef": "01CREATED0000000000000000" + str(i)}
                    for i, r in enumerate(refs[:-1])
                ],
                "failed": [{"proposalRef": refs[-1], "reason": "startsAt: required"}],
            }

        core.replies[("POST", "/internal/v1/import/apply")] = apply
        done = await imports.apply(deps, USER, imp.id)
        assert (done.status, done.course_id) == ("Applied", COURSE)
        assert done.proposal_counts == {"Applied": 2, "Failed": 1}
        lec = next(p for p in sent[0]["proposals"] if p["kind"] == "Component")
        assert lec["payload"]["name"] == "Variance rules", "edits merged into the payload"
        with pytest.raises(Conflict):
            await imports.apply(deps, USER, imp.id)


async def test_past_paper_extraction() -> None:
    async with env() as (deps, fake, core, files):
        await turn_on(deps)
        files.files[FILE] = {"ownerRef": USER.user_ref, "scanStatus": "Clean", "extractStatus": "Done"}
        core.replies[("GET", f"/internal/v1/ai/paper?user={OTHER.user_ref}&paper={PAPER}")] = CoreError(404, None)
        with pytest.raises(NotFound):
            await imports.extract_paper(deps, OTHER, PAPER)
        core.replies[("GET", f"/internal/v1/ai/paper?user={USER.user_ref}&paper={PAPER}")] = {
            "paperId": PAPER,
            "courseId": COURSE,
            "label": "Exam 2025",
            "fileRef": FILE,
        }
        imp = await imports.extract_paper(deps, USER, PAPER)
        assert (imp.status, imp.import_past_exams, imp.course_id) == ("Queued", True, COURSE)
        q_linked = {**mc(), "paperId": PAPER, "paperQuestionNo": "1"}
        fake.json_replies = [
            {
                "files": [{"fileRef": FILE, "kind": "PastExam"}],
                "proposals": [
                    prop("Course", "c", {"name": "x"}),  # dropped: importing into an existing course
                    prop("Question", "q1", q_linked, "l1"),
                    prop("Question", "q2", mc(prompt="E[X + Y]?"), "l1"),  # Official without a paper link -> Derived
                ],
            },
            # The second opinion only sees the Derived key, and disagrees with it (B, not A).
            {"answers": [{"index": 0, "letters": ["B"], "value": 0}]},
        ]
        assert await imports.run_one(deps)
        qs = (await imports.proposals(deps, USER, imp.id))["Question"]
        assert [q.payload["content"]["keyReliability"] for q in qs] == ["Official", "Derived"]
        solve = fake.calls[-1]["messages"][0]["content"]
        assert "E[X + Y]?" in solve and "isCorrect" not in solve and "Var(2X)" not in solve
        assert (qs[0].confidence, qs[1].confidence) == ("High", "Low")
        assert qs[1].rationale and "second check" in qs[1].rationale


async def test_large_import_reads_in_parts_and_flags_duplicates(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(imports, "TEXT_BUDGET", 100)
    monkeypatch.setattr(imports, "MAX_PARTS", 2)
    async with env() as (deps, fake, core, files):
        await turn_on(deps)
        files.files[FILE] = {"ownerRef": USER.user_ref, "scanStatus": "Clean", "extractStatus": "Done"}
        files.chunk_rows[FILE] = [
            {"fileRef": FILE, "chunkNo": n, "pageNo": n, "text": f"Page {n} " + "x" * 70} for n in (1, 2, 3)
        ]
        lecture, question = "01EXISTINGLECTURE000000001", "01EXISTINGQUESTION00000001"
        core.replies[("GET", f"/internal/v1/ai/import-catalog?user={USER.user_ref}&course={COURSE}")] = {
            "components": [{"id": lecture, "code": "L1", "name": "Variance"}],
            "questions": [{"id": question, "prompt": "Var(2X)?"}],
        }
        fake.json_replies = [
            {
                "files": [],
                "proposals": [
                    prop("Component", "l1", {"code": "L1", "name": "Variance"}),
                    prop("Question", "q1", {**mc(), "componentTempKey": "l1"}, "l1"),
                ],
            },
            {
                "files": [],
                "proposals": [
                    prop("Component", "l1", {"code": "l1", "name": "Variance again"}),  # repeats part 1
                    prop("Component", "l2", {"code": "L2", "name": "Covariance"}),
                    prop("Question", "q1", {**mc(prompt="Cov(X, X)?"), "componentTempKey": "l2"}, "l2"),  # key reused
                    prop("Flashcard", "f1", {"front": "Var(aX)", "back": "a² Var(X)"}, "l1"),
                ],
            },
        ]
        imp = await imports.create(
            deps, USER, imports.ImportInput(course_id=COURSE, file_ids=[FILE], import_past_exams=True), status="Queued"
        )
        assert await imports.run_one(deps)
        calls = [c for c in fake.calls if c["kind"] == "json" and c["schema"] is imports.SCHEMA]
        assert len(calls) == 2, "three pages of ~80 characters, 100 per part, 2 parts at most"
        second = calls[1]["messages"][0]["content"]
        assert "<proposed>" in second and '"tempKey": "l1"' in second and '"part": 2' in second
        got = await imports.get(deps, USER, imp.id)
        assert (got.parts, got.truncated) == (2, True), "page 3 didn't fit"

        grouped = await imports.proposals(deps, USER, imp.id)
        comps = {p.temp_key: p for p in grouped["Component"]}
        assert set(comps) == {"l1", "l2"}, "part 2's L1 is the same lecture"
        assert (comps["l1"].duplicate_of_id, comps["l2"].duplicate_of_id) == (lecture, None)
        qs = {p.temp_key: p for p in grouped["Question"]}
        assert set(qs) == {"q1", "p2.q1"}
        assert (qs["q1"].duplicate_of_id, qs["p2.q1"].duplicate_of_id) == (question, None)
        assert (qs["p2.q1"].parent_temp_key, qs["p2.q1"].payload["componentTempKey"]) == ("l2", "l2")
        assert grouped["Flashcard"][0].parent_temp_key == "l1"

        sent: list[dict[str, Any]] = []
        core.replies[("POST", "/internal/v1/import/apply")] = lambda body: (
            sent.append(body) or {"courseRef": COURSE, "created": [], "failed": []}
        )
        await imports.bulk(deps, USER, imp.id, imports.BulkInput(proposal_ids=[comps["l1"].id], action="accept"))
        await imports.apply(deps, USER, imp.id)
        assert sent[0]["proposals"][0]["duplicateOf"] == lecture
