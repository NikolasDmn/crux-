// Question bank filters (frontend-spec 4, bank): what the URL holds, what the API takes, and the
// export of the filtered list as Markdown or JSON, built in the browser (0.1).

import type { Question } from "@/features/bank/api";

export interface BankFilters {
  componentId?: string | undefined;
  type?: string | undefined;
  source?: string | undefined;
  paperId?: string | undefined;
  keyReliability?: string | undefined;
  reviewState?: string | undefined;
  untested?: boolean | undefined;
  includeArchived?: boolean | undefined;
  q?: string | undefined;
}

const STRINGS = ["componentId", "type", "source", "paperId", "keyReliability", "reviewState", "q"] as const;

/** From the route's search params: unknown keys and empty values dropped. */
export function readFilters(s: Record<string, unknown>): BankFilters {
  const f: BankFilters = {};
  for (const k of STRINGS) {
    const v = s[k];
    if (typeof v === "string" && v.trim()) f[k] = v.trim();
  }
  if (s.untested === true || s.untested === "true") f.untested = true;
  if (s.includeArchived === true || s.includeArchived === "true") f.includeArchived = true;
  return f;
}

/** The API's query: only the set filters. */
export function toQuery(f: BankFilters): Record<string, string | boolean> {
  const out: Record<string, string | boolean> = {};
  for (const [k, v] of Object.entries(f)) if (v !== undefined && v !== "" && v !== false) out[k] = v;
  return out;
}

export function activeCount(f: BankFilters): number {
  return Object.entries(f).filter(([k, v]) => k !== "q" && v !== undefined && v !== false && v !== "").length;
}

/** The filtered questions as Markdown: prompt, options with the key marked, numeric/open answers. */
export function toMarkdown(qs: Question[], lectureCode: (id: string) => string): string {
  return qs
    .map((q, i) => {
      const v = q.current;
      const head = `## ${i + 1}. ${lectureCode(q.componentId)} · ${q.type}${q.sourceLabel ? ` · ${q.sourceLabel}` : ""}`;
      const lines = [head, "", v.prompt.trim(), ""];
      for (const [n, o] of (v.options ?? []).entries()) {
        lines.push(`${String.fromCharCode(65 + n)}. ${o.text}${o.isCorrect ? " **(correct)**" : ""}`);
      }
      if (v.numeric) lines.push(`**Answer:** ${v.numeric.value}${v.numeric.unit ? ` ${v.numeric.unit}` : ""}`);
      if (v.open?.modelAnswer) lines.push("**Model answer:**", "", v.open.modelAnswer.trim());
      if (v.explanation) lines.push("", `_${v.explanation.trim()}_`);
      return lines.join("\n").trimEnd();
    })
    .join("\n\n");
}

export function toJson(qs: Question[]): string {
  return JSON.stringify(
    qs.map((q) => ({
      id: q.id,
      componentId: q.componentId,
      type: q.type,
      source: q.source,
      sourceLabel: q.sourceLabel ?? null,
      paperQuestionNo: q.paperQuestionNo ?? null,
      prompt: q.current.prompt,
      options: q.current.options ?? null,
      numeric: q.current.numeric ?? null,
      open: q.current.open ?? null,
      explanation: q.current.explanation ?? null,
      keyReliability: q.current.keyReliability,
    })),
    null,
    2,
  );
}

/** Offer `text` as a file download (a blob URL, same origin). */
export function download(name: string, text: string, type: string): void {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/** One line of a markdown prompt for a table cell: maths kept as written, markup and breaks dropped. */
export function plainLine(md: string, max = 140): string {
  const s = md
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[*_`#>]+/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return s.length > max ? `${s.slice(0, max - 1)}…` : s;
}

export type LinePiece = { text: string } | { tex: string };

/**
 * A one-line preview (plainLine) as text and inline-maths pieces, so lists show "C₄H₈" and not
 * "C$4$H$8$". Accepts $…$ and \(…\); a lone $ stays text. `max` counts visible characters (a
 * formula as half its source) and never cuts a formula in half.
 */
export function linePieces(md: string, max = 140): LinePiece[] {
  // Formulas are found before markdown is stripped: "_" and "*" mean something inside them.
  const line = md
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\s+/g, " ")
    .trim();
  const re = /\$([^$]+)\$|\\\((.+?)\\\)/g;
  const out: LinePiece[] = [];
  let used = 0;
  let at = 0;
  const pushText = (raw: string) => {
    const t = raw.replace(/[*_`#>]+/g, "");
    if (!t) return true;
    const room = max - used;
    if (t.length > room) {
      out.push({ text: `${t.slice(0, Math.max(0, room - 1))}…` });
      used = max;
      return false;
    }
    out.push({ text: t });
    used += t.length;
    return true;
  };
  for (const m of line.matchAll(re)) {
    if (!pushText(line.slice(at, m.index))) return out;
    const tex = (m[1] ?? m[2] ?? "").trim();
    const cost = Math.ceil(tex.length / 2);
    if (used + cost > max) {
      out.push({ text: "…" });
      return out;
    }
    out.push({ tex });
    used += cost;
    at = m.index + m[0].length;
  }
  pushText(line.slice(at));
  return out;
}
