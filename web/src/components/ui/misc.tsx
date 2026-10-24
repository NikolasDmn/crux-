import type { HTMLAttributes } from "react";
import { cn } from "@/lib/cn";

/** The Crux mark; the dot takes the current course colour (frontend-spec 0, 5.2). */
export function Mark({ size = 24, className }: { size?: number; className?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true" className={className}>
      <line x1="14" y1="46" x2="50" y2="14" stroke="var(--text)" strokeWidth="8" strokeLinecap="round" />
      <line x1="16" y1="20" x2="44" y2="48" stroke="var(--text)" strokeWidth="8" strokeLinecap="round" />
      <circle cx="28.8" cy="32.8" r="9" fill="var(--bg)" />
      <circle cx="28.8" cy="32.8" r="5.5" fill="var(--course)" />
    </svg>
  );
}

/** Loading uses skeletons, not spinners, for page content (spec 5.7). */
export function Skeleton({ className, ...p }: HTMLAttributes<HTMLDivElement>) {
  return <div aria-hidden="true" className={cn("animate-pulse rounded-md bg-sunken", className)} {...p} />;
}

export function Kbd({ className, ...p }: HTMLAttributes<HTMLElement>) {
  return <kbd className={cn("rounded-sm border border-border px-1 font-mono text-xs text-muted", className)} {...p} />;
}

/** The 8 px course dot used in lists. */
export function Dot({ color, className }: { color?: string | undefined; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn("inline-block size-2 shrink-0 rounded-full", className)}
      style={{ background: color ?? "var(--course)" }}
    />
  );
}
