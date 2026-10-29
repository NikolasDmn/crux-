import { createFileRoute, Link } from "@tanstack/react-router";
import { Check, ChevronLeft, HelpCircle, MoreHorizontal, Pencil } from "lucide-react";
import { useState } from "react";
import { useUser } from "@/app/auth";
import { Markdown } from "@/components/markdown";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import { useConcepts, usePapers, useQuestion, useQuestionAction } from "@/features/bank/api";
import { useCourse } from "@/features/courses/api";
import { QuestionImage } from "@/features/files/question-image";
import { canEdit } from "@/features/courses/lib/roles";
import { useLectures } from "@/features/lectures/api";
import { QuizDialog } from "@/features/practice/components";
import { cn } from "@/lib/cn";
import { messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";

// One question, read-only (the default click everywhere): the prompt, then the answer (correct option
// with why the others tempt, numeric value, model answer and rubric) and the explanation, and where it
// comes from. A student sees the question first and asks for the answer ("Show answer"); whoever can
// edit it sees the answer at once. Owners and Editors (and a private question's author) get Edit.
export const Route = createFileRoute("/_app/c/$courseId/q/$questionId/")({ component: ViewQuestion });

const KEY_TONE = { Official: "success", Derived: "info", Disputed: "danger" } as const;

function ViewQuestion() {
  const { courseId, questionId } = Route.useParams();
  const user = useUser();
  const q = useQuestion(questionId);
  const course = useCourse(courseId);
  const lectures = useLectures(courseId);
  const concepts = useConcepts(courseId);
  const papers = usePapers(courseId);
  const act = useQuestionAction();
  const toast = useToast();
  const [quiz, setQuiz] = useState<{ componentId?: string; conceptId?: string; title: string } | null>(null);
  // null: the default for this viewer (shown to editors, hidden otherwise).
  const [reveal, setReveal] = useState<boolean | null>(null);

  if (q.isPending) return <Skeleton className="h-96" />;
  if (!q.data) return <EmptyList title="Not found, or you don't have access." />;
  const d = q.data;
  const v = d.current;
  const mayEdit = canEdit(course.data?.myRole) || d.private;
  const shown = reveal ?? mayEdit;
  const lecture = lectures.data?.find((l) => l.id === d.componentId);
  const paper = papers.data?.find((p) => p.id === d.paperId);
  const name = (id: string) => concepts.data?.find((c) => c.id === id)?.name ?? "…";
  const run = (a: "approve" | "archive" | "promote") =>
    act.mutate(
      { id: d.id, action: a },
      {
        onSuccess: () =>
          toast({ message: a === "approve" ? "Approved" : a === "archive" ? "Archived" : "Added to the course bank" }),
        onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
      },
    );

  return (
    <div className="flex max-w-[860px] flex-col gap-4">
      <Link
        to="/c/$courseId/bank"
        params={{ courseId }}
        className="inline-flex items-center gap-1 self-start text-sm text-muted hover:text-text"
      >
        <ChevronLeft className="size-4" aria-hidden="true" />
        Questions
      </Link>

      <div className="flex flex-wrap items-center gap-2">
        <Badge kind="status">{d.type}</Badge>
        {lecture ? (
          <Link
            to="/c/$courseId/l/$componentId"
            params={{ courseId, componentId: lecture.id }}
            className="text-sm text-muted hover:text-text"
          >
            {lecture.code} {lecture.name}
          </Link>
        ) : null}
        {d.reviewState === "Draft" ? (
          <Badge kind="status" tone="warning">
            Draft
          </Badge>
        ) : null}
        {d.private ? <Badge kind="status">Private</Badge> : null}
        {d.source === "Generated" ? (
          <Badge kind="status" tone="info">
            Generated
          </Badge>
        ) : null}
        {d.archivedAt ? (
          <Badge kind="status" tone="danger">
            Archived
          </Badge>
        ) : null}
        <span className="flex-1" />
        {lecture ? (
          <Button
            size="sm"
            onClick={() => setQuiz({ componentId: lecture.id, title: `Quiz: ${lecture.code} ${lecture.name}` })}
          >
            <HelpCircle className="size-4" aria-hidden="true" />
            Quiz me on {lecture.code}
          </Button>
        ) : null}
        {mayEdit ? (
          <>
            <Button asChild size="sm" variant="primary">
              <Link to="/c/$courseId/q/$questionId/edit" params={{ courseId, questionId }}>
                <Pencil className="size-4" aria-hidden="true" />
                Edit
              </Link>
            </Button>
            <Menu>
              <MenuTrigger asChild>
                <Button size="iconSm" variant="ghost" aria-label="More actions">
                  <MoreHorizontal className="size-4" />
                </Button>
              </MenuTrigger>
              <MenuContent align="end">
                {d.reviewState === "Draft" ? <MenuItem onSelect={() => run("approve")}>Approve</MenuItem> : null}
                {d.private ? <MenuItem onSelect={() => run("promote")}>Add to the course bank</MenuItem> : null}
                {!d.archivedAt ? <MenuItem onSelect={() => run("archive")}>Archive</MenuItem> : null}
              </MenuContent>
            </Menu>
          </>
        ) : null}
      </div>

      <Card className="flex flex-col gap-4">
        <Markdown className="text-md">{v.prompt}</Markdown>
        {v.imageFileId ? <QuestionImage fileId={v.imageFileId} /> : null}
        {v.options?.length ? (
          <ul className="m-0 flex list-none flex-col gap-2 p-0">
            {v.options.map((o, n) => (
              <li
                key={o.id}
                className={cn(
                  "flex gap-3 rounded-lg border px-3.5 py-3",
                  shown && o.isCorrect ? "border-success bg-success-bg/40" : "border-border",
                )}
              >
                <span className="inline-flex size-[26px] shrink-0 items-center justify-center rounded-[6px] bg-sunken font-mono text-sm">
                  {String.fromCharCode(65 + n)}
                </span>
                <div className="flex min-w-0 flex-1 flex-col gap-1 [&_.crux-prose>*]:my-0">
                  <Markdown>{o.text}</Markdown>
                  {shown && !o.isCorrect && o.distractorNote ? (
                    <span className="text-sm text-muted">{o.distractorNote}</span>
                  ) : null}
                </div>
                {shown && o.isCorrect ? (
                  <span className="flex items-center gap-1 text-sm font-medium text-success-text">
                    <Check className="size-4" aria-hidden="true" />
                    Correct
                  </span>
                ) : null}
              </li>
            ))}
          </ul>
        ) : null}
        <div>
          <Button size="sm" aria-expanded={shown} onClick={() => setReveal(!shown)}>
            {shown ? "Hide answer" : "Show answer"}
          </Button>
        </div>
        {shown && v.numeric ? (
          <div className="flex flex-col gap-1">
            <SectionLabel>Answer</SectionLabel>
            <p className="m-0">
              <span className="font-mono text-md tnum">{v.numeric.value}</span> {v.numeric.unit ?? ""}
              {v.numeric.tolerance ? (
                <span className="text-sm text-muted">
                  {" "}
                  (± {v.numeric.toleranceKind === "Relative" ? `${v.numeric.tolerance * 100}%` : v.numeric.tolerance}
                  {v.numeric.unitRequired ? ", unit required" : ""})
                </span>
              ) : null}
            </p>
            {v.numeric.modelAnswer ? <Markdown>{v.numeric.modelAnswer}</Markdown> : null}
          </div>
        ) : null}
        {shown && v.open ? (
          <div className="flex flex-col gap-2">
            {v.open.modelAnswer ? (
              <>
                <SectionLabel>Model answer</SectionLabel>
                <Markdown>{v.open.modelAnswer}</Markdown>
              </>
            ) : null}
            {Array.isArray(v.open.rubric) && v.open.rubric.length ? (
              <>
                <SectionLabel>Rubric</SectionLabel>
                <ul className="m-0 flex flex-col gap-1 pl-5 text-sm">
                  {(v.open.rubric as { criterion: string; points: number }[]).map((r, n) => (
                    <li key={n}>
                      {r.criterion} <span className="font-mono text-muted tnum">({r.points} pt)</span>
                    </li>
                  ))}
                </ul>
              </>
            ) : null}
            {v.open.rubricNote ? <p className="m-0 text-sm text-muted">{v.open.rubricNote}</p> : null}
          </div>
        ) : null}
        {shown && v.explanation ? (
          <div className="flex flex-col gap-1 border-t border-border pt-3">
            <SectionLabel>Explanation</SectionLabel>
            <Markdown>{v.explanation}</Markdown>
          </div>
        ) : null}
      </Card>

      <Card className="flex flex-col gap-3 text-sm">
        <SectionLabel>About this question</SectionLabel>
        <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-6 gap-y-1.5">
          <dt className="text-muted">Key</dt>
          <dd className="m-0 flex flex-wrap items-center gap-2">
            <Badge kind="status" tone={KEY_TONE[v.keyReliability as keyof typeof KEY_TONE] ?? "neutral"}>
              {v.keyReliability}
            </Badge>
            {v.keyNote ? <span>{v.keyNote}</span> : null}
          </dd>
          <dt className="text-muted">Source</dt>
          <dd className="m-0">
            {d.source}
            {d.sourceLabel ? ` · ${d.sourceLabel}` : ""}
          </dd>
          {paper ? (
            <>
              <dt className="text-muted">Past paper</dt>
              <dd className="m-0">
                {paper.label}
                {d.paperQuestionNo ? `, question ${d.paperQuestionNo}` : ""}
              </dd>
            </>
          ) : null}
          {d.concepts.length ? (
            <>
              <dt className="text-muted">Concepts</dt>
              <dd className="m-0 flex flex-wrap gap-1.5">
                {d.concepts.map((c) => (
                  <button
                    key={c.id}
                    type="button"
                    title={`Quiz me on ${name(c.id)}`}
                    onClick={() => setQuiz({ conceptId: c.id, title: `Quiz: ${name(c.id)}` })}
                    className={cn(
                      "inline-flex h-7 items-center gap-1 rounded-sm border px-2 hover:bg-sunken",
                      c.primary ? "border-text font-medium" : "border-border",
                    )}
                  >
                    {name(c.id)}
                    <HelpCircle className="size-3.5 text-muted" aria-hidden="true" />
                  </button>
                ))}
              </dd>
            </>
          ) : null}
          <dt className="text-muted">Version</dt>
          <dd className="m-0">
            {v.versionNo} · {formatInZone(v.createdAt, user.timeZone, "d MMM yyyy")}
            {v.changeNote ? ` · ${v.changeNote}` : ""}
          </dd>
        </dl>
      </Card>

      {quiz ? (
        <QuizDialog
          courseId={courseId}
          componentId={quiz.componentId}
          conceptId={quiz.conceptId}
          title={quiz.title}
          onClose={() => setQuiz(null)}
        />
      ) : null}
    </div>
  );
}
