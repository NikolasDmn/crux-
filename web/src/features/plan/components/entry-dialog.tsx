import { useState, type FormEvent } from "react";
import { useUser } from "@/app/auth";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Field, Input, Select } from "@/components/ui/field";
import { Dialog } from "@/components/ui/overlays";
import { useCourses } from "@/features/courses/api";
import { useLectures } from "@/features/lectures/api";
import { useCreateEntry, useDeleteEntry, usePatchEntry, type PlanEntry } from "@/features/plan/api";
import { conflictCurrent, fieldErrors, messageFor } from "@/lib/errors";
import { fromLocalInput, toLocalInput } from "@/lib/time";

export interface Draft {
  courseId?: string | undefined;
  startAt: Date;
  endAt: Date;
}

/** Edit an entry (description, times, done/skip) or add your own. Times here are the keyboard
 *  alternative to dragging on the calendar (spec 2.7). */
export function EntryDialog({
  entry,
  draft,
  onClose,
}: {
  entry?: PlanEntry | undefined;
  draft?: Draft | undefined;
  onClose: () => void;
}) {
  const user = useUser();
  const courses = useCourses();
  const live = (courses.data ?? []).filter((c) => !c.trashedAt && c.mySubscription === "Active");
  const [f, setF] = useState(() => ({
    courseId: entry?.courseId ?? draft?.courseId ?? "",
    componentId: entry?.componentId ?? "",
    kind: entry?.kind ?? "Other",
    description: entry?.description ?? "",
    startAt: toLocalInput(entry?.startAt ?? draft?.startAt ?? new Date(), user.timeZone),
    endAt: toLocalInput(entry?.endAt ?? draft?.endAt ?? new Date(), user.timeZone),
  }));
  const lectures = useLectures(f.courseId);
  const create = useCreateEntry();
  const patch = usePatchEntry();
  const remove = useDeleteEntry();
  const [localError, setLocalError] = useState<string | null>(null);
  const m = entry ? patch : create;
  const errors = fieldErrors(m.error);
  const failure = m.error ?? remove.error;
  const general = failure && !Object.keys(errors).length ? messageFor(failure) : null;
  const done = { onSuccess: onClose };
  const planner = entry && entry.createdBy !== "User";

  const times = () => {
    const s = fromLocalInput(f.startAt, user.timeZone);
    const e = fromLocalInput(f.endAt, user.timeZone);
    if (!s || !e) return (setLocalError("Pick a start and an end."), null);
    if (e <= s) return (setLocalError("The end comes after the start."), null);
    return { startAt: s.toISOString(), endAt: e.toISOString() };
  };

  const setStatus = (status: "Planned" | "Done" | "Skipped") =>
    entry && patch.mutate({ id: entry.id, version: entry.version, patch: { status } }, done);

  const submit = (ev: FormEvent) => {
    ev.preventDefault();
    setLocalError(null);
    const t = times();
    if (!t) return;
    if (!entry) {
      create.mutate(
        {
          courseId: f.courseId,
          componentId: f.componentId || null,
          kind: f.kind,
          description: f.description.trim(),
          ...t,
        },
        done,
      );
      return;
    }
    const moved =
      t.startAt !== new Date(entry.startAt).toISOString() || t.endAt !== new Date(entry.endAt).toISOString();
    patch.mutate(
      {
        id: entry.id,
        version: entry.version,
        patch: { description: f.description.trim(), ...(moved ? t : {}) },
      },
      {
        ...done,
        onError: (e) => {
          // Changed elsewhere: take theirs and close; the calendar refetches.
          if (conflictCurrent(e)) onClose();
        },
      },
    );
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={entry ? `${entry.courseCode} · ${entry.kind}` : "Add to your plan"}
      description={
        entry?.status === "Planned" && entry.createdBy !== "User"
          ? "A new time pins this entry, so a rebuild leaves it where you put it."
          : undefined
      }
    >
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        {general ? <Banner tone="danger">{general}</Banner> : null}
        {!entry ? (
          <div className="grid gap-4 sm:grid-cols-2">
            <Field label="Course" error={errors.courseId}>
              {(ids) => (
                <Select
                  {...ids}
                  required
                  value={f.courseId}
                  onChange={(e) => setF({ ...f, courseId: e.currentTarget.value, componentId: "" })}
                >
                  <option value="">Pick a course</option>
                  {live.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.code} · {c.name}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
            <Field label="Kind" error={errors.kind}>
              {(ids) => (
                <Select {...ids} value={f.kind} onChange={(e) => setF({ ...f, kind: e.currentTarget.value })}>
                  <option value="Lecture">Lecture</option>
                  <option value="Revision">Revision</option>
                  <option value="Mock">Mock</option>
                  <option value="Other">Other</option>
                </Select>
              )}
            </Field>
            {f.courseId && lectures.data?.length ? (
              <Field label="Lecture" error={errors.componentId} className="sm:col-span-2">
                {(ids) => (
                  <Select
                    {...ids}
                    value={f.componentId}
                    onChange={(e) => setF({ ...f, componentId: e.currentTarget.value })}
                  >
                    <option value="">None</option>
                    {lectures.data.map((l) => (
                      <option key={l.id} value={l.id}>
                        {l.code} {l.name}
                      </option>
                    ))}
                  </Select>
                )}
              </Field>
            ) : null}
          </div>
        ) : null}
        <Field label="Description" error={errors.description}>
          {(ids) => (
            <Input
              {...ids}
              autoFocus={!entry}
              required
              placeholder="Practise exercises 4.1–4.6"
              value={f.description}
              onChange={(e) => setF({ ...f, description: e.currentTarget.value })}
            />
          )}
        </Field>
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Start" error={localError ?? errors.startAt}>
            {(ids) => (
              <Input
                {...ids}
                type="datetime-local"
                step={900}
                className="font-mono tnum"
                disabled={!!entry && entry.status !== "Planned"}
                value={f.startAt}
                onChange={(e) => setF({ ...f, startAt: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field label="End" error={errors.endAt}>
            {(ids) => (
              <Input
                {...ids}
                type="datetime-local"
                step={900}
                className="font-mono tnum"
                disabled={!!entry && entry.status !== "Planned"}
                value={f.endAt}
                onChange={(e) => setF({ ...f, endAt: e.currentTarget.value })}
              />
            )}
          </Field>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {entry ? (
            <div className="mr-auto flex flex-wrap gap-2">
              {entry.status === "Planned" ? (
                <>
                  <Button size="sm" disabled={patch.isPending} onClick={() => setStatus("Done")}>
                    Mark done
                  </Button>
                  <Button size="sm" disabled={patch.isPending} onClick={() => setStatus("Skipped")}>
                    Skip
                  </Button>
                </>
              ) : (
                <Button size="sm" disabled={patch.isPending} onClick={() => setStatus("Planned")}>
                  Back to planned
                </Button>
              )}
              {!planner ? (
                <Button
                  size="sm"
                  variant="destructive"
                  disabled={remove.isPending}
                  onClick={() => remove.mutate(entry.id, done)}
                >
                  Delete
                </Button>
              ) : null}
            </div>
          ) : null}
          <Button onClick={onClose}>Cancel</Button>
          <Button
            type="submit"
            variant="primary"
            disabled={m.isPending || !f.description.trim() || (!entry && !f.courseId)}
          >
            {entry ? "Save" : "Add"}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
