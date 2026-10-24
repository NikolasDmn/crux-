// The plan calendar's pure parts (frontend-spec 4, Plan): weeks in the user's time zone, the study
// window, 15-minute snapping, and side-by-side columns for overlapping entries.

import { TZDate } from "@date-fns/tz";
import { addDays, addWeeks, getISOWeek, getISOWeekYear, startOfISOWeek } from "date-fns";

export const SNAP_MIN = 15;
export const DEFAULT_WINDOW = { start: 9 * 60, end: 22 * 60 };

/** Monday 00:00 of the week holding `at`, in `tz`. */
export function weekStart(at: Date, tz: string): TZDate {
  return startOfISOWeek(new TZDate(at, tz));
}

/** "2026-W43" for the week holding `at` in `tz`. */
export function isoWeekOf(at: Date, tz: string): string {
  const d = new TZDate(at, tz);
  return `${getISOWeekYear(d)}-W${String(getISOWeek(d)).padStart(2, "0")}`;
}

/** Monday 00:00 of an ISO week ("2026-W43") in `tz`; `null` when it doesn't parse. */
export function parseIsoWeek(w: string, tz: string): TZDate | null {
  const m = /^(\d{4})-W(\d{2})$/.exec(w);
  if (!m) return null;
  const year = Number(m[1]);
  const week = Number(m[2]);
  if (week < 1 || week > 53) return null;
  // 4 January is always in week 1.
  const start = addWeeks(startOfISOWeek(new TZDate(year, 0, 4, tz)), week - 1) as TZDate;
  return getISOWeekYear(start) === year ? start : null;
}

export function shiftWeek(w: string, by: number, tz: string): string {
  const start = parseIsoWeek(w, tz);
  return start ? isoWeekOf(addWeeks(start, by), tz) : w;
}

export function weekDays(start: TZDate): TZDate[] {
  return Array.from({ length: 7 }, (_, i) => addDays(start, i));
}

/** "09:30" to minutes after midnight; `null` when it doesn't parse. */
export function parseHhmm(s: unknown): number | null {
  if (typeof s !== "string") return null;
  const m = /^(\d{1,2}):(\d{2})$/.exec(s);
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  return h <= 24 && min < 60 && h * 60 + min <= 24 * 60 ? h * 60 + min : null;
}

/** The study window from `/me/settings` extra (`studyStart`, `studyEnd`), default 09:00–22:00. */
export function studyWindow(extra: Record<string, unknown> | undefined): { start: number; end: number } {
  const start = parseHhmm(extra?.studyStart);
  const end = parseHhmm(extra?.studyEnd);
  return start !== null && end !== null && end - start >= 60 ? { start, end } : DEFAULT_WINDOW;
}

/** Minutes after local midnight in `tz`. */
export function minuteOfDay(at: Date | string, tz: string): number {
  const d = new TZDate(typeof at === "string" ? new Date(at) : at, tz);
  return d.getHours() * 60 + d.getMinutes();
}

export function snap(minutes: number, step = SNAP_MIN): number {
  return Math.round(minutes / step) * step;
}

export function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}

/** The instant `minutes` after `day`'s local midnight (DST-safe: built from the wall clock). */
export function atMinute(day: TZDate, minutes: number): Date {
  const local = new TZDate(
    day.getFullYear(),
    day.getMonth(),
    day.getDate(),
    Math.floor(minutes / 60),
    minutes % 60,
    day.timeZone,
  );
  // A plain Date, so toISOString() is UTC for the API.
  return new Date(local.getTime());
}

/** The local date key ("2026-10-24") of an instant, for grouping entries by day. */
export function dayKey(at: Date | string, tz: string): string {
  const d = new TZDate(typeof at === "string" ? new Date(at) : at, tz);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export interface Span {
  id: string;
  start: number;
  end: number;
}

/** Side-by-side columns for overlapping spans: each gets a column and how many share its cluster. */
export function columns<T extends Span>(spans: T[]): Map<string, { col: number; of: number }> {
  const out = new Map<string, { col: number; of: number }>();
  const sorted = [...spans].sort((a, b) => a.start - b.start || b.end - a.end);
  let cluster: { id: string; col: number }[] = [];
  let clusterEnd = -Infinity;
  let ends: number[] = [];
  const flush = () => {
    const of = Math.max(1, ends.length);
    for (const c of cluster) out.set(c.id, { col: c.col, of });
    cluster = [];
    ends = [];
  };
  for (const s of sorted) {
    if (s.start >= clusterEnd) {
      flush();
      clusterEnd = -Infinity;
    }
    let col = ends.findIndex((e) => e <= s.start);
    if (col === -1) {
      col = ends.length;
      ends.push(s.end);
    } else {
      ends[col] = s.end;
    }
    cluster.push({ id: s.id, col });
    clusterEnd = Math.max(clusterEnd, s.end);
  }
  flush();
  return out;
}

/** "1 h 30", "45 min". */
export function duration(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = Math.round(minutes % 60);
  if (!h) return `${m} min`;
  return m ? `${h} h ${String(m).padStart(2, "0")}` : `${h} h`;
}

/** "9:00" style label for a minute of the day. */
export function hhmm(minutes: number): string {
  return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}`;
}

/** An entry's label without the course code twice ("BSA L1 …" already names its course). */
export function entryTitle(e: { courseCode: string; description: string }): { code: string | null; text: string } {
  const d = e.description.trim();
  return d.toUpperCase().startsWith(`${e.courseCode.toUpperCase()} `) || d.toUpperCase() === e.courseCode.toUpperCase()
    ? { code: null, text: d }
    : { code: e.courseCode, text: d };
}
