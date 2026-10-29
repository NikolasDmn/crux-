import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { subDays } from "date-fns";
import { Plus } from "lucide-react";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { useQuestions, type Question } from "@/features/bank/api";
import { useSlips } from "@/features/mistakes/api";
import { SlipList } from "@/features/mistakes/components/slip-list";
import { api, unwrap } from "@/lib/api";
import { formatInZone } from "@/lib/time";
import { PromptLine } from "@/features/bank/components/prompt-line";

// A lecture's Questions, Slips and Sessions tabs (frontend-spec 4).

export function LectureQuestions({ courseId, componentId }: { courseId: string; componentId: string }) {
  const list = useQuestions(courseId, { componentId });
  const rows = (list.data?.pages ?? []).flatMap((p) => p.items as Question[]);
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-sm text-muted">
          {rows.length ? `${rows.length}${list.hasNextPage ? "+" : ""} questions` : ""}
        </span>
        <Button asChild size="sm" variant="primary">
          <Link to="/c/$courseId/bank/new" params={{ courseId }} search={{ lecture: componentId }}>
            <Plus className="size-4" aria-hidden="true" />
            New question
          </Link>
        </Button>
      </div>
      {list.isPending ? (
        <Skeleton className="h-32" />
      ) : !rows.length ? (
        <EmptyList title="No questions for this lecture yet" />
      ) : (
        <Card className="p-0">
          <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
            {rows.map((q) => (
              <li key={q.id} className="flex items-center gap-3 px-5 py-2.5 text-sm">
                <span className="w-16 text-muted">{q.type}</span>
                <Link
                  to="/c/$courseId/q/$questionId"
                  params={{ courseId, questionId: q.id }}
                  className="min-w-0 flex-1 truncate hover:underline"
                >
                  <PromptLine md={q.current.prompt} empty="(no prompt)" />
                </Link>
                {q.reviewState === "Draft" ? (
                  <Badge kind="status" tone="warning">
                    Draft
                  </Badge>
                ) : null}
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}

export function LectureSlips({ courseId, componentId }: { courseId: string; componentId: string }) {
  const open = useSlips({ courseId, status: "open" });
  const slips = open.data?.filter((s) => s.componentId === componentId);
  return <SlipList slips={slips} loading={open.isPending} empty="No open slips on this lecture." />;
}

/** The last 30 days of study sessions for this lecture: timer, manual and derived from activity. */
export function LectureSessions({ courseId, componentId }: { courseId: string; componentId: string }) {
  const user = useUser();
  const from = subDays(new Date(), 30).toISOString();
  const sessions = useQuery({
    queryKey: ["sessions", courseId, from.slice(0, 10)],
    queryFn: () => unwrap(api.GET("/sessions", { params: { query: { courseId, from } } })),
  });
  const rows = (sessions.data ?? []).filter((s) => s.componentId === componentId);
  if (sessions.isPending) return <Skeleton className="h-32" />;
  if (!rows.length) return <EmptyList title="No study time on this lecture in the last 30 days" />;
  return (
    <Card className="p-0">
      <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
        {rows.map((s, i) => (
          <li key={s.id ?? i} className="flex items-center gap-3 px-5 py-2.5 text-sm">
            <span className="w-36 font-mono text-muted tnum">
              {formatInZone(s.startedAt, user.timeZone, "EEE d MMM HH:mm")}
            </span>
            <span className="min-w-0 flex-1">{s.kind}</span>
            <span className="text-muted">{s.source}</span>
            <span className="w-16 text-right font-mono tnum">{s.minutes} min</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}
