import { addDays } from "date-fns";
import { ChevronLeft, ChevronRight, Plus, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useUser } from "@/app/auth";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { PlanBlock } from "@/components/ui/plan-block";
import { EntryLabel } from "@/features/plan/components/entry-label";
import { Skeleton } from "@/components/ui/misc";
import { useCourses, useDeadlines, useNextExams } from "@/features/courses/api";
import { usePlan, useRebuild, useSettings, useShort, type PlanEntry } from "@/features/plan/api";
import { EntryDialog, type Draft } from "@/features/plan/components/entry-dialog";
import { WeekCalendar } from "@/features/plan/components/week-calendar";
import { hours } from "@/features/week/lib/summary";
import { dayKey, isoWeekOf, parseIsoWeek, shiftWeek, studyWindow, weekDays, weekStart } from "@/features/plan/lib/week";
import { messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";

// Plan (frontend-spec 4): the week inside the study window; drag to move (the old entry becomes
// Moved, the new one is pinned), drag the bottom edge to resize, click to edit, done or skip;
// "+ Add"; "Rebuild plan" with its change summary; "X h short" warnings.
export function PlanView({
  courseId,
  isoWeek,
  onWeek,
}: {
  courseId?: string;
  isoWeek?: string | undefined;
  onWeek: (w: string) => void;
}) {
  const user = useUser();
  const tz = user.timeZone;
  const thisWeek = isoWeekOf(new Date(), tz);
  const week = isoWeek && parseIsoWeek(isoWeek, tz) ? isoWeek : thisWeek;
  const start = parseIsoWeek(week, tz) ?? weekStart(new Date(), tz);
  const days = weekDays(start);
  const plan = usePlan(start, addDays(start, 7), courseId);
  const settings = useSettings();
  const span = studyWindow(settings.data?.extra as Record<string, unknown> | undefined);
  const short = useShort();
  const rebuild = useRebuild();
  const courses = useCourses();
  const colors = Object.fromEntries((courses.data ?? []).map((c) => [c.id, c.color]));
  const nextAll = useNextExams(courseId ? [] : courses.data);
  const own = useDeadlines(courseId ?? "", !!courseId);
  const exams = (courseId ? (own.data ?? []) : Object.values(nextAll).filter((d) => !!d)).filter(
    (d) => d.type !== "Deadline" || courseId,
  );
  const [open, setOpen] = useState<{ entry?: PlanEntry; draft?: Draft } | null>(null);
  const warnings = (short.data ?? []).filter((s) => !courseId || s.courseId === courseId);
  const entries = plan.data ?? [];

  const addNow = () => {
    const at = new Date();
    at.setMinutes(Math.ceil(at.getMinutes() / 15) * 15, 0, 0);
    setOpen({ draft: { courseId, startAt: at, endAt: new Date(at.getTime() + 3_600_000) } });
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex items-center gap-1">
          <Button
            size="iconSm"
            variant="ghost"
            aria-label="Previous week"
            onClick={() => onWeek(shiftWeek(week, -1, tz))}
          >
            <ChevronLeft className="size-4" />
          </Button>
          <Button size="iconSm" variant="ghost" aria-label="Next week" onClick={() => onWeek(shiftWeek(week, 1, tz))}>
            <ChevronRight className="size-4" />
          </Button>
        </div>
        <h2 className="m-0 text-md font-semibold">
          {formatInZone(days[0]!, tz, "d MMM")} – {formatInZone(days[6]!, tz, "d MMM yyyy")}
        </h2>
        {week !== thisWeek ? (
          <Button size="sm" variant="ghost" onClick={() => onWeek(thisWeek)}>
            This week
          </Button>
        ) : null}
        <div className="ml-auto flex gap-2">
          <Button size="sm" disabled={rebuild.isPending} onClick={() => rebuild.mutate()}>
            <RefreshCw className={rebuild.isPending ? "size-4 animate-spin" : "size-4"} aria-hidden="true" />
            Rebuild plan
          </Button>
          <Button size="sm" variant="primary" onClick={addNow}>
            <Plus className="size-4" aria-hidden="true" />
            Add
          </Button>
        </div>
      </div>
      {rebuild.data ? <Banner tone="notice">{rebuild.data.changeSummary}</Banner> : null}
      {rebuild.error ? <Banner tone="danger">{messageFor(rebuild.error)}</Banner> : null}
      {warnings.map((s) => (
        <Banner key={s.courseId} tone="warning">
          {s.courseCode} is {hours(s.hours)} short: some lectures land in the final week or don't fit before the exam.
          Widen your study window or daily hours in Settings, or drop a lecture.
        </Banner>
      ))}
      {plan.isPending ? (
        <Skeleton className="h-[480px]" />
      ) : (
        <>
          <div className="hidden md:block">
            <WeekCalendar
              days={days}
              entries={entries}
              exams={exams}
              span={span}
              colors={colors}
              onOpen={(entry) => setOpen({ entry })}
              onAdd={(draft) => setOpen({ draft: { ...draft, courseId } })}
            />
            <p className="m-0 mt-2 text-sm text-muted">
              Drag to move, drag the bottom edge to resize, double-click an empty slot to add. Moving an entry pins it.
            </p>
          </div>
          <Agenda days={days} entries={entries} colors={colors} onOpen={(entry) => setOpen({ entry })} />
        </>
      )}
      {open ? <EntryDialog entry={open.entry} draft={open.draft} onClose={() => setOpen(null)} /> : null}
    </div>
  );
}

/** Phones: the week as a list per day (edit times in the dialog). */
function Agenda({
  days,
  entries,
  colors,
  onOpen,
}: {
  days: ReturnType<typeof weekDays>;
  entries: PlanEntry[];
  colors: Record<string, string>;
  onOpen: (e: PlanEntry) => void;
}) {
  const user = useUser();
  const tz = user.timeZone;
  return (
    <div className="flex flex-col gap-4 md:hidden">
      {days.map((d) => {
        const list = entries
          .filter((e) => e.status !== "Moved" && dayKey(e.startAt, tz) === dayKey(d, tz))
          .sort((a, b) => a.startAt.localeCompare(b.startAt));
        return (
          <section key={dayKey(d, tz)} className="flex flex-col gap-2">
            <h3 className="m-0 text-xs font-medium uppercase tracking-[0.06em] text-muted">
              {formatInZone(d, tz, "EEEE d MMM")}
            </h3>
            {list.length ? (
              list.map((e) => (
                <button key={e.id} type="button" className="text-left" onClick={() => onOpen(e)}>
                  <PlanBlock
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
                </button>
              ))
            ) : (
              <p className="m-0 text-sm text-muted">Nothing planned.</p>
            )}
          </section>
        );
      })}
    </div>
  );
}
