import { useNavigate } from "@tanstack/react-router";
import { Plus, Trash2, X } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Field, Input, Select, Textarea } from "@/components/ui/field";
import { Segmented } from "@/components/ui/segmented";
import { useToast } from "@/components/ui/toast";
import {
  idempotencyKeyFor,
  useConcepts,
  useCreateConcept,
  useCreateQuestion,
  usePapers,
  usePatchQuestion,
  useQuestionAction,
  type Question,
} from "@/features/bank/api";
import { check, emptyForm, formFrom, toContent, type Form, type QType } from "@/features/bank/lib/editor";
import { useCourse } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import { useLectures } from "@/features/lectures/api";
import { conflictCurrent, fieldErrors, messageFor } from "@/lib/errors";
import { cn } from "@/lib/cn";

// Question editor (frontend-spec 4): type (fixed once saved), prompt with live preview, MC options
// with a correct toggle and distractor notes, numeric value/tolerance/unit, open model answer and
// rubric, concepts, paper and number. Every save is a new version. Image upload and "crop from a
// PDF page" join with the storage step's file picker.
export function QuestionEditor({
  courseId,
  question,
  initialLecture,
}: {
  courseId: string;
  question?: Question | undefined;
  initialLecture?: string | undefined;
}) {
  const navigate = useNavigate();
  const toast = useToast();
  const course = useCourse(courseId);
  const editor = canEdit(course.data?.myRole);
  const lectures = useLectures(courseId);
  const papers = usePapers(courseId);
  const concepts = useConcepts(courseId);
  const newConcept = useCreateConcept(courseId);
  const create = useCreateQuestion(courseId);
  const patch = usePatchQuestion();
  const act = useQuestionAction();
  const [key] = useState(idempotencyKeyFor);
  const [f, setF] = useState<Form>(() => (question ? formFrom(question) : emptyForm(initialLecture ?? "")));
  const [local, setLocal] = useState<Record<string, string>>({});
  const [conceptName, setConceptName] = useState("");
  const m = question ? patch : create;
  const server = fieldErrors(m.error);
  const errors = { ...server, ...local };
  const general = m.error && !Object.keys(server).length ? messageFor(m.error) : null;
  const set = (p: Partial<Form>) => setF((x) => ({ ...x, ...p }));

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const problems = check(f);
    setLocal(problems);
    if (Object.keys(problems).length) return;
    const content = toContent(f);
    const shared = {
      componentId: f.componentId,
      conceptIds: f.conceptIds,
      primaryConceptId: f.primaryConceptId,
      paperId: f.paperId || null,
      paperQuestionNo: f.paperQuestionNo.trim() || null,
      sourceLabel: f.sourceLabel.trim() || null,
    };
    if (question) {
      patch.mutate(
        { id: question.id, version: question.version, body: { ...shared, content } },
        {
          onSuccess: (q) => {
            toast({ message: `Saved as version ${q.current.versionNo}` });
            setF((x) => ({ ...x, changeNote: "" }));
          },
          onError: (err) => {
            if (conflictCurrent(err))
              toast({ message: "Someone saved a newer version. Reload to see it.", tone: "danger" });
          },
        },
      );
    } else {
      create.mutate(
        { key, body: { ...shared, type: f.type, content, private: f.private, source: f.source } },
        {
          onSuccess: (q) =>
            void navigate({ to: "/c/$courseId/q/$questionId", params: { courseId, questionId: q.id }, replace: true }),
        },
      );
    }
  };

  const addConcept = () => {
    const name = conceptName.trim();
    if (!name) return;
    const known = concepts.data?.find((c) => c.name.toLowerCase() === name.toLowerCase());
    if (known) {
      if (!f.conceptIds.includes(known.id)) set({ conceptIds: [...f.conceptIds, known.id] });
      setConceptName("");
      return;
    }
    newConcept.mutate(name, {
      onSuccess: (c) => {
        set({ conceptIds: [...f.conceptIds, c.id], primaryConceptId: f.primaryConceptId ?? c.id });
        setConceptName("");
      },
    });
  };

  const action = (a: "approve" | "archive" | "promote") =>
    question &&
    act.mutate(
      { id: question.id, action: a },
      {
        onSuccess: () =>
          toast({ message: a === "approve" ? "Approved" : a === "archive" ? "Archived" : "Added to the course bank" }),
        onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
      },
    );

  return (
    <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
      {general ? <Banner tone="danger">{general}</Banner> : null}
      {question ? (
        <div className="flex flex-wrap items-center gap-2">
          <Badge kind="status">{question.type}</Badge>
          <span className="text-sm text-muted">Version {question.current.versionNo}</span>
          {question.reviewState === "Draft" ? (
            <Badge kind="status" tone="warning">
              Draft
            </Badge>
          ) : null}
          {question.private ? <Badge kind="status">Private</Badge> : null}
          {question.archivedAt ? (
            <Badge kind="status" tone="danger">
              Archived
            </Badge>
          ) : null}
          <span className="flex-1" />
          {question.reviewState === "Draft" && (editor || question.private) ? (
            <Button size="sm" onClick={() => action("approve")}>
              Approve
            </Button>
          ) : null}
          {question.private ? (
            <Button size="sm" onClick={() => action("promote")}>
              Add to course bank
            </Button>
          ) : null}
          {!question.archivedAt ? (
            <Button size="sm" variant="ghost" onClick={() => action("archive")}>
              Archive
            </Button>
          ) : null}
        </div>
      ) : (
        <Segmented<QType>
          label="Question type"
          size="sm"
          value={f.type}
          onChange={(type) => set({ type })}
          options={[
            { value: "MC", label: "Multiple choice" },
            { value: "Numeric", label: "Numeric" },
            { value: "Open", label: "Open" },
          ]}
        />
      )}

      <Card className="flex flex-col gap-4">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Lecture" error={errors.componentId}>
            {(ids) => (
              <Select {...ids} value={f.componentId} onChange={(e) => set({ componentId: e.currentTarget.value })}>
                <option value="">Pick a lecture</option>
                {(lectures.data ?? []).map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.code} {l.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Key" error={errors["content.keyReliability"]}>
            {(ids) => (
              <Select
                {...ids}
                value={f.keyReliability}
                onChange={(e) => set({ keyReliability: e.currentTarget.value as Form["keyReliability"] })}
              >
                <option value="Derived">Derived (worked out)</option>
                <option value="Official">Official</option>
                <option value="Disputed">Disputed</option>
              </Select>
            )}
          </Field>
        </div>
        <div className="grid gap-4 lg:grid-cols-2">
          <Field label="Question (markdown, $maths$)" error={errors.prompt ?? errors["content.prompt"]}>
            {(ids) => (
              <Textarea
                {...ids}
                className="min-h-40 font-mono text-sm"
                value={f.prompt}
                onChange={(e) => set({ prompt: e.currentTarget.value })}
              />
            )}
          </Field>
          <div className="flex flex-col gap-1.5">
            <span className="text-sm font-medium">Preview</span>
            <div className="min-h-40 rounded-md border border-border bg-sunken/40 p-3">
              <Markdown>{f.prompt || "_Nothing yet._"}</Markdown>
            </div>
          </div>
        </div>
      </Card>

      {f.type === "MC" ? <OptionsCard f={f} set={set} error={errors.options ?? errors["content.options"]} /> : null}
      {f.type === "Numeric" ? <NumericCard f={f} set={set} errors={errors} /> : null}
      {f.type === "Open" ? <OpenCard f={f} set={set} error={errors.rubric} /> : null}

      <Card className="flex flex-col gap-4">
        <SectionLabel>Explanation and sources</SectionLabel>
        <Field label="Explanation (shown with the key)">
          {(ids) => (
            <Textarea {...ids} value={f.explanation} onChange={(e) => set({ explanation: e.currentTarget.value })} />
          )}
        </Field>
        <div className="flex flex-col gap-2">
          <span className="text-sm font-medium">Concepts</span>
          <div className="flex flex-wrap items-center gap-1.5">
            {f.conceptIds.map((id) => {
              const name = concepts.data?.find((c) => c.id === id)?.name ?? "…";
              const primary = f.primaryConceptId === id;
              return (
                <span
                  key={id}
                  className={cn(
                    "inline-flex h-7 items-center gap-1 rounded-sm border px-2 text-sm",
                    primary ? "border-text font-medium" : "border-border",
                  )}
                >
                  <button type="button" title="Make primary" onClick={() => set({ primaryConceptId: id })}>
                    {name}
                  </button>
                  <button
                    type="button"
                    aria-label={`Remove ${name}`}
                    className="text-muted hover:text-text"
                    onClick={() =>
                      set({
                        conceptIds: f.conceptIds.filter((c) => c !== id),
                        primaryConceptId: primary ? null : f.primaryConceptId,
                      })
                    }
                  >
                    <X className="size-3.5" />
                  </button>
                </span>
              );
            })}
            <Input
              aria-label="Add a concept"
              list="crux-concepts"
              placeholder="Add a concept"
              className="h-7 w-48 text-sm"
              value={conceptName}
              onChange={(e) => setConceptName(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addConcept();
                }
              }}
            />
            <datalist id="crux-concepts">
              {(concepts.data ?? []).map((c) => (
                <option key={c.id} value={c.name} />
              ))}
            </datalist>
          </div>
          <span className="text-xs text-muted">Click a concept to make it the primary one (outlined).</span>
        </div>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="Past paper">
            {(ids) => (
              <Select {...ids} value={f.paperId} onChange={(e) => set({ paperId: e.currentTarget.value })}>
                <option value="">None</option>
                {(papers.data ?? []).map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.label}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Question number" error={errors.paperQuestionNo}>
            {(ids) => (
              <Input
                {...ids}
                placeholder="3b"
                value={f.paperQuestionNo}
                onChange={(e) => set({ paperQuestionNo: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field label="Source label">
            {(ids) => (
              <Input
                {...ids}
                placeholder="Tutorial 2"
                value={f.sourceLabel}
                onChange={(e) => set({ sourceLabel: e.currentTarget.value })}
              />
            )}
          </Field>
        </div>
        {!question ? (
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Source">
              {(ids) => (
                <Select {...ids} value={f.source} onChange={(e) => set({ source: e.currentTarget.value })}>
                  {["Manual", "Lecture", "Tutorial", "PastExam", "Practice", "Imported", "Unknown"].map((s) => (
                    <option key={s}>{s}</option>
                  ))}
                </Select>
              )}
            </Field>
            <label className="flex items-center gap-2 self-end pb-2 text-sm">
              <input
                type="checkbox"
                className="size-4 accent-[var(--primary)]"
                checked={f.private || !editor}
                disabled={!editor}
                onChange={(e) => set({ private: e.currentTarget.checked })}
              />
              Keep it to myself (not in the course bank)
            </label>
          </div>
        ) : (
          <Field label="What changed (optional)">
            {(ids) => (
              <Input {...ids} value={f.changeNote} onChange={(e) => set({ changeNote: e.currentTarget.value })} />
            )}
          </Field>
        )}
      </Card>

      <div className="flex justify-end gap-2">
        <Button onClick={() => window.history.back()}>Cancel</Button>
        <Button type="submit" variant="primary" disabled={m.isPending}>
          {question ? "Save new version" : "Create question"}
        </Button>
      </div>
    </form>
  );
}

function OptionsCard({ f, set, error }: { f: Form; set: (p: Partial<Form>) => void; error?: string | undefined }) {
  const update = (i: number, p: Partial<Form["options"][number]>) =>
    set({ options: f.options.map((o, n) => (n === i ? { ...o, ...p } : o)) });
  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <SectionLabel>Options</SectionLabel>
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            className="size-4 accent-[var(--primary)]"
            checked={f.shuffleOptions}
            onChange={(e) => set({ shuffleOptions: e.currentTarget.checked })}
          />
          Shuffle in mocks
        </label>
      </div>
      {error ? <span className="text-sm text-danger-text">{error}</span> : null}
      <div role="radiogroup" aria-label="Correct option" className="flex flex-col gap-3">
        {f.options.map((o, i) => (
          <div key={i} className="grid gap-2 sm:grid-cols-[auto_1fr_1fr_auto] sm:items-center">
            <button
              type="button"
              role="radio"
              aria-checked={o.isCorrect}
              aria-label={`Option ${String.fromCharCode(65 + i)} is correct`}
              onClick={() => set({ options: f.options.map((x, n) => ({ ...x, isCorrect: n === i })) })}
              className={cn(
                "inline-flex size-8 items-center justify-center rounded-md border font-mono text-sm",
                o.isCorrect ? "border-success bg-success-bg text-success-text" : "border-border bg-sunken",
              )}
            >
              {String.fromCharCode(65 + i)}
            </button>
            <Input
              aria-label={`Option ${String.fromCharCode(65 + i)}`}
              placeholder="Option text ($maths$ works)"
              value={o.text}
              onChange={(e) => update(i, { text: e.currentTarget.value })}
            />
            <Input
              aria-label={`Why option ${String.fromCharCode(65 + i)} is tempting`}
              placeholder={o.isCorrect ? "" : "Why it's tempting (optional)"}
              disabled={o.isCorrect}
              value={o.distractorNote}
              onChange={(e) => update(i, { distractorNote: e.currentTarget.value })}
            />
            <Button
              size="iconSm"
              variant="ghost"
              aria-label={`Remove option ${String.fromCharCode(65 + i)}`}
              disabled={f.options.length <= 2}
              onClick={() => set({ options: f.options.filter((_, n) => n !== i) })}
            >
              <Trash2 className="size-4" />
            </Button>
          </div>
        ))}
      </div>
      <div>
        <Button
          size="sm"
          variant="ghost"
          disabled={f.options.length >= 9}
          onClick={() => set({ options: [...f.options, { text: "", isCorrect: false, distractorNote: "" }] })}
        >
          <Plus className="size-4" aria-hidden="true" />
          Add option
        </Button>
      </div>
    </Card>
  );
}

function NumericCard({
  f,
  set,
  errors,
}: {
  f: Form;
  set: (p: Partial<Form>) => void;
  errors: Record<string, string | undefined>;
}) {
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>Answer</SectionLabel>
      <div className="grid gap-4 sm:grid-cols-4">
        <Field label="Value" error={errors.value}>
          {(ids) => (
            <Input
              {...ids}
              inputMode="decimal"
              className="font-mono tnum"
              value={f.value}
              onChange={(e) => set({ value: e.currentTarget.value })}
            />
          )}
        </Field>
        <Field label="Tolerance" error={errors.tolerance}>
          {(ids) => (
            <Input
              {...ids}
              inputMode="decimal"
              className="font-mono tnum"
              placeholder="0"
              value={f.tolerance}
              onChange={(e) => set({ tolerance: e.currentTarget.value })}
            />
          )}
        </Field>
        <Field label="Tolerance is">
          {(ids) => (
            <Select
              {...ids}
              value={f.toleranceKind}
              onChange={(e) => set({ toleranceKind: e.currentTarget.value as Form["toleranceKind"] })}
            >
              <option value="Relative">Relative (0.01 = 1%)</option>
              <option value="Absolute">Absolute</option>
            </Select>
          )}
        </Field>
        <Field label="Unit" error={errors.unit}>
          {(ids) => (
            <Input {...ids} placeholder="kg" value={f.unit} onChange={(e) => set({ unit: e.currentTarget.value })} />
          )}
        </Field>
      </div>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          className="size-4 accent-[var(--primary)]"
          checked={f.unitRequired}
          onChange={(e) => set({ unitRequired: e.currentTarget.checked })}
        />
        The unit must be given
      </label>
      <Field label="Worked solution">
        {(ids) => (
          <Textarea {...ids} value={f.modelAnswer} onChange={(e) => set({ modelAnswer: e.currentTarget.value })} />
        )}
      </Field>
    </Card>
  );
}

function OpenCard({ f, set, error }: { f: Form; set: (p: Partial<Form>) => void; error?: string | undefined }) {
  const total = f.rubric.reduce((n, r) => n + (Number(r.points.replace(",", ".")) || 0), 0);
  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>Model answer and rubric</SectionLabel>
      <Field label="Model answer">
        {(ids) => (
          <Textarea
            {...ids}
            className="min-h-28"
            value={f.modelAnswer}
            onChange={(e) => set({ modelAnswer: e.currentTarget.value })}
          />
        )}
      </Field>
      {error ? <span className="text-sm text-danger-text">{error}</span> : null}
      {f.rubric.map((r, i) => (
        <div key={i} className="grid grid-cols-[1fr_6rem_auto] items-center gap-2">
          <Input
            aria-label={`Criterion ${i + 1}`}
            placeholder="States the null hypothesis"
            value={r.criterion}
            onChange={(e) =>
              set({ rubric: f.rubric.map((x, n) => (n === i ? { ...x, criterion: e.currentTarget.value } : x)) })
            }
          />
          <Input
            aria-label={`Points for criterion ${i + 1}`}
            inputMode="decimal"
            className="font-mono tnum"
            placeholder="pts"
            value={r.points}
            onChange={(e) =>
              set({ rubric: f.rubric.map((x, n) => (n === i ? { ...x, points: e.currentTarget.value } : x)) })
            }
          />
          <Button
            size="iconSm"
            variant="ghost"
            aria-label={`Remove criterion ${i + 1}`}
            onClick={() => set({ rubric: f.rubric.filter((_, n) => n !== i) })}
          >
            <Trash2 className="size-4" />
          </Button>
        </div>
      ))}
      <div className="flex items-center gap-3">
        <Button size="sm" variant="ghost" onClick={() => set({ rubric: [...f.rubric, { criterion: "", points: "" }] })}>
          <Plus className="size-4" aria-hidden="true" />
          Add criterion
        </Button>
        {f.rubric.length ? <span className="font-mono text-sm text-muted tnum">{total} pts</span> : null}
      </div>
      <Field label="Rubric note">
        {(ids) => <Input {...ids} value={f.rubricNote} onChange={(e) => set({ rubricNote: e.currentTarget.value })} />}
      </Field>
    </Card>
  );
}
