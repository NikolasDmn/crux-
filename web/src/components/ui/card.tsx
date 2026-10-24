import type { HTMLAttributes } from "react";
import { cn } from "@/lib/cn";

// Picked: Border (frontend-spec 3.3). No shadow on resting surfaces.
export function Card({ className, ...p }: HTMLAttributes<HTMLElement>) {
  return <section className={cn("rounded-lg border border-border bg-surface p-5", className)} {...p} />;
}

/** The 12 px uppercase muted label over a card or section. */
export function SectionLabel({ className, ...p }: HTMLAttributes<HTMLSpanElement>) {
  return <span className={cn("text-xs font-medium uppercase tracking-[0.06em] text-muted", className)} {...p} />;
}
