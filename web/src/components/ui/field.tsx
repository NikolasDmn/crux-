import {
  forwardRef,
  useId,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
  type TextareaHTMLAttributes,
} from "react";
import { cn } from "@/lib/cn";

// Inputs (frontend-spec 3.3): 36 px, 1 px border; an error is a danger border + message, a warning
// (the breached password) uses the warning tokens.

const control =
  "w-full min-w-0 rounded-md border border-border bg-surface px-3 text-base text-text placeholder:text-muted disabled:opacity-60 aria-[invalid=true]:border-danger data-[warn=true]:border-warning";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(function Input(
  { className, ...p },
  ref,
) {
  return <input ref={ref} className={cn(control, "h-9", className)} {...p} />;
});

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(function Textarea(
  { className, ...p },
  ref,
) {
  return <textarea ref={ref} className={cn(control, "min-h-20 py-2", className)} {...p} />;
});

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(function Select(
  { className, ...p },
  ref,
) {
  return <select ref={ref} className={cn(control, "h-9 px-2.5", className)} {...p} />;
});

export interface FieldProps {
  label: ReactNode;
  /** Shown under the control in danger colours; marks the control invalid. */
  error?: string | undefined;
  /** Shown under the control in warning colours (not blocking). */
  warning?: ReactNode;
  hint?: ReactNode;
  className?: string;
  /** Receives the ids to wire the control to its label and messages. */
  children: (ids: {
    id: string;
    "aria-describedby"?: string;
    "aria-invalid"?: boolean;
    "data-warn"?: boolean;
  }) => ReactNode;
}

/** Label + control + one message line. `children` is a render function so the ids reach the control. */
export function Field({ label, error, warning, hint, className, children }: FieldProps) {
  const id = useId();
  const msgId = `${id}-msg`;
  const message = error ?? warning ?? hint;
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label htmlFor={id} className="text-sm font-medium">
        {label}
      </label>
      {children({
        id,
        ...(message ? { "aria-describedby": msgId } : {}),
        ...(error ? { "aria-invalid": true } : {}),
        ...(!error && warning ? { "data-warn": true } : {}),
      })}
      {message ? (
        <span
          id={msgId}
          className={cn("text-sm", error ? "text-danger-text" : warning ? "text-warning-text" : "text-muted")}
        >
          {message}
        </span>
      ) : null}
    </div>
  );
}
