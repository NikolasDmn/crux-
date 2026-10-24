import { Link } from "@tanstack/react-router";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { useToast } from "@/components/ui/toast";
import { usePatchSlip, type Slip } from "@/features/mistakes/api";
import { messageFor } from "@/lib/errors";
import { relativeDays } from "@/lib/time";

export function slipLabel(type: string): string {
  return type === "OptionMatching" ? "Option matching" : type;
}

/** Slips with resolve/reopen; `codes` maps course ids to codes when the list spans courses. */
export function SlipList({
  slips,
  loading,
  codes,
  empty = "No slips. Misses you log from mocks and practice land here.",
}: {
  slips: Slip[] | undefined;
  loading: boolean;
  codes?: Record<string, string>;
  empty?: string;
}) {
  const user = useUser();
  const patch = usePatchSlip();
  const toast = useToast();
  if (loading) return <Skeleton className="h-32" />;
  if (!slips?.length) return <EmptyList title={empty} />;
  return (
    <Card className="p-0">
      <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
        {slips.map((s) => (
          <li key={s.id} className="flex flex-wrap items-center gap-3 px-5 py-3">
            {codes ? <span className="font-mono text-xs font-medium">{codes[s.courseId] ?? ""}</span> : null}
            <Badge kind="status">{slipLabel(s.type)}</Badge>
            <span className={s.resolvedAt ? "min-w-0 flex-1 text-muted line-through" : "min-w-0 flex-1"}>
              {s.description}
            </span>
            {s.mockId ? (
              <Link
                to="/mock/$mockId/result"
                params={{ mockId: s.mockId }}
                className="text-sm text-muted hover:text-text"
              >
                Mock Q{s.mockQuestionOrder}
              </Link>
            ) : null}
            <span className="text-sm text-muted">{relativeDays(s.occurredAt, user.timeZone)}</span>
            <Button
              size="sm"
              variant="ghost"
              disabled={patch.isPending}
              onClick={() =>
                patch.mutate(
                  { id: s.id, body: { resolved: !s.resolvedAt } },
                  { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) },
                )
              }
            >
              {s.resolvedAt ? "Reopen" : "Resolved"}
            </Button>
          </li>
        ))}
      </ul>
    </Card>
  );
}
