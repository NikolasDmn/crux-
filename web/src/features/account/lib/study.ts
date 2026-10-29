import type { Schemas } from "@/lib/api";
import { readChosen } from "@/features/courses/lib/scales";
import type { Settings } from "../api";

// The Study settings form (frontend-spec 4) and its `/me/settings` patch.

const DEFAULT_HOURS = 4;

/** "00:00" … "24:00" in 15-minute steps. */
export const TIMES = Array.from({ length: 97 }, (_, i) => {
  const m = i * 15;
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
});

export interface StudyForm {
  retention: string;
  newPerDay: string;
  reviewLimit: string;
  realLayout: boolean;
  goEasy: boolean;
  studyStart: string;
  studyEnd: string;
  dailyHours: string;
  oneCourse: boolean;
  /** Built-in grade scales to offer in pickers; null = all (Settings > Study > Grade scales). */
  gradeScales: string[] | null;
}

export function fromSettings(s: Settings): StudyForm {
  const x = s.extra as Record<string, unknown>;
  const str = (v: unknown, d: string) => (typeof v === "string" && TIMES.includes(v) ? v : d);
  return {
    retention: String(Math.round(s.flashRetention * 100)),
    newPerDay: String(s.flashNewPerDay),
    reviewLimit: String(s.flashReviewLimit),
    realLayout: s.mockRealLayout,
    goEasy: s.mockGoEasy,
    studyStart: str(x.studyStart, "09:00"),
    studyEnd: str(x.studyEnd, "22:00"),
    dailyHours: String(typeof x.dailyHours === "number" ? x.dailyHours : DEFAULT_HOURS),
    oneCourse: x.multiCoursePerDay !== true,
    gradeScales: readChosen(x),
  };
}

function withoutScales(x: Record<string, unknown>): Record<string, unknown> {
  const { gradeScales: _, ...rest } = x;
  return rest;
}

/** The patch for `/me/settings`. `extra` replaces the stored object, so other keys are kept. */
export function toPatch(f: StudyForm, current: Settings) {
  return {
    flashRetention: Number(f.retention) / 100,
    flashNewPerDay: Number(f.newPerDay),
    flashReviewLimit: Number(f.reviewLimit),
    mockRealLayout: f.realLayout,
    mockGoEasy: f.goEasy,
    extra: {
      ...withoutScales(current.extra as Record<string, unknown>),
      studyStart: f.studyStart,
      studyEnd: f.studyEnd,
      dailyHours: Number(f.dailyHours),
      multiCoursePerDay: !f.oneCourse,
      // Only once chosen: absent means all of them.
      ...(f.gradeScales ? { gradeScales: f.gradeScales } : {}),
      // The schema types `extra` as a closed object; it's free-form JSON.
    } as unknown as Schemas["SettingsPatch"]["extra"],
  };
}
