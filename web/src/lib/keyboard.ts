// Keyboard maps (frontend-spec 2.6). Digits use `event.code`, so Shift and Alt don't change them.

export type Confidence = "Sure" | "Unsure" | "Guess";

export type MockKey = { kind: "option"; index: number; confidence: Confidence } | { kind: "next" } | { kind: "flag" };

export interface KeyLike {
  code: string;
  key: string;
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}

function digit(code: string): number | null {
  const m = /^(?:Digit|Numpad)([1-9])$/.exec(code);
  return m ? Number(m[1]) : null;
}

/** Mock taking: 1–9 choose (Sure), Shift+digit Unsure, Alt/Option+digit Guess; Space next; F flag. */
export function mockKey(e: KeyLike): MockKey | null {
  if (e.ctrlKey || e.metaKey) return null;
  const d = digit(e.code);
  if (d !== null) {
    const confidence: Confidence = e.altKey ? "Guess" : e.shiftKey ? "Unsure" : "Sure";
    return { kind: "option", index: d - 1, confidence };
  }
  if (e.code === "Space" && !e.shiftKey && !e.altKey) return { kind: "next" };
  if (e.code === "KeyF" && !e.shiftKey && !e.altKey) return { kind: "flag" };
  return null;
}

export type Rating = "Again" | "Hard" | "Good" | "Easy";
export const RATINGS: readonly Rating[] = ["Again", "Hard", "Good", "Easy"];

export type FlashKey = { kind: "reveal" } | { kind: "rate"; rating: Rating };

/** Flashcards: Space reveals; 1–4 rate Again, Hard, Good, Easy. */
export function flashKey(e: KeyLike): FlashKey | null {
  if (e.ctrlKey || e.metaKey || e.altKey) return null;
  if (e.code === "Space") return { kind: "reveal" };
  const d = digit(e.code);
  if (d !== null && d <= 4) return { kind: "rate", rating: RATINGS[d - 1]! };
  return null;
}

/** True when focus is in a text field, where single-key shortcuts must not fire. */
export function typingIn(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || !el.tagName) return false;
  const tag = el.tagName.toLowerCase();
  return tag === "input" || tag === "textarea" || tag === "select" || el.isContentEditable === true;
}
