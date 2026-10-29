// Taking a mock (frontend-spec 5.3, must not lose answers): the timer from the server's clock,
// answer states for the navigator, and the unsaved answers kept in sessionStorage until saved.

import type { AnswerInput, Item } from "@/features/mocks/api";

/** Milliseconds left: server `endsAt` against local time corrected by the measured offset. */
export function remaining(endsAt: string | null | undefined, offset: number, now: number = Date.now()): number | null {
  if (!endsAt) return null;
  return Math.max(0, new Date(endsAt).getTime() - (now + offset));
}

/** The warning (10 or 1 minute) crossed between two readings of the timer, if any. A mark at or
 *  past the whole limit (10 minutes into a 10-minute mock) isn't a warning. */
export function crossed(before: number | null, after: number | null, limitMin?: number | null): 10 | 1 | null {
  if (before === null || after === null) return null;
  for (const min of [1, 10] as const) {
    if (limitMin && min >= limitMin) continue;
    const t = min * 60_000;
    if (before > t && after <= t) return min;
  }
  return null;
}

export function answered(a: AnswerInput | undefined, type: string): boolean {
  if (!a) return false;
  if (type === "MC") return !!a.choiceOptionId || !!a.choiceOptionIds?.length;
  if (type === "Numeric") return a.numeric !== null && a.numeric !== undefined;
  return !!a.text?.trim();
}

/** "Choose all that apply": `id` in or out of the chosen ones. */
export function toggleChoice(ids: readonly string[] | null | undefined, id: string): string[] {
  const now = ids ?? [];
  return now.includes(id) ? now.filter((x) => x !== id) : [...now, id];
}

export type NavState = "answered" | "flagged" | "blank";

export function navState(a: AnswerInput | undefined, type: string): NavState {
  if (a?.flagged) return "flagged";
  return answered(a, type) ? "answered" : "blank";
}

/** The saved answer as the input we'd send again. */
export function fromItem(i: Item): AnswerInput {
  const a = i.answer;
  return {
    choiceOptionId: a.choiceOptionId ?? null,
    choiceOptionIds: a.choiceOptionIds?.length ? a.choiceOptionIds : null,
    numeric: a.numeric ?? null,
    unit: a.unit ?? null,
    text: a.text ?? null,
    confidence: a.confidence ?? null,
    flagged: a.flagged,
  };
}

// Unsaved answers, per mock, survive a reload or a dropped connection (sessionStorage; never
// required: the page works without it).
const KEY = (mockId: string) => `crux.mock.${mockId}.pending`;

export function loadPending(mockId: string, store: Storage | undefined = safeSession()): Record<number, AnswerInput> {
  try {
    const raw = store?.getItem(KEY(mockId));
    const v: unknown = raw ? JSON.parse(raw) : null;
    return v && typeof v === "object" ? (v as Record<number, AnswerInput>) : {};
  } catch {
    return {};
  }
}

export function savePending(
  mockId: string,
  pending: Record<number, AnswerInput>,
  store: Storage | undefined = safeSession(),
): void {
  try {
    if (Object.keys(pending).length) store?.setItem(KEY(mockId), JSON.stringify(pending));
    else store?.removeItem(KEY(mockId));
  } catch {
    // Storage full or blocked: answers still go to the server.
  }
}

function safeSession(): Storage | undefined {
  try {
    return window.sessionStorage;
  } catch {
    return undefined;
  }
}

/** "7/10", "6.5/10" for a score. */
export function scoreText(score: number | null | undefined, max: number | null | undefined): string {
  if (score === null || score === undefined || !max) return "–";
  const s = Number.isInteger(score) ? String(score) : score.toFixed(1);
  return `${s}/${Number.isInteger(max) ? max : max.toFixed(1)}`;
}
