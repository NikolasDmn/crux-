import { useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { Check, ChevronLeft } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Field, Select, Textarea } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { OptionRow } from "@/components/ui/option-row";
import { useToast } from "@/components/ui/toast";
import { AskButton } from "@/features/ai/ask-panel";
import { getQuestion } from "@/features/bank/api";
import { CardDialog, type CardDraft } from "@/features/flash/components/card-dialog";
import { QuestionImage } from "@/features/files/question-image";
import { SLIP_TYPES, useLogSlip } from "@/features/mistakes/api";
import {
  useAiDecision,
  useAiGrades,
  useResult,
  useReview,
  type AiGrade,
  type Item,
  type Mock,
} from "@/features/mocks/api";
import { scoreText } from "@/features/mocks/lib/taking";
import { fieldErrors, messageFor } from "@/lib/errors";

// Mock result and review (frontend-spec 4): the score (open answers: Right 1, Partial 0.5); per
// question Right/Wrong/Blank with confidence (a lucky guess counts as a miss); right answers show
// their key, misses stay hidden until "Show answer"; "I fixed this"; self-grading of open answers;
// proposed slips to log. Keys render only when the API returned them (5.3). AI grading review
// joins with step 8, "make a flashcard" with step 5.
export const Route = createFileRoute("/_app/mock/$mockId/result")({ component: Result });

const TONE = { Right: "success", Partial: "warning", Wrong: "danger", Blank: "neutral" } as const;

function Result() {
  const { mockId } = Route.useParams();
  const r = useResult(mockId);
  const grades = useAiGrades(r.data);
  if (r.isPending) return <Skeleton className="h-96" />;
  if (r.isError) return <Banner tone="danger">{messageFor(r.error)}</Banner>;
  const m = r.data;
  const items = [...m.items].sort((a, b) => a.order - b.order);
  const toGrade = items.filter((i) => i.type === "Open" && !i.outcome).length;
  const misses = items.filter((i) => i.outcome && i.outcome !== "Right").length;

  return (
    <div className="flex max-w-[860px] flex-col gap-5">
      <Link
        to="/c/$courseId/mocks"
        params={{ courseId: m.courseId }}
        className="inline-flex items-center gap-1 self-start text-sm text-muted hover:text-text"
      >
        <ChevronLeft className="size-4" aria-hidden="true" />
        Mocks
      </Link>
      <div className="flex flex-wrap items-end gap-x-6 gap-y-2">
        <div className="flex flex-col gap-1">
          <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">{m.name}</h1>
          <span className="text-sm text-muted">
            {m.status === "Reviewed" ? "Reviewed" : misses ? `${misses} to review` : "Nothing to review"}
          </span>
        </div>
        <span className="ml-auto flex items-center gap-4">
          <AskButton target={{ courseId: m.courseId, scope: "Mock", mockId: m.id, title: m.name }} />
          <span className="font-mono text-3xl font-semibold tnum">{scoreText(m.score, m.maxScore)}</span>
        </span>
      </div>
      {m.gradingMode === "AI" ? (
        <AiBanner mock={m} grades={grades.data} failed={grades.isError} />
      ) : toGrade ? (
        <Banner tone="notice">
          Mark your {toGrade} open {toGrade === 1 ? "answer" : "answers"} against the model answer to finish the score.
        </Banner>
      ) : null}
      {m.status === "Reviewed" ? (
        <Banner tone="notice">Every miss is fixed or shown. All answers are open now.</Banner>
      ) : null}
      {items.map((it) => (
        <ItemCard
          key={it.order}
          mock={m}
          item={it}
          suggestion={grades.data?.find((g) => g.order === it.order && g.status === "Pending")}
        />
      ))}
    </div>
  );
}

function ItemCard({ mock, item, suggestion }: { mock: Mock; item: Item; suggestion: AiGrade | undefined }) {
  const review = useReview(mock.id);
  const toast = useToast();
  const qc = useQueryClient();
  const [slip, setSlip] = useState<{ type: string; description: string } | null>(null);
  const [card, setCard] = useState<CardDraft | null>(null);
  // A card from a missed question: its lecture, the prompt, and the right answer with the why.
  const makeCard = async () => {
    try {
      const q = await qc.fetchQuery({
        queryKey: ["question", item.questionId],
        queryFn: () => getQuestion(item.questionId),
      });
      const right = (item.options as { id: string; text: string }[] | null | undefined)?.find(
        (o) => o.id === k?.correctOptionId,
      );
      const answer =
        right?.text ??
        (k?.value !== null && k?.value !== undefined
          ? `${k.value}${k.unit ? ` ${k.unit}` : ""}`
          : (k?.modelAnswer ?? ""));
      setCard({
        componentId: q.componentId,
        front: item.prompt,
        back: [answer, k?.explanation].filter(Boolean).join("\n\n"),
      });
    } catch (e) {
      toast({ message: messageFor(e), tone: "danger" });
    }
  };
  const k = item.key;
  const miss = !!item.outcome && item.outcome !== "Right";
  // Self-grading stays open in AI mode too, also while a suggestion waits.
  const needsGrade = item.type === "Open" && !item.outcome;
  const options = (item.options ?? []) as { id: string; text: string; isCorrect?: boolean | null }[];
  const run = (a: Parameters<typeof review.mutate>[0]) =>
    review.mutate(a, { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) });

  return (
    <Card className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-mono text-sm text-muted tnum">Q{item.order}</span>
        {item.outcome ? (
          <Badge kind="status" tone={TONE[item.outcome as keyof typeof TONE] ?? "neutral"}>
            {item.outcome}
          </Badge>
        ) : needsGrade ? (
          <Badge kind="status" tone="info">
            To mark
          </Badge>
        ) : null}
        {item.answer.confidence ? <span className="text-sm text-muted">{item.answer.confidence}</span> : null}
        {item.fixed ? (
          <span className="flex items-center gap-1 text-sm text-success-text">
            <Check className="size-3.5" aria-hidden="true" />
            Fixed
          </span>
        ) : null}
        <span className="ml-auto font-mono text-sm text-muted tnum">
          {item.grade ?? "–"}/{item.maxGrade}
        </span>
      </div>
      <Markdown>{item.prompt}</Markdown>
      {item.imageFileId ? <QuestionImage fileId={item.imageFileId} /> : null}

      {item.type === "MC" ? (
        <div className="flex flex-col gap-2">
          {options.map((o, n) => (
            <OptionRow
              key={o.id}
              index={n}
              disabled
              selected={item.answer.choiceOptionId === o.id}
              onSelect={() => undefined}
              state={
                k
                  ? o.id === k.correctOptionId
                    ? "right"
                    : item.answer.choiceOptionId === o.id
                      ? "wrong"
                      : undefined
                  : undefined
              }
            >
              <Markdown>{o.text}</Markdown>
            </OptionRow>
          ))}
        </div>
      ) : (
        <div className="rounded-md bg-sunken p-3 text-sm">
          <span className="text-xs font-medium uppercase tracking-[0.06em] text-muted">Your answer</span>
          <div className="mt-1 whitespace-pre-wrap">
            {item.type === "Numeric"
              ? item.answer.numeric !== null && item.answer.numeric !== undefined
                ? `${item.answer.numeric} ${item.answer.unit ?? ""}`
                : "(blank)"
              : item.answer.text?.trim() || "(blank)"}
          </div>
        </div>
      )}

      {k ? (
        <div className="flex flex-col gap-2 border-t border-border pt-3 text-sm">
          {k.value !== null && k.value !== undefined ? (
            <p className="m-0">
              Answer: <span className="font-mono tnum">{k.value}</span> {k.unit ?? ""}
              {k.tolerance ? (
                <span className="text-muted">
                  {" "}
                  (± {k.toleranceKind === "Relative" ? `${k.tolerance * 100}%` : k.tolerance})
                </span>
              ) : null}
            </p>
          ) : null}
          {k.modelAnswer ? <Markdown>{k.modelAnswer}</Markdown> : null}
          {k.explanation ? <Markdown className="text-muted">{k.explanation}</Markdown> : null}
          {k.keyReliability === "Disputed" ? (
            <Badge kind="status" tone="danger">
              Key disputed
            </Badge>
          ) : null}
        </div>
      ) : null}

      {suggestion && !item.outcome ? <Suggestion mock={mock} grade={suggestion} onSlip={(s) => setSlip(s)} /> : null}

      {needsGrade ? (
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm text-muted">
            {suggestion ? "Or mark it yourself:" : "Compared with the model answer:"}
          </span>
          {(["Right", "Partial", "Wrong"] as const).map((o) => (
            <Button
              key={o}
              size="sm"
              disabled={review.isPending}
              onClick={() => run({ kind: "grade", order: item.order, outcome: o })}
            >
              {o === "Partial" ? "Partly" : o}
            </Button>
          ))}
        </div>
      ) : null}

      {miss ? (
        <div className="flex flex-wrap items-center gap-2">
          {!k ? (
            <Button size="sm" disabled={review.isPending} onClick={() => run({ kind: "show", order: item.order })}>
              Show answer
            </Button>
          ) : null}
          {!item.fixed ? (
            <Button size="sm" disabled={review.isPending} onClick={() => run({ kind: "fixed", order: item.order })}>
              I fixed this
            </Button>
          ) : null}
          {item.slipProposed && !slip ? (
            <Button size="sm" variant="ghost" onClick={() => setSlip({ type: "Concept", description: "" })}>
              Log a slip
            </Button>
          ) : null}
          {k ? (
            <Button size="sm" variant="ghost" onClick={() => void makeCard()}>
              Make a flashcard
            </Button>
          ) : null}
        </div>
      ) : null}
      {slip ? (
        <SlipForm key={slip.description} mock={mock} item={item} initial={slip} onDone={() => setSlip(null)} />
      ) : null}
      {card ? <CardDialog courseId={mock.courseId} draft={card} onClose={() => setCard(null)} /> : null}
    </Card>
  );
}

/** "What went wrong?" for a missed item; it becomes a slip (and maybe a weak spot). */
function SlipForm({
  mock,
  item,
  initial,
  onDone,
}: {
  mock: Mock;
  item: Item;
  initial: { type: string; description: string };
  onDone: () => void;
}) {
  const log = useLogSlip();
  const toast = useToast();
  const [type, setType] = useState<string>(initial.type);
  const [description, setDescription] = useState(initial.description);
  const errors = fieldErrors(log.error);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    log.mutate(
      {
        courseId: mock.courseId,
        mockId: mock.id,
        mockQuestionOrder: item.order,
        type,
        description: description.trim(),
        confidence: item.answer.confidence ?? null,
      },
      {
        onSuccess: () => {
          toast({ message: "Slip logged" });
          onDone();
        },
      },
    );
  };
  return (
    <form className="flex flex-col gap-3 rounded-md border border-border p-3" onSubmit={submit} noValidate>
      {log.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(log.error)}</Banner> : null}
      <div className="grid gap-3 sm:grid-cols-[12rem_1fr]">
        <Field label="Kind of slip" error={errors.type}>
          {(ids) => (
            <Select {...ids} value={type} onChange={(e) => setType(e.currentTarget.value)}>
              {SLIP_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t === "OptionMatching" ? "Option matching" : t}
                </option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="What went wrong" error={errors.description}>
          {(ids) => (
            <Textarea
              {...ids}
              className="min-h-16"
              placeholder="Mixed up the sample and population variance"
              value={description}
              onChange={(e) => setDescription(e.currentTarget.value)}
            />
          )}
        </Field>
      </div>
      <div className="flex justify-end gap-2">
        <Button size="sm" onClick={onDone}>
          Cancel
        </Button>
        <Button size="sm" type="submit" variant="primary" disabled={log.isPending || !description.trim()}>
          Log slip
        </Button>
      </div>
    </form>
  );
}

/** AI mode: still grading, or suggestions waiting with "Accept all". */
function AiBanner({ mock, grades, failed }: { mock: Mock; grades: AiGrade[] | undefined; failed: boolean }) {
  const decide = useAiDecision(mock.id);
  const toast = useToast();
  const open = mock.items.filter((i) => i.type === "Open" && !i.outcome);
  const pending = (grades ?? []).filter((g) => g.status === "Pending" && open.some((i) => i.order === g.order));
  if (!open.length) return null;
  if (failed)
    return (
      <Banner tone="notice">AI suggestions can't be reached right now. Mark your open answers yourself below.</Banner>
    );
  if (!pending.length)
    return (
      <Banner tone="notice">
        AI is grading your {open.length} open {open.length === 1 ? "answer" : "answers"}. Suggestions show up here; you
        can also mark them yourself.
      </Banner>
    );
  return (
    <Banner tone="notice">
      <span className="flex flex-wrap items-center gap-3">
        <span className="flex-1">
          {pending.length} AI {pending.length === 1 ? "suggestion waits" : "suggestions wait"} for you. Nothing counts
          until you accept it.
        </span>
        <Button
          size="sm"
          disabled={decide.isPending}
          onClick={() =>
            decide.mutate(
              { kind: "acceptAll" },
              {
                onSuccess: () => toast({ message: "Accepted" }),
                onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
              },
            )
          }
        >
          Accept all
        </Button>
      </span>
    </Banner>
  );
}

type SlipDraft = { type?: string; description?: string };

/** One AI suggestion: grade and feedback, Accept / Override / Reject, and its slip drafts. */
function Suggestion({
  mock,
  grade,
  onSlip,
}: {
  mock: Mock;
  grade: AiGrade;
  onSlip: (s: { type: string; description: string }) => void;
}) {
  const decide = useAiDecision(mock.id);
  const toast = useToast();
  const [override, setOverride] = useState<"Right" | "Partial" | "Wrong" | null>(null);
  const [dismissed, setDismissed] = useState<number[]>([]);
  const onError = (e: unknown) => toast({ message: messageFor(e), tone: "danger" });
  const slips = (grade.slips as SlipDraft[])
    .map((s, i) => ({ ...s, i }))
    .filter((s) => s.description && !dismissed.includes(s.i));
  return (
    <div className="flex flex-col gap-2 rounded-md border border-dashed border-border p-3 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-medium uppercase tracking-[0.06em] text-muted">AI suggests</span>
        <Badge kind="status" tone={TONE[grade.outcome as keyof typeof TONE] ?? "neutral"}>
          {grade.outcome}
        </Badge>
        <span className="font-mono tnum text-muted">
          {grade.grade}/{grade.maxGrade}
        </span>
      </div>
      {grade.feedback ? <Markdown>{grade.feedback}</Markdown> : null}
      {slips.length ? (
        <ul className="m-0 flex list-none flex-col gap-1.5 p-0">
          {slips.map((s) => (
            <li key={s.i} className="flex flex-wrap items-center gap-2">
              <span className="min-w-0 flex-1">
                <span className="text-muted">Slip ({s.type}): </span>
                {s.description}
              </span>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onSlip({ type: s.type ?? "Concept", description: s.description ?? "" })}
              >
                Log it
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setDismissed([...dismissed, s.i])}>
                Dismiss
              </Button>
            </li>
          ))}
        </ul>
      ) : null}
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          variant="primary"
          disabled={decide.isPending}
          onClick={() => decide.mutate({ kind: "accept", order: grade.order, gradeId: grade.id }, { onError })}
        >
          Accept
        </Button>
        <Select
          aria-label="Your own grade"
          className="h-8 w-36 text-sm"
          value={override ?? ""}
          onChange={(e) => setOverride((e.currentTarget.value || null) as typeof override)}
        >
          <option value="">Override…</option>
          <option value="Right">Right</option>
          <option value="Partial">Partly</option>
          <option value="Wrong">Wrong</option>
        </Select>
        {override ? (
          <Button
            size="sm"
            disabled={decide.isPending}
            onClick={() =>
              decide.mutate({ kind: "override", order: grade.order, gradeId: grade.id, outcome: override }, { onError })
            }
          >
            Save {override === "Partial" ? "Partly" : override}
          </Button>
        ) : null}
        <Button
          size="sm"
          variant="ghost"
          disabled={decide.isPending}
          onClick={() => decide.mutate({ kind: "reject", order: grade.order, gradeId: grade.id }, { onError })}
        >
          Reject
        </Button>
      </div>
    </div>
  );
}
