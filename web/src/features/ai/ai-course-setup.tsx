import { useNavigate } from "@tanstack/react-router";
import { X } from "lucide-react";
import { useState } from "react";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { DropBox } from "@/components/ui/empty";
import { Check, Field, Textarea } from "@/components/ui/field";
import { useUploads } from "@/features/files/api";
import { ACCEPT, formatBytes, phaseLabel } from "@/features/files/lib/upload";
import { messageFor } from "@/lib/errors";
import { cn } from "@/lib/cn";
import { startImport, useAi } from "./api";
import { reasonText } from "./lib/format";

// New course, "Set up with AI" (frontend-spec 4): drop PDFs, add instructions, optionally turn past
// exams into questions, then review the proposals on the import page. With `courseId`, the same
// for an existing course ("Import with AI" on Lectures): what it already has shows as matches.

export function AiCourseSetup({ courseId }: { courseId?: string } = {}) {
  const navigate = useNavigate();
  const pastExams = useAi("PastExamImport", courseId);
  // A new course's files go in uncoursed; the import attaches them when it's applied.
  const { uploads, add, dismiss } = useUploads({ courseId, onClean: () => undefined });
  const [instructions, setInstructions] = useState("");
  const [withExams, setWithExams] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Ready once the text is read (the import needs it): every upload ready or dropped.
  const ready = uploads.filter((u) => u.state.phase === "ready" && u.file);
  const waiting = uploads.some((u) => u.state.phase !== "ready" && u.state.phase !== "failed");

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      const id = await startImport({
        courseId: courseId ?? null,
        fileIds: ready.map((u) => u.file!.id),
        instructions: instructions.trim() || null,
        importPastExams: withExams && pastExams.allowed,
      });
      void navigate({ to: "/imports/$importId", params: { importId: id } });
    } catch (e) {
      setError(messageFor(e));
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-4">
      <p className="m-0 text-muted">
        {courseId
          ? "Drop more slides or past exams. AI proposes lectures, materials, questions and dates for this course; you review everything before anything is added."
          : "Drop your slides, syllabus and past exams. AI proposes the course, lectures, materials, exam dates and more; you review everything before anything is added."}
      </p>
      {error ? <Banner tone="danger">{error}</Banner> : null}
      <DropBox
        title="Drop course PDFs here"
        hint="Up to 20 PDFs, or click to choose"
        accept={ACCEPT}
        multiple
        onFiles={add}
      />
      {uploads.length ? (
        <ul className="m-0 flex list-none flex-col gap-2 p-0" aria-live="polite">
          {uploads.map((u) => (
            <li
              key={u.key}
              className="flex items-center gap-3 rounded-md border border-border bg-surface px-3 py-2 text-sm"
            >
              <span className="min-w-0 flex-1 truncate">{u.name}</span>
              <span className="font-mono text-xs text-muted">{formatBytes(u.size)}</span>
              <span
                className={cn(
                  "w-48 text-right",
                  u.state.phase === "failed"
                    ? "text-danger-text"
                    : u.state.phase === "ready"
                      ? "text-success-text"
                      : "text-muted",
                )}
              >
                {phaseLabel(u.state)}
              </span>
              {u.state.phase === "ready" || u.state.phase === "failed" ? (
                <Button size="iconSm" variant="ghost" aria-label={`Remove ${u.name}`} onClick={() => dismiss(u.key)}>
                  <X />
                </Button>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}
      <Field
        label="Instructions"
        hint="Optional: what to focus on, what to skip, the exam date if it's not in the files."
      >
        {(ids) => (
          <Textarea
            {...ids}
            rows={3}
            maxLength={4000}
            value={instructions}
            onChange={(e) => setInstructions(e.currentTarget.value)}
          />
        )}
      </Field>
      {pastExams.visible ? (
        <Check
          checked={withExams && pastExams.allowed}
          disabled={!pastExams.allowed}
          onChange={setWithExams}
          label="Turn past exams into questions"
          hint={
            pastExams.allowed
              ? "Questions from past papers, with the key marked Official only when an answer file has it."
              : reasonText(pastExams.reason)?.text
          }
        />
      ) : null}
      <div className="flex items-center justify-end gap-3">
        {waiting ? <span className="text-sm text-muted">Waiting for your files to be read…</span> : null}
        <Button
          variant="primary"
          disabled={busy || waiting || !ready.length || ready.length > 20}
          onClick={() => void start()}
        >
          {busy ? "Starting…" : "Read my files"}
        </Button>
      </div>
    </div>
  );
}
