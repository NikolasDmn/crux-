import { createFileRoute, Link } from "@tanstack/react-router";
import { Plus, Trash2 } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Field, Input, Select, Textarea } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { usePapers, useQuestions, useSavePaper, type Paper } from "@/features/bank/api";
import { useCourse } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import { fieldErrors, messageFor } from "@/lib/errors";

// Past papers (frontend-spec 4): the exam map: label, date, how many of its questions are in the
// bank, key status, and what was left out and why. "Extract questions" (AI) joins with step 8.
export const Route = createFileRoute("/_app/c/$courseId/papers")({ component: Papers });

const KEY_TONE = { Official: "success", Partial: "warning", Derived: "info", None: "neutral" } as const;

function Papers() {
  const { courseId } = Route.useParams();
  const course = useCourse(courseId);
  const editor = canEdit(course.data?.myRole);
  const papers = usePapers(courseId);
  const [editing, setEditing] = useState<Paper | "new" | null>(null);
  const list = [...(papers.data ?? [])].sort((a, b) => (b.examDate ?? "").localeCompare(a.examDate ?? ""));

  return (
    <div className="flex max-w-[960px] flex-col gap-4">
      <div className="flex items-center justify-between">
        <SectionLabel>Past papers</SectionLabel>
        {editor ? (
          <Button size="sm" variant="primary" onClick={() => setEditing("new")}>
            <Plus className="size-4" aria-hidden="true" />
            Add paper
          </Button>
        ) : null}
      </div>
      {papers.isPending ? (
        <Skeleton className="h-32" />
      ) : !list.length ? (
        <EmptyList
          title="No past papers yet"
          body="Add each past exam, then link its questions in the bank (paper + number) to see how much of it you can practise."
        />
      ) : (
        list.map((p) => (
          <PaperCard key={p.id} courseId={courseId} paper={p} onEdit={editor ? () => setEditing(p) : undefined} />
        ))
      )}
      {editing ? (
        <PaperDialog
          courseId={courseId}
          paper={editing === "new" ? undefined : editing}
          onClose={() => setEditing(null)}
        />
      ) : null}
    </div>
  );
}

function PaperCard({ courseId, paper, onEdit }: { courseId: string; paper: Paper; onEdit?: (() => void) | undefined }) {
  const inBank = useQuestions(courseId, { paperId: paper.id });
  const first = inBank.data?.pages[0];
  const n = first ? `${first.items.length}${first.nextCursor ? "+" : ""}` : "…";
  return (
    <Card className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">{paper.label}</span>
        <span className="font-mono text-xs text-muted">{paper.paperKey}</span>
        {paper.examDate ? <span className="text-sm text-muted">{paper.examDate}</span> : null}
        <Badge kind="status" tone={KEY_TONE[paper.keyStatus as keyof typeof KEY_TONE] ?? "neutral"}>
          Key: {paper.keyStatus}
        </Badge>
        <span className="flex-1" />
        <Link
          to="/c/$courseId/bank"
          params={{ courseId }}
          search={{ paperId: paper.id }}
          className="text-sm text-muted hover:text-text"
        >
          {n}
          {paper.totalQuestions ? ` of ${paper.totalQuestions}` : ""} in the bank
        </Link>
        {onEdit ? (
          <Button size="sm" variant="ghost" onClick={onEdit}>
            Edit
          </Button>
        ) : null}
      </div>
      {paper.omissions.length ? (
        <ul className="m-0 flex flex-col gap-0.5 pl-5 text-sm text-muted">
          {paper.omissions.map((o) => (
            <li key={o.questionNumber}>
              Q{o.questionNumber} left out: {o.reason}
            </li>
          ))}
        </ul>
      ) : null}
      {paper.note ? <p className="m-0 text-sm">{paper.note}</p> : null}
    </Card>
  );
}

function PaperDialog({
  courseId,
  paper,
  onClose,
}: {
  courseId: string;
  paper?: Paper | undefined;
  onClose: () => void;
}) {
  const save = useSavePaper(courseId);
  const [f, setF] = useState({
    label: paper?.label ?? "",
    paperKey: paper?.paperKey ?? "",
    examDate: paper?.examDate ?? "",
    totalQuestions: paper?.totalQuestions ? String(paper.totalQuestions) : "",
    keyStatus: paper?.keyStatus ?? "None",
    note: paper?.note ?? "",
    omissions: (paper?.omissions ?? []).map((o) => ({ ...o })),
  });
  const errors = fieldErrors(save.error);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    save.mutate(
      {
        paper,
        body: {
          label: f.label.trim(),
          paperKey: f.paperKey.trim(),
          examDate: f.examDate || null,
          totalQuestions: Number(f.totalQuestions) || null,
          keyStatus: f.keyStatus,
          note: f.note.trim() || null,
          omissions: f.omissions.filter((o) => o.questionNumber.trim()),
        },
      },
      { onSuccess: onClose },
    );
  };
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title={paper ? "Edit paper" : "Add a past paper"}>
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        {save.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(save.error)}</Banner> : null}
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Label" error={errors.label}>
            {(ids) => (
              <Input
                {...ids}
                autoFocus
                placeholder="June 2024 exam"
                value={f.label}
                onChange={(e) => setF({ ...f, label: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field label="Short key" error={errors.paperKey} hint="Unique in the course: 2024-06.">
            {(ids) => (
              <Input
                {...ids}
                className="font-mono"
                value={f.paperKey}
                onChange={(e) => setF({ ...f, paperKey: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field label="Exam date" error={errors.examDate}>
            {(ids) => (
              <Input
                {...ids}
                type="date"
                value={f.examDate}
                onChange={(e) => setF({ ...f, examDate: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field label="Questions in the paper" error={errors.totalQuestions}>
            {(ids) => (
              <Input
                {...ids}
                inputMode="numeric"
                className="font-mono tnum"
                value={f.totalQuestions}
                onChange={(e) => setF({ ...f, totalQuestions: e.currentTarget.value.replace(/\D/g, "") })}
              />
            )}
          </Field>
          <Field label="Answer key" error={errors.keyStatus} className="sm:col-span-2">
            {(ids) => (
              <Select {...ids} value={f.keyStatus} onChange={(e) => setF({ ...f, keyStatus: e.currentTarget.value })}>
                <option value="Official">Official key</option>
                <option value="Partial">Partial key</option>
                <option value="Derived">Worked out ourselves</option>
                <option value="None">No key</option>
              </Select>
            )}
          </Field>
        </div>
        <div className="flex flex-col gap-2">
          <span className="text-sm font-medium">Left out of the bank</span>
          {f.omissions.map((o, i) => (
            <div key={i} className="grid grid-cols-[5rem_1fr_auto] items-center gap-2">
              <Input
                aria-label="Question number"
                placeholder="4"
                value={o.questionNumber}
                onChange={(e) =>
                  setF({
                    ...f,
                    omissions: f.omissions.map((x, n) =>
                      n === i ? { ...x, questionNumber: e.currentTarget.value } : x,
                    ),
                  })
                }
              />
              <Input
                aria-label="Why"
                placeholder="Not in this year's syllabus"
                value={o.reason}
                onChange={(e) =>
                  setF({
                    ...f,
                    omissions: f.omissions.map((x, n) => (n === i ? { ...x, reason: e.currentTarget.value } : x)),
                  })
                }
              />
              <Button
                size="iconSm"
                variant="ghost"
                aria-label="Remove"
                onClick={() => setF({ ...f, omissions: f.omissions.filter((_, n) => n !== i) })}
              >
                <Trash2 className="size-4" />
              </Button>
            </div>
          ))}
          <div>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setF({ ...f, omissions: [...f.omissions, { questionNumber: "", reason: "" }] })}
            >
              <Plus className="size-4" aria-hidden="true" />
              Add
            </Button>
          </div>
        </div>
        <Field label="Note" error={errors.note}>
          {(ids) => <Textarea {...ids} value={f.note} onChange={(e) => setF({ ...f, note: e.currentTarget.value })} />}
        </Field>
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={save.isPending || !f.label.trim() || !f.paperKey.trim()}>
            Save
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
