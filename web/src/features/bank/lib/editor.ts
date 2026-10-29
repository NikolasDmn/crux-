// The question editor's form (frontend-spec 4, question editor): form state to the API's version
// content and back, the checks that run before saving, and a line diff for version history.

import type { Question, Version, VersionInput } from "@/features/bank/api";

export type QType = "MC" | "Numeric" | "Open";

export interface OptionRow {
  text: string;
  isCorrect: boolean;
  distractorNote: string;
}

export interface Form {
  type: QType;
  componentId: string;
  prompt: string;
  options: OptionRow[];
  shuffleOptions: boolean;
  /** "Choose all that apply": one or more correct options. */
  multiSelect: boolean;
  value: string;
  tolerance: string;
  toleranceKind: "Relative" | "Absolute";
  unit: string;
  unitRequired: boolean;
  modelAnswer: string;
  rubric: { criterion: string; points: string }[];
  rubricNote: string;
  explanation: string;
  keyReliability: "Official" | "Derived" | "Disputed";
  keyNote: string;
  conceptIds: string[];
  primaryConceptId: string | null;
  paperId: string;
  paperQuestionNo: string;
  source: string;
  sourceLabel: string;
  private: boolean;
  changeNote: string;
  imageFileId: string | null;
}

const blankOption = (): OptionRow => ({ text: "", isCorrect: false, distractorNote: "" });

export function emptyForm(componentId = ""): Form {
  return {
    type: "MC",
    componentId,
    prompt: "",
    options: [blankOption(), blankOption(), blankOption(), blankOption()],
    shuffleOptions: true,
    multiSelect: false,
    value: "",
    tolerance: "",
    toleranceKind: "Relative",
    unit: "",
    unitRequired: false,
    modelAnswer: "",
    rubric: [],
    rubricNote: "",
    explanation: "",
    keyReliability: "Derived",
    keyNote: "",
    conceptIds: [],
    primaryConceptId: null,
    paperId: "",
    paperQuestionNo: "",
    source: "Manual",
    sourceLabel: "",
    private: false,
    changeNote: "",
    imageFileId: null,
  };
}

export function formFrom(q: Question): Form {
  const v = q.current;
  const f = emptyForm(q.componentId);
  return {
    ...f,
    type: q.type as QType,
    prompt: v.prompt,
    options: (v.options ?? []).map((o) => ({
      text: o.text,
      isCorrect: o.isCorrect,
      distractorNote: o.distractorNote ?? "",
    })),
    shuffleOptions: v.shuffleOptions ?? true,
    multiSelect: v.multiSelect ?? false,
    value: v.numeric ? String(v.numeric.value) : "",
    tolerance: v.numeric ? String(v.numeric.tolerance) : "",
    toleranceKind: v.numeric?.toleranceKind === "Absolute" ? "Absolute" : "Relative",
    unit: v.numeric?.unit ?? "",
    unitRequired: v.numeric?.unitRequired ?? false,
    modelAnswer: v.numeric?.modelAnswer ?? v.open?.modelAnswer ?? "",
    rubric: rubricRows(v.open?.rubric),
    rubricNote: v.open?.rubricNote ?? "",
    explanation: v.explanation ?? "",
    keyReliability: (v.keyReliability as Form["keyReliability"]) ?? "Derived",
    keyNote: v.keyNote ?? "",
    conceptIds: q.concepts.map((c) => c.id),
    primaryConceptId: q.concepts.find((c) => c.primary)?.id ?? null,
    paperId: q.paperId ?? "",
    paperQuestionNo: q.paperQuestionNo ?? "",
    source: q.source,
    sourceLabel: q.sourceLabel ?? "",
    private: q.private,
    imageFileId: v.imageFileId ?? null,
  };
}

function rubricRows(r: unknown): Form["rubric"] {
  if (!Array.isArray(r)) return [];
  return r
    .filter((x): x is { criterion: string; points: number } => !!x && typeof x === "object" && "criterion" in x)
    .map((x) => ({ criterion: String(x.criterion), points: String(x.points ?? "") }));
}

const num = (s: string): number | null => {
  const n = Number(s.trim().replace(",", "."));
  return s.trim() && Number.isFinite(n) ? n : null;
};

/** Problems to fix before saving, by field; empty when the form can be sent. */
export function check(f: Form): Record<string, string> {
  const e: Record<string, string> = {};
  if (!f.componentId) e.componentId = "Pick the lecture.";
  if (!f.prompt.trim()) e.prompt = "Write the question.";
  if (f.type === "MC") {
    const filled = f.options.filter((o) => o.text.trim());
    if (filled.length < 2) e.options = "Give at least two options.";
    else if (f.multiSelect && !filled.some((o) => o.isCorrect)) e.options = "Mark at least one option correct.";
    else if (!f.multiSelect && filled.filter((o) => o.isCorrect).length !== 1)
      e.options = "Mark exactly one option correct.";
  }
  if (f.type === "Numeric") {
    if (num(f.value) === null) e.value = "Enter the answer as a number.";
    if (f.tolerance.trim() && (num(f.tolerance) === null || num(f.tolerance)! < 0))
      e.tolerance = "A number, 0 or more.";
    if (f.unitRequired && !f.unit.trim()) e.unit = "Give the unit, or don't require one.";
  }
  if (f.rubric.some((r) => r.criterion.trim() && num(r.points) === null)) e.rubric = "Each criterion needs points.";
  return e;
}

/** The version content the API takes. */
export function toContent(f: Form): VersionInput {
  const content: VersionInput = {
    prompt: f.prompt.trim(),
    explanation: f.explanation.trim() || null,
    keyReliability: f.keyReliability,
    keyNote: f.keyNote.trim() || null,
    imageFileId: f.imageFileId,
    changeNote: f.changeNote.trim() || null,
  };
  if (f.type === "MC") {
    content.options = f.options
      .filter((o) => o.text.trim())
      .map((o) => ({ text: o.text.trim(), isCorrect: o.isCorrect, distractorNote: o.distractorNote.trim() || null }));
    content.shuffleOptions = f.shuffleOptions;
    content.multiSelect = f.multiSelect;
  }
  if (f.type === "Numeric") {
    content.numeric = {
      value: num(f.value) ?? 0,
      tolerance: num(f.tolerance) ?? 0,
      toleranceKind: f.toleranceKind,
      unit: f.unit.trim() || null,
      unitRequired: f.unitRequired,
      modelAnswer: f.modelAnswer.trim() || null,
    };
  }
  if (f.type === "Open") {
    const rubric = f.rubric
      .filter((r) => r.criterion.trim())
      .map((r) => ({ criterion: r.criterion.trim(), points: num(r.points) ?? 0 }));
    content.open = {
      modelAnswer: f.modelAnswer.trim() || null,
      rubric: rubric.length ? rubric : null,
      rubricNote: f.rubricNote.trim() || null,
    };
  }
  return content;
}

/** A version as plain text lines, for the history diff. */
export function versionText(v: Version): string[] {
  const lines = v.prompt.split("\n");
  for (const [i, o] of (v.options ?? []).entries()) {
    lines.push(`${String.fromCharCode(65 + i)}. ${o.text}${o.isCorrect ? "  ✓" : ""}`);
  }
  if (v.numeric) lines.push(`Answer: ${v.numeric.value} ± ${v.numeric.tolerance} ${v.numeric.unit ?? ""}`.trimEnd());
  const model = v.numeric?.modelAnswer ?? v.open?.modelAnswer;
  if (model) lines.push("Model answer:", ...model.split("\n"));
  if (v.explanation) lines.push("Explanation:", ...v.explanation.split("\n"));
  return lines;
}

export type DiffLine = { op: "same" | "add" | "del"; text: string };

/** Line diff (longest common subsequence), old to new. */
export function diffLines(a: string[], b: string[]): DiffLine[] {
  const n = a.length;
  const m = b.length;
  const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--)
    for (let j = m - 1; j >= 0; j--)
      lcs[i]![j] = a[i] === b[j] ? lcs[i + 1]![j + 1]! + 1 : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
  const out: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      out.push({ op: "same", text: a[i]! });
      i++;
      j++;
    } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) out.push({ op: "del", text: a[i++]! });
    else out.push({ op: "add", text: b[j++]! });
  }
  while (i < n) out.push({ op: "del", text: a[i++]! });
  while (j < m) out.push({ op: "add", text: b[j++]! });
  return out;
}
