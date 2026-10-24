import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { ulid } from "ulid";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Input, Textarea } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { OptionRow } from "@/components/ui/option-row";
import { RatingButtons, RATING_ORDER } from "@/components/ui/rating-buttons";
import { Segmented } from "@/components/ui/segmented";
import { useAttempt, useWarmup, type WarmupItem } from "@/features/next/api";
import { api, unwrap, type Schemas } from "@/lib/api";
import { messageFor } from "@/lib/errors";

type Result = Schemas["AttemptResult"];

// The warm-up (spec 5.9): at most 3 items, one at a time, answered cold. Questions go to
// POST /attempts (context Warmup); flagged cards to POST /flash/reviews, which clears the flag.
export function Warmup({ courseId, onFinished }: { courseId: string; onFinished: () => void }) {
  const items = useWarmup(courseId);
  const [i, setI] = useState(0);
  if (items.isPending) return <Skeleton className="h-48" />;
  const list = (items.data ?? []).slice(0, 3);
  const item = list[i];
  if (!item) {
    return (
      <Card className="flex flex-col items-start gap-3">
        <p className="m-0">Nothing to warm up with right now.</p>
        <Button variant="primary" onClick={onFinished}>
          Continue
        </Button>
      </Card>
    );
  }
  const next = () => (i + 1 >= list.length ? onFinished() : setI(i + 1));
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2 text-sm text-muted">
        <span>
          Warm-up {i + 1} of {list.length}
        </span>
        {item.topic ? <Badge>{item.topic}</Badge> : null}
      </div>
      {item.kind === "card" && item.card ? (
        <CardItem key={i} item={item} onNext={next} />
      ) : item.question ? (
        <QuestionItem key={i} item={item} onNext={next} />
      ) : (
        <Button onClick={next}>Next</Button>
      )}
    </div>
  );
}

function QuestionItem({ item, onNext }: { item: WarmupItem; onNext: () => void }) {
  const q = item.question!;
  const attempt = useAttempt();
  const [clientId] = useState(ulid);
  const [choice, setChoice] = useState<string | null>(null);
  const [value, setValue] = useState("");
  const [self, setSelf] = useState<"Right" | "Partial" | "Wrong" | null>(null);
  const [result, setResult] = useState<Result | null>(null);
  const type = q.type;
  const ready = type === "MC" ? !!choice : type === "Numeric" ? value.trim() !== "" : !!self;

  const check = () =>
    attempt.mutate(
      {
        clientId,
        questionId: q.id,
        context: "Warmup",
        cold: true,
        answeredAt: new Date().toISOString(),
        ...(type === "MC" ? { choiceOptionId: choice } : {}),
        ...(type === "Numeric" ? { numeric: Number(value.replace(",", ".")), unit: q.unit ?? null } : {}),
        ...(type === "Open" ? { text: value, selfOutcome: self } : {}),
      },
      { onSuccess: setResult },
    );

  const key = result?.key;
  const right = (key?.options ?? []).find((o) => o.isCorrect)?.id;
  return (
    <Card className="flex flex-col gap-4">
      <Markdown className="text-md">{q.prompt}</Markdown>
      {type === "MC" ? (
        <div role="radiogroup" aria-label="Options" className="flex flex-col gap-2">
          {(q.options ?? []).map((o, n) => (
            <OptionRow
              key={o.id}
              index={n}
              selected={choice === o.id}
              disabled={!!result}
              onSelect={() => setChoice(o.id)}
              state={result ? (o.id === right ? "right" : o.id === choice ? "wrong" : undefined) : undefined}
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
          {key?.numeric ? (
            <p className="m-0">
              Answer: <span className="font-mono tnum">{key.numeric.value}</span> {key.numeric.unit ?? ""}
            </p>
          ) : null}
          {key?.open?.modelAnswer ? <Markdown>{key.open.modelAnswer}</Markdown> : null}
          {key?.explanation ? <Markdown className="text-muted">{key.explanation}</Markdown> : null}
          {item.keyRule ? (
            <div className="flex flex-col gap-1">
              <SectionLabel>Key rule</SectionLabel>
              <Markdown>{item.keyRule}</Markdown>
            </div>
          ) : null}
        </div>
      ) : null}
      <div className="flex justify-end gap-2">
        {result ? (
          <Button variant="primary" onClick={onNext}>
            Next
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
    </Card>
  );
}

function CardItem({ item, onNext }: { item: WarmupItem; onNext: () => void }) {
  const card = item.card!;
  const [shown, setShown] = useState(false);
  const [started] = useState(Date.now);
  const [clientId] = useState(ulid);
  const review = useMutation({
    mutationFn: (rating: number) =>
      unwrap(
        api.POST("/flash/reviews", {
          body: {
            reviews: [
              {
                clientId,
                flashcardId: card.id,
                rating,
                reviewedAt: new Date().toISOString(),
                durationMs: Date.now() - started,
              },
            ],
          },
        }),
      ),
    onSuccess: onNext,
  });
  return (
    <Card className="flex flex-col gap-4">
      {item.lesson ? (
        <div className="flex flex-col gap-1 rounded-md bg-sunken p-3">
          <SectionLabel>Before the card</SectionLabel>
          <Markdown>{item.lesson}</Markdown>
        </div>
      ) : null}
      <div className="flex items-center gap-2">
        <Badge kind="code">{card.code}</Badge>
      </div>
      <Markdown className="text-md">{card.front}</Markdown>
      {shown ? (
        <>
          <div className="border-t border-border pt-3">
            <Markdown>{card.back}</Markdown>
          </div>
          {review.error ? <Banner tone="danger">{messageFor(review.error)}</Banner> : null}
          <RatingButtons
            intervals={{}}
            disabled={review.isPending}
            onRate={(r) => review.mutate(RATING_ORDER.indexOf(r) + 1)}
          />
        </>
      ) : (
        <Button variant="primary" size="lg" onClick={() => setShown(true)}>
          Show answer
        </Button>
      )}
    </Card>
  );
}
