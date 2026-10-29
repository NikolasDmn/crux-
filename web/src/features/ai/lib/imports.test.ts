import { describe, expect, it } from "vitest";
import { applyBlocker, defaultSelected, editsFor, readField, selected, summary, type Proposal } from "./imports";

const p = (over: Partial<Proposal>): Proposal => ({
  id: "p",
  kind: "Component",
  status: "Pending",
  confidence: "High",
  duplicateOfId: null,
  payload: {},
  userEdits: null,
  ...over,
});

describe("imports", () => {
  it("leaves low confidence and question/flashcard duplicates unselected", () => {
    expect(defaultSelected(p({ confidence: "Low" }))).toBe(false);
    expect(defaultSelected(p({ kind: "Question", duplicateOfId: "q1" }))).toBe(false);
    expect(defaultSelected(p({ kind: "Component", duplicateOfId: "c1" }))).toBe(true);
    expect(selected(p({ status: "Rejected" }))).toBe(false);
    expect(selected(p({ status: "Accepted", confidence: "Low" }))).toBe(true);
  });

  it("summarises proposals", () => {
    expect(summary("Component", { code: "L3", name: "Variance", estHours: 2 })).toEqual({
      title: "L3 Variance",
      detail: "2 h",
    });
    expect(summary("Question", { type: "MC", content: { prompt: "Why?", keyReliability: "Derived" } }).title).toBe(
      "Why?",
    );
  });

  it("builds edits for changed fields only, with question content whole", () => {
    const payload = { content: { prompt: "Old", explanation: "E", options: [1] } };
    expect(readField(payload, "content.prompt")).toBe("Old");
    expect(editsFor("Question", payload, { "content.prompt": "New", "content.explanation": "E" })).toEqual({
      content: { prompt: "New", explanation: "E", options: [1] },
    });
    expect(editsFor("Component", { name: "A", estHours: 2 }, { name: "A", estHours: "", code: "" })).toEqual({
      estHours: null,
    });
  });

  it("needs the course accepted for a new course", () => {
    expect(applyBlocker(true, [p({ kind: "Component" })])).toBe("Accept the course first.");
    expect(applyBlocker(true, [p({ kind: "Course" }), p({ kind: "Component" })])).toBeNull();
    expect(applyBlocker(false, [p({ status: "Rejected" })])).toBe("Select at least one proposal.");
  });
});
