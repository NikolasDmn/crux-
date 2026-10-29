import { describe, expect, it } from "vitest";
import type { Settings } from "../api";
import { exportStatus } from "../api";
import { fromSettings, TIMES, toPatch } from "./study";

const settings = (extra: Record<string, unknown> = {}): Settings => ({
  flashRetention: 0.9,
  flashNewPerDay: 20,
  flashReviewLimit: 200,
  mockGoEasy: false,
  mockRealLayout: true,
  extra: extra as Settings["extra"],
  updatedAt: "2026-10-29T08:00:00Z",
  version: 3,
});

describe("study settings", () => {
  it("offers 00:00 to 24:00 in 15-minute steps", () => {
    expect(TIMES[0]).toBe("00:00");
    expect(TIMES[37]).toBe("09:15");
    expect(TIMES.at(-1)).toBe("24:00");
  });

  it("fills the planner defaults when extra is empty", () => {
    const f = fromSettings(settings());
    expect(f).toMatchObject({ studyStart: "09:00", studyEnd: "22:00", dailyHours: "4", oneCourse: true, retention: "90" });
  });

  it("round-trips and keeps other extra keys", () => {
    const s = settings({ studyStart: "08:30", dailyHours: 6, multiCoursePerDay: true, theme: "dark" });
    const f = fromSettings(s);
    expect(f).toMatchObject({ studyStart: "08:30", dailyHours: "6", oneCourse: false });
    const p = toPatch({ ...f, retention: "85", oneCourse: true }, s);
    expect(p.flashRetention).toBeCloseTo(0.85);
    expect(p.extra).toEqual({ studyStart: "08:30", studyEnd: "22:00", dailyHours: 6, multiCoursePerDay: false, theme: "dark" });
  });

  it("ignores a stored time that isn't on the 15-minute grid", () => {
    expect(fromSettings(settings({ studyStart: "09:07" })).studyStart).toBe("09:00");
  });
});

describe("exportStatus", () => {
  const now = Date.parse("2026-10-29T12:00:00Z");
  it("calls a Ready export past its 24 h Expired", () => {
    expect(exportStatus({ status: "Ready", expiresAt: "2026-10-29T11:00:00Z" }, now)).toBe("Expired");
    expect(exportStatus({ status: "Ready", expiresAt: "2026-10-30T11:00:00Z" }, now)).toBe("Ready");
    expect(exportStatus({ status: "Building", expiresAt: null }, now)).toBe("Building");
  });
});
