import { cn } from "@/lib/cn";

export type Severity = "Low" | "Medium" | "High";
const WIDTH: Record<Severity, string> = { Low: "33%", Medium: "66%", High: "100%" };
const FILL: Record<Severity, string> = { Low: "bg-muted", Medium: "bg-warning", High: "bg-danger" };

// Picked: Bar + word (frontend-spec 3.3). The word carries the meaning; the bar is a glance cue.
export function SeverityBar({ topic, severity, className }: { topic: string; severity: Severity; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-1", className)}>
      <div className="flex justify-between gap-3 text-sm">
        <span className="min-w-0 truncate">{topic}</span>
        <span className="text-muted">{severity}</span>
      </div>
      <div className="h-1 overflow-hidden rounded-full bg-sunken" aria-hidden="true">
        <div className={cn("h-full rounded-full", FILL[severity])} style={{ width: WIDTH[severity] }} />
      </div>
    </div>
  );
}
