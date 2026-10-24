import { createFileRoute, Link, Navigate } from "@tanstack/react-router";
import { ChevronLeft } from "lucide-react";
import { useState } from "react";
import { useUser } from "@/app/auth";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { useQuestion, useVersions } from "@/features/bank/api";
import { useCourse } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import { QuestionEditor } from "@/features/bank/components/question-editor";
import { diffLines, versionText } from "@/features/bank/lib/editor";
import { cn } from "@/lib/cn";
import { formatInZone } from "@/lib/time";

// Editing a question (Owners and Editors; a private question's author): the editor, then its
// version history with a line diff against the version before. Everyone else gets the read-only view.
export const Route = createFileRoute("/_app/c/$courseId/q/$questionId/edit")({ component: EditQuestion });

function EditQuestion() {
  const { courseId, questionId } = Route.useParams();
  const q = useQuestion(questionId);
  const course = useCourse(courseId);
  if (q.data && course.data && !canEdit(course.data.myRole) && !q.data.private) {
    return <Navigate to="/c/$courseId/q/$questionId" params={{ courseId, questionId }} replace />;
  }
  return (
    <div className="flex max-w-[960px] flex-col gap-4">
      <Link
        to="/c/$courseId/q/$questionId"
        params={{ courseId, questionId }}
        className="inline-flex items-center gap-1 self-start text-sm text-muted hover:text-text"
      >
        <ChevronLeft className="size-4" aria-hidden="true" />
        Back to the question
      </Link>
      {q.isPending ? (
        <Skeleton className="h-96" />
      ) : !q.data ? (
        <EmptyList title="Not found, or you don't have access." />
      ) : (
        <>
          {/* Keyed by version: a save (or someone else's) reloads the form from the new version. */}
          <QuestionEditor key={q.data.version} courseId={courseId} question={q.data} />
          <History questionId={questionId} />
        </>
      )}
    </div>
  );
}

function History({ questionId }: { questionId: string }) {
  const user = useUser();
  const versions = useVersions(questionId);
  const list = [...(versions.data ?? [])].sort((a, b) => b.versionNo - a.versionNo);
  const [open, setOpen] = useState<number | null>(null);
  if (list.length < 2) return null;
  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>Version history</SectionLabel>
      <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
        {list.map((v, i) => {
          const before = list[i + 1];
          return (
            <li key={v.id} className="py-2">
              <button
                type="button"
                aria-expanded={open === v.versionNo}
                className="flex w-full items-center gap-3 text-left text-sm"
                onClick={() => setOpen(open === v.versionNo ? null : v.versionNo)}
              >
                <span className="font-mono tnum">v{v.versionNo}</span>
                <span className="text-muted">{formatInZone(v.createdAt, user.timeZone, "d MMM yyyy, HH:mm")}</span>
                <span className="min-w-0 flex-1 truncate">{v.changeNote ?? ""}</span>
              </button>
              {open === v.versionNo ? (
                <pre className="m-0 mt-2 overflow-x-auto rounded-md bg-sunken p-3 font-mono text-xs">
                  {(before
                    ? diffLines(versionText(before), versionText(v))
                    : versionText(v).map((text) => ({ op: "same" as const, text }))
                  ).map((d, n) => (
                    <div
                      key={n}
                      className={cn(
                        d.op === "add" && "bg-success-bg text-success-text",
                        d.op === "del" && "bg-danger-bg text-danger-text line-through",
                      )}
                    >
                      {d.op === "add" ? "+ " : d.op === "del" ? "− " : "  "}
                      {d.text}
                    </div>
                  ))}
                </pre>
              ) : null}
            </li>
          );
        })}
      </ul>
    </Card>
  );
}
