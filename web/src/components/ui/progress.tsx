import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

// Picked: Thin bar (frontend-spec 3.3): 6 px track, --course fill, a label row above.
export function ProgressBar({
  value,
  max = 1,
  label,
  detail,
  className,
  tone = "course",
}: {
  value: number;
  max?: number;
  label?: ReactNode;
  detail?: ReactNode;
  className?: string;
  tone?: "course" | "text";
}) {
  const pct = max > 0 ? Math.max(0, Math.min(100, (value / max) * 100)) : 0;
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {label || detail ? (
        <div className="flex justify-between gap-3 text-sm text-muted">
          <span>{label}</span>
          <span className="font-mono tnum">{detail}</span>
        </div>
      ) : null}
      <div
        className="h-1.5 overflow-hidden rounded-full bg-sunken"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-label={typeof label === "string" ? label : undefined}
      >
        <div
          className={cn("h-full rounded-full", tone === "course" ? "bg-course" : "bg-text")}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
