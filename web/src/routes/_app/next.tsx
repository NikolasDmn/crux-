import { useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { Check, ExternalLink } from "lucide-react";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Dot, Skeleton } from "@/components/ui/misc";
import { CheckInForm } from "@/features/deadlines/components/check-in-form";
import { keys, useAdvance, useNext, type Step } from "@/features/next/api";
import { Warmup } from "@/features/next/components/warmup";
import { cn } from "@/lib/cn";
import { messageFor } from "@/lib/errors";

// Take me there (frontend-spec 4): the steps from GET /next one at a time: the warm-up, then the
// lecture, mock or flashcards. "Skip" and "Done" post /next/advance (stateless) and move on.
// The step lives in the URL, so leaving for the lecture and coming back keeps your place.
export const Route = createFileRoute("/_app/next")({
  validateSearch: (s: Record<string, unknown>): { step?: number } =>
    Number(s.step) > 0 ? { step: Math.floor(Number(s.step)) } : {},
  component: TakeMeThere,
});

function TakeMeThere() {
  const next = useNext();
  const advance = useAdvance();
  const qc = useQueryClient();
  const navigate = useNavigate({ from: Route.fullPath });
  const i = Route.useSearch().step ?? 0;
  const setI = (n: number) => void navigate({ search: n ? { step: n } : {}, replace: true });

  if (next.isPending) return <Skeleton className="h-64" />;
  if (next.isError) return <Banner tone="danger">{messageFor(next.error)}</Banner>;
  const { steps, course, reason, checkIns } = next.data;
  const step = steps[i];

  const go = (skipped: boolean) => {
    if (!step) return;
    advance.mutate(
      {
        stepIndex: i,
        kind: step.kind,
        targetType: step.target.type,
        targetId: step.target.id,
        planEntryId: step.planEntryId ?? null,
        skipped,
      },
      { onSuccess: () => setI(i + 1) },
    );
  };

  return (
    <div className="flex max-w-[760px] flex-col gap-5">
      <div className="flex flex-col gap-1">
        <h1 className="m-0 flex items-center gap-2 text-xl font-semibold tracking-[-0.01em]">
          {course ? <Dot color={course.color} /> : null}
          Take me there
        </h1>
        <p className="m-0 text-muted">{reason}</p>
      </div>
      {checkIns.map((d) => (
        <Card key={d.id} className="flex flex-col gap-3">
          <SectionLabel>Exam check-in</SectionLabel>
          <CheckInForm deadline={d} onDone={() => void qc.invalidateQueries({ queryKey: keys.next })} />
        </Card>
      ))}
      {!steps.length ? (
        <EmptyList
          title="Nothing lined up"
          body={course ? "You're up to date. Check the plan for what's next." : reason}
          action={
            <Button asChild size="sm" variant="primary">
              <Link to={course ? "/plan" : "/courses/new"}>{course ? "Open the plan" : "New course"}</Link>
            </Button>
          }
        />
      ) : (
        <>
          <ol className="m-0 flex list-none flex-wrap gap-2 p-0" aria-label="Steps">
            {steps.map((s, n) => (
              <li
                key={n}
                aria-current={n === i ? "step" : undefined}
                className={cn(
                  "flex items-center gap-1.5 rounded-md border px-2.5 py-1 text-sm",
                  n === i ? "border-text font-medium" : "border-border text-muted",
                )}
              >
                {n < i ? (
                  <Check className="size-3.5" aria-label="Done" />
                ) : (
                  <span className="font-mono tnum">{n + 1}</span>
                )}
                {s.title}
                <span className="font-mono text-xs tnum">{s.estMinutes} min</span>
              </li>
            ))}
          </ol>
          {advance.error ? <Banner tone="danger">{messageFor(advance.error)}</Banner> : null}
          {step ? (
            step.kind === "warmup" && course ? (
              <Warmup key={i} courseId={course.id} onFinished={() => go(false)} />
            ) : (
              <StepCard
                step={step}
                courseId={course?.id}
                onDone={() => go(false)}
                onSkip={() => go(true)}
                busy={advance.isPending}
              />
            )
          ) : (
            <Card className="flex flex-col items-start gap-3">
              <h2 className="m-0 text-lg font-semibold">That's everything for now.</h2>
              <p className="m-0 text-muted">Nice work. Come back later, or look at the rest of the week.</p>
              <div className="flex gap-2">
                <Button
                  variant="primary"
                  onClick={() => {
                    setI(0);
                    void next.refetch();
                  }}
                >
                  What's next?
                </Button>
                <Button asChild>
                  <Link to="/week">This week</Link>
                </Button>
              </div>
            </Card>
          )}
        </>
      )}
    </div>
  );
}

function StepCard({
  step,
  courseId,
  onDone,
  onSkip,
  busy,
}: {
  step: Step;
  courseId: string | undefined;
  onDone: () => void;
  onSkip: () => void;
  busy: boolean;
}) {
  const link =
    step.kind === "lecture" && step.componentId && courseId ? (
      <Link to="/c/$courseId/l/$componentId" params={{ courseId, componentId: step.componentId }}>
        Open the lecture
      </Link>
    ) : step.kind === "mock" && courseId ? (
      <Link to="/c/$courseId/mocks" params={{ courseId }}>
        Start a mock
      </Link>
    ) : step.kind === "flashcards" ? (
      <Link to="/flash">Review the cards</Link>
    ) : courseId ? (
      <Link to="/c/$courseId" params={{ courseId }}>
        Open the course
      </Link>
    ) : null;
  const what =
    step.kind === "lecture"
      ? "Work through the lecture: the slides, the notes and their check questions."
      : step.kind === "mock"
        ? "A timed mock, as close to the real exam as the bank allows."
        : step.kind === "flashcards"
          ? "Clear the cards that are due."
          : "Revise: go over your weak spots and the key rules.";
  return (
    <Card className="flex flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h2 className="m-0 text-lg font-semibold">{step.title}</h2>
        <p className="m-0 text-muted">
          {what} About {step.estMinutes} minutes.
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        {link ? (
          <Button asChild variant="primary">
            {link}
          </Button>
        ) : null}
        <span className="flex-1" />
        <Button variant="ghost" disabled={busy} onClick={onSkip}>
          Skip
        </Button>
        <Button disabled={busy} onClick={onDone}>
          <Check className="size-4" aria-hidden="true" />
          Done
        </Button>
      </div>
      {step.kind === "lecture" ? (
        <p className="m-0 flex items-center gap-1 text-sm text-muted">
          <ExternalLink className="size-3.5" aria-hidden="true" />
          Come back here and press Done when you've finished.
        </p>
      ) : null}
    </Card>
  );
}
