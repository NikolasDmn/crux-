import type { ReactNode } from "react";
import { cn } from "@/lib/cn";

// Picked: Row with key (frontend-spec 3.3). The key square shows the shortcut digit; selected = 2 px
// text border and an ink key. Use inside a `role="radiogroup"`, or a `role="group"` with `multi`
// ("choose all that apply": each row is a checkbox).
export function OptionRow({
  index,
  selected,
  onSelect,
  children,
  disabled,
  state,
  multi = false,
}: {
  index: number;
  selected: boolean;
  onSelect: () => void;
  children: ReactNode;
  disabled?: boolean;
  /** After grading: shown with a word and icon, never colour alone (spec 2.7). */
  state?: "right" | "wrong" | undefined;
  multi?: boolean;
}) {
  return (
    <button
      type="button"
      role={multi ? "checkbox" : "radio"}
      aria-checked={selected}
      disabled={disabled}
      onClick={onSelect}
      className={cn(
        "flex min-h-12 w-full items-center gap-3 rounded-lg border bg-surface px-3.5 py-3 text-left text-md",
        selected ? "border-2 border-text px-[13px] py-[11px]" : "border-border",
        state === "right" && "border-success",
        state === "wrong" && "border-danger",
      )}
    >
      <span
        className={cn(
          "inline-flex size-[26px] shrink-0 items-center justify-center rounded-[6px] font-mono text-sm",
          selected ? "bg-primary text-primary-text" : "bg-sunken",
        )}
        aria-hidden="true"
      >
        {index + 1}
      </span>
      <span className="min-w-0 flex-1 [&_.crux-prose>*]:my-0">{children}</span>
      {state ? (
        <span className={cn("text-sm font-medium", state === "right" ? "text-success-text" : "text-danger-text")}>
          {state === "right" ? "Right" : "Wrong"}
        </span>
      ) : null}
    </button>
  );
}
