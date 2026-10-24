import { useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { useUser } from "@/app/auth";
import { ConflictDialog } from "@/components/conflict-dialog";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Field, Input, Select } from "@/components/ui/field";
import { Dialog } from "@/components/ui/overlays";
import { keys as courseKeys, type Deadline } from "@/features/courses/api";
import { useCreateDeadline, usePatchDeadline, type DeadlineInput } from "@/features/deadlines/api";
import { conflictCurrent, fieldErrors, messageFor } from "@/lib/errors";
import { fromLocalInput, toLocalInput } from "@/lib/time";

type Kind = "Exam" | "Resit" | "Deadline";

/** Add or edit a deadline; `resitOf` prefills a resit of that exam. */
export function DeadlineDialog({
  courseId,
  open,
  onOpenChange,
  editing,
  resitOf,
}: {
  courseId: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  editing?: Deadline | undefined;
  resitOf?: Deadline | undefined;
}) {
  const user = useUser();
  const qc = useQueryClient();
  const create = useCreateDeadline(courseId);
  const patch = usePatchDeadline();
  const base = editing ?? resitOf;
  const [f, setF] = useState(() => ({
    type: (resitOf ? "Resit" : (editing?.type ?? "Exam")) as Kind,
    title: resitOf ? "" : (editing?.title ?? ""),
    startsAt: editing ? toLocalInput(editing.startsAt, user.timeZone) : "",
    durationMin: base?.durationMin ? String(base.durationMin) : "",
    format: base?.format ?? "",
    passMark: base?.passMark !== null && base?.passMark !== undefined ? String(base.passMark) : "",
  }));
  const [localError, setLocalError] = useState<string | null>(null);
  const [conflict, setConflict] = useState<Deadline | null>(null);
  const m = editing ? patch : create;
  const errors = fieldErrors(m.error);
  const general = m.error && !Object.keys(errors).length && !conflict ? messageFor(m.error) : null;

  const body = (): DeadlineInput | null => {
    const at = fromLocalInput(f.startsAt, user.timeZone);
    if (!at) {
      setLocalError("Pick a date and time.");
      return null;
    }
    const duration = f.durationMin.trim() ? Number(f.durationMin) : null;
    const pass = f.passMark.trim() ? Number(f.passMark.replace(",", ".")) : null;
    return {
      type: f.type,
      title: f.title.trim() || null,
      startsAt: at.toISOString(),
      durationMin: duration !== null && Number.isFinite(duration) ? Math.round(duration) : null,
      format: f.format.trim() || null,
      passMark: pass !== null && Number.isFinite(pass) ? pass : null,
      ...(resitOf ? { resitOfId: resitOf.id } : {}),
    };
  };

  const send = (b: DeadlineInput, version?: number) => {
    const close = { onSuccess: () => onOpenChange(false) };
    if (editing) {
      patch.mutate(
        { id: editing.id, version: version ?? editing.version, body: b },
        {
          ...close,
          onError: (e) => {
            const current = conflictCurrent<Deadline>(e);
            if (current) setConflict(current);
          },
        },
      );
    } else create.mutate(b, close);
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    setLocalError(null);
    const b = body();
    if (b) send(b);
  };

  return (
    <>
      <Dialog
        open={open}
        onOpenChange={onOpenChange}
        title={editing ? "Edit deadline" : resitOf ? "Plan a resit" : "Add an exam or deadline"}
        description={!editing ? "The plan rebuilds around it, and you get a reminder the evening before." : undefined}
      >
        <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
          {general ? <Banner tone="danger">{general}</Banner> : null}
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Type" error={errors.type}>
              {(ids) => (
                <Select
                  {...ids}
                  value={f.type}
                  disabled={!!resitOf || !!editing?.resitOfId}
                  onChange={(e) => setF({ ...f, type: e.currentTarget.value as Kind })}
                >
                  <option value="Exam">Exam</option>
                  <option value="Resit">Resit</option>
                  <option value="Deadline">Deadline</option>
                </Select>
              )}
            </Field>
            <Field label="Title" error={errors.title}>
              {(ids) => (
                <Input
                  {...ids}
                  placeholder={f.type === "Deadline" ? "Assignment 2" : "Final exam"}
                  value={f.title}
                  onChange={(e) => setF({ ...f, title: e.currentTarget.value })}
                />
              )}
            </Field>
            <Field label="Starts" error={localError ?? errors.startsAt} className="sm:col-span-2">
              {(ids) => (
                <Input
                  {...ids}
                  type="datetime-local"
                  required
                  autoFocus
                  className="font-mono tnum"
                  value={f.startsAt}
                  onChange={(e) => setF({ ...f, startsAt: e.currentTarget.value })}
                />
              )}
            </Field>
            {f.type !== "Deadline" ? (
              <>
                <Field label="Duration (minutes)" error={errors.durationMin}>
                  {(ids) => (
                    <Input
                      {...ids}
                      inputMode="numeric"
                      className="font-mono tnum"
                      placeholder="120"
                      value={f.durationMin}
                      onChange={(e) => setF({ ...f, durationMin: e.currentTarget.value.replace(/\D/g, "") })}
                    />
                  )}
                </Field>
                <Field label="Pass mark" error={errors.passMark} hint="Empty: the grade scale's.">
                  {(ids) => (
                    <Input
                      {...ids}
                      inputMode="decimal"
                      className="font-mono tnum"
                      value={f.passMark}
                      onChange={(e) => setF({ ...f, passMark: e.currentTarget.value })}
                    />
                  )}
                </Field>
                <Field label="Format" error={errors.format} className="sm:col-span-2">
                  {(ids) => (
                    <Input
                      {...ids}
                      placeholder="40 MC + 3 open, closed book, formula sheet"
                      value={f.format}
                      onChange={(e) => setF({ ...f, format: e.currentTarget.value })}
                    />
                  )}
                </Field>
              </>
            ) : null}
          </div>
          <div className="flex justify-end gap-2">
            <Button onClick={() => onOpenChange(false)}>Cancel</Button>
            <Button type="submit" variant="primary" disabled={m.isPending || !f.startsAt}>
              {editing ? "Save" : "Add"}
            </Button>
          </div>
        </form>
      </Dialog>
      <ConflictDialog
        open={!!conflict}
        onClose={() => setConflict(null)}
        onReload={() => {
          setConflict(null);
          onOpenChange(false);
          void qc.invalidateQueries({ queryKey: courseKeys.deadlines(courseId) });
        }}
        onOverwrite={() => {
          const b = body();
          const v = conflict?.version;
          setConflict(null);
          if (b && v !== undefined) send(b, v);
        }}
      />
    </>
  );
}
