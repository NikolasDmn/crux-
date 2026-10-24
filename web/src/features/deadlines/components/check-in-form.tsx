import { useState, type FormEvent } from "react";
import { useUser } from "@/app/auth";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Field, Input, Select, Textarea } from "@/components/ui/field";
import { Segmented } from "@/components/ui/segmented";
import { useToast } from "@/components/ui/toast";
import { useCourse, useGradeScales, type Deadline } from "@/features/courses/api";
import { usePatchDeadline } from "@/features/deadlines/api";
import { deadlineName, gradeBody, gradeControl, wouldPass } from "@/features/deadlines/lib/grades";
import { fieldErrors, messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";

type Outcome = "Done" | "Missed" | "Cancelled";

// "How did it go?" (spec 5.10): a grade on the course's scale, provisional toggle and a note. A pass
// marks the course Passed (archived for you); a fail offers a resit.
export function CheckInForm({
  deadline,
  onDone,
  compact = false,
}: {
  deadline: Deadline;
  onDone?: (d: Deadline) => void;
  compact?: boolean;
}) {
  const user = useUser();
  const course = useCourse(deadline.courseId);
  const scales = useGradeScales();
  const scale = scales.data?.find((s) => s.id === course.data?.gradeScale.id);
  const control = gradeControl(scale);
  const patch = usePatchDeadline();
  const toast = useToast();
  const [outcome, setOutcome] = useState<Outcome>("Done");
  const [grade, setGrade] = useState("");
  const [provisional, setProvisional] = useState(false);
  const [note, setNote] = useState("");
  const [localError, setLocalError] = useState<string | null>(null);
  const errors = fieldErrors(patch.error);
  const passes = outcome === "Done" ? wouldPass(control, grade, deadline.passMark) : null;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    setLocalError(null);
    const g = outcome === "Done" ? gradeBody(control, grade) : { gradeValue: null, gradeText: null };
    if ("error" in g) return setLocalError(g.error);
    patch.mutate(
      {
        id: deadline.id,
        version: deadline.version,
        body: {
          status: outcome,
          ...g,
          gradeProvisional: outcome === "Done" ? provisional : false,
          checkInNote: note.trim() || null,
        },
      },
      {
        onSuccess: (d) => {
          const code = course.data?.code ?? "";
          toast({
            message:
              d.passed === true
                ? `Passed. ${code} moves to your archive.`
                : d.passed === false
                  ? "Saved. You can plan a resit from the Exams tab."
                  : "Result saved.",
          });
          onDone?.(d);
        },
      },
    );
  };

  return (
    <form className="flex flex-col gap-3" onSubmit={submit} noValidate>
      {!compact ? (
        <p className="m-0 text-sm text-muted">
          {course.data?.code} {deadlineName(deadline)} ·{" "}
          {formatInZone(deadline.startsAt, user.timeZone, "EEE d MMM, HH:mm")}
        </p>
      ) : null}
      {patch.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(patch.error)}</Banner> : null}
      <Segmented<Outcome>
        label="How did it go?"
        size="sm"
        value={outcome}
        onChange={setOutcome}
        options={[
          { value: "Done", label: "I took it" },
          { value: "Missed", label: "Missed it" },
          { value: "Cancelled", label: "Cancelled" },
        ]}
      />
      {outcome === "Done" ? (
        <div className="flex flex-wrap items-end gap-3">
          <Field
            label="Grade"
            className="w-40"
            error={localError ?? errors.gradeValue ?? errors.gradeText}
            hint={passes === null ? "Leave empty if you don't know yet." : passes ? "A pass" : "Not a pass"}
          >
            {(ids) =>
              control.kind === "choice" ? (
                <Select {...ids} value={grade} onChange={(e) => setGrade(e.currentTarget.value)}>
                  <option value="">Not yet</option>
                  {control.options.map((o) => (
                    <option key={o.label} value={o.label}>
                      {o.label}
                    </option>
                  ))}
                </Select>
              ) : (
                <Input
                  {...ids}
                  inputMode="decimal"
                  className="font-mono tnum"
                  placeholder={`${control.min}–${control.max}`}
                  value={grade}
                  onChange={(e) => setGrade(e.currentTarget.value)}
                />
              )
            }
          </Field>
          <label className="flex h-9 items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={provisional}
              onChange={(e) => setProvisional(e.currentTarget.checked)}
              className="size-4 accent-[var(--primary)]"
            />
            Provisional
          </label>
        </div>
      ) : null}
      <Field label="Note" error={errors.checkInNote}>
        {(ids) => (
          <Textarea
            {...ids}
            className="min-h-16"
            placeholder="What went well, what to change next time"
            value={note}
            onChange={(e) => setNote(e.currentTarget.value)}
          />
        )}
      </Field>
      <div className="flex justify-end">
        <Button type="submit" variant="primary" size="sm" disabled={patch.isPending}>
          Save result
        </Button>
      </div>
    </form>
  );
}
