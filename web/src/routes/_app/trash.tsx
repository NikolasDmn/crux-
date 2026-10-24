import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useUser } from "@/app/auth";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Dot, Skeleton } from "@/components/ui/misc";
import { useToast } from "@/components/ui/toast";
import { useRestoreCourse, useTrash } from "@/features/courses/api";
import { messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";

// Trash (frontend-spec 0.1): trashed courses, days left of 30, Restore.
export const Route = createFileRoute("/_app/trash")({ component: Trash });

function daysLeft(purgeAt: string | null | undefined, now = Date.now()): number | null {
  if (!purgeAt) return null;
  // Purge runs on the hour after day 30: never show more than the 30 days the Trash promises.
  return Math.min(30, Math.max(0, Math.ceil((new Date(purgeAt).getTime() - now) / 86_400_000)));
}

function Trash() {
  const user = useUser();
  const trash = useTrash();
  const restore = useRestoreCourse();
  const toast = useToast();
  const navigate = useNavigate();
  return (
    <div className="flex max-w-[760px] flex-col gap-4">
      <h1 className="m-0 text-xl font-semibold tracking-[-0.01em]">Trash</h1>
      <p className="m-0 text-muted">
        Deleted courses stay here for 30 days, hidden from everyone they were shared with. Restore one to bring it back
        as it was.
      </p>
      {trash.isPending ? (
        <Skeleton className="h-32" />
      ) : !trash.data?.length ? (
        <EmptyList title="The Trash is empty" body="Courses you delete wait here for 30 days." />
      ) : (
        <Card className="p-0">
          <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
            {trash.data.map((c) => {
              const left = daysLeft(c.purgeAt);
              return (
                <li key={c.id} className="flex flex-wrap items-center gap-3 px-5 py-3">
                  <Dot color={c.color} />
                  <span className="font-medium">{c.code}</span>
                  <span className="min-w-0 flex-1 truncate text-muted">{c.name}</span>
                  <span className="text-sm text-muted">
                    {c.trashedAt ? `Deleted ${formatInZone(c.trashedAt, user.timeZone, "d MMM")}` : ""}
                    {left !== null ? ` · ${left} ${left === 1 ? "day" : "days"} left` : ""}
                  </span>
                  <Button
                    size="sm"
                    disabled={restore.isPending}
                    onClick={() =>
                      restore.mutate(c.id, {
                        onSuccess: () => {
                          toast({ message: `${c.code} restored` });
                          void navigate({ to: "/c/$courseId", params: { courseId: c.id } });
                        },
                        onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
                      })
                    }
                  >
                    Restore
                  </Button>
                </li>
              );
            })}
          </ul>
        </Card>
      )}
    </div>
  );
}
