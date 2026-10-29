import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Input, Select } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { useQuestions, type Question } from "@/features/bank/api";
import { fetchQuiz, useSetChecks, type MaterialCheck } from "@/features/practice/api";
import { PracticeQuestionCard, type PracticeQuestion } from "@/features/practice/practice-question";
import { fieldErrors, messageFor } from "@/lib/errors";
import { PromptLine } from "@/features/bank/components/prompt-line";

const OUTCOME_TONE = { Right: "success", Partial: "warning", Wrong: "danger", Blank: "neutral" } as const;

/** Check questions placed in a material; each says how you did last time. */
export function CheckBlock({ checks, materialId }: { checks: MaterialCheck[]; materialId: string }) {
  if (!checks.length) return null;
  return (
    <section
      className="my-4 flex flex-col gap-3 rounded-lg border border-dashed border-[color:var(--course)] p-3"
      aria-label="Check yourself"
    >
      <span className="text-xs font-medium uppercase tracking-[0.06em] text-muted">Check yourself</span>
      {checks.map((c) => (
        <div key={c.question.id} className="flex flex-col gap-1.5">
          {c.lastOutcome ? (
            <span className="flex items-center gap-1.5 text-xs text-muted">
              Last time
              <Badge kind="status" tone={OUTCOME_TONE[c.lastOutcome as keyof typeof OUTCOME_TONE] ?? "neutral"}>
                {c.lastOutcome}
              </Badge>
            </span>
          ) : null}
          <PracticeQuestionCard compact question={c.question} context={{ kind: "Notes", materialId }} />
        </div>
      ))}
    </section>
  );
}

/** "Quiz me": two questions on a lecture or concept, one at a time, then "one more" as often as
 *  you like. Never-tried questions come first, then the ones you missed. */
export function QuizDialog({
  courseId,
  componentId,
  conceptId,
  title,
  onClose,
}: {
  courseId: string;
  componentId?: string | undefined;
  conceptId?: string | undefined;
  title: string;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const first = useQuery({
    queryKey: ["quiz", courseId, componentId ?? "", conceptId ?? ""],
    queryFn: () => fetchQuiz(courseId, { componentId, conceptId, n: 2 }),
    staleTime: 0,
    gcTime: 0,
  });
  const [extra, setExtra] = useState<PracticeQuestion[]>([]);
  const [i, setI] = useState(0);
  const [score, setScore] = useState({ right: 0, done: 0 });
  const [more, setMore] = useState<"idle" | "loading" | "none">("idle");
  const list = [...(first.data ?? []), ...extra];
  const q = list[i];

  const oneMore = async () => {
    setMore("loading");
    try {
      const next = await fetchQuiz(courseId, { componentId, conceptId, n: 1, exclude: list.map((x) => x.id) });
      if (!next.length) return setMore("none");
      setExtra((x) => [...x, ...next]);
      setI(list.length);
      setMore("idle");
    } catch {
      setMore("none");
    }
  };

  const close = () => {
    // What you answered moves stats, weak spots and the warm-up.
    void qc.invalidateQueries({ queryKey: ["stats", courseId] });
    void qc.invalidateQueries({ queryKey: ["weak-spots"] });
    void qc.invalidateQueries({ queryKey: ["next"] });
    onClose();
  };

  return (
    <Dialog open onOpenChange={(o) => !o && close()} title={title} className="w-[min(640px,calc(100vw-32px))]">
      {first.isPending ? (
        <Skeleton className="h-48" />
      ) : first.isError ? (
        <Banner tone="danger">{messageFor(first.error)}</Banner>
      ) : !list.length ? (
        <div className="flex flex-col gap-3">
          <p className="m-0 text-muted">
            No questions here yet. Add some to the bank and they show up in quizzes and mocks.
          </p>
          <div>
            <Button asChild size="sm" variant="primary">
              <Link
                to="/c/$courseId/bank/new"
                params={{ courseId }}
                search={componentId ? { lecture: componentId } : {}}
              >
                New question
              </Link>
            </Button>
          </div>
        </div>
      ) : q && i < list.length ? (
        <div className="flex flex-col gap-3">
          <span className="text-sm text-muted">
            Question {i + 1} of {list.length}
          </span>
          <PracticeQuestionCard
            key={q.id}
            question={q}
            context={{ kind: "Practice" }}
            onAnswered={(o) => setScore((s) => ({ right: s.right + (o === "Right" ? 1 : 0), done: s.done + 1 }))}
            onNext={() => setI(i + 1)}
            nextLabel={i + 1 < list.length ? "Next" : "Finish"}
          />
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          <p className="m-0 text-md">
            {score.done ? (
              <>
                <span className="font-mono font-semibold tnum">
                  {score.right}/{score.done}
                </span>{" "}
                right.
              </>
            ) : (
              "All skipped."
            )}{" "}
            {score.right < score.done ? "The misses come back in your warm-ups." : ""}
          </p>
          {more === "none" ? <p className="m-0 text-sm text-muted">That's every question here for now.</p> : null}
          <div className="flex gap-2">
            <Button variant="primary" onClick={close}>
              Done
            </Button>
            {more !== "none" ? (
              <Button disabled={more === "loading"} onClick={() => void oneMore()}>
                One more
              </Button>
            ) : null}
          </div>
        </div>
      )}
    </Dialog>
  );
}

/** Owners and Editors: which bank questions follow this material, and after which page/section. */
export function ManageChecksDialog({
  courseId,
  componentId,
  materialId,
  checks,
  unit,
  onClose,
}: {
  courseId: string;
  componentId: string;
  materialId: string;
  checks: MaterialCheck[];
  /** "page" for a PDF, "section" for notes. */
  unit: "page" | "section";
  onClose: () => void;
}) {
  const bank = useQuestions(courseId, { componentId });
  const pool = (bank.data?.pages ?? [])
    .flatMap((p) => p.items as Question[])
    .filter((q) => !q.private && !q.archivedAt);
  const save = useSetChecks(materialId);
  const [rows, setRows] = useState(() =>
    checks.map((c) => ({ questionId: c.question.id, prompt: c.question.prompt, position: String(c.position || "") })),
  );
  const [adding, setAdding] = useState("");
  const errors = fieldErrors(save.error);
  const prompt = (id: string) =>
    pool.find((q) => q.id === id)?.current.prompt ?? rows.find((r) => r.questionId === id)?.prompt ?? "";

  const submit = () =>
    save.mutate(
      rows.map((r) => ({ questionId: r.questionId, position: Number(r.position) || 0 })),
      { onSuccess: onClose },
    );

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Check questions"
      description={`Questions from this lecture's bank, shown after a ${unit}. Leave the ${unit} empty to put one at the end.`}
      className="w-[min(640px,calc(100vw-32px))]"
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={submit}>
            Save
          </Button>
        </>
      }
    >
      {save.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(save.error)}</Banner> : null}
      {Object.values(errors)[0] ? <Banner tone="danger">{Object.values(errors)[0]}</Banner> : null}
      {rows.length ? (
        <ul className="m-0 flex list-none flex-col gap-2 p-0">
          {rows.map((r, n) => (
            <li key={r.questionId} className="grid grid-cols-[1fr_6.5rem_auto] items-center gap-2">
              <span className="min-w-0 truncate text-sm">
                <PromptLine md={prompt(r.questionId)} max={90} />
              </span>
              <Input
                aria-label={`After ${unit}`}
                inputMode="numeric"
                placeholder="end"
                className="h-8 font-mono tnum"
                value={r.position}
                onChange={(e) =>
                  setRows(
                    rows.map((x, k) =>
                      k === n ? { ...x, position: e.currentTarget.value.replace(/\D/g, "").slice(0, 4) } : x,
                    ),
                  )
                }
              />
              <Button
                size="iconSm"
                variant="ghost"
                aria-label="Remove"
                onClick={() => setRows(rows.filter((_, k) => k !== n))}
              >
                <Trash2 className="size-4" />
              </Button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="m-0 text-sm text-muted">No checks yet.</p>
      )}
      <div className="flex items-center gap-2">
        <Select
          aria-label="Add a question"
          className="h-8 text-sm"
          value={adding}
          onChange={(e) => setAdding(e.currentTarget.value)}
        >
          <option value="">
            {bank.isPending
              ? "Loading questions…"
              : pool.length
                ? "Pick a question to add"
                : "No questions in this lecture yet"}
          </option>
          {pool
            .filter((q) => !rows.some((r) => r.questionId === q.id))
            .map((q) => (
              <option key={q.id} value={q.id}>
                {q.type} · <PromptLine md={q.current.prompt} max={80} />
              </option>
            ))}
        </Select>
        <Button
          size="sm"
          disabled={!adding}
          onClick={() => {
            setRows([...rows, { questionId: adding, prompt: prompt(adding), position: "" }]);
            setAdding("");
          }}
        >
          <Plus className="size-4" aria-hidden="true" />
          Add
        </Button>
      </div>
    </Dialog>
  );
}
