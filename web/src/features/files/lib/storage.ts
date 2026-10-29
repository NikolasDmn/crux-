// The storage saver's pure parts (frontend-spec 4 and 5.1): quota wording and the per-course
// breakdown.

import { ApiError } from "@/lib/api";
import { formatBytes } from "./upload";

export interface CourseUsage {
  courseId: string | null;
  bytes: number;
  originals: number;
  purgeable: number;
  pendingPurge: number;
}

export interface UsageRow extends CourseUsage {
  name: string;
  code: string | null;
  /** Only the owner frees up space, so the row links to course settings only for owned courses. */
  owned: boolean;
}

/** "98 MB of 100 MB" from a 413's details, or null when the error isn't a quota error. */
export function quotaDetail(e: unknown): string | null {
  if (!(e instanceof ApiError) || e.code !== "quota_exceeded") return null;
  const used = e.details?.usedBytes;
  const quota = e.details?.quotaBytes;
  if (typeof used !== "number" || typeof quota !== "number") return null;
  return `${formatBytes(used)} of ${formatBytes(quota)} used`;
}

/** Biggest first; files outside any course (or in a course you've left) last. */
export function usageRows(
  usage: CourseUsage[],
  courses: { id: string; name: string; code: string; myRole: string }[],
): UsageRow[] {
  const byId = new Map(courses.map((c) => [c.id, c]));
  return usage
    .map((u) => {
      const c = u.courseId ? byId.get(u.courseId) : undefined;
      return {
        ...u,
        name: c?.name ?? (u.courseId ? "A course you're no longer in" : "Not in a course"),
        code: c?.code ?? null,
        owned: c?.myRole === "Owner",
      };
    })
    .sort((a, b) => Number(!a.code) - Number(!b.code) || b.bytes - a.bytes);
}

/** 0–1, capped: reservations can briefly push used + reserved past the quota. */
export function usedShare(used: number, quota: number): number {
  return quota > 0 ? Math.min(1, used / quota) : 0;
}
