// The AI screens' pure parts: money, gate reasons, feature names and tutor rules.

import type { AiFeature, AiReason } from "../api";

/** Micro-dollars to "$0.0042" / "$1.20": small sums keep enough digits to be non-zero. */
export function usd(micro: number): string {
  const d = micro / 1_000_000;
  if (d === 0) return "$0";
  if (d < 0.01) return `$${d.toPrecision(2)}`;
  return `$${d.toFixed(2)}`;
}

/** "$2.00 in · $10.00 out per 1M tokens". */
export function priceLine(p: { inputPerMtok: number; outputPerMtok: number }): string {
  return `$${p.inputPerMtok.toFixed(2)} in · $${p.outputPerMtok.toFixed(2)} out per 1M tokens`;
}

/** A dollar cap typed by the user to micro-dollars; empty means no cap, anything else invalid is null. */
export function capToMicro(text: string): number | null | "none" {
  const s = text.trim().replace(/^\$/, "");
  if (!s) return "none";
  if (!/^\d+(\.\d{1,2})?$/.test(s)) return null;
  return Math.round(Number(s) * 1_000_000);
}

export const FEATURE_LABEL: Record<AiFeature, string> = {
  MaterialChat: "Ask AI",
  Grading: "Grading open answers",
  CourseImport: "Set up a course from PDFs",
  PastExamImport: "Past exams into questions",
  Generation: "Generating practice questions",
};

export const PROVIDER_LABEL: Record<string, string> = {
  anthropic: "Anthropic",
  openai: "OpenAI",
  google: "Google",
  mistral: "Mistral",
  fake: "Fake (dev only)",
};

/** Why a control is disabled, for its tooltip; the link fixes it. `null` for the hidden states. */
export function reasonText(reason: AiReason | null, courseId?: string): { text: string; to: string } | null {
  switch (reason) {
    case "feature_off":
      return { text: "This AI feature is off in your settings.", to: "/settings/ai" };
    case "no_key":
      return { text: "Add an API key to use AI.", to: "/settings/ai" };
    case "course_off":
      return {
        text: "AI is off for this course.",
        to: courseId ? `/c/${courseId}/settings` : "/settings/ai",
      };
    case "material_blocked":
      return { text: "This material is blocked from AI.", to: courseId ? `/c/${courseId}/lectures` : "/settings/ai" };
    default:
      return null;
  }
}

export const PERSONALITIES = ["Direct", "Warm", "Socratic", "Playful", "Formal"] as const;
export const STYLES = ["HintFirst", "ExplainFirst", "QuizMe", "WorkedExamples"] as const;
export const STYLE_LABEL: Record<string, string> = {
  HintFirst: "Hints first",
  ExplainFirst: "Explain first",
  QuizMe: "Quiz me",
  WorkedExamples: "Worked examples",
};

export interface TutorRules {
  approachCheckYesNoHint: boolean;
  oneQuestionAtATime: boolean;
  hintsBeforeSolution: number;
  maxWords: number;
}

/** The rules the AI service accepts, with their ranges (7.3); unknown or out-of-range values fall back. */
export function readRules(r: Record<string, unknown> | null | undefined): TutorRules {
  const int = (v: unknown, lo: number, hi: number, d: number) =>
    typeof v === "number" && Number.isInteger(v) && v >= lo && v <= hi ? v : d;
  return {
    approachCheckYesNoHint: r?.approachCheckYesNoHint === true,
    oneQuestionAtATime: r?.oneQuestionAtATime === true,
    hintsBeforeSolution: int(r?.hintsBeforeSolution, 0, 5, 0),
    maxWords: int(r?.maxWords, 50, 1000, 400),
  };
}

export const MAX_TUTOR_TEXT = 2000;
