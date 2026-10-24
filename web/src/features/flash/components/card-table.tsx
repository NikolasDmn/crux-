import { MoreHorizontal, Plus } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card as Panel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/misc";
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import { useCourse } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import { plainLine } from "@/features/bank/lib/filters";
import { useCardFlags, useCards, type Card } from "@/features/flash/api";
import { CardDialog } from "@/features/flash/components/card-dialog";
import { useLectures } from "@/features/lectures/api";
import { messageFor } from "@/lib/errors";

/** A course's (or one lecture's) cards: code, front, source, lapses; edit, suspend, warm-up flag. */
export function CardTable({ courseId, componentId }: { courseId: string; componentId?: string }) {
  const course = useCourse(courseId);
  const editor = canEdit(course.data?.myRole);
  const cards = useCards(courseId, componentId);
  const lectures = useLectures(courseId);
  const code = new Map((lectures.data ?? []).map((l) => [l.id, l.code]));
  const { suspend, warmup } = useCardFlags();
  const toast = useToast();
  const [open, setOpen] = useState<Card | "new" | null>(null);
  const fail = (e: unknown) => toast({ message: messageFor(e), tone: "danger" });
  const list = [...(cards.data ?? [])].sort((a, b) => a.code.localeCompare(b.code, undefined, { numeric: true }));

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <span className="text-sm text-muted">{list.length ? `${list.length} cards` : ""}</span>
        <Button size="sm" variant="primary" onClick={() => setOpen("new")}>
          <Plus className="size-4" aria-hidden="true" />
          New card
        </Button>
      </div>
      {cards.isPending ? (
        <Skeleton className="h-32" />
      ) : !list.length ? (
        <EmptyList
          title="No flashcards yet"
          body="Make cards for formulas, definitions and the things you keep forgetting."
        />
      ) : (
        <Panel className="overflow-x-auto p-0">
          <table className="w-full min-w-[640px] border-collapse text-sm">
            <thead>
              <tr className="text-left text-xs uppercase tracking-[0.06em] text-muted">
                <th className="w-24 py-2 pl-4 font-medium">Code</th>
                {!componentId ? <th className="w-20 py-2 pr-3 font-medium">Lecture</th> : null}
                <th className="py-2 font-medium">Front</th>
                <th className="w-20 py-2 font-medium">Lapses</th>
                <th className="w-48 py-2 font-medium" />
                <th className="w-10 py-2 pr-2" />
              </tr>
            </thead>
            <tbody>
              {list.map((c) => {
                const mine = c.personal || editor;
                return (
                  <tr key={c.id} className="h-8 border-t border-border">
                    <td className="pl-4 font-mono text-xs">{c.code}</td>
                    {!componentId ? (
                      <td className="font-mono text-xs text-muted">{code.get(c.componentId) ?? ""}</td>
                    ) : null}
                    <td className="max-w-0 truncate pr-3">
                      <button
                        type="button"
                        className="max-w-full truncate text-left hover:underline"
                        onClick={() => setOpen(c)}
                      >
                        {plainLine(c.front) || "(empty)"}
                      </button>
                    </td>
                    <td className="font-mono tnum">{c.progress.lapses}</td>
                    <td>
                      <span className="flex flex-wrap gap-1">
                        {c.personal ? <Badge kind="status">Personal</Badge> : null}
                        {c.source === "Slip" ? (
                          <Badge kind="status" tone="warning">
                            From a slip
                          </Badge>
                        ) : null}
                        {c.progress.suspended ? (
                          <Badge kind="status" tone="neutral">
                            Suspended
                          </Badge>
                        ) : null}
                        {c.progress.warmupFlagged ? (
                          <Badge kind="status" tone="info">
                            Warm-up
                          </Badge>
                        ) : null}
                      </span>
                    </td>
                    <td className="pr-2">
                      <Menu>
                        <MenuTrigger asChild>
                          <Button size="iconSm" variant="ghost" aria-label={`Actions for ${c.code}`}>
                            <MoreHorizontal className="size-4" />
                          </Button>
                        </MenuTrigger>
                        <MenuContent align="end">
                          {mine ? <MenuItem onSelect={() => setOpen(c)}>Edit</MenuItem> : null}
                          <MenuItem
                            onSelect={() =>
                              suspend.mutate({ id: c.id, suspended: !c.progress.suspended }, { onError: fail })
                            }
                          >
                            {c.progress.suspended ? "Resume" : "Suspend"}
                          </MenuItem>
                          <MenuItem
                            onSelect={() =>
                              warmup.mutate({ id: c.id, flagged: !c.progress.warmupFlagged }, { onError: fail })
                            }
                          >
                            {c.progress.warmupFlagged ? "Take out of the warm-up" : "Include in my next warm-up"}
                          </MenuItem>
                        </MenuContent>
                      </Menu>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </Panel>
      )}
      {open ? (
        <CardDialog
          courseId={courseId}
          card={open === "new" ? undefined : open}
          draft={open === "new" ? { componentId } : undefined}
          onClose={() => setOpen(null)}
        />
      ) : null}
    </div>
  );
}
