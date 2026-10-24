import { Pin } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

// Picked: Colour edge (frontend-spec 3.3): a 3 px --course left edge, 1 px border elsewhere.
export function PlanBlock({
  time,
  title,
  pinned,
  color,
  status,
  className,
}: {
  time: string;
  title: ReactNode;
  pinned?: boolean;
  /** The entry's course colour; defaults to the current --course. */
  color?: string;
  status?: "Planned" | "Done" | "Skipped" | "Moved";
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col gap-0.5 rounded-sm border border-l-[3px] border-border bg-surface px-2.5 py-2",
        status === "Skipped" && "opacity-60",
        status === "Done" && "text-muted",
        className,
      )}
      style={{ borderLeftColor: color ?? "var(--course)" }}
    >
      <div className="flex items-center justify-between gap-2 font-mono text-xs text-muted tnum">
        <span>{time}</span>
        {pinned ? <Pin className="size-3" aria-label="Pinned" strokeWidth={1.75} /> : null}
      </div>
      <div className={cn("font-medium", status === "Done" && "line-through decoration-1")}>{title}</div>
    </div>
  );
}
