// Check-in results on the course's grade scale (frontend-spec 4, Exams; spec 5.10).

import type { Deadline, GradeScale } from "@/features/courses/api";

export type GradeControl =
  | { kind: "number"; min: number; max: number; step: number; pass: number | null }
  | { kind: "choice"; options: { label: string; pass: boolean }[] };

/** How to ask for a grade on this scale (no scale: Percent). */
export function gradeControl(scale: GradeScale | undefined): GradeControl {
  if (scale && (scale.kind === "PassFail" || scale.kind === "Letter")) {
    const labels = Array.isArray(scale.labels) ? (scale.labels as { label?: unknown; pass?: unknown }[]) : [];
    const options = labels
      .filter((l) => typeof l.label === "string")
      .map((l) => ({ label: l.label as string, pass: l.pass === true }));
    if (options.length) return { kind: "choice", options };
  }
  return {
    kind: "number",
    min: scale?.scaleMin ?? 0,
    max: scale?.scaleMax ?? 100,
    step: scale?.step ?? 1,
    pass: scale?.passValue ?? null,
  };
}

/** The PATCH body for a Done result, or an error line. */
export function gradeBody(
  control: GradeControl,
  raw: string,
): { gradeValue?: number | null; gradeText?: string | null } | { error: string } {
  const v = raw.trim();
  if (!v) return { gradeValue: null, gradeText: null };
  if (control.kind === "choice") {
    return control.options.some((o) => o.label === v) ? { gradeText: v, gradeValue: null } : { error: "Pick a grade." };
  }
  const n = Number(v.replace(",", "."));
  if (!Number.isFinite(n)) return { error: "Enter a number." };
  if (n < control.min || n > control.max) return { error: `Between ${control.min} and ${control.max}.` };
  return { gradeValue: n, gradeText: null };
}

/** Would this grade pass? `null` when it can't tell (no pass mark). */
export function wouldPass(control: GradeControl, raw: string, passMark?: number | null): boolean | null {
  const v = raw.trim();
  if (!v) return null;
  if (control.kind === "choice") return control.options.find((o) => o.label === v)?.pass ?? null;
  const n = Number(v.replace(",", "."));
  const threshold = passMark ?? control.pass;
  return Number.isFinite(n) && threshold !== null ? n >= threshold : null;
}

export function deadlineName(d: Pick<Deadline, "title" | "type">): string {
  return d.title?.trim() || (d.type === "Resit" ? "Resit" : d.type === "Exam" ? "Exam" : "Deadline");
}

/** "7.5", "Pass", "7.5 (provisional)" for a Done deadline. */
export function gradeLabel(d: Pick<Deadline, "gradeValue" | "gradeText" | "gradeProvisional">): string | null {
  const g = d.gradeText ?? (d.gradeValue !== null && d.gradeValue !== undefined ? String(d.gradeValue) : null);
  if (!g) return null;
  return d.gradeProvisional ? `${g} (provisional)` : g;
}
