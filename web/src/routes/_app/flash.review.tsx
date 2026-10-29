import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { X } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type FormEvent } from "react";
import { ulid } from "ulid";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card as Panel, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Field, Select, Textarea } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { RatingButtons } from "@/components/ui/rating-buttons";
import { Segmented } from "@/components/ui/segmented";
import { useToast } from "@/components/ui/toast";
import { useCourses } from "@/features/courses/api";
import { fetchCustom, keys, useCardFlags, useQueue, type Card } from "@/features/flash/api";
import { interval, RATING_NUMBER, ReviewBuffer, summarize, type Rated } from "@/features/flash/lib/session";
import { SLIP_TYPES, useLogSlip } from "@/features/mistakes/api";
import { slipLabel } from "@/features/mistakes/components/slip-list";
import { cn } from "@/lib/cn";
import { fieldErrors, messageFor } from "@/lib/errors";
import { flashKey, typingIn, type Confidence, type Rating } from "@/lib/keyboard";
import { sendReviews } from "@/lib/reviews";

// Flashcard review (frontend-spec 3.4, 4, 5.4), full screen: the remaining count, a segmented
// progress line, the card (front, then the answer under a divider), Show answer, the four ratings
// with their intervals, optional confidence and "include in my next warm-up". Ratings advance at
// once and post in batches; the end shows the tally, lapses and sure-but-wrong cards to log.
type Search = { courseId?: string; which?: "slips" | "forgotten" | "unseen" | "all"; n?: number; lectures?: string };

export const Route = createFileRoute("/_app/flash/review")({
  staticData: { focus: true },
  validateSearch: (s: Record<string, unknown>): Search => ({
    ...(typeof s.courseId === "string" && s.courseId ? { courseId: s.courseId } : {}),
    ...(s.which === "slips" || s.which === "forgotten" || s.which === "unseen" || s.which === "all"
      ? { which: s.which }
      : {}),
    ...(Number(s.n) > 0 ? { n: Math.min(200, Math.floor(Number(s.n))) } : {}),
    ...(typeof s.lectures === "string" && s.lectures ? { lectures: s.lectures } : {}),
  }),
  component: Review,
});

function Review() {
  const search = Route.useSearch();
  const custom = !!search.which && !!search.courseId;
  const queue = useQueue(search.courseId, !custom);
  const set = useQuery({
    queryKey: ["flash", "custom", search],
    enabled: custom,
    staleTime: Infinity,
    queryFn: () =>
      fetchCustom({
        courseId: search.courseId!,
        which: search.which!,
        n: search.n ?? 30,
        componentIds: search.lectures ? search.lectures.split(",") : [],
      }),
  });
  const data = custom ? set : queue;
  if (data.isPending) return <Skeleton className="m-6 h-96" />;
  if (data.isError) return <Banner tone="danger">{messageFor(data.error)}</Banner>;
  const cards = custom ? (set.data ?? []) : (queue.data?.cards ?? []);
  const next = custom ? [] : (queue.data?.nextDue ?? []);
  // Keyed by the request, not the cards: a refetch mid-session (a warm-up flag, the end of the
  // session) must not restart it.
  return <Session key={JSON.stringify(search)} cards={cards} nextDue={next} courseId={search.courseId} />;
}

function Session({
  cards: initialCards,
  nextDue: initialNext,
  courseId,
}: {
  cards: Card[];
  nextDue: { cardId: string; again: number; hard: number; good: number; easy: number }[];
  courseId?: string | undefined;
}) {
  const qc = useQueryClient();
  const toast = useToast();
  // The session works on the cards it started with.
  const [cards] = useState(initialCards);
  const [nextDue] = useState(initialNext);
  const courses = useCourses();
  const color = new Map((courses.data ?? []).map((c) => [c.id, c.color]));
  const code = new Map((courses.data ?? []).map((c) => [c.id, c.code]));
  const { warmup } = useCardFlags();
  const [i, setI] = useState(0);
  const [shown, setShown] = useState(false);
  const [confidence, setConfidence] = useState<Confidence | null>(null);
  const [toWarmup, setToWarmup] = useState(false);
  const [rated, setRated] = useState<Rated[]>([]);
  const [done, setDone] = useState(false);
  const shownAt = useRef(Date.now());
  const buffer = useMemo(
    () =>
      new ReviewBuffer(sendReviews, () => {
        // The queue moved on elsewhere (another device): refetch it for next time.
        void qc.invalidateQueries({ queryKey: keys.queue(courseId) });
        void qc.invalidateQueries({ queryKey: ["flash"] });
      }),
    [qc, courseId],
  );
  useEffect(() => {
    const stop = buffer.start();
    return () => {
      stop();
      void buffer.flush(true);
    };
  }, [buffer]);

  const card = cards[i];
  const intervals = useMemo(() => {
    const n = nextDue.find((x) => x.cardId === card?.id);
    return n
      ? { Again: interval(n.again), Hard: interval(n.hard), Good: interval(n.good), Easy: interval(n.easy) }
      : {};
  }, [nextDue, card?.id]);

  const finish = async () => {
    setDone(true);
    if (!(await buffer.drain()))
      toast({ message: "Some ratings are still waiting to sync; they'll retry.", tone: "danger" });
    void qc.invalidateQueries({ queryKey: ["flash"] });
    void qc.invalidateQueries({ queryKey: ["next"] });
  };

  const rate = (r: Rating) => {
    if (!card || !shown) return;
    const clientId = ulid();
    buffer.push({
      clientId,
      flashcardId: card.id,
      rating: RATING_NUMBER[r],
      reviewedAt: new Date().toISOString(),
      durationMs: Date.now() - shownAt.current,
      confidence,
    });
    // A review clears the card's warm-up flag on the server, so the rating goes first.
    if (toWarmup && !card.progress.warmupFlagged) {
      const id = card.id;
      void buffer.drain().finally(() => warmup.mutate({ id, flagged: true }));
    }
    setRated((x) => [
      ...x,
      {
        cardId: card.id,
        clientId,
        courseId: card.courseId,
        code: card.code,
        rating: r,
        confidence,
        stateBefore: card.progress.state,
      },
    ]);
    setShown(false);
    setConfidence(null);
    setToWarmup(false);
    shownAt.current = Date.now();
    if (i + 1 >= cards.length) void finish();
    else setI(i + 1);
  };

  // Space shows the answer; 1–4 rate once it's shown (frontend-spec 2.6).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (typingIn(e.target) || done) return;
      const k = flashKey(e);
      if (!k) return;
      e.preventDefault();
      if (k.kind === "reveal") setShown(true);
      else rate(k.rating);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const style = { "--course": (card && color.get(card.courseId)) ?? "var(--text-muted)" } as CSSProperties;
  const left = cards.length - i;

  return (
    <div className="flex min-h-screen flex-col" style={style}>
      <header className="sticky top-0 z-20 flex h-14 items-center gap-3 border-b border-border bg-bg px-4">
        <Button asChild size="iconSm" variant="ghost" aria-label="Close">
          <Link to="/flash">
            <X className="size-4" />
          </Link>
        </Button>
        <span className="text-sm font-medium">Flashcards</span>
        {card && !done ? <span className="font-mono text-sm text-muted">{code.get(card.courseId)}</span> : null}
        <span className="flex-1" />
        {!done && cards.length ? <span className="font-mono text-sm tnum">{left} left</span> : null}
      </header>
      {cards.length ? <Progress total={cards.length} done={done ? cards.length : i} /> : null}
      {!cards.length ? (
        <div className="mx-auto w-full max-w-[640px] px-4 py-10">
          <EmptyList
            title="Nothing due"
            body="You're through today's cards. New ones arrive tomorrow, up to your daily limit."
            action={
              <Button asChild size="sm">
                <Link to="/flash">Back to flashcards</Link>
              </Button>
            }
          />
        </div>
      ) : done ? (
        <SummaryView rated={rated} />
      ) : card ? (
        <div className="mx-auto flex w-full max-w-[640px] flex-1 flex-col gap-5 px-4 py-6">
          <div className="flex items-center gap-2">
            <Badge kind="code">{card.code}</Badge>
            {card.progress.state === "New" ? (
              <Badge kind="status" tone="info">
                New
              </Badge>
            ) : null}
          </div>
          <Panel className="flex min-h-[40vh] flex-col gap-4">
            <Markdown className="text-[17px]">{card.front}</Markdown>
            {shown ? (
              <div className="border-t border-border pt-4">
                <Markdown className="text-[17px]">{card.back}</Markdown>
              </div>
            ) : null}
          </Panel>
          {shown ? (
            <>
              <div className="flex flex-wrap items-center gap-3">
                <span className="text-sm text-muted">How sure were you?</span>
                <Segmented<Confidence>
                  label="How sure were you?"
                  size="sm"
                  value={confidence}
                  onChange={setConfidence}
                  options={[
                    { value: "Sure", label: "Sure" },
                    { value: "Unsure", label: "Unsure" },
                    { value: "Guess", label: "Guess" },
                  ]}
                />
              </div>
              <RatingButtons intervals={intervals} onRate={rate} />
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  className="size-4 accent-[var(--primary)]"
                  checked={toWarmup || card.progress.warmupFlagged}
                  disabled={card.progress.warmupFlagged}
                  onChange={(e) => setToWarmup(e.currentTarget.checked)}
                />
                Include in my next warm-up
              </label>
            </>
          ) : (
            <Button variant="primary" size="lg" onClick={() => setShown(true)}>
              Show answer
            </Button>
          )}
        </div>
      ) : null}
    </div>
  );
}

/** Segments while they stay readable (40 or fewer), else a plain bar. */
function Progress({ total, done }: { total: number; done: number }) {
  if (total > 40) {
    return (
      <div className="h-[3px] bg-sunken" aria-hidden="true">
        <div className="h-full bg-[var(--course)]" style={{ width: `${(done / total) * 100}%` }} />
      </div>
    );
  }
  return (
    <div className="flex h-[3px] gap-px" aria-hidden="true">
      {Array.from({ length: total }, (_, n) => (
        <div key={n} className={cn("flex-1", n < done ? "bg-[var(--course)]" : n === done ? "bg-text" : "bg-sunken")} />
      ))}
    </div>
  );
}

function SummaryView({ rated }: { rated: Rated[] }) {
  const s = summarize(rated);
  return (
    <div className="mx-auto flex w-full max-w-[640px] flex-col gap-5 px-4 py-8">
      <h1 className="m-0 text-xl font-semibold">
        {s.count} {s.count === 1 ? "card" : "cards"} reviewed
      </h1>
      <div className="grid grid-cols-4 gap-2">
        {(["Again", "Hard", "Good", "Easy"] as const).map((r) => (
          <Panel key={r} className="flex flex-col items-center gap-1 py-3">
            <span className="font-mono text-xl font-semibold tnum">{s.byRating[r]}</span>
            <span className="text-sm text-muted">{r}</span>
          </Panel>
        ))}
      </div>
      {s.lapsed.length ? (
        <Panel className="flex flex-col gap-2">
          <SectionLabel>Forgotten this time</SectionLabel>
          <p className="m-0 text-sm text-muted">You knew these before; they come back soon.</p>
          <div className="flex flex-wrap gap-1.5">
            {s.lapsed.map((r) => (
              <Badge key={r.clientId ?? r.cardId} kind="code">
                {r.code}
              </Badge>
            ))}
          </div>
        </Panel>
      ) : null}
      {s.sureWrong.length ? (
        <Panel className="flex flex-col gap-3">
          <SectionLabel>Sure, but wrong</SectionLabel>
          <p className="m-0 text-sm text-muted">
            A confident miss is usually a misconception. Log it as a slip so the warm-ups come back to it.
          </p>
          {s.sureWrong.map((r) => (
            <CardSlip key={r.clientId ?? r.cardId} r={r} />
          ))}
        </Panel>
      ) : null}
      <div className="flex gap-2">
        <Button asChild variant="primary">
          <Link to="/flash">Done</Link>
        </Button>
        <Button asChild>
          <Link to="/next">Take me there</Link>
        </Button>
      </div>
    </div>
  );
}

function CardSlip({ r }: { r: Rated }) {
  const log = useLogSlip();
  const [open, setOpen] = useState(false);
  const [type, setType] = useState("Concept");
  const [description, setDescription] = useState("");
  const errors = fieldErrors(log.error);
  if (log.isSuccess) return <p className="m-0 text-sm text-success-text">{r.code}: slip logged.</p>;
  if (!open) {
    return (
      <div className="flex items-center gap-3">
        <Badge kind="code">{r.code}</Badge>
        <Button size="sm" onClick={() => setOpen(true)}>
          Log a slip
        </Button>
      </div>
    );
  }
  const submit = (e: FormEvent) => {
    e.preventDefault();
    log.mutate({
      courseId: r.courseId ?? "",
      type,
      description: description.trim(),
      confidence: r.confidence,
      flashcardReviewClientId: r.clientId ?? null,
    });
  };
  return (
    <form className="flex flex-col gap-3 rounded-md border border-border p-3" onSubmit={submit} noValidate>
      <span className="font-mono text-sm">{r.code}</span>
      {log.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(log.error)}</Banner> : null}
      <div className="grid gap-3 sm:grid-cols-[12rem_1fr]">
        <Field label="Kind of slip" error={errors.type}>
          {(ids) => (
            <Select {...ids} value={type} onChange={(e) => setType(e.currentTarget.value)}>
              {SLIP_TYPES.map((t) => (
                <option key={t} value={t}>
                  {slipLabel(t)}
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
              value={description}
              onChange={(e) => setDescription(e.currentTarget.value)}
            />
          )}
        </Field>
      </div>
      <div className="flex justify-end gap-2">
        <Button size="sm" onClick={() => setOpen(false)}>
          Cancel
        </Button>
        <Button size="sm" type="submit" variant="primary" disabled={log.isPending || !description.trim()}>
          Log slip
        </Button>
      </div>
    </form>
  );
}
