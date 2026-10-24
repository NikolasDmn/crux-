import { ToggleGroup } from "radix-ui";
import { cn } from "@/lib/cn";

// Picked: Segmented (frontend-spec 3.3), e.g. Sure / Unsure / Guess. One bordered group; the
// selected segment on --surface-sunken (`strong` makes it ink, for the phone mock screen).
export function Segmented<T extends string>({
  value,
  onChange,
  options,
  label,
  size = "md",
  strong = false,
  className,
}: {
  value: T | null;
  onChange: (v: T) => void;
  options: readonly { value: T; label: string }[];
  label: string;
  size?: "sm" | "md" | "lg";
  strong?: boolean;
  className?: string;
}) {
  return (
    <ToggleGroup.Root
      type="single"
      aria-label={label}
      value={value ?? ""}
      onValueChange={(v) => v && onChange(v as T)}
      className={cn("inline-flex overflow-hidden rounded-md border border-border bg-surface", className)}
    >
      {options.map((o) => (
        <ToggleGroup.Item
          key={o.value}
          value={o.value}
          className={cn(
            "flex-1 border-l border-border font-medium first:border-l-0",
            size === "sm" && "h-8 px-3 text-sm",
            size === "md" && "h-9 px-4 text-base",
            size === "lg" && "h-11 px-4 text-md",
            strong ? "data-[state=on]:bg-primary data-[state=on]:text-primary-text" : "data-[state=on]:bg-sunken",
          )}
        >
          {o.label}
        </ToggleGroup.Item>
      ))}
    </ToggleGroup.Root>
  );
}
