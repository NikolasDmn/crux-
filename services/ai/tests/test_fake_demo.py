"""The dev fake's demo answers fit the schemas the real flows ask for."""

import json

from app.clients.fake_demo import demo_json
from app.domain import generation as rules
from app.domain.generation import SCHEMA as GENERATION
from app.service import grading, imports


def test_import_reads_lectures_from_the_text() -> None:
    text = '<request>{"part": 1}</request>\n<file ref="F1">\n[page 1]\nWeek 1: Variance\nWeek 2: Covariance\n</file>'
    got = demo_json(imports.SCHEMA, [{"role": "user", "content": text}], "")
    kinds = [p["kind"] for p in got["proposals"]]
    assert kinds.count("Component") == 2 and "Course" in kinds and "Deadline" in kinds
    names = [json.loads(p["payloadJson"])["name"] for p in got["proposals"] if p["kind"] == "Component"]
    assert names == ["Variance", "Covariance"]
    assert {p["sourceFileRef"] for p in got["proposals"]} == {"F1"}


def test_generation_passes_the_rules() -> None:
    got = demo_json(GENERATION, [{"role": "user", "content": "<existing>[]</existing>"}], "")
    assert len(got["questions"]) == 2
    kept: list[str] = []
    for q in got["questions"]:
        assert rules.check(q, kept) is None
        kept.append(q["prompt"])


def test_grading_is_valid() -> None:
    got = demo_json(grading.SCHEMA, [{"role": "user", "content": "<answer>x</answer>"}], "")
    assert grading.validate(got, 2)["outcome"] == "Partial"
