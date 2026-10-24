import { describe, expect, it } from "vitest";
import type { Item } from "@/features/mocks/api";
import { answered, crossed, fromItem, loadPending, navState, remaining, savePending, scoreText } from "./taking";

describe("timer", () => {
  it("counts down from the server's end time, corrected for clock skew", () => {
    const now = Date.parse("2026-10-24T10:00:00Z");
    // The server is 5 s ahead of this device.
    expect(remaining("2026-10-24T10:10:00Z", 5000, now)).toBe(595_000);
    expect(remaining("2026-10-24T09:00:00Z", 0, now)).toBe(0);
    expect(remaining(null, 0, now)).toBeNull();
  });

  it("notices the 10- and 1-minute marks once", () => {
    expect(crossed(600_500, 599_500)).toBe(10);
    expect(crossed(599_500, 598_500)).toBeNull();
    expect(crossed(60_100, 59_000)).toBe(1);
    expect(crossed(700_000, 30_000)).toBe(1);
    expect(crossed(null, 1)).toBeNull();
    // A 10-minute mock starts at 10:00: no "10 minutes left".
    expect(crossed(600_500, 599_500, 10)).toBeNull();
    expect(crossed(60_100, 59_000, 10)).toBe(1);
  });
});

describe("answers", () => {
  it("knows what counts as answered", () => {
    expect(answered({ choiceOptionId: "o1", flagged: false }, "MC")).toBe(true);
    expect(answered({ numeric: 0, flagged: false }, "Numeric")).toBe(true);
    expect(answered({ text: "  ", flagged: false }, "Open")).toBe(false);
    expect(answered(undefined, "MC")).toBe(false);
  });

  it("puts flags before answered in the navigator", () => {
    expect(navState({ choiceOptionId: "o1", flagged: true }, "MC")).toBe("flagged");
    expect(navState({ choiceOptionId: "o1", flagged: false }, "MC")).toBe("answered");
    expect(navState(undefined, "Open")).toBe("blank");
  });

  it("turns a saved answer back into input", () => {
    const item = { answer: { flagged: true, numeric: 2, unit: "kg" } } as Item;
    expect(fromItem(item)).toEqual({
      choiceOptionId: null,
      numeric: 2,
      unit: "kg",
      text: null,
      confidence: null,
      flagged: true,
    });
  });
});

describe("pending store", () => {
  const store = () => {
    const m = new Map<string, string>();
    return {
      getItem: (k: string) => m.get(k) ?? null,
      setItem: (k: string, v: string) => void m.set(k, v),
      removeItem: (k: string) => void m.delete(k),
      size: () => m.size,
    } as unknown as Storage & { size: () => number };
  };

  it("keeps unsaved answers per mock and clears when empty", () => {
    const s = store();
    savePending("m1", { 2: { choiceOptionId: "o", flagged: false } }, s);
    expect(loadPending("m1", s)).toEqual({ 2: { choiceOptionId: "o", flagged: false } });
    expect(loadPending("m2", s)).toEqual({});
    savePending("m1", {}, s);
    expect((s as unknown as { size: () => number }).size()).toBe(0);
  });

  it("survives junk and a missing store", () => {
    const s = store();
    s.setItem("crux.mock.m1.pending", "{nope");
    expect(loadPending("m1", s)).toEqual({});
    expect(loadPending("m1", undefined)).toEqual({});
  });
});

describe("score", () => {
  it("formats halves and blanks", () => {
    expect(scoreText(6.5, 10)).toBe("6.5/10");
    expect(scoreText(7, 10)).toBe("7/10");
    expect(scoreText(null, 10)).toBe("–");
  });
});
