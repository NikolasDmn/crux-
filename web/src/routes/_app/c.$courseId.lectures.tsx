import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowDown, ArrowUp, MoreHorizontal } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Field, Input, Select } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Dialog, Menu, MenuContent, MenuItem, MenuTrigger } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import { useCourse } from "@/features/courses/api";
import { canEdit } from "@/features/courses/lib/roles";
import {
  reorder,
  useCreateLecture,
  useDeleteLecture,
  useLectures,
  usePatchLecture,
  useProgress,
  useSetProgress,
  type Lecture,
} from "@/features/lectures/api";
import { idempotencyKey } from "@/lib/api";
import { fieldErrors, messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";
import { useUser } from "@/app/auth";

// Lectures (frontend-spec 4): ordered list with status, steps and hours; Editors add, edit, reorder.
export const Route = createFileRoute("/_app/c/$courseId/lectures")({ component: Lectures });

function LectureForm({
  initial,
  busy,
  error,
  onSubmit,
  submitLabel,
}: {
  initial?: Lecture;
  busy: boolean;
  error: unknown;
  onSubmit: (f: { code: string; name: string; estHours: number | null }) => void;
  submitLabel: string;
}) {
  const [f, setF] = useState({
    code: initial?.code ?? "",
    name: initial?.name ?? "",
    est: initial?.estHours?.toString() ?? "",
  });
  const errors = fieldErrors(error);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    onSubmit({ code: f.code.trim(), name: f.name.trim(), estHours: f.est ? Number(f.est) : null });
  };
  return (
    <form className="grid gap-3 sm:grid-cols-[110px_1fr_120px_auto] sm:items-end" onSubmit={submit} noValidate>
      <Field label="Code" error={errors.code}>
        {(ids) => (
          <Input
            {...ids}
            placeholder="L1"
            required
            value={f.code}
            onChange={(e) => setF({ ...f, code: e.currentTarget.value })}
          />
        )}
      </Field>
      <Field label="Name" error={errors.name}>
        {(ids) => (
          <Input
            {...ids}
            placeholder="Descriptive statistics"
            required
            value={f.name}
            onChange={(e) => setF({ ...f, name: e.currentTarget.value })}
          />
        )}
      </Field>
      <Field label="Hours" error={errors.estHours}>
        {(ids) => (
          <Input
            {...ids}
            type="number"
            min={0}
            max={500}
            step={0.5}
            placeholder="1.5"
            value={f.est}
            onChange={(e) => setF({ ...f, est: e.currentTarget.value })}
          />
        )}
      </Field>
      <Button type="submit" variant="primary" disabled={busy || !f.code.trim() || !f.name.trim()}>
        {submitLabel}
      </Button>
      {error && !Object.keys(errors).length ? (
        <p className="m-0 text-sm text-danger-text sm:col-span-4">{messageFor(error)}</p>
      ) : null}
    </form>
  );
}

function Lectures() {
  const { courseId } = Route.useParams();
  const user = useUser();
  const toast = useToast();
  const course = useCourse(courseId);
  const lectures = useLectures(courseId);
  const subscribed = course.data?.mySubscription === "Active";
  const progress = useProgress(courseId);
  const setProgress = useSetProgress(courseId);
  const create = useCreateLecture(courseId);
  const patch = usePatchLecture(courseId);
  const remove = useDeleteLecture(courseId);
  const editor = canEdit(course.data?.myRole);
  const [key, setKey] = useState(idempotencyKey);
  const [editing, setEditing] = useState<Lecture | null>(null);
  const [deleting, setDeleting] = useState<Lecture | null>(null);

  if (lectures.isPending) return <Skeleton className="h-64" />;
  const list = lectures.data ?? [];
  const byId = new Map((progress.data ?? []).map((p) => [p.componentId, p]));

  const move = async (from: number, to: number) => {
    for (const p of reorder(list, from, to)) {
      const l = list.find((x) => x.id === p.id)!;
      try {
        await patch.mutateAsync({ id: l.id, version: l.version, fields: { sortOrder: p.sortOrder } });
      } catch (e) {
        toast({ message: messageFor(e), tone: "danger" });
        return;
      }
    }
  };

  return (
    <div className="flex flex-col gap-4">
      {editor ? (
        <Card>
          <LectureForm
            key={key}
            busy={create.isPending}
            error={create.error}
            submitLabel="Add lecture"
            onSubmit={(fields) =>
              create.mutate(
                { key, fields },
                {
                  onSuccess: () => {
                    setKey(idempotencyKey());
                    create.reset();
                  },
                },
              )
            }
          />
        </Card>
      ) : null}
      {!list.length ? (
        <EmptyList
          title="No lectures yet"
          body={
            editor
              ? "Add the first one above: a lecture, week or chapter."
              : "The course owner hasn't added any lectures yet."
          }
        />
      ) : (
        <Card className="p-0">
          <ol className="m-0 list-none divide-y divide-border p-0">
            {list.map((l, i) => {
              const p = byId.get(l.id);
              return (
                <li key={l.id} className="flex flex-wrap items-center gap-x-4 gap-y-2 px-5 py-3">
                  <span className="w-10 font-mono text-sm text-muted">{l.code}</span>
                  <Link
                    to="/c/$courseId/l/$componentId"
                    params={{ courseId, componentId: l.id }}
                    className="min-w-0 flex-1 font-medium underline-offset-4 hover:underline"
                  >
                    {l.name}
                  </Link>
                  <span className="font-mono text-sm text-muted tnum">
                    {p ? `${p.actualHours.toFixed(1)} / ` : ""}
                    {l.estHours ?? 1.5} h
                  </span>
                  {subscribed && p ? (
                    <div className="flex items-center gap-2">
                      {p.status === "Done" && p.finishedAt ? (
                        <span className="text-xs text-muted">{formatInZone(p.finishedAt, user.timeZone, "d MMM")}</span>
                      ) : null}
                      <label className="sr-only" htmlFor={`st-${l.id}`}>
                        Status of {l.code}
                      </label>
                      <Select
                        id={`st-${l.id}`}
                        className="h-8 w-[130px]"
                        value={p.status}
                        onChange={(e) => setProgress.mutate({ componentId: l.id, status: e.currentTarget.value })}
                      >
                        <option value="Todo">To do</option>
                        <option value="InProgress">In progress</option>
                        <option value="Done">Done</option>
                      </Select>
                    </div>
                  ) : null}
                  {editor ? (
                    <div className="flex items-center">
                      <Button
                        size="iconSm"
                        variant="ghost"
                        aria-label={`Move ${l.code} up`}
                        disabled={i === 0 || patch.isPending}
                        onClick={() => void move(i, i - 1)}
                      >
                        <ArrowUp />
                      </Button>
                      <Button
                        size="iconSm"
                        variant="ghost"
                        aria-label={`Move ${l.code} down`}
                        disabled={i === list.length - 1 || patch.isPending}
                        onClick={() => void move(i, i + 1)}
                      >
                        <ArrowDown />
                      </Button>
                      <Menu>
                        <MenuTrigger asChild>
                          <Button size="iconSm" variant="ghost" aria-label={`More for ${l.code}`}>
                            <MoreHorizontal />
                          </Button>
                        </MenuTrigger>
                        <MenuContent align="end">
                          <MenuItem onSelect={() => setEditing(l)}>Edit</MenuItem>
                          <MenuItem className="text-danger-text" onSelect={() => setDeleting(l)}>
                            Delete
                          </MenuItem>
                        </MenuContent>
                      </Menu>
                    </div>
                  ) : null}
                </li>
              );
            })}
          </ol>
        </Card>
      )}

      <Dialog
        open={!!editing}
        onOpenChange={(o) => !o && setEditing(null)}
        title={`Edit ${editing?.code ?? ""}`}
        className="w-[min(640px,calc(100vw-32px))]"
      >
        {editing ? (
          <LectureForm
            initial={editing}
            busy={patch.isPending}
            error={patch.error}
            submitLabel="Save"
            onSubmit={(fields) =>
              patch.mutate({ id: editing.id, version: editing.version, fields }, { onSuccess: () => setEditing(null) })
            }
          />
        ) : null}
      </Dialog>
      <Dialog
        open={!!deleting}
        onOpenChange={(o) => !o && setDeleting(null)}
        title={`Delete ${deleting?.code ?? ""}?`}
        description="Its materials go with it. Questions and cards that point at it keep working but lose the link."
        footer={
          <>
            <Button onClick={() => setDeleting(null)}>Cancel</Button>
            <Button
              variant="destructive"
              disabled={remove.isPending}
              onClick={() =>
                deleting &&
                remove.mutate(deleting.id, {
                  onSuccess: () => setDeleting(null),
                  onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
                })
              }
            >
              Delete lecture
            </Button>
          </>
        }
      />
    </div>
  );
}
