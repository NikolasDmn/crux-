import { describe, expect, it } from "vitest";
import type { Question } from "@/features/bank/api";
import { check, diffLines, emptyForm, formFrom, toContent, versionText, type Form } from "./editor";

const mc = (patch: Partial<Form> = {}): Form => ({
  ...emptyForm("L1"),
  prompt: "Which is the median of 1, 2, 9?",
  options: [
    { text: "2", isCorrect: true, distractorNote: "" },
    { text: "4", isCorrect: false, distractorNote: "That's the mean." },
    { text: "", isCorrect: false, distractorNote: "" },
  ],
  ...patch,
});

describe("check", () => {
  it("passes a complete MC question", () => {
    expect(check(mc())).toEqual({});
  });

  it("asks for a lecture, a prompt and exactly one right option", () => {
    expect(check({ ...emptyForm(), type: "MC" })).toMatchObject({
      componentId: expect.any(String),
      prompt: expect.any(String),
      options: "Give at least two options.",
    });
    const two = mc({ options: mc().options.map((o) => ({ ...o, isCorrect: !!o.text })) });
    expect(check(two).options).toBe("Mark exactly one option correct.");
  });

  it("checks numeric answers, tolerance and units", () => {
    const f = { ...mc(), type: "Numeric" as const, value: "x", tolerance: "-1", unitRequired: true };
    expect(check(f)).toEqual({
      value: "Enter the answer as a number.",
      tolerance: "A number, 0 or more.",
      unit: "Give the unit, or don't require one.",
    });
    expect(check({ ...f, value: "3,5", tolerance: "0.01", unit: "kg" })).toEqual({});
  });

  it("wants points on every rubric row", () => {
    expect(check({ ...mc(), type: "Open", rubric: [{ criterion: "States H0", points: "" }] }).rubric).toBeDefined();
  });
});

describe("content", () => {
  it("sends MC options without blanks, with notes", () => {
    const c = toContent(mc());
    expect(c.options).toEqual([
      { text: "2", isCorrect: true, distractorNote: null },
      { text: "4", isCorrect: false, distractorNote: "That's the mean." },
    ]);
    expect(c.numeric).toBeUndefined();
  });

  it("sends numeric and open keys", () => {
    const n = toContent({ ...mc(), type: "Numeric", value: "3,5", tolerance: "", unit: "kg", modelAnswer: " m " });
    expect(n.numeric).toEqual({
      value: 3.5,
      tolerance: 0,
      toleranceKind: "Relative",
      unit: "kg",
      unitRequired: false,
      modelAnswer: "m",
    });
    const o = toContent({
      ...mc(),
      type: "Open",
      rubric: [
        { criterion: "States H0", points: "1" },
        { criterion: " ", points: "" },
      ],
    });
    expect(o.open).toEqual({ modelAnswer: null, rubric: [{ criterion: "States H0", points: 1 }], rubricNote: null });
  });

  it("round-trips a saved question", () => {
    const q = {
      id: "q",
      courseId: "c",
      componentId: "L1",
      type: "Numeric",
      private: false,
      source: "PastExam",
      reviewState: "Approved",
      concepts: [{ id: "k1", primary: true }],
      paperId: "p1",
      paperQuestionNo: "3b",
      updatedAt: "",
      version: 2,
      current: {
        id: "v",
        versionNo: 2,
        prompt: "How heavy?",
        keyReliability: "Official",
        createdAt: "",
        numeric: { value: 2, tolerance: 0.1, toleranceKind: "Absolute", unit: "kg", unitRequired: true },
      },
    } as Question;
    const f = formFrom(q);
    expect(f).toMatchObject({
      type: "Numeric",
      value: "2",
      toleranceKind: "Absolute",
      primaryConceptId: "k1",
      paperQuestionNo: "3b",
    });
    expect(toContent(f).numeric?.tolerance).toBe(0.1);
  });
});

describe("history diff", () => {
  it("marks added and removed lines", () => {
    expect(diffLines(["a", "b", "c"], ["a", "x", "c"])).toEqual([
      { op: "same", text: "a" },
      { op: "del", text: "b" },
      { op: "add", text: "x" },
      { op: "same", text: "c" },
    ]);
    expect(diffLines([], ["n"])).toEqual([{ op: "add", text: "n" }]);
  });

  it("writes a version as lines", () => {
    const lines = versionText({
      id: "v",
      versionNo: 1,
      prompt: "Pick one",
      keyReliability: "Derived",
      createdAt: "",
      options: [
        { id: "o1", text: "A", isCorrect: true },
        { id: "o2", text: "B", isCorrect: false },
      ],
      explanation: "Because.",
    });
    expect(lines).toEqual(["Pick one", "A. A  ✓", "B. B", "Explanation:", "Because."]);
  });
});
