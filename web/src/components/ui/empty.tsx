import { Check } from "lucide-react";
import { useState, type DragEvent, type ReactNode } from "react";
import { cn } from "@/lib/cn";

// Empty states (frontend-spec 3.3): three patterns, each for its own place.

/** An empty list: one line on what goes there, one action. */
export function EmptyList({ title, body, action }: { title: string; body?: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-start gap-2 py-6">
      <strong className="font-semibold">{title}</strong>
      {body ? <span className="text-sm text-muted">{body}</span> : null}
      {action}
    </div>
  );
}

/** Wherever files can be added: a dashed box that takes a drop or opens the picker. */
export function DropBox({
  title,
  hint,
  accept,
  multiple,
  onFiles,
}: {
  title: string;
  hint?: string;
  accept?: string;
  multiple?: boolean;
  onFiles: (files: File[]) => void;
}) {
  const [over, setOver] = useState(false);
  const drop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    const files = [...e.dataTransfer.files];
    if (files.length) onFiles(multiple ? files : files.slice(0, 1));
  };
  return (
    <label
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={drop}
      className={cn(
        "flex cursor-pointer flex-col items-center justify-center gap-1.5 rounded-lg border-[1.5px] border-dashed border-border px-6 py-8 text-center focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-focus",
        over && "border-text bg-sunken",
      )}
    >
      <strong className="font-semibold">{title}</strong>
      {hint ? <span className="text-sm text-muted">{hint}</span> : null}
      <input
        type="file"
        className="sr-only"
        accept={accept}
        multiple={multiple}
        onChange={(e) => {
          const files = [...(e.currentTarget.files ?? [])];
          if (files.length) onFiles(files);
          e.currentTarget.value = "";
        }}
      />
    </label>
  );
}

export interface ChecklistStep {
  label: string;
  done: boolean;
  /** Where the step is done (a link or button); shown while not done. */
  action?: ReactNode;
}

/** First run on Home: the steps tick themselves as they get done. */
export function Checklist({ steps }: { steps: ChecklistStep[] }) {
  return (
    <ol className="flex flex-col gap-1">
      {steps.map((s) => (
        <li key={s.label} className="flex min-h-10 items-center gap-3">
          <span
            className={cn(
              "inline-flex size-5 shrink-0 items-center justify-center rounded-full border",
              s.done ? "border-primary bg-primary text-primary-text" : "border-border",
            )}
            aria-hidden="true"
          >
            {s.done ? <Check className="size-3" strokeWidth={2.5} /> : null}
          </span>
          <span className={cn("flex-1", s.done && "text-muted line-through decoration-1")}>
            {s.label}
            <span className="sr-only">{s.done ? " (done)" : ""}</span>
          </span>
          {!s.done ? s.action : null}
        </li>
      ))}
    </ol>
  );
}
