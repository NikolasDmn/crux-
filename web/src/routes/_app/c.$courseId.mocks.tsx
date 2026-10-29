import { useAi } from "@/features/ai/api";
import { reasonText } from "@/features/ai/lib/format";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { useState, type FormEvent } from "react";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Field, Input, Select } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { nextExam, useCourse, useDeadlines } from "@/features/courses/api";
import { useLectures } from "@/features/lectures/api";
import { useConfidence, useCreateMock, useMocks } from "@/features/mocks/api";
import { scoreText } from "@/features/mocks/lib/taking";
import { useSettings } from "@/features/plan/api";
import { fieldErrors, messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";

// Mocks (frontend-spec 4): past mocks, sure-vs-unsure per lecture, and the New mock builder (real
// exam layout, lectures, n, time limit, go easy, misses only, grading Self or AI with the gate open).
export const Route = createFileRoute("/_app/c/$courseId/mocks")({ component: Mocks });

const STATUS: Record<string, "info" | "warning" | "success" | "neutral"> = {
  Draft: "neutral",
  InProgress: "info",
  Submitted: "warning",
  Grading: "warning",
  Graded: "success",
  Reviewed: "success",
};

function Mocks() {
  const { courseId } = Route.useParams();
  const user = useUser();
  const course = useCourse(courseId);
  const subscribed = course.data?.mySubscription === "Active";
  const mocks = useMocks(courseId);
  const [open, setOpen] = useState(false);

  return (
    <div className="flex max-w-[960px] flex-col gap-4">
      {!subscribed && course.data ? (
        <Banner tone="notice">Subscribe to this course (on the Overview tab) to take mocks.</Banner>
      ) : null}
      <div className="flex items-center justify-between">
        <SectionLabel>Mocks</SectionLabel>
        {subscribed ? (
          <Button size="sm" variant="primary" onClick={() => setOpen(true)}>
            <Plus className="size-4" aria-hidden="true" />
            New mock
          </Button>
        ) : null}
      </div>
      {mocks.isPending ? (
        <Skeleton className="h-32" />
      ) : !mocks.data?.length ? (
        <EmptyList
          title="No mocks yet"
          body="A mock draws questions from the bank, timed like the exam. Misses become slips and weak spots."
        />
      ) : (
        <Card className="p-0">
          <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
            {[...mocks.data]
              .sort((a, b) => b.createdAt.localeCompare(a.createdAt))
              .map((m) => {
                const taking = m.status === "Draft" || m.status === "InProgress";
                return (
                  <li key={m.id} className="flex flex-wrap items-center gap-3 px-5 py-3">
                    <Link
                      to={taking ? "/mock/$mockId" : "/mock/$mockId/result"}
                      params={{ mockId: m.id }}
                      className="min-w-0 flex-1 truncate font-medium hover:underline"
                    >
                      {m.name}
                    </Link>
                    <span className="text-sm text-muted">
                      {m.questionCount} questions{m.timeLimitMin ? ` · ${m.timeLimitMin} min` : ""}
                    </span>
                    <span className="font-mono text-sm text-muted tnum">
                      {formatInZone(m.finishedAt ?? m.createdAt, user.timeZone, "d MMM")}
                    </span>
                    <span className="w-16 text-right font-mono tnum">{scoreText(m.score, m.maxScore)}</span>
                    <Badge kind="status" tone={STATUS[m.status] ?? "neutral"}>
                      {m.status === "InProgress" ? "In progress" : m.status}
                    </Badge>
                  </li>
                );
              })}
          </ul>
        </Card>
      )}
      <ConfidenceCard courseId={courseId} />
      {open ? <Builder courseId={courseId} onClose={() => setOpen(false)} /> : null}
    </div>
  );
}

function rate(b: { answered: number; rightRate?: number | null }): string {
  return b.answered ? `${Math.round((b.rightRate ?? 0) * 100)}% of ${b.answered}` : "–";
}

/** Spec 5.4: how often you're right when sure, unsure or guessing, per lecture. */
function ConfidenceCard({ courseId }: { courseId: string }) {
  const c = useConfidence(courseId);
  const rows = (c.data ?? []).filter((r) => r.sure.answered + r.unsure.answered + r.guess.answered > 0);
  if (!rows.length) return null;
  return (
    <Card className="flex flex-col gap-3 overflow-x-auto">
      <SectionLabel>Sure vs unsure</SectionLabel>
      <table className="w-full min-w-[520px] border-collapse text-sm">
        <thead>
          <tr className="text-left text-xs uppercase tracking-[0.06em] text-muted">
            <th className="py-1.5 font-medium">Lecture</th>
            <th className="py-1.5 font-medium">Sure</th>
            <th className="py-1.5 font-medium">Unsure</th>
            <th className="py-1.5 font-medium">Guess</th>
            <th className="py-1.5 font-medium" />
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.componentId} className="h-8 border-t border-border">
              <td className="font-mono text-xs">{r.code}</td>
              <td className="font-mono tnum">{rate(r.sure)}</td>
              <td className="font-mono tnum">{rate(r.unsure)}</td>
              <td className="font-mono tnum">{rate(r.guess)}</td>
              <td>
                {r.realGap ? (
                  <Badge kind="status" tone="warning">
                    Real gap
                  </Badge>
                ) : null}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="m-0 text-sm text-muted">
        A real gap: when unsure or guessing you're right less than half the time. Lucky guesses count as misses.
      </p>
    </Card>
  );
}

function Builder({ courseId, onClose }: { courseId: string; onClose: () => void }) {
  const navigate = useNavigate();
  const lectures = useLectures(courseId);
  const deadlines = useDeadlines(courseId);
  const settings = useSettings();
  const exam = nextExam(deadlines.data ?? []);
  const hasLayout = !!exam?.layout.length;
  const create = useCreateMock(courseId);
  const aiGate = useAi("Grading", courseId);
  const [f, setF] = useState(() => ({
    name: "",
    realLayout: hasLayout && (settings.data?.mockRealLayout ?? true),
    componentIds: [] as string[],
    n: "20",
    timeLimit: exam?.durationMin ? String(exam.durationMin) : "",
    goEasy: settings.data?.mockGoEasy ?? false,
    missesOnly: false,
    includeDrafts: false,
    aiGrading: false,
  }));
  const errors = fieldErrors(create.error);
  const general = create.error && !Object.keys(errors).length ? messageFor(create.error) : null;
  const layout = f.realLayout && hasLayout;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate(
      {
        name: f.name.trim() || null,
        deadlineId: exam?.id ?? null,
        realLayout: layout,
        componentIds: !layout && f.componentIds.length ? f.componentIds : null,
        n: layout ? null : Number(f.n) || 20,
        timeLimitMin: Number(f.timeLimit) || null,
        goEasy: f.goEasy,
        missesOnly: f.missesOnly,
        includeDrafts: f.includeDrafts,
        gradingMode: f.aiGrading && aiGate.allowed ? "AI" : "Self",
      },
      { onSuccess: (m) => void navigate({ to: "/mock/$mockId", params: { mockId: m.id } }) },
    );
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="New mock"
      description="Questions come from this course's bank. You can review every miss afterwards."
      className="w-[min(560px,calc(100vw-32px))]"
    >
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        {general ? <Banner tone="danger">{general}</Banner> : null}
        <Field label="Name" error={errors.name}>
          {(ids) => (
            <Input
              {...ids}
              placeholder={exam ? "Dress rehearsal" : "Practice mock"}
              value={f.name}
              onChange={(e) => setF({ ...f, name: e.currentTarget.value })}
            />
          )}
        </Field>
        {hasLayout ? (
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="size-4 accent-[var(--primary)]"
              checked={f.realLayout}
              onChange={(e) => setF({ ...f, realLayout: e.currentTarget.checked })}
            />
            Real exam layout ({exam!.layout.reduce((n, l) => n + (l.questionCount ?? 0), 0)} questions, per lecture as
            in the exam)
          </label>
        ) : null}
        {!layout ? (
          <>
            <div className="flex flex-col gap-1.5">
              <span className="text-sm font-medium">Lectures</span>
              <div className="flex flex-wrap gap-1.5">
                {(lectures.data ?? []).map((l) => {
                  const on = f.componentIds.includes(l.id);
                  return (
                    <button
                      key={l.id}
                      type="button"
                      aria-pressed={on}
                      className={
                        on
                          ? "h-8 rounded-md border border-text bg-sunken px-2.5 text-sm font-medium"
                          : "h-8 rounded-md border border-border px-2.5 text-sm text-muted"
                      }
                      onClick={() =>
                        setF({
                          ...f,
                          componentIds: on ? f.componentIds.filter((x) => x !== l.id) : [...f.componentIds, l.id],
                        })
                      }
                    >
                      {l.code}
                    </button>
                  );
                })}
              </div>
              <span className="text-xs text-muted">None picked: all lectures.</span>
            </div>
            <Field label="Questions" error={errors.n}>
              {(ids) => (
                <Select {...ids} value={f.n} onChange={(e) => setF({ ...f, n: e.currentTarget.value })}>
                  {["5", "10", "20", "30", "40"].map((n) => (
                    <option key={n}>{n}</option>
                  ))}
                </Select>
              )}
            </Field>
          </>
        ) : null}
        <Field label="Time limit (minutes)" hint="Empty: untimed." error={errors.timeLimitMin}>
          {(ids) => (
            <Input
              {...ids}
              inputMode="numeric"
              className="font-mono tnum"
              value={f.timeLimit}
              onChange={(e) => setF({ ...f, timeLimit: e.currentTarget.value.replace(/\D/g, "") })}
            />
          )}
        </Field>
        <div className="flex flex-col gap-2 text-sm">
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              className="size-4 accent-[var(--primary)]"
              checked={f.goEasy}
              onChange={(e) => setF({ ...f, goEasy: e.currentTarget.checked })}
            />
            Go easy (more questions you've got right before)
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              className="size-4 accent-[var(--primary)]"
              checked={f.missesOnly}
              onChange={(e) => setF({ ...f, missesOnly: e.currentTarget.checked })}
            />
            Only questions I've missed
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              className="size-4 accent-[var(--primary)]"
              checked={f.includeDrafts}
              onChange={(e) => setF({ ...f, includeDrafts: e.currentTarget.checked })}
            />
            Include my draft questions
          </label>
        </div>
        {aiGate.visible ? (
          <label className="flex items-start gap-2">
            <input
              type="checkbox"
              className="mt-1 size-4 accent-[var(--primary)]"
              disabled={!aiGate.allowed}
              checked={f.aiGrading && aiGate.allowed}
              onChange={(e) => setF({ ...f, aiGrading: e.currentTarget.checked })}
            />
            <span className="flex flex-col gap-0.5">
              <span>Grade open answers with AI</span>
              <span className="text-sm text-muted">
                {aiGate.allowed
                  ? "AI suggests a grade and feedback; you accept or change each one."
                  : (reasonText(aiGate.reason, courseId)?.text ?? "AI grading isn't available here.")}
              </span>
            </span>
          </label>
        ) : (
          <p className="m-0 text-sm text-muted">Grading: you mark open answers yourself against the model answer.</p>
        )}
        <div className="flex justify-end gap-2">
          <Button onClick={onClose}>Cancel</Button>
          <Button type="submit" variant="primary" disabled={create.isPending}>
            Build mock
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
