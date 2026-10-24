import { cva, type VariantProps } from "class-variance-authority";
import type { HTMLAttributes } from "react";
import { cn } from "@/lib/cn";

// Picked: Square tags (frontend-spec 3.3). Status words uppercase; codes in mono, as written.
const badge = cva(
  "inline-flex h-[22px] items-center gap-1 whitespace-nowrap rounded-sm px-2 font-medium [&_svg]:size-3",
  {
    variants: {
      tone: {
        neutral: "bg-sunken text-text",
        info: "bg-info-bg text-info-text",
        warning: "bg-warning-bg text-warning-text",
        danger: "bg-danger-bg text-danger-text",
        success: "bg-success-bg text-success-text",
      },
      kind: {
        status: "text-[11px] uppercase tracking-[0.04em]",
        code: "font-mono text-xs",
        label: "text-xs",
      },
    },
    defaultVariants: { tone: "neutral", kind: "status" },
  },
);

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement>, VariantProps<typeof badge> {}

export function Badge({ className, tone, kind, ...p }: BadgeProps) {
  return <span className={cn(badge({ tone, kind }), className)} {...p} />;
}
