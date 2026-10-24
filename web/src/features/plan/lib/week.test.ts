import { describe, expect, it } from "vitest";
import {
  atMinute,
  columns,
  dayKey,
  duration,
  entryTitle,
  isoWeekOf,
  minuteOfDay,
  parseHhmm,
  parseIsoWeek,
  shiftWeek,
  snap,
  studyWindow,
  weekDays,
  weekStart,
} from "./week";

const tz = "Europe/Amsterdam";

describe("weeks", () => {
  it("names the ISO week in the user's zone", () => {
    // Sunday 23:30 UTC is already Monday in Amsterdam.
    expect(isoWeekOf(new Date("2026-10-25T23:30:00Z"), tz)).toBe("2026-W44");
    expect(isoWeekOf(new Date("2026-10-25T23:30:00Z"), "UTC")).toBe("2026-W43");
  });

  it("parses a week back to its Monday", () => {
    const start = parseIsoWeek("2026-W43", tz)!;
    expect(dayKey(start, tz)).toBe("2026-10-19");
    expect(minuteOfDay(start, tz)).toBe(0);
    expect(parseIsoWeek("2026-W99", tz)).toBeNull();
    expect(parseIsoWeek("nonsense", tz)).toBeNull();
  });

  it("round-trips weekStart and steps across a year", () => {
    const start = weekStart(new Date("2026-10-24T12:00:00Z"), tz);
    expect(dayKey(start, tz)).toBe("2026-10-19");
    expect(weekDays(start).map((d) => dayKey(d, tz))[6]).toBe("2026-10-25");
    expect(shiftWeek("2026-W53", 1, tz)).toBe("2027-W01");
    expect(shiftWeek("2027-W01", -1, tz)).toBe("2026-W53");
  });
});

describe("study window", () => {
  it("reads HH:MM and falls back to 09:00–22:00", () => {
    expect(parseHhmm("07:15")).toBe(435);
    expect(parseHhmm("24:00")).toBe(1440);
    expect(parseHhmm("25:00")).toBeNull();
    expect(parseHhmm(9)).toBeNull();
    expect(studyWindow({ studyStart: "08:00", studyEnd: "18:30" })).toEqual({ start: 480, end: 1110 });
    expect(studyWindow({ studyStart: "10:00", studyEnd: "10:30" })).toEqual({ start: 540, end: 1320 });
    expect(studyWindow(undefined)).toEqual({ start: 540, end: 1320 });
  });
});

describe("times", () => {
  it("snaps to 15 minutes", () => {
    expect(snap(52)).toBe(45);
    expect(snap(53)).toBe(60);
  });

  it("builds instants from the wall clock across DST", () => {
    // 25 October 2026: clocks go back in Amsterdam.
    const day = parseIsoWeek("2026-W43", tz)!;
    const sunday = weekDays(day)[6]!;
    const at = atMinute(sunday, 10 * 60);
    expect(at.toISOString()).toBe("2026-10-25T09:00:00.000Z");
    expect(minuteOfDay(at, tz)).toBe(600);
  });

  it("formats durations", () => {
    expect(duration(45)).toBe("45 min");
    expect(duration(90)).toBe("1 h 30");
    expect(duration(120)).toBe("2 h");
  });
});

describe("columns", () => {
  it("puts overlaps side by side and leaves the rest full width", () => {
    const c = columns([
      { id: "a", start: 540, end: 600 },
      { id: "b", start: 570, end: 630 },
      { id: "c", start: 600, end: 660 },
      { id: "d", start: 700, end: 760 },
    ]);
    expect(c.get("a")).toEqual({ col: 0, of: 2 });
    expect(c.get("b")).toEqual({ col: 1, of: 2 });
    expect(c.get("c")).toEqual({ col: 0, of: 2 });
    expect(c.get("d")).toEqual({ col: 0, of: 1 });
  });
});

describe("entry titles", () => {
  it("shows the course code once", () => {
    expect(entryTitle({ courseCode: "BSA", description: "BSA L1 Descriptive statistics" })).toEqual({
      code: null,
      text: "BSA L1 Descriptive statistics",
    });
    expect(entryTitle({ courseCode: "BSA", description: "Practise 4.1" })).toEqual({
      code: "BSA",
      text: "Practise 4.1",
    });
    expect(entryTitle({ courseCode: "BSA", description: "BSAx" })).toEqual({ code: "BSA", text: "BSAx" });
  });
});
