import { TZDate } from "@date-fns/tz";
import { useQueryClient } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { addDays, startOfDay } from "date-fns";
import { ArrowRight } from "lucide-react";
import type { CSSProperties } from "react";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Checklist } from "@/components/ui/empty";
import { Dot, Skeleton } from "@/components/ui/misc";
import { PlanBlock } from "@/components/ui/plan-block";
import { EntryLabel } from "@/features/plan/components/entry-label";
import { ProgressBar } from "@/components/ui/progress";
import { SeverityBar, type Severity } from "@/components/ui/severity";
import { useCourses, useNextExams, type Course } from "@/features/courses/api";
import { CheckInForm } from "@/features/deadlines/components/check-in-form";
import { deadlineName } from "@/features/deadlines/lib/grades";
import { keys as nextKeys, useNext, usePatterns, useSummary } from "@/features/next/api";
import { usePlan, useShort } from "@/features/plan/api";
import { hours } from "@/features/week/lib/summary";
import { formatInZone, relativeDays } from "@/lib/time";
import { t } from "@/lib/t";

// Home (frontend-spec 3.4, 4): greeting + date; the ink Take me there card with the check-in beside
// it; "X h short" banners; Today and Exams; course cards; recurring mistakes. A new user sees the
// first-run checklist instead.
export const Route = createFileRoute("/_app/")({ component: Home });

function Home() {
  const user = useUser();
  const courses = useCourses();
  const live = (courses.data ?? []).filter((c) => !c.trashedAt && c.mySubscription === "Active");
  const next = useNextExams(live);
  const hasExam = Object.values(next).some(Boolean);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-baseline gap-3">
        <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">
          {t("home.greeting", { name: user.firstName || user.username })}
        </h1>
        <span className="text-sm text-muted">{formatInZone(new Date(), user.timeZone, "EEEE d MMMM")}</span>
      </div>
      {courses.isPending ? (
        <Skeleton className="h-40" />
      ) : !live.length || !hasExam ? (
        <FirstRun live={live} hasExam={hasExam} />
      ) : (
        <Dashboard live={live} />
      )}
    </div>
  );
}

function FirstRun({ live, hasExam }: { live: Course[]; hasExam: boolean }) {
  return (
    <Card className="flex max-w-[560px] flex-col gap-4">
      <SectionLabel>Get started</SectionLabel>
      <Checklist
        steps={[
          {
            label: "Create your first course",
            done: live.length > 0,
            action: (
              <Button asChild size="sm" variant="primary">
                <Link to="/courses/new">{t("nav.newCourse")}</Link>
              </Button>
            ),
          },
          {
            label: "Add the exam date",
            done: hasExam,
            action: live[0] ? (
              <Button asChild size="sm" variant={live.length ? "primary" : "secondary"}>
                <Link to="/c/$courseId/exams" params={{ courseId: live[0].id }}>
                  Add {live[0].code}'s exam
                </Link>
              </Button>
            ) : undefined,
          },
          {
            label: "Upload the slides",
            done: false,
            action: live[0] ? (
              <Button asChild size="sm">
                <Link to="/c/$courseId/lectures" params={{ courseId: live[0].id }}>
                  Open lectures
                </Link>
              </Button>
            ) : undefined,
          },
        ]}
      />
    </Card>
  );
}

function Dashboard({ live }: { live: Course[] }) {
  const user = useUser();
  const tz = user.timeZone;
  const nextView = useNext();
  const short = useShort();
  const exams = useNextExams(live);
  const qc = useQueryClient();
  const checkIns = nextView.data?.checkIns ?? [];
  const byCourse = Object.fromEntries(live.map((c) => [c.id, c]));
  const upcoming = Object.values(exams)
    .filter((d) => !!d)
    .sort((a, b) => a.startsAt.localeCompare(b.startsAt));

  return (
    <>
      <div className="grid gap-4 lg:grid-cols-3">
        <Card className="flex flex-col gap-4 border-primary bg-primary text-primary-text lg:col-span-2">
          <span className="text-xs font-medium uppercase tracking-[0.06em] opacity-70">{t("nav.takeMeThere")}</span>
          {nextView.isPending ? (
            <Skeleton className="h-10 opacity-20" />
          ) : (
            <p className="m-0 text-lg font-medium">
              {nextView.data?.steps.length
                ? nextView.data.steps.map((s) => s.title).join(", then ")
                : "You're up to date."}
            </p>
          )}
          {nextView.data?.reason ? <p className="m-0 text-sm opacity-75">{nextView.data.reason}</p> : null}
          <div className="mt-auto">
            <Button asChild className="border-primary-text/30 bg-primary-text text-primary hover:bg-primary-text/90">
              <Link to="/next">
                Start
                <ArrowRight className="size-4" aria-hidden="true" />
              </Link>
            </Button>
          </div>
        </Card>
        {checkIns[0] ? (
          <Card className="flex flex-col gap-3">
            <SectionLabel>
              How did {byCourse[checkIns[0].courseId]?.code ?? ""} {deadlineName(checkIns[0])} go?
            </SectionLabel>
            <CheckInForm
              deadline={checkIns[0]}
              compact
              onDone={() => void qc.invalidateQueries({ queryKey: nextKeys.next })}
            />
          </Card>
        ) : (
          <ExamsCard upcoming={upcoming} byCourse={byCourse} />
        )}
      </div>
      {(short.data ?? []).map((s) => (
        <Banner
          key={s.courseId}
          tone="warning"
          action={
            <Button asChild size="sm">
              <Link to="/c/$courseId/plan" params={{ courseId: s.courseId }}>
                See the plan
              </Link>
            </Button>
          }
        >
          {s.courseCode} is {hours(s.hours)} short before the exam.
        </Banner>
      ))}
      <div className="grid gap-4 lg:grid-cols-2">
        <Today tz={tz} />
        {checkIns[0] ? <ExamsCard upcoming={upcoming} byCourse={byCourse} /> : <Patterns />}
      </div>
      <section className="flex flex-col gap-3">
        <SectionLabel>Your courses</SectionLabel>
        <div className="grid gap-4 [grid-template-columns:repeat(auto-fit,minmax(280px,1fr))]">
          {live.map((c) => (
            <CourseCard key={c.id} course={c} />
          ))}
        </div>
      </section>
      {checkIns[0] ? <Patterns /> : null}
    </>
  );
}

function ExamsCard({
  upcoming,
  byCourse,
}: {
  upcoming: NonNullable<ReturnType<typeof useNextExams>[string]>[];
  byCourse: Record<string, Course>;
}) {
  const user = useUser();
  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>Exams</SectionLabel>
      {upcoming.length ? (
        <ul className="m-0 flex list-none flex-col gap-2.5 p-0">
          {upcoming.map((d) => (
            <li key={d.id} className="flex items-center gap-2">
              <Dot color={byCourse[d.courseId]?.color} />
              <Link
                to="/c/$courseId/exams"
                params={{ courseId: d.courseId }}
                className="min-w-0 flex-1 truncate font-medium hover:underline"
              >
                {byCourse[d.courseId]?.code} {deadlineName(d)}
              </Link>
              <span className="font-mono text-sm text-muted tnum">
                {formatInZone(d.startsAt, user.timeZone, "d MMM")}
              </span>
              <Badge kind="status" tone="info">
                {relativeDays(d.startsAt, user.timeZone)}
              </Badge>
            </li>
          ))}
        </ul>
      ) : (
        <p className="m-0 text-muted">No upcoming exams.</p>
      )}
    </Card>
  );
}

function Today({ tz }: { tz: string }) {
  const from = startOfDay(new TZDate(new Date(), tz));
  const plan = usePlan(from, addDays(from, 1));
  const courses = useCourses();
  const colors = Object.fromEntries((courses.data ?? []).map((c) => [c.id, c.color]));
  const entries = (plan.data ?? []).filter((e) => e.status !== "Moved");
  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <SectionLabel>Today</SectionLabel>
        <Link to="/plan" className="text-sm text-muted hover:text-text">
          Plan
        </Link>
      </div>
      {plan.isPending ? (
        <Skeleton className="h-24" />
      ) : entries.length ? (
        <div className="flex flex-col gap-2">
          {entries.map((e) => (
            <PlanBlock
              key={e.id}
              time={`${formatInZone(e.startAt, tz, "HH:mm")}–${formatInZone(e.endAt, tz, "HH:mm")}`}
              title={
                <>
                  <EntryLabel entry={e} />
                </>
              }
              pinned={e.pinned || e.createdBy === "User"}
              color={colors[e.courseId]}
              status={e.status as "Planned" | "Done" | "Skipped"}
            />
          ))}
        </div>
      ) : (
        <p className="m-0 text-muted">Nothing planned today.</p>
      )}
    </Card>
  );
}

function CourseCard({ course }: { course: Course }) {
  const s = useSummary(course.id);
  const user = useUser();
  return (
    <Card className="flex flex-col gap-3" style={{ "--course": course.color } as CSSProperties}>
      <Link to="/c/$courseId" params={{ courseId: course.id }} className="flex items-center gap-2 hover:underline">
        <Dot color={course.color} />
        <span className="font-mono text-sm font-medium">{course.code}</span>
        <span className="min-w-0 flex-1 truncate">{course.name}</span>
      </Link>
      {s.isPending ? (
        <Skeleton className="h-20" />
      ) : s.data ? (
        <>
          <ProgressBar
            value={s.data.lecturesDone}
            max={Math.max(1, s.data.lecturesTotal)}
            label={`Lectures ${s.data.lecturesDone} / ${s.data.lecturesTotal}`}
            detail={`${hours(Math.round(s.data.hoursActual * 10) / 10)} of ${hours(Math.round(s.data.hoursEstimate))}`}
          />
          <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted">
            <span>
              MC first try{" "}
              <span className="font-mono text-text tnum">
                {s.data.mcFirstTryRate === null || s.data.mcFirstTryRate === undefined
                  ? "–"
                  : `${Math.round(s.data.mcFirstTryRate * 100)}%`}
              </span>
            </span>
            {s.data.nextExam ? <span>Exam {relativeDays(s.data.nextExam.startsAt, user.timeZone)}</span> : null}
          </div>
          {s.data.topWeakSpots.length ? (
            <div className="flex flex-col gap-1.5">
              {s.data.topWeakSpots.map((w) => (
                <SeverityBar key={w.id} topic={w.topic} severity={w.severity as Severity} />
              ))}
            </div>
          ) : (
            <p className="m-0 text-sm text-muted">No open weak spots.</p>
          )}
        </>
      ) : null}
    </Card>
  );
}

function Patterns() {
  const p = usePatterns();
  const list = (p.data ?? []).filter((x) => x.courses.length > 1 || x.openSlips > 1);
  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>Recurring mistakes</SectionLabel>
      {p.isPending ? (
        <Skeleton className="h-16" />
      ) : list.length ? (
        <ul className="m-0 flex list-none flex-col gap-2 p-0">
          {list.map((x) => (
            <li key={x.type} className="flex flex-wrap items-center gap-2">
              <span className="font-medium">{x.type}</span>
              <span className="text-sm text-muted">{x.openSlips} open</span>
              <span className="ml-auto flex flex-wrap gap-1">
                {x.courses.map((c) => (
                  <Badge key={c.courseId} kind="code">
                    {c.code} {c.count}
                  </Badge>
                ))}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <p className="m-0 text-muted">No mistake keeps coming back. Slips from mocks and practice collect here.</p>
      )}
    </Card>
  );
}
