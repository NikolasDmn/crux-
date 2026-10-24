import { Check } from "lucide-react";
import { COURSE_COLORS, readableOn } from "@/lib/color";
import { cn } from "@/lib/cn";

/** The eight course colours (frontend-spec 3.1); Owner only (5.2). A radio group. */
export function ColourPicker({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (hex: string) => void;
  disabled?: boolean;
}) {
  return (
    <div role="radiogroup" aria-label="Course colour" className="flex flex-wrap gap-2">
      {COURSE_COLORS.map((c) => {
        const on = value.toLowerCase() === c.hex;
        return (
          <button
            key={c.hex}
            type="button"
            role="radio"
            aria-checked={on}
            aria-label={c.name}
            disabled={disabled}
            onClick={() => onChange(c.hex)}
            className={cn(
              "inline-flex size-8 items-center justify-center rounded-full border-2 disabled:opacity-50",
              on ? "border-text" : "border-transparent",
            )}
            style={{ background: c.hex, color: readableOn(c.hex) }}
          >
            {on ? <Check className="size-4" strokeWidth={2.5} aria-hidden="true" /> : null}
          </button>
        );
      })}
    </div>
  );
}
