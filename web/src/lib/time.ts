// Every time shows in the user's time zone (frontend-spec 2.2), deadlines as relative days.

import { TZDate } from "@date-fns/tz";
import { differenceInCalendarDays, format } from "date-fns";

export function inZone(iso: string | Date, tz: string): TZDate {
  return new TZDate(typeof iso === "string" ? new Date(iso) : iso, tz);
}

/** `pattern` uses date-fns tokens, e.g. `EEE d MMM`, `HH:mm`. */
export function formatInZone(iso: string | Date, tz: string, pattern: string): string {
  return format(inZone(iso, tz), pattern);
}

/** "today", "tomorrow", "in 6 days", "yesterday", "3 days ago", counted in calendar days in `tz`. */
export function relativeDays(iso: string | Date, tz: string, now: Date = new Date()): string {
  const d = differenceInCalendarDays(inZone(iso, tz), inZone(now, tz));
  if (d === 0) return "today";
  if (d === 1) return "tomorrow";
  if (d === -1) return "yesterday";
  return d > 0 ? `in ${d} days` : `${-d} days ago`;
}

/** Compact countdown for the sidebar: "10 d", "5 h", "now". */
export function shortUntil(iso: string | Date, now: Date = new Date()): string {
  const ms = new Date(iso).getTime() - now.getTime();
  if (ms <= 0) return "now";
  const h = ms / 3_600_000;
  return h < 24 ? `${Math.ceil(h)} h` : `${Math.ceil(h / 24)} d`;
}

/** "42:10" or "1:02:05" for a mock timer; never negative. */
export function clock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h ? String(m).padStart(2, "0") : String(m);
  return `${h ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
}

/** Server minus local clock, from a response's `Date` header (mock timers, spec 5.3). */
export function clockOffset(dateHeader: string | null, localNow: number = Date.now()): number {
  if (!dateHeader) return 0;
  const server = Date.parse(dateHeader);
  return Number.isNaN(server) ? 0 : server - localNow;
}

/** The browser's time zone, for signup. */
export function browserZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "Europe/Amsterdam";
  } catch {
    return "Europe/Amsterdam";
  }
}
