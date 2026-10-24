import { createFileRoute, Link } from "@tanstack/react-router";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { ProgressBar } from "@/components/ui/progress";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { nextExam, useCourse, useDeadlines, useSubscription } from "@/features/courses/api";
import { deadlineName } from "@/features/deadlines/lib/grades";
import { TrapsPanel, WeakSpotsPanel } from "@/features/mistakes/components/weak-spots";
import { useSummary } from "@/features/next/api";
import { hours } from "@/features/week/lib/summary";
import { canEdit } from "@/features/courses/lib/roles";
import { useLectures, useProgress } from "@/features/lectures/api";
import { messageFor } from "@/lib/errors";
import { formatInZone, relativeDays } from "@/lib/time";

// Course overview: subscription, the next exam and its format, lecture progress and hours with the
// remaining range, the MC first-try trend, the weak spots (with proposals and resolved) and traps.
export const Route = createFileRoute("/_app/c/$courseId/")({ component: Overview });

function Overview() {
  const { courseId } = Route.useParams();
  const course = useCourse(courseId);
  const lectures = useLectures(courseId);
  const subscribed = course.data?.mySubscription === "Active";
  const progress = useProgress(courseId);
  const subscribe = useSubscription(courseId);
  const summary = useSummary(courseId, subscribed);
  const deadlines = useDeadlines(courseId, subscribed);
  const user = useUser();
  const exam = nextExam(deadlines.data ?? []);

  if (course.isPending || lectures.isPending) return <Skeleton className="h-48" />;
  const done = (progress.data ?? []).filter((p) => p.status === "Done").length;
  const actual = (progress.data ?? []).reduce((h, p) => h + p.actualHours, 0);
  const estimate = (lectures.data ?? []).reduce((h, l) => h + (l.estHours ?? 1.5), 0);

  return (
    <div className="flex flex-col gap-4">
      {!subscribed ? (
        <Banner
          tone="notice"
          action={
            <Button
              size="sm"
              variant="primary"
              disabled={subscribe.isPending}
              onClick={() => subscribe.mutate("Active")}
            >
              Subscribe
            </Button>
          }
        >
          {course.data?.mySubscription === "Archived"
            ? "This course is archived for you. Subscribe again to plan and track it."
            : "Subscribe to track your progress, plan your study and get reminders."}
          {subscribe.error ? <span className="block text-danger-text">{messageFor(subscribe.error)}</span> : null}
        </Banner>
      ) : null}
      <div className="grid gap-4 md:grid-cols-2">
        <Card className="flex flex-col gap-4">
          <SectionLabel>Progress</SectionLabel>
          {lectures.data?.length ? (
            <ProgressBar
              value={done}
              max={lectures.data.length}
              label={`Lectures ${done} / ${lectures.data.length}`}
              detail={`${actual.toFixed(1)} of ${estimate.toFixed(0)} h`}
            />
          ) : (
            <EmptyList
              title="No lectures yet"
              body="Add the course's lectures or weeks; materials and questions hang off them."
              action={
                canEdit(course.data?.myRole) ? (
                  <Button asChild size="sm" variant="primary">
                    <Link to="/c/$courseId/lectures" params={{ courseId }}>
                      Add lectures
                    </Link>
                  </Button>
                ) : undefined
              }
            />
          )}
        </Card>
        {subscribed ? (
          <Card className="flex flex-col gap-3">
            <div className="flex items-center justify-between">
              <SectionLabel>Next exam</SectionLabel>
              <Link to="/c/$courseId/exams" params={{ courseId }} className="text-sm text-muted hover:text-text">
                Exams
              </Link>
            </div>
            {exam ? (
              <>
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-md font-medium">{deadlineName(exam)}</span>
                  <Badge kind="status" tone="info">
                    {relativeDays(exam.startsAt, user.timeZone)}
                  </Badge>
                </div>
                <span className="font-mono text-sm text-muted tnum">
                  {formatInZone(exam.startsAt, user.timeZone, "EEEE d MMMM, HH:mm")}
                  {exam.durationMin ? ` · ${exam.durationMin} min` : ""}
                </span>
                {exam.format ? <p className="m-0 text-sm">{exam.format}</p> : null}
              </>
            ) : (
              <p className="m-0 text-muted">No exam date yet. Add it so the plan can work back from it.</p>
            )}
          </Card>
        ) : null}
        {summary.data ? (
          <Card className="flex flex-col gap-3">
            <SectionLabel>Time left</SectionLabel>
            <p className="m-0">
              About{" "}
              <span className="font-mono font-medium tnum">{hours(Math.round(summary.data.remainingHours.mid))}</span>{" "}
              to go{" "}
              <span className="text-sm text-muted">
                ({hours(Math.round(summary.data.remainingHours.low))}–
                {hours(Math.round(summary.data.remainingHours.high))})
              </span>
            </p>
            {summary.data.remainingHours.ratio ? (
              <p className="m-0 text-sm text-muted">
                You take {Math.round(summary.data.remainingHours.ratio * 100)}% of the estimate on finished lectures.
              </p>
            ) : (
              <p className="m-0 text-sm text-muted">The range narrows once you've finished 3 lectures.</p>
            )}
            <SectionLabel className="mt-2">MC first try, last 6 weeks</SectionLabel>
            <Trend weeks={summary.data.mcTrend} />
          </Card>
        ) : null}
        {subscribed ? <WeakSpotsPanel courseId={courseId} /> : null}
        {subscribed ? <TrapsPanel courseId={courseId} /> : null}
        <Card className="flex flex-col gap-3">
          <SectionLabel>About</SectionLabel>
          <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 text-sm">
            <dt className="text-muted">Owner</dt>
            <dd className="m-0">{course.data?.owner.name}</dd>
            <dt className="text-muted">Your role</dt>
            <dd className="m-0">{course.data?.myRole}</dd>
            <dt className="text-muted">Grade scale</dt>
            <dd className="m-0">{course.data?.gradeScale.name}</dd>
            {course.data?.institution ? (
              <>
                <dt className="text-muted">Institution</dt>
                <dd className="m-0">{course.data.institution}</dd>
              </>
            ) : null}
            {course.data?.term ? (
              <>
                <dt className="text-muted">Term</dt>
                <dd className="m-0">{course.data.term}</dd>
              </>
            ) : null}
          </dl>
        </Card>
      </div>
    </div>
  );
}

/** Six small bars, one per week; a week without tries shows an empty slot (with a table for readers). */
function Trend({ weeks }: { weeks: { isoWeek: string; rate?: number | null; tries: number }[] }) {
  if (!weeks.some((w) => w.tries)) return <p className="m-0 text-sm text-muted">No MC answers yet.</p>;
  return (
    <div className="flex flex-col gap-1">
      <div className="flex h-16 items-end gap-1.5" aria-hidden="true">
        {weeks.map((w) => (
          <div key={w.isoWeek} className="flex h-full max-w-12 flex-1 items-end rounded-sm bg-sunken">
            {w.rate !== null && w.rate !== undefined ? (
              <div
                className="w-full rounded-sm bg-[var(--course)]"
                style={{ height: `${Math.max(4, w.rate * 100)}%` }}
              />
            ) : null}
          </div>
        ))}
      </div>
      <table className="sr-only">
        <tbody>
          {weeks.map((w) => (
            <tr key={w.isoWeek}>
              <th>{w.isoWeek}</th>
              <td>
                {w.rate !== null && w.rate !== undefined ? `${Math.round(w.rate * 100)}% of ${w.tries}` : "no tries"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="flex gap-1.5 font-mono text-[11px] text-muted tnum">
        {weeks.map((w) => (
          <span key={w.isoWeek} className="max-w-12 flex-1 text-center">
            {w.rate !== null && w.rate !== undefined ? `${Math.round(w.rate * 100)}` : "–"}
          </span>
        ))}
      </div>
    </div>
  );
}
