import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useEffect, useState, type FormEvent } from "react";
import { useUser } from "@/app/auth";
import { ConflictDialog } from "@/components/conflict-dialog";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Field, Input, Select } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import {
  useCourse,
  useShownGradeScales,
  useMembers,
  usePatchCourse,
  useRemoveMember,
  useShare,
  useSubscription,
  useTrashCourse,
  type Course,
} from "@/features/courses/api";
import { ColourPicker } from "@/features/courses/components/colour-picker";
import { canEdit, isOwner, normalizeCode } from "@/features/courses/lib/roles";
import { conflictCurrent, fieldErrors, messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";
import { CourseStorage } from "@/features/materials/course-storage";
import { CourseAiSettings } from "@/features/ai/course-ai";

// Course settings (frontend-spec 4): details (colour and grade scale are the Owner's), sharing,
// subscription, AI for this course, Storage ("Free up space", undo for 7 days), delete to Trash.
export const Route = createFileRoute("/_app/c/$courseId/settings")({ component: Settings });

function Details({ course }: { course: Course }) {
  const patch = usePatchCourse(course.id);
  const scales = useShownGradeScales(course.gradeScale.id);
  const toast = useToast();
  const owner = isOwner(course.myRole);
  const editor = canEdit(course.myRole);
  const blank = (c: Course) => ({
    name: c.name,
    code: c.code,
    institution: c.institution ?? "",
    term: c.term ?? "",
    color: c.color,
    gradeScaleId: c.gradeScale.id,
  });
  const [f, setF] = useState(() => blank(course));
  const [conflict, setConflict] = useState<Course | null>(null);
  useEffect(() => setF(blank(course)), [course]);
  const errors = fieldErrors(patch.error);

  const send = (version: number) =>
    patch.mutate(
      {
        version,
        fields: {
          name: f.name.trim(),
          code: f.code,
          institution: f.institution.trim() || null,
          term: f.term.trim() || null,
          ...(owner ? { color: f.color, gradeScaleId: f.gradeScaleId } : {}),
        },
      },
      {
        onSuccess: () => toast({ message: "Saved" }),
        onError: (e) => {
          const cur = conflictCurrent<Course>(e);
          if (cur) setConflict(cur);
        },
      },
    );
  const submit = (e: FormEvent) => {
    e.preventDefault();
    send(course.version);
  };

  return (
    <Card>
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        <SectionLabel>Details</SectionLabel>
        {patch.error && !conflictCurrent(patch.error) && !Object.keys(errors).length ? (
          <Banner tone="danger">{messageFor(patch.error)}</Banner>
        ) : null}
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Name" error={errors.name}>
            {(ids) => (
              <Input
                {...ids}
                disabled={!editor}
                value={f.name}
                onChange={(e) => setF({ ...f, name: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field label="Code" error={errors.code}>
            {(ids) => (
              <Input
                {...ids}
                disabled={!editor}
                className="font-mono uppercase"
                value={f.code}
                onChange={(e) => setF({ ...f, code: normalizeCode(e.currentTarget.value) })}
              />
            )}
          </Field>
          <Field label="Institution" error={errors.institution}>
            {(ids) => (
              <Input
                {...ids}
                disabled={!editor}
                value={f.institution}
                onChange={(e) => setF({ ...f, institution: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field label="Term" error={errors.term}>
            {(ids) => (
              <Input
                {...ids}
                disabled={!editor}
                value={f.term}
                onChange={(e) => setF({ ...f, term: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field
            label="Grade scale"
            error={errors.gradeScaleId}
            hint={owner ? undefined : "Only the owner changes this."}
          >
            {(ids) => (
              <Select
                {...ids}
                disabled={!owner}
                value={f.gradeScaleId}
                onChange={(e) => setF({ ...f, gradeScaleId: e.currentTarget.value })}
              >
                {(scales.data ?? [{ id: course.gradeScale.id, name: course.gradeScale.name }]).map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        </div>
        {owner ? (
          <div className="flex flex-col gap-2">
            <span className="text-sm font-medium">Colour</span>
            <ColourPicker value={f.color} onChange={(color) => setF({ ...f, color })} />
          </div>
        ) : null}
        {editor ? (
          <div className="flex justify-end">
            <Button type="submit" variant="primary" disabled={patch.isPending}>
              Save changes
            </Button>
          </div>
        ) : null}
      </form>
      <ConflictDialog
        open={!!conflict}
        onClose={() => setConflict(null)}
        onReload={() => {
          if (conflict) setF(blank(conflict));
          setConflict(null);
          patch.reset();
        }}
        onOverwrite={() => {
          if (conflict) send(conflict.version);
          setConflict(null);
        }}
      />
    </Card>
  );
}

function Sharing({ course }: { course: Course }) {
  const user = useUser();
  const owner = isOwner(course.myRole);
  const members = useMembers(course.id);
  const share = useShare(course.id);
  const remove = useRemoveMember(course.id);
  const toast = useToast();
  const navigate = useNavigate();
  const [who, setWho] = useState("");
  const [role, setRole] = useState("Viewer");
  const errors = fieldErrors(share.error);
  const me = members.data?.members.find((m) => m.userId === user.id);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    share.mutate(
      { usernameOrEmail: who.trim(), role },
      {
        onSuccess: () => {
          toast({ message: who.includes("@") ? `Invite sent to ${who.trim()}` : `Shared with ${who.trim()}` });
          setWho("");
        },
      },
    );
  };

  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>{canEdit(course.myRole) ? "Sharing" : "Membership"}</SectionLabel>
      {owner ? (
        <form className="grid gap-3 sm:grid-cols-[1fr_140px_auto] sm:items-end" onSubmit={submit} noValidate>
          <Field label="Username or email" error={errors.usernameOrEmail}>
            {(ids) => <Input {...ids} value={who} onChange={(e) => setWho(e.currentTarget.value)} />}
          </Field>
          <Field label="Role" error={errors.role}>
            {(ids) => (
              <Select {...ids} value={role} onChange={(e) => setRole(e.currentTarget.value)}>
                <option value="Viewer">Viewer</option>
                <option value="Editor">Editor</option>
              </Select>
            )}
          </Field>
          <Button type="submit" variant="primary" disabled={!who.trim() || share.isPending}>
            Share
          </Button>
          {/* Under the whole row, so the three controls line up. */}
          <p className="m-0 text-sm text-muted sm:col-span-3">An email without an account gets an invite to sign up.</p>
          {share.error && !Object.keys(errors).length ? (
            <p className="m-0 text-sm text-danger-text sm:col-span-3">{messageFor(share.error)}</p>
          ) : null}
        </form>
      ) : null}
      {!canEdit(course.myRole) ? null : members.isPending ? (
        <Skeleton className="h-20" />
      ) : (
        <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
          <li className="flex items-center gap-3 py-2">
            {/* The owner arrives as a username: shown like the members' @handle. */}
            <span className="min-w-0 flex-1 truncate text-muted">@{members.data?.owner.name}</span>
            <Badge kind="label">Owner</Badge>
            {/* Where the members' Remove sits, so the badges line up. */}
            {owner ? <span className="w-[72px] shrink-0" aria-hidden="true" /> : null}
          </li>
          {members.data?.members.map((m) => (
            <li key={m.userId} className="flex items-center gap-3 py-2">
              <span className="min-w-0 flex-1 truncate">
                {m.name} <span className="text-muted">@{m.username}</span>
              </span>
              {m.status === "Pending" ? <Badge tone="warning">Pending</Badge> : null}
              <Badge kind="label">{m.role}</Badge>
              {owner ? (
                <Button size="sm" variant="ghost" disabled={remove.isPending} onClick={() => remove.mutate(m.userId)}>
                  Remove
                </Button>
              ) : null}
            </li>
          ))}
          {members.data?.pendingEmails.map((p) => (
            <li key={p.id} className="flex items-center gap-3 py-2">
              <span className="min-w-0 flex-1 truncate">{p.email}</span>
              <Badge tone="warning">Invited</Badge>
              <Badge kind="label">{p.role}</Badge>
              {owner ? (
                <Button size="sm" variant="ghost" disabled={remove.isPending} onClick={() => remove.mutate(p.id)}>
                  Withdraw
                </Button>
              ) : null}
            </li>
          ))}
        </ul>
      )}
      {!owner ? (
        <div className="flex flex-wrap items-center gap-3">
          <p className="m-0 flex-1 text-sm text-muted">Shared with you by {course.owner.name}.</p>
          {me ? (
            <Button
              variant="secondary"
              disabled={remove.isPending}
              onClick={() => remove.mutate(me.userId, { onSuccess: () => void navigate({ to: "/" }) })}
            >
              Leave course
            </Button>
          ) : null}
        </div>
      ) : null}
    </Card>
  );
}

function Subscription({ course }: { course: Course }) {
  const sub = useSubscription(course.id);
  const status = course.mySubscription;
  return (
    <Card className="flex flex-col gap-3">
      <SectionLabel>Your subscription</SectionLabel>
      <p className="m-0 text-muted">
        {status === "Active"
          ? "You're tracking this course: it's in your plan, Take me there and reminders."
          : status === "Archived"
            ? "Archived: out of your plan and reminders; its history stays."
            : status === "Passed"
              ? "Passed. The course is kept read-only in Archived."
              : "Not subscribed: you can read the course but nothing is planned or tracked."}
      </p>
      {sub.error ? <p className="m-0 text-sm text-danger-text">{messageFor(sub.error)}</p> : null}
      <div className="flex flex-wrap gap-2">
        {status !== "Active" ? (
          <Button variant="primary" disabled={sub.isPending} onClick={() => sub.mutate("Active")}>
            {status === "Archived" ? "Subscribe again" : "Subscribe"}
          </Button>
        ) : (
          <Button disabled={sub.isPending} onClick={() => sub.mutate("Archived")}>
            Archive for me
          </Button>
        )}
        {status && status !== "Passed" && course.myRole !== "Owner" ? (
          <Button variant="ghost" disabled={sub.isPending} onClick={() => sub.mutate(null)}>
            Unsubscribe
          </Button>
        ) : null}
      </div>
    </Card>
  );
}

function Danger({ course }: { course: Course }) {
  const user = useUser();
  const trash = useTrashCourse(course.id);
  const navigate = useNavigate();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  return (
    <Card className="flex flex-col gap-3 border-danger/40">
      <SectionLabel>Delete</SectionLabel>
      <p className="m-0 text-muted">
        The course goes to the Trash for 30 days, hidden from everyone it's shared with. You can restore it until then.
      </p>
      <div>
        <Button variant="secondary" className="text-danger-text" onClick={() => setOpen(true)}>
          Delete course
        </Button>
      </div>
      <Dialog
        open={open}
        onOpenChange={setOpen}
        title={`Delete ${course.code}?`}
        description="It moves to the Trash for 30 days, then is deleted for good with its materials, questions and history."
        footer={
          <>
            <Button onClick={() => setOpen(false)}>Cancel</Button>
            <Button
              variant="destructive"
              disabled={trash.isPending}
              onClick={() =>
                trash.mutate(undefined, {
                  onSuccess: (c) => {
                    toast({
                      message: `${course.code} moved to the Trash${c.purgeAt ? ` until ${formatInZone(c.purgeAt, user.timeZone, "d MMM")}` : ""}`,
                    });
                    void navigate({ to: "/trash" });
                  },
                  onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
                })
              }
            >
              Move to Trash
            </Button>
          </>
        }
      />
    </Card>
  );
}

function Settings() {
  const { courseId } = Route.useParams();
  const course = useCourse(courseId);
  if (!course.data) return <Skeleton className="h-96" />;
  const c = course.data;
  return (
    <div className="flex max-w-[760px] flex-col gap-4">
      {/* A viewer can't change details, sharing or storage: those cards stay out of their way. */}
      {canEdit(c.myRole) ? <Details course={c} /> : null}
      <Sharing course={c} />
      <Subscription course={c} />
      <CourseAiSettings courseId={c.id} />
      {isOwner(c.myRole) ? <CourseStorage course={c} /> : null}
      {isOwner(c.myRole) ? <Danger course={c} /> : null}
    </div>
  );
}
