import { createFileRoute } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";
import { useUser } from "@/app/auth";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { EmptyList } from "@/components/ui/empty";
import { Field, Input, Select } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { useToast } from "@/components/ui/toast";
import {
  exportStatus,
  useCancelDeletion,
  useCreateInvite,
  useDeletion,
  useExports,
  useInvites,
  useRequestExport,
  useRevokeInvite,
  useScheduleDeletion,
  type CreatedInvite,
  type Deletion,
  type Invite,
} from "@/features/account/api";
import { downloadFile } from "@/features/files/api";
import { idempotencyKey } from "@/lib/api";
import { fieldErrors, messageFor } from "@/lib/errors";
import { formatInZone } from "@/lib/time";

// Account (frontend-spec 4): export my data (Queued / Building / Ready with a 24 h download /
// Expired / Failed with "Try again"), my invites (create, copy, revoke), and delete account
// (password, a decision for each course others use, 14 days to change your mind).
export const Route = createFileRoute("/_app/settings/account")({ component: Account });

function size(bytes: number | null | undefined): string {
  if (!bytes) return "";
  return bytes < 1_048_576 ? `${Math.max(1, Math.round(bytes / 1024))} KB` : `${(bytes / 1_048_576).toFixed(1)} MB`;
}

const EXPORT_TONE = { Ready: "success", Failed: "danger", Expired: "neutral" } as const;

function Exports() {
  const user = useUser();
  const exports = useExports();
  const request = useRequestExport();
  const toast = useToast();
  const [downloading, setDownloading] = useState<string | null>(null);
  const busy = exports.data?.some((e) => e.status === "Queued" || e.status === "Building") ?? false;
  const start = () => request.mutate(undefined, { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) });

  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>Your data</SectionLabel>
      <p className="m-0 text-muted">
        A zip of everything you've made in Crux: courses you own with their files, your answers, cards, plan and
        notes. It takes a few minutes; the inbox tells you when it's ready, and the link works for 24 hours.
      </p>
      <Button className="self-start" disabled={busy || request.isPending} onClick={start}>
        {busy ? "Export in progress…" : "Export my data"}
      </Button>
      {exports.isPending ? (
        <Skeleton className="h-16" />
      ) : exports.data?.length ? (
        <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
          {exports.data.map((e) => {
            const status = exportStatus(e);
            return (
              <li key={e.id} className="flex flex-wrap items-center gap-3 py-2.5">
                <span className="min-w-0 flex-1 text-sm">
                  {formatInZone(e.requestedAt, user.timeZone, "EEE d MMM, HH:mm")}
                  {e.sizeBytes ? <span className="text-muted"> · {size(e.sizeBytes)}</span> : null}
                  {status === "Ready" && e.expiresAt ? (
                    <span className="text-muted"> · until {formatInZone(e.expiresAt, user.timeZone, "EEE HH:mm")}</span>
                  ) : null}
                </span>
                <Badge tone={EXPORT_TONE[status as keyof typeof EXPORT_TONE] ?? "info"}>{status}</Badge>
                {status === "Ready" && e.fileId ? (
                  <Button
                    size="sm"
                    disabled={downloading === e.id}
                    onClick={() => {
                      setDownloading(e.id);
                      downloadFile(e.fileId!)
                        .catch((err: unknown) => toast({ message: messageFor(err), tone: "danger" }))
                        .finally(() => setDownloading(null));
                    }}
                  >
                    Download
                  </Button>
                ) : status === "Failed" ? (
                  <Button size="sm" disabled={busy || request.isPending} onClick={start}>
                    Try again
                  </Button>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : null}
    </Card>
  );
}

function NewInvite({ onCreated }: { onCreated: (i: CreatedInvite) => void }) {
  const create = useCreateInvite();
  const [key, setKey] = useState(idempotencyKey);
  const [email, setEmail] = useState("");
  const [uses, setUses] = useState("1");
  const [days, setDays] = useState("14");
  const errors = fieldErrors(create.error);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate(
      { key, email: email.trim() || null, maxUses: Number(uses), expiresInDays: Number(days) },
      {
        onSuccess: (i) => {
          onCreated(i);
          setKey(idempotencyKey());
          setEmail("");
          create.reset();
        },
      },
    );
  };
  return (
    <form className="flex flex-col gap-3" onSubmit={submit} noValidate>
      {create.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(create.error)}</Banner> : null}
      <div className="grid gap-3 sm:grid-cols-[1fr_7rem_7rem]">
        <Field label="Email (optional)" error={errors.email} hint="Only this address can use it, and we email the link.">
          {(ids) => <Input {...ids} type="email" value={email} onChange={(e) => setEmail(e.currentTarget.value)} />}
        </Field>
        <Field label="Uses" error={errors.maxUses}>
          {(ids) => (
            <Select {...ids} value={uses} onChange={(e) => setUses(e.currentTarget.value)}>
              {[1, 2, 3, 4, 5].map((n) => (
                <option key={n}>{n}</option>
              ))}
            </Select>
          )}
        </Field>
        <Field label="Valid for" error={errors.expiresInDays}>
          {(ids) => (
            <Select {...ids} value={days} onChange={(e) => setDays(e.currentTarget.value)}>
              {[1, 7, 14, 30].map((n) => (
                <option key={n} value={n}>
                  {n} {n === 1 ? "day" : "days"}
                </option>
              ))}
            </Select>
          )}
        </Field>
      </div>
      <Button type="submit" className="self-start" disabled={create.isPending}>
        Create invite
      </Button>
    </form>
  );
}

function inviteState(i: Invite, now = Date.now()): string {
  if (i.revokedAt) return "Revoked";
  if (i.uses >= i.maxUses) return "Used";
  if (i.expiresAt && new Date(i.expiresAt).getTime() <= now) return "Expired";
  return "Open";
}

function Invites() {
  const user = useUser();
  const invites = useInvites();
  const revoke = useRevokeInvite();
  const toast = useToast();
  const [created, setCreated] = useState<CreatedInvite | null>(null);
  const copy = (text: string) =>
    navigator.clipboard.writeText(text).then(
      () => toast({ message: "Link copied" }),
      () => toast({ message: "Couldn't copy; select the link instead.", tone: "danger" }),
    );

  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>Invites</SectionLabel>
      <p className="m-0 text-muted">Crux is invite-only. Invite someone to make their own account.</p>
      {!user.emailVerified ? (
        <Banner>Verify your email to create invites.</Banner>
      ) : (
        <NewInvite onCreated={setCreated} />
      )}
      {created ? (
        <Banner
          action={
            <Button size="sm" onClick={() => void copy(created.signupUrl)}>
              Copy link
            </Button>
          }
        >
          <span className="flex flex-col gap-1">
            <span>
              {created.email ? `Invite emailed to ${created.email}. ` : ""}Copy the link now; it isn't shown again.
            </span>
            <code className="break-all text-sm">{created.signupUrl}</code>
          </span>
        </Banner>
      ) : null}
      {invites.isPending ? (
        <Skeleton className="h-16" />
      ) : invites.data?.items.length ? (
        <ul className="m-0 flex list-none flex-col divide-y divide-border p-0">
          {invites.data.items.map((i) => {
            const state = inviteState(i);
            return (
              <li key={i.id} className="flex flex-wrap items-center gap-3 py-2.5">
                <span className="min-w-0 flex-1 truncate text-sm">
                  {i.email ?? "Anyone with the link"}
                  <span className="text-muted">
                    {" "}
                    · {i.uses}/{i.maxUses} used
                    {i.expiresAt ? ` · until ${formatInZone(i.expiresAt, user.timeZone, "d MMM")}` : ""}
                  </span>
                </span>
                <Badge tone={state === "Open" ? "info" : "neutral"}>{state}</Badge>
                {state === "Open" ? (
                  <Button
                    size="sm"
                    disabled={revoke.isPending}
                    onClick={() =>
                      revoke.mutate(i.id, { onError: (e) => toast({ message: messageFor(e), tone: "danger" }) })
                    }
                  >
                    Revoke
                  </Button>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : (
        <EmptyList title="No invites yet" />
      )}
    </Card>
  );
}

type Decision = { action: "transfer" | "delete"; toUserId: string };

function DeleteDialog({ open, onClose, deletion }: { open: boolean; onClose: () => void; deletion: Deletion }) {
  const schedule = useScheduleDeletion();
  const [password, setPassword] = useState("");
  const [decisions, setDecisions] = useState<Record<string, Decision>>({});
  const errors = fieldErrors(schedule.error);
  const decide = (id: string): Decision => {
    const first = deletion.sharedCourses.find((c) => c.courseId === id)?.members[0]?.id ?? "";
    return decisions[id] ?? { action: first ? "transfer" : "delete", toUserId: first };
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    schedule.mutate(
      {
        password,
        courses: deletion.sharedCourses.map((c) => {
          const d = decide(c.courseId);
          return d.action === "transfer"
            ? { courseId: c.courseId, action: "transfer", toUserId: d.toUserId }
            : { courseId: c.courseId, action: "delete" };
        }),
      },
      { onSuccess: onClose },
    );
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      title="Delete your account"
      description="Everything goes after 14 days: your courses, files, answers and cards. Until then you can cancel."
    >
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        {schedule.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(schedule.error)}</Banner> : null}
        {deletion.sharedCourses.length ? (
          <div className="flex flex-col gap-3">
            <span className="text-sm font-medium">Courses others use</span>
            {deletion.sharedCourses.map((c) => {
              const d = decide(c.courseId);
              return (
                <Field key={c.courseId} label={c.name}>
                  {(ids) => (
                    <Select
                      {...ids}
                      value={d.action === "delete" ? "" : d.toUserId}
                      onChange={(e) => {
                        const v = e.currentTarget.value;
                        setDecisions({
                          ...decisions,
                          [c.courseId]: v ? { action: "transfer", toUserId: v } : { action: "delete", toUserId: "" },
                        });
                      }}
                    >
                      {c.members.map((m) => (
                        <option key={m.id} value={m.id}>
                          Give it to {m.name}
                        </option>
                      ))}
                      <option value="">Delete it for everyone</option>
                    </Select>
                  )}
                </Field>
              );
            })}
          </div>
        ) : null}
        <Field label="Your password" error={errors.password}>
          {(ids) => (
            <Input
              {...ids}
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
            />
          )}
        </Field>
        <div className="flex flex-wrap justify-end gap-2">
          <Button onClick={onClose}>Keep my account</Button>
          <Button type="submit" variant="destructive" disabled={!password || schedule.isPending}>
            Delete in 14 days
          </Button>
        </div>
      </form>
    </Dialog>
  );
}

function DeleteAccount() {
  const user = useUser();
  const deletion = useDeletion();
  const cancel = useCancelDeletion();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const d = deletion.data;
  const pending = d?.status === "Pending";

  return (
    <Card className="flex flex-col gap-4">
      <SectionLabel>Delete account</SectionLabel>
      {deletion.isPending ? (
        <Skeleton className="h-10" />
      ) : pending ? (
        <>
          <p className="m-0">
            Your account will be deleted on{" "}
            <strong>{d.runsAt ? formatInZone(d.runsAt, user.timeZone, "EEEE d MMMM") : "…"}</strong>.
          </p>
          <Button
            className="self-start"
            disabled={cancel.isPending}
            onClick={() =>
              cancel.mutate(undefined, {
                onSuccess: () => toast({ message: "Deletion cancelled" }),
                onError: (e) => toast({ message: messageFor(e), tone: "danger" }),
              })
            }
          >
            Cancel deletion
          </Button>
        </>
      ) : (
        <>
          <p className="m-0 text-muted">
            Deletes your account and everything in it after 14 days. Courses others use can go to one of them instead.
          </p>
          <Button variant="destructive" className="self-start" disabled={!d} onClick={() => setOpen(true)}>
            Delete my account…
          </Button>
          {d ? <DeleteDialog open={open} onClose={() => setOpen(false)} deletion={d} /> : null}
        </>
      )}
    </Card>
  );
}

function Account() {
  return (
    <>
      <Exports />
      <Invites />
      <DeleteAccount />
    </>
  );
}
