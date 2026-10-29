import { Tabs as R } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";

// Text tabs with a 2 px underline on the active one: --course inside a course, --text outside
// (frontend-spec 3.3).
export const Tabs = R.Root;

/** Radix makes a panel a tab stop, so clicking anywhere in it drew a focus ring round the whole
 *  panel. Ours always hold their own controls; the panel itself needn't take focus. */
export function TabPanel(p: ComponentProps<typeof R.Content>) {
  return <R.Content tabIndex={-1} {...p} />;
}

export function TabList({ className, ...p }: ComponentProps<typeof R.List>) {
  return (
    <R.List className={cn("flex gap-6 overflow-x-auto overflow-y-hidden border-b border-border", className)} {...p} />
  );
}

export function Tab({ className, ...p }: ComponentProps<typeof R.Trigger>) {
  return (
    <R.Trigger
      className={cn(
        "-mb-px whitespace-nowrap border-b-2 border-transparent py-2 text-muted data-[state=active]:border-[var(--tab-accent,var(--text))] data-[state=active]:font-medium data-[state=active]:text-text",
        className,
      )}
      {...p}
    />
  );
}
