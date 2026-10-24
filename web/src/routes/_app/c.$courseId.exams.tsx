import { createFileRoute } from "@tanstack/react-router";
import { MoreHorizontal, Plus } from "lucide-react";
import { useState } from "react";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import { useCourse, useDeadlines, type Deadline } from "@/features/courses/api";
import { usePatchDeadline } from "@/features/deadlines/api";
import { CheckInForm } from "@/features/deadlines/components/check-in-form";
import { DeadlineDialog } from "@/features/deadlines/components/deadline-dialog";
import { LayoutDialog } from "@/features/deadlines/components/layout-dialog";
import { deadlineName, gradeLabel } from "@/features/deadlines/lib/grades";
import { messageFor } from "@/lib/errors";
import { formatInZone, relativeDays } from "@/lib/time";

// Exams (frontend-spec 4): exams, resits and deadlines with format and pass mark; the layout
// editor; the check-in result form; "Create resit" after a fail.
export const Route = createFileRoute("/_app/c/$courseId/exams")({ component: Exams });

type Open =
  | { kind: "add" }
  | { kind: "edit"; d: Deadline }
  | { kind: "resit"; d: Deadline }
  | { kind: "layout"; d: Deadline }
  | null;

const STATUS_TONE = { Upcoming: "info", Done: "success", Missed: "danger", Cancelled: "neutral" } as const;

function Exams() {
  const { courseId } = Route.useParams();
  const course = useCourse(courseId);
  const subscribed = course.data?.mySubscription === "Active";
  const deadlines = useDeadlines(courseId, !!course.data?.mySubscription);
  const [open, setOpen] = useState<Open>(null);
  const close = (o: boolean) => !o && setOpen(null);

  if (course.isPending || deadlines.isPending) return <Skeleton className="h-48" />;
  const list = [...(deadlines.data ?? [])].sort((a, b) => a.startsAt.localeCompare(b.startsAt));
  const upcoming = list.filter((d) => d.status === "Upcoming" && !d.awaitingResult);
  const waiting = list.filter((d) => d.awaitingResult);
  const past = list.filter((d) => d.status !== "Upcoming").reverse();

  return (
    <div className="flex max-w-[860px] flex-col gap-4">
      {!course.data?.mySubscription ? (
        <Banner tone="notice">Subscribe to this course (on the Overview tab) to add your exam dates.</Banner>
      ) : null}
      <div className="flex items-center justify-between gap-3">
        <SectionLabel>Exams and deadlines</SectionLabel>
        {subscribed ? (
          <Button size="sm" variant="primary" onClick={() => setOpen({ kind: "add" })}>
            <Plus className="size-4" aria-hidden="true" />
            Add
          </Button>
        ) : null}
      </div>
      {waiting.map((d) => (
        <Card key={d.id} className="flex flex-col gap-3 border-[color:var(--course)]">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="m-0 text-md font-semibold">How did {deadlineName(d)} go?</h3>
            <Badge tone="warning" kind="status">
              Waiting for a result
            </Badge>
          </div>
          <CheckInForm deadline={d} />
        </Card>
      ))}
      {!list.length ? (
        <EmptyList
          title="No exam dates yet"
          body="Add the exam and Crux plans your lectures, revision and a timed mock before it."
          action={
            subscribed ? (
              <Button size="sm" variant="primary" onClick={() => setOpen({ kind: "add" })}>
                Add the exam
              </Button>
            ) : undefined
          }
        />
      ) : null}
      {upcoming.length ? (
        <Card className="p-0">
          <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
            {upcoming.map((d) => (
              <DeadlineRow key={d.id} d={d} onOpen={setOpen} canAct={subscribed} />
            ))}
          </ul>
        </Card>
      ) : null}
      {past.length ? (
        <>
          <SectionLabel className="mt-2">Past</SectionLabel>
          <Card className="p-0">
            <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
              {past.map((d) => (
                <DeadlineRow key={d.id} d={d} onOpen={setOpen} canAct={subscribed} />
              ))}
            </ul>
          </Card>
        </>
      ) : null}
      {open?.kind === "add" ? <DeadlineDialog courseId={courseId} open onOpenChange={close} /> : null}
      {open?.kind === "edit" ? <DeadlineDialog courseId={courseId} open onOpenChange={close} editing={open.d} /> : null}
      {open?.kind === "resit" ? (
        <DeadlineDialog courseId={courseId} open onOpenChange={close} resitOf={open.d} />
      ) : null}
      {open?.kind === "layout" ? <LayoutDialog deadline={open.d} open onOpenChange={close} /> : null}
    </div>
  );
}

function DeadlineRow({ d, onOpen, canAct }: { d: Deadline; onOpen: (o: Open) => void; canAct: boolean }) {
  const user = useUser();
  const patch = usePatchDeadline();
  const toast = useToast();
  const exam = d.type !== "Deadline";
  const grade = gradeLabel(d);
  const questions = d.layout.reduce((n, l) => n + (l.questionCount ?? 0), 0);
  const cancel = () =>
    patch.mutate(
      { id: d.id, version: d.version, body: { status: "Cancelled" } },
      { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) },
    );

  return (
    <li className="flex flex-col gap-2 px-5 py-3.5">
      <div className="flex flex-wrap items-center gap-2">
        <Badge kind="status">{d.type}</Badge>
        <span className="font-medium">{deadlineName(d)}</span>
        {d.status !== "Upcoming" ? (
          <Badge kind="status" tone={STATUS_TONE[d.status as keyof typeof STATUS_TONE] ?? "neutral"}>
            {d.status === "Done" && d.passed === true
              ? "Passed"
              : d.status === "Done" && d.passed === false
                ? "Not passed"
                : d.status}
          </Badge>
        ) : null}
        <span className="ml-auto flex items-center gap-2 text-sm text-muted">
          <span className="font-mono tnum">{formatInZone(d.startsAt, user.timeZone, "EEE d MMM yyyy, HH:mm")}</span>
          {d.status === "Upcoming" ? <span>· {relativeDays(d.startsAt, user.timeZone)}</span> : null}
        </span>
        {canAct ? (
          <Menu>
            <MenuTrigger asChild>
              <Button size="iconSm" variant="ghost" aria-label={`Actions for ${deadlineName(d)}`}>
                <MoreHorizontal className="size-4" />
              </Button>
            </MenuTrigger>
            <MenuContent align="end">
              <MenuItem onSelect={() => onOpen({ kind: "edit", d })}>Edit</MenuItem>
              {exam ? <MenuItem onSelect={() => onOpen({ kind: "layout", d })}>Exam layout</MenuItem> : null}
              {exam && d.status !== "Upcoming" ? (
                <MenuItem onSelect={() => onOpen({ kind: "resit", d })}>Create resit</MenuItem>
              ) : null}
              {d.status === "Upcoming" ? <MenuItem onSelect={cancel}>Mark cancelled</MenuItem> : null}
            </MenuContent>
          </Menu>
        ) : null}
      </div>
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted">
        {d.durationMin ? <span>{d.durationMin} min</span> : null}
        {d.format ? <span>{d.format}</span> : null}
        {d.passMark !== null && d.passMark !== undefined ? <span>Pass mark {d.passMark}</span> : null}
        {exam && questions ? <span>Layout: {questions} questions</span> : null}
        {grade ? <span className="text-text">Grade {grade}</span> : null}
      </div>
      {d.checkInNote ? <p className="m-0 text-sm">{d.checkInNote}</p> : null}
      {d.offerResit && canAct ? (
        <Banner
          tone="notice"
          action={
            <Button size="sm" variant="primary" onClick={() => onOpen({ kind: "resit", d })}>
              Create resit
            </Button>
          }
        >
          Not a pass this time. Add the resit date and Crux plans around it.
        </Banner>
      ) : null}
    </li>
  );
}
