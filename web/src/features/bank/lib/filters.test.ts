import { describe, expect, it } from "vitest";
import type { Question } from "@/features/bank/api";
import { activeCount, plainLine, readFilters, toJson, toMarkdown, toQuery } from "./filters";

const q = (patch: Partial<Question["current"]> = {}, type = "MC"): Question =>
  ({
    id: "q1",
    courseId: "c",
    componentId: "L1",
    type,
    private: false,
    source: "Manual",
    reviewState: "Approved",
    concepts: [],
    updatedAt: "",
    version: 1,
    current: {
      id: "v",
      versionNo: 1,
      prompt: "What is $\bar{x}$?",
      keyReliability: "Derived",
      createdAt: "",
      ...patch,
    },
  }) as Question;

describe("filters", () => {
  it("reads only known, non-empty filters from the URL", () => {
    expect(readFilters({ type: "MC", q: "  ", untested: "true", junk: 1, componentId: 5 })).toEqual({
      type: "MC",
      untested: true,
    });
  });

  it("sends only set filters and counts them (search apart)", () => {
    expect(toQuery({ type: "MC", untested: false, q: "mean" })).toEqual({ type: "MC", q: "mean" });
    expect(activeCount({ type: "MC", q: "mean", untested: true })).toBe(2);
  });
});

describe("export", () => {
  it("writes Markdown with the key marked", () => {
    const md = toMarkdown(
      [
        q({
          options: [
            { id: "a", text: "1", isCorrect: false },
            { id: "b", text: "2", isCorrect: true },
          ],
          explanation: "Mean.",
        }),
      ],
      () => "L1",
    );
    expect(md).toBe("## 1. L1 · MC\n\nWhat is $\bar{x}$?\n\nA. 1\nB. 2 **(correct)**\n\n_Mean._");
  });

  it("writes numeric answers and JSON", () => {
    const n = q(
      { numeric: { value: 2, tolerance: 0, toleranceKind: "Relative", unit: "kg", unitRequired: false } },
      "Numeric",
    );
    expect(toMarkdown([n], () => "L1")).toContain("**Answer:** 2 kg");
    expect(JSON.parse(toJson([n]))[0]).toMatchObject({ type: "Numeric", numeric: { value: 2 } });
  });

  it("flattens a prompt to one line", () => {
    expect(plainLine("**Bold** and [link](x)\n\nnext `code`")).toBe("Bold and link next code");
    expect(plainLine("a".repeat(200), 10)).toBe("aaaaaaaaa…");
  });
});
