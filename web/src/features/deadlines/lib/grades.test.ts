import { describe, expect, it } from "vitest";
import type { GradeScale } from "@/features/courses/api";
import { deadlineName, gradeBody, gradeControl, gradeLabel, wouldPass } from "./grades";

const nl: GradeScale = {
  id: "1",
  name: "NL 1-10",
  kind: "Numeric",
  builtIn: true,
  scaleMin: 1,
  scaleMax: 10,
  step: 0.1,
  passValue: 5.5,
};
const pf: GradeScale = {
  id: "2",
  name: "Pass/Fail",
  kind: "PassFail",
  builtIn: true,
  labels: [
    { label: "Pass", value: 1, pass: true },
    { label: "Fail", value: 0, pass: false },
  ] as unknown as GradeScale["labels"],
};

describe("grade control", () => {
  it("asks for a number on a numeric scale, Percent by default", () => {
    expect(gradeControl(nl)).toEqual({ kind: "number", min: 1, max: 10, step: 0.1, pass: 5.5 });
    expect(gradeControl(undefined)).toEqual({ kind: "number", min: 0, max: 100, step: 1, pass: null });
  });

  it("offers the labels on a pass/fail scale", () => {
    expect(gradeControl(pf)).toEqual({
      kind: "choice",
      options: [
        { label: "Pass", pass: true },
        { label: "Fail", pass: false },
      ],
    });
  });
});

describe("grade body", () => {
  it("parses decimals with a comma and checks the range", () => {
    expect(gradeBody(gradeControl(nl), "7,5")).toEqual({ gradeValue: 7.5, gradeText: null });
    expect(gradeBody(gradeControl(nl), "11")).toEqual({ error: "Between 1 and 10." });
    expect(gradeBody(gradeControl(nl), "abc")).toEqual({ error: "Enter a number." });
    expect(gradeBody(gradeControl(nl), "")).toEqual({ gradeValue: null, gradeText: null });
  });

  it("sends a label as text", () => {
    expect(gradeBody(gradeControl(pf), "Pass")).toEqual({ gradeText: "Pass", gradeValue: null });
    expect(gradeBody(gradeControl(pf), "Maybe")).toEqual({ error: "Pick a grade." });
  });
});

describe("would pass", () => {
  it("uses the exam's pass mark before the scale's", () => {
    expect(wouldPass(gradeControl(nl), "5.6")).toBe(true);
    expect(wouldPass(gradeControl(nl), "5.6", 6)).toBe(false);
    expect(wouldPass(gradeControl(undefined), "60")).toBeNull();
    expect(wouldPass(gradeControl(pf), "Fail")).toBe(false);
  });
});

describe("labels", () => {
  it("names and grades deadlines", () => {
    expect(deadlineName({ title: null, type: "Resit" })).toBe("Resit");
    expect(deadlineName({ title: " Final ", type: "Exam" })).toBe("Final");
    expect(gradeLabel({ gradeValue: 7.5, gradeText: null, gradeProvisional: true })).toBe("7.5 (provisional)");
    expect(gradeLabel({ gradeValue: null, gradeText: null, gradeProvisional: false })).toBeNull();
  });
});
