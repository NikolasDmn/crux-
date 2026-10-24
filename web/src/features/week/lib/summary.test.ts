import { describe, expect, it } from "vitest";
import { barRows, heat, hours, readSummary } from "./summary";

const raw = {
  isoWeek: "2026-W43",
  from: "2026-10-19",
  to: "2026-10-25",
  days: [
    {
      date: "2026-10-19",
      stepsDone: 3,
      courses: [
        { courseId: "a", plannedHours: 2, actualHours: 1.5, slips: 1 },
        { courseId: "b", plannedHours: 1, actualHours: 0, slips: 0 },
      ],
    },
    { date: "2026-10-20", stepsDone: 0, courses: [] },
  ],
  courses: [
    {
      courseId: "a",
      code: "BSA",
      plannedHours: 2,
      actualHours: 1.5,
      mcFirstTry: { tries: 10, rights: 7 },
      slips: 1,
      weakSpotsOpened: 1,
      weakSpotsResolved: 0,
      nextExamAt: "2026-11-02T08:00:00Z",
      onTrack: false,
    },
  ],
  totals: { plannedHours: 3, actualHours: 1.5, stepsDone: 3, slips: 1, mcFirstTryRate: 0.7 },
};

describe("week summary", () => {
  it("reads the nightly summary", () => {
    const s = readSummary(raw);
    expect(s.days).toHaveLength(2);
    expect(s.courses[0]).toMatchObject({ code: "BSA", mcTries: 10, mcRights: 7, onTrack: false });
    expect(s.totals.mcFirstTryRate).toBe(0.7);
  });

  it("survives missing and wrong-typed fields", () => {
    const s = readSummary({ days: [{ courses: [{ actualHours: "x" }] }], courses: "nope" });
    expect(s.days[0]?.courses[0]?.actualHours).toBe(0);
    expect(s.courses).toEqual([]);
    expect(s.totals.mcFirstTryRate).toBeNull();
    expect(readSummary(null).days).toEqual([]);
  });

  it("builds stacked bar rows per day", () => {
    const rows = barRows(readSummary(raw), (d) => d.slice(-2));
    expect(rows[0]).toEqual({ label: "19", planned: 3, a: 1.5, b: 0 });
    expect(rows[1]).toEqual({ label: "20", planned: 0 });
  });

  it("scales the heatmap and formats hours", () => {
    expect(heat(0, 3)).toBe(0);
    expect(heat(0.1, 3)).toBe(1);
    expect(heat(3, 3)).toBe(4);
    expect(hours(0.5)).toBe("30 min");
    expect(hours(2)).toBe("2 h");
    expect(hours(1.25)).toBe("1.3 h");
  });
});
