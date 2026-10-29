// The Ask AI panel's pure parts (frontend-spec 5.5): what a conversation is about, the streamed
// reply, and `[p.N]` citations turned into links the panel renders as chips.

export type Scope = "Material" | "Component" | "Question" | "Mock" | "Course";

/** What the panel is opened on. `anchor` is the selected text in the viewer, or a mock question. */
export interface AskTarget {
  courseId: string;
  scope: Scope;
  materialId?: string;
  componentId?: string;
  questionId?: string;
  mockId?: string;
  /** Shown in the panel header ("L3 Variance", "Question 4"). */
  title: string;
  anchor?: { fileRef: string; page: number; quote: string } | { order: number };
}

export interface ContextUsed {
  pages?: { fileId: string; page: number | null; quote: string }[];
  weakSpots?: { id: string; label: string | null }[];
  slips?: { id: string; label: string | null }[];
  traps?: { id: string; label: string | null }[];
  questionId?: string | null;
  shared?: string[];
}

export interface Citation {
  fileId: string;
  page: number | null;
  chunkNo: number;
  quote: string | null;
}

/** A reply as it streams in, then as stored. */
export interface Reply {
  text: string;
  citations: Citation[];
  messageId: number | null;
  servedBy: string | null;
  contextUsed: ContextUsed | null;
  costMicroUsd: number | null;
  error: { code: string; message: string } | null;
  done: boolean;
}

export const emptyReply = (): Reply => ({
  text: "",
  citations: [],
  messageId: null,
  servedBy: null,
  contextUsed: null,
  costMicroUsd: null,
  error: null,
  done: false,
});

/** Folds one stream event (`delta`, `citation`, `done`, `error`) into the reply. */
export function applyEvent(r: Reply, event: string, data: unknown): Reply {
  const d = (data ?? {}) as Record<string, unknown>;
  switch (event) {
    case "delta":
      return { ...r, text: r.text + (typeof d.text === "string" ? d.text : "") };
    case "citation":
      return { ...r, citations: [...r.citations, d as unknown as Citation] };
    case "done":
      return {
        ...r,
        done: true,
        messageId: typeof d.messageId === "number" ? d.messageId : null,
        servedBy: typeof d.servedBy === "string" ? d.servedBy : null,
        contextUsed: (d.contextUsed as ContextUsed | undefined) ?? null,
        costMicroUsd: typeof d.costMicroUsd === "number" ? d.costMicroUsd : null,
      };
    case "error":
      return {
        ...r,
        done: true,
        error: {
          code: typeof d.code === "string" ? d.code : "provider_error",
          message: typeof d.message === "string" ? d.message : "The AI provider failed.",
        },
      };
    default:
      return r;
  }
}

const CITE = /\[p\.\s?(\d+)(?:\s?[–-]\s?\d+)?\]/g;

/** `[p.12]` (or `[p.12–13]`) to a `#p-12` link, so the markdown renderer can show a chip. */
export function linkCitations(text: string): string {
  return text.replace(CITE, (m, n: string) => `[${m.slice(1, -1)}](#p-${n})`);
}

/** The page of a `#p-N` link, else null (the sanitiser may have prefixed the fragment). */
export function citedPage(href: string | undefined): number | null {
  const m = href?.match(/^#(?:user-content-)?p-(\d+)$/);
  return m ? Number(m[1]) : null;
}

/** Whether a stored conversation is about this target (the list endpoint filters by material only). */
export function sameTarget(
  c: {
    scope: string;
    materialId: string | null;
    componentId: string | null;
    questionId: string | null;
    mockId: string | null;
  },
  t: AskTarget,
): boolean {
  return (
    c.scope === t.scope &&
    (c.materialId ?? undefined) === t.materialId &&
    (c.componentId ?? undefined) === t.componentId &&
    (c.questionId ?? undefined) === t.questionId &&
    (c.mockId ?? undefined) === t.mockId
  );
}

/** The first line of a question as a conversation title, at most 80 characters. */
export function titleFrom(text: string): string {
  const line = text.trim().split("\n")[0] ?? "";
  return line.length > 80 ? `${line.slice(0, 79)}…` : line;
}
