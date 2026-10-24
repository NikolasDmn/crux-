import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { ulid } from "ulid";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/misc";
import { RatingButtons, RATING_ORDER } from "@/components/ui/rating-buttons";
import { useWarmup, type WarmupItem } from "@/features/next/api";
import { PracticeQuestionCard } from "@/features/practice/practice-question";
import { api, unwrap } from "@/lib/api";
import { messageFor } from "@/lib/errors";

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
  return (
    <PracticeQuestionCard
      question={item.question!}
      context={{ kind: "Warmup" }}
      keyRule={item.keyRule}
      onNext={onNext}
    />
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
