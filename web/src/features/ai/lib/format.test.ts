import { describe, expect, it } from "vitest";
import { capToMicro, priceLine, readRules, reasonText, usd } from "./format";

describe("ai format", () => {
  it("formats micro-dollars", () => {
    expect(usd(0)).toBe("$0");
    expect(usd(4_200)).toBe("$0.0042");
    expect(usd(1_200_000)).toBe("$1.20");
    expect(priceLine({ inputPerMtok: 2, outputPerMtok: 10 })).toBe("$2.00 in · $10.00 out per 1M tokens");
  });

  it("reads a typed cap", () => {
    expect(capToMicro("")).toBe("none");
    expect(capToMicro("$5")).toBe(5_000_000);
    expect(capToMicro("2.50")).toBe(2_500_000);
    expect(capToMicro("2.505")).toBeNull();
    expect(capToMicro("-1")).toBeNull();
  });

  it("explains a closed gate with a fix link, and nothing for the hidden states", () => {
    expect(reasonText("course_off", "c1")).toEqual({ text: "AI is off for this course.", to: "/c/c1/settings" });
    expect(reasonText("no_key")?.to).toBe("/settings/ai");
    expect(reasonText("user_off")).toBeNull();
    expect(reasonText("service_unavailable")).toBeNull();
  });

  it("keeps tutor rules in range", () => {
    expect(readRules({ maxWords: 20, hintsBeforeSolution: 3, oneQuestionAtATime: true })).toEqual({
      approachCheckYesNoHint: false,
      oneQuestionAtATime: true,
      hintsBeforeSolution: 3,
      maxWords: 400,
    });
    expect(readRules(null).maxWords).toBe(400);
  });
});
