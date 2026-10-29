// Error codes to plain language (frontend-spec 2.2). Messages say what happened and what to do.

import { ApiError } from "./api";

const MESSAGES: Record<string, string> = {
  validation_failed: "Some fields need attention.",
  unauthenticated: "Sign in to continue.",
  not_found: "Not found, or you don't have access.",
  conflict: "This changed somewhere else.",
  invite_invalid: "This invite can't be used.",
  email_unverified: "Verify your email address first.",
  rate_limited: "Too many attempts. Try again in a moment.",
  ai_disabled: "AI is off here.",
  ai_budget_exceeded: "This month's AI spending cap is reached. Raise it in Settings > AI.",
  mock_time_over: "Time is up for this mock.",
  quota_exceeded: "You're out of storage space.",
  internal: "Something went wrong on our side. Try again.",
  service_unavailable: "That part of Crux is unavailable right now.",
};

/** One sentence for the user. */
export function messageFor(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.code === "rate_limited") {
      const s = retryAfter(e);
      if (s) return `Too many attempts. Try again in ${s} s.`;
    }
    return MESSAGES[e.code] ?? e.message;
  }
  // fetch rejects with a TypeError when offline; a TypeError from a bug in the page is not that.
  if (e instanceof TypeError && /fetch|network|load failed/i.test(e.message)) {
    return "Can't reach Crux. Check your connection.";
  }
  return MESSAGES.internal!;
}

/** `details.retryAfterSecs` of a 429, if any. */
export function retryAfter(e: ApiError): number | null {
  const v = e.details?.retryAfterSecs;
  return typeof v === "number" ? v : null;
}

/** Field errors of a 422 as `{field: message}` (dotted fields stay dotted: `extra.studyEnd`). */
export function fieldErrors(e: unknown): Record<string, string> {
  if (!(e instanceof ApiError) || e.code !== "validation_failed") return {};
  const fields = e.details?.fields;
  const out: Record<string, string> = {};
  if (Array.isArray(fields)) {
    for (const f of fields as { field?: string; message?: string }[]) {
      if (f.field && !(f.field in out)) out[f.field] = f.message ?? "Check this field.";
    }
  }
  return out;
}

/** Field codes of a 422, e.g. to spot the `breached` password warning. */
export function fieldCodes(e: unknown): Record<string, string> {
  if (!(e instanceof ApiError) || e.code !== "validation_failed") return {};
  const fields = e.details?.fields;
  const out: Record<string, string> = {};
  if (Array.isArray(fields)) {
    for (const f of fields as { field?: string; code?: string }[]) {
      if (f.field && f.code && !(f.field in out)) out[f.field] = f.code;
    }
  }
  return out;
}

/** The resource as it is now, from a 409 (for the "Changed somewhere else" dialog). */
export function conflictCurrent<T>(e: unknown): T | null {
  if (!(e instanceof ApiError) || e.code !== "conflict") return null;
  return (e.details?.current as T | undefined) ?? null;
}
