import { useState } from "react";
import { ulid } from "ulid";
import { Markdown } from "@/components/markdown";
import { TexMath } from "@/components/math";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Input, Textarea } from "@/components/ui/field";
import { OptionRow } from "@/components/ui/option-row";
import { Segmented } from "@/components/ui/segmented";
import { useAttempt } from "@/features/next/api";
import { QuestionImage } from "@/features/files/question-image";
import type { Schemas } from "@/lib/api";
import { messageFor } from "@/lib/errors";
import { cn } from "@/lib/cn";

type Result = Schemas["AttemptResult"];
export type PracticeQuestion = Schemas["PracticeQuestion"];

/** Where an answer was given: the warm-up (cold), a check in a material, or a quiz. */
export type PracticeContext = { kind: "Warmup" } | { kind: "Notes"; materialId: string } | { kind: "Practice" };

/** One question to answer (no key until you check): MC, numeric or open (judged by you). The
 *  answer goes to POST /attempts, which returns the key and feeds stats, weak spots and warm-ups. */
export function PracticeQuestionCard({
  question: q,
  context,
  keyRule,
  onNext,
  nextLabel = "Next",
  onAnswered,
  compact = false,
}: {
  question: PracticeQuestion;
  context: PracticeContext;
  /** A weak spot's rule (TeX), shown after the answer. */
  keyRule?: string | null | undefined;
  onNext?: (() => void) | undefined;
  nextLabel?: string;
  onAnswered?: ((outcome: string) => void) | undefined;
  compact?: boolean;
}) {
  const attempt = useAttempt();
  const type = q.type;
  const [clientId] = useState(ulid);
  const [choice, setChoice] = useState<string | null>(null);
  // "Choose all that apply": every picked option.
  const [choices, setChoices] = useState<string[]>([]);
  const multi = type === "MC" && !!q.multiSelect;
  const [value, setValue] = useState("");
  const [self, setSelf] = useState<"Right" | "Partial" | "Wrong" | null>(null);
  const [result, setResult] = useState<Result | null>(null);
  const ready =
    type === "MC" ? (multi ? choices.length > 0 : !!choice) : type === "Numeric" ? value.trim() !== "" : !!self;

  const check = () =>
    attempt.mutate(
      {
        clientId,
        questionId: q.id,
        context: context.kind,
        tryNo: 1,
        ...(context.kind === "Warmup" ? { cold: true } : {}),
        ...(context.kind === "Notes" ? { materialId: context.materialId } : {}),
        answeredAt: new Date().toISOString(),
        ...(type === "MC" ? (multi ? { choiceOptionIds: choices } : { choiceOptionId: choice }) : {}),
        ...(type === "Numeric" ? { numeric: Number(value.replace(",", ".")), unit: q.unit ?? null } : {}),
        ...(type === "Open" ? { text: value, selfOutcome: self } : {}),
      },
      {
        onSuccess: (r) => {
          setResult(r);
          onAnswered?.(r.outcome);
        },
      },
    );

  const key = result?.key;
  const rightIds = (key?.options ?? []).filter((o) => o.isCorrect).map((o) => o.id);
  const picked = (id: string) => (multi ? choices.includes(id) : choice === id);
  // Why each wrong pick was tempting (several for "choose all that apply").
  const tempting = result
    ? (key?.options ?? [])
        .filter((o) => picked(o.id) && !rightIds.includes(o.id) && o.distractorNote)
        .map((o) => o.distractorNote!)
    : [];
  return (
    <Card className={cn("flex flex-col gap-4", compact && "gap-3 p-4")}>
      <Markdown className={compact ? undefined : "text-md"}>{q.prompt}</Markdown>
      {q.imageFileId ? <QuestionImage fileId={q.imageFileId} /> : null}
      {type === "MC" ? (
        <div
          role={multi ? "group" : "radiogroup"}
          aria-label={multi ? "Options, choose all that apply" : "Options"}
          className="flex flex-col gap-2"
        >
          {multi ? <p className="m-0 text-sm text-muted">Choose all that apply.</p> : null}
          {(q.options ?? []).map((o, n) => (
            <OptionRow
              key={o.id}
              index={n}
              multi={multi}
              selected={picked(o.id)}
              disabled={!!result}
              onSelect={() =>
                multi
                  ? setChoices((c) => (c.includes(o.id) ? c.filter((x) => x !== o.id) : [...c, o.id]))
                  : setChoice(o.id)
              }
              state={result ? (rightIds.includes(o.id) ? "right" : picked(o.id) ? "wrong" : undefined) : undefined}
            >
              <Markdown>{o.text}</Markdown>
            </OptionRow>
          ))}
        </div>
      ) : type === "Numeric" ? (
        <div className="flex items-center gap-2">
          <Input
            aria-label="Your answer"
            inputMode="decimal"
            className="w-48 font-mono tnum"
            disabled={!!result}
            value={value}
            onChange={(e) => setValue(e.currentTarget.value)}
          />
          {q.unit ? <span className="text-muted">{q.unit}</span> : null}
        </div>
      ) : (
        <>
          <Textarea
            aria-label="Your answer"
            disabled={!!result}
            value={value}
            onChange={(e) => setValue(e.currentTarget.value)}
          />
          {!result ? (
            <div className="flex flex-wrap items-center gap-3">
              <span className="text-sm text-muted">How did you do?</span>
              <Segmented
                label="Your own judgement"
                size="sm"
                value={self}
                onChange={setSelf}
                options={[
                  { value: "Right", label: "Right" },
                  { value: "Partial", label: "Partly" },
                  { value: "Wrong", label: "Wrong" },
                ]}
              />
            </div>
          ) : null}
        </>
      )}
      {attempt.error ? <Banner tone="danger">{messageFor(attempt.error)}</Banner> : null}
      {result ? (
        <div className="flex flex-col gap-2 border-t border-border pt-3">
          <span
            className={result.outcome === "Right" ? "font-medium text-success-text" : "font-medium text-danger-text"}
          >
            {result.outcome === "Right" ? "Right" : result.outcome === "Partial" ? "Partly right" : "Not quite"}
          </span>
          {tempting.map((t) => (
            <p key={t} className="m-0 text-sm">
              {t}
            </p>
          ))}
          {key?.numeric ? (
            <p className="m-0">
              Answer: <span className="font-mono tnum">{key.numeric.value}</span> {key.numeric.unit ?? ""}
            </p>
          ) : null}
          {key?.numeric?.modelAnswer ? <Markdown>{key.numeric.modelAnswer}</Markdown> : null}
          {key?.open?.modelAnswer ? <Markdown>{key.open.modelAnswer}</Markdown> : null}
          {key?.explanation ? <Markdown className="text-muted">{key.explanation}</Markdown> : null}
          {keyRule ? (
            <div className="flex flex-col gap-1">
              <SectionLabel>Key rule</SectionLabel>
              <div className="overflow-x-auto rounded-md bg-sunken px-3 py-1">
                <TexMath tex={keyRule} display />
              </div>
            </div>
          ) : null}
        </div>
      ) : null}
      {onNext ? (
        <div className="flex justify-end gap-2">
          {result ? (
            <Button variant="primary" onClick={onNext}>
              {nextLabel}
            </Button>
          ) : (
            <>
              <Button variant="ghost" onClick={onNext}>
                Skip
              </Button>
              <Button variant="primary" disabled={!ready || attempt.isPending} onClick={check}>
                Check
              </Button>
            </>
          )}
        </div>
      ) : !result ? (
        <div className="flex justify-end">
          <Button variant="primary" size="sm" disabled={!ready || attempt.isPending} onClick={check}>
            Check
          </Button>
        </div>
      ) : null}
    </Card>
  );
}
