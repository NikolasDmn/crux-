import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

// Banners (frontend-spec 3.3): a bordered row with a status dot, or a danger/warning tint.
export function Banner({
  tone = "notice",
  children,
  action,
  className,
}: {
  tone?: "notice" | "warning" | "danger";
  children: ReactNode;
  action?: ReactNode;
  className?: string;
}) {
  return (
    <div
      role={tone === "danger" ? "alert" : "status"}
      className={cn(
        "flex flex-wrap items-center gap-x-3 gap-y-2 rounded-md px-4 py-2.5",
        tone === "notice" && "border border-border bg-surface",
        tone === "warning" && "bg-warning-bg text-warning-text",
        tone === "danger" && "bg-danger-bg text-danger-text",
        className,
      )}
    >
      {tone === "notice" ? <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-warning" /> : null}
      <span className="min-w-0 flex-1">{children}</span>
      {action}
    </div>
  );
}
