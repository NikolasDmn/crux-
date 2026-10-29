// Import review's pure parts (frontend-spec 4, 5.6): which proposals start selected, one line per
// proposal, the fields an edit can change, and when "Apply accepted" may run.

export interface Proposal {
  id: string;
  kind: string;
  status: string;
  confidence: string;
  duplicateOfId: string | null;
  payload: Record<string, unknown>;
  userEdits: Record<string, unknown> | null;
}

/** The kinds in apply order, with their group titles. */
export const KIND_TITLE: Record<string, string> = {
  Course: "Course",
  Component: "Lectures",
  Material: "Materials",
  Concept: "Concepts",
  Paper: "Past papers",
  Question: "Questions",
  Flashcard: "Flashcards",
  Deadline: "Deadlines",
  DeadlineComponent: "Exam layout",
  PlanEntry: "Plan entries",
  ExamAids: "Exam aids",
};

/** Still working (progress shows, review waits). */
export function working(status: string): boolean {
  return ["Uploading", "Queued", "Extracting", "Applying"].includes(status);
}

export const STATUS_TEXT: Record<string, string> = {
  Uploading: "Waiting for your files",
  Queued: "Waiting to start",
  Extracting: "Reading your files",
  Proposed: "Ready for review",
  Applying: "Adding to your course",
  Applied: "Added",
  Failed: "Couldn't finish",
  Cancelled: "Cancelled",
};

export function effective(p: Pick<Proposal, "payload" | "userEdits">): Record<string, unknown> {
  return { ...p.payload, ...(p.userEdits ?? {}) };
}

/** 5.6: Low confidence and question/flashcard duplicates start unselected. A lecture, concept or
 *  paper duplicate stays selected: accepting it links to the existing row. */
export function defaultSelected(p: Pick<Proposal, "confidence" | "duplicateOfId" | "kind">): boolean {
  if (p.confidence === "Low") return false;
  return !(p.duplicateOfId && (p.kind === "Question" || p.kind === "Flashcard"));
}

/** Selected now: an explicit decision wins, a Pending one follows the default. */
export function selected(p: Proposal): boolean {
  if (p.status === "Accepted" || p.status === "Edited" || p.status === "Applied") return true;
  if (p.status === "Pending") return defaultSelected(p);
  return false;
}

const str = (v: unknown) => (typeof v === "string" ? v : typeof v === "number" ? String(v) : "");

/** A title and a short detail line for a proposal card. */
export function summary(kind: string, payload: Record<string, unknown>): { title: string; detail: string } {
  const p = payload;
  switch (kind) {
    case "Course":
      return {
        title: [str(p.code), str(p.name)].filter(Boolean).join(" "),
        detail: [str(p.institution), str(p.term)].filter(Boolean).join(" · "),
      };
    case "Component":
      return {
        title: [str(p.code), str(p.name)].filter(Boolean).join(" "),
        detail: p.estHours ? `${str(p.estHours)} h` : "",
      };
    case "Material": {
      const pages = p.pageFrom ? `pages ${str(p.pageFrom)}${p.pageTo ? `–${str(p.pageTo)}` : ""}` : "";
      return { title: str(p.title), detail: [str(p.kind), pages].filter(Boolean).join(" · ") };
    }
    case "Concept":
      return { title: str(p.name), detail: "" };
    case "Paper":
      return { title: str(p.label) || str(p.paperKey), detail: str(p.examDate) };
    case "Question": {
      const c = (p.content ?? {}) as Record<string, unknown>;
      return {
        title: str(c.prompt),
        detail: [str(p.type), p.paperQuestionNo ? `Q${str(p.paperQuestionNo)}` : "", str(c.keyReliability)]
          .filter(Boolean)
          .join(" · "),
      };
    }
    case "Flashcard":
      return { title: str(p.front), detail: str(p.back) };
    case "Deadline":
      return {
        title: [str(p.type), str(p.title)].filter(Boolean).join(": "),
        detail: [
          str(p.startsAt)
            .replace("T", " ")
            .replace(/:00(\.\d+)?Z$/, " UTC"),
          p.durationMin ? `${str(p.durationMin)} min` : "",
          str(p.format),
        ]
          .filter(Boolean)
          .join(" · "),
      };
    case "DeadlineComponent":
      return { title: `${str(p.questionCount) || "?"} questions`, detail: str(p.componentTempKey) };
    case "PlanEntry":
      return {
        title: str(p.description),
        detail: [str(p.kind), str(p.startAt).slice(0, 16).replace("T", " ")].filter(Boolean).join(" · "),
      };
    case "ExamAids":
      return {
        title:
          [p.calculator ? "Calculator" : "", p.cheatSheet ? "Cheat sheet" : "", str(p.other)]
            .filter(Boolean)
            .join(", ") || "No aids",
        detail: "",
      };
    default:
      return { title: kind, detail: "" };
  }
}

export interface EditField {
  /** A top-level payload key, or `content.prompt` style for a question's content. */
  key: string;
  label: string;
  type: "text" | "textarea" | "number" | "date";
}

/** What the inline edit can change per kind (the rest stays as proposed). */
export const EDIT_FIELDS: Record<string, EditField[]> = {
  Course: [
    { key: "name", label: "Name", type: "text" },
    { key: "code", label: "Code", type: "text" },
    { key: "institution", label: "Institution", type: "text" },
    { key: "term", label: "Term", type: "text" },
  ],
  Component: [
    { key: "code", label: "Code", type: "text" },
    { key: "name", label: "Name", type: "text" },
    { key: "estHours", label: "Estimated hours", type: "number" },
  ],
  Material: [
    { key: "title", label: "Title", type: "text" },
    { key: "pageFrom", label: "From page", type: "number" },
    { key: "pageTo", label: "To page", type: "number" },
  ],
  Concept: [{ key: "name", label: "Name", type: "text" }],
  Paper: [
    { key: "label", label: "Label", type: "text" },
    { key: "examDate", label: "Exam date", type: "date" },
  ],
  Question: [
    { key: "content.prompt", label: "Prompt", type: "textarea" },
    { key: "content.explanation", label: "Explanation", type: "textarea" },
  ],
  Flashcard: [
    { key: "front", label: "Front", type: "textarea" },
    { key: "back", label: "Back", type: "textarea" },
  ],
  Deadline: [
    { key: "title", label: "Title", type: "text" },
    { key: "durationMin", label: "Duration (min)", type: "number" },
    { key: "format", label: "Format", type: "text" },
  ],
  DeadlineComponent: [{ key: "questionCount", label: "Questions", type: "number" }],
  PlanEntry: [{ key: "description", label: "Description", type: "text" }],
  ExamAids: [{ key: "other", label: "Other aids", type: "text" }],
};

export function readField(payload: Record<string, unknown>, key: string): string {
  const [a, b] = key.split(".");
  const v = b ? ((payload[a!] ?? {}) as Record<string, unknown>)[b] : payload[a!];
  return v === null || v === undefined ? "" : String(v);
}

/** The `edits` object for changed fields; a question's content is sent whole (edits merge shallowly). */
export function editsFor(
  kind: string,
  payload: Record<string, unknown>,
  values: Record<string, string>,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const f of EDIT_FIELDS[kind] ?? []) {
    const raw = values[f.key] ?? "";
    if (raw === readField(payload, f.key)) continue;
    const v = f.type === "number" ? (raw.trim() === "" ? null : Number(raw)) : raw;
    const [a, b] = f.key.split(".");
    if (b) out[a!] = { ...((out[a!] ?? payload[a!] ?? {}) as Record<string, unknown>), [b]: v };
    else out[a!] = v;
  }
  return out;
}

/** "Apply accepted" needs something selected and, for a new course, the Course proposal (5.6). */
export function applyBlocker(newCourse: boolean, all: Proposal[]): string | null {
  const chosen = all.filter(selected);
  if (!chosen.length) return "Select at least one proposal.";
  if (newCourse && !chosen.some((p) => p.kind === "Course")) return "Accept the course first.";
  return null;
}
