import { Check, RotateCcw, TrendingDown, Zap, type LucideIcon } from "lucide-react";
import type { Rating } from "@/lib/keyboard";
import { cn } from "@/lib/cn";

// Picked: Four with intervals, plus Nick's note: an icon and a soft semantic tint per rating so the
// feeling reads at a glance (frontend-spec 3.3). Same shape and weight for all four; keys 1–4.
const LOOK: Record<Rating, { icon: LucideIcon; tone: string }> = {
  Again: { icon: RotateCcw, tone: "bg-danger-bg text-danger-text" },
  Hard: { icon: TrendingDown, tone: "bg-warning-bg text-warning-text" },
  Good: { icon: Check, tone: "bg-success-bg text-success-text" },
  Easy: { icon: Zap, tone: "bg-info-bg text-info-text" },
};

export const RATING_ORDER: readonly Rating[] = ["Again", "Hard", "Good", "Easy"];

export function RatingButtons({
  intervals,
  onRate,
  disabled,
}: {
  /** The next interval per rating, e.g. `{Again: "1 min", Good: "3 d"}`. */
  intervals: Partial<Record<Rating, string>>;
  onRate: (r: Rating) => void;
  disabled?: boolean;
}) {
  return (
    <div className="grid grid-cols-4 gap-2" role="group" aria-label="Rate this card">
      {RATING_ORDER.map((r, i) => {
        const { icon: Icon, tone } = LOOK[r];
        return (
          <button
            key={r}
            type="button"
            disabled={disabled}
            onClick={() => onRate(r)}
            aria-keyshortcuts={String(i + 1)}
            className={cn(
              "flex h-16 flex-col items-center justify-center gap-0.5 rounded-lg font-medium disabled:opacity-50",
              tone,
            )}
          >
            <span className="flex items-center gap-1.5 text-base">
              <Icon className="size-4" aria-hidden="true" strokeWidth={1.75} />
              {r}
            </span>
            <span className="font-mono text-xs opacity-80 tnum">{intervals[r] ?? ""}</span>
          </button>
        );
      })}
    </div>
  );
}
