import { createFileRoute } from "@tanstack/react-router";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useAuth, useUser } from "@/app/auth";
import { ConflictDialog } from "@/components/conflict-dialog";
import { Badge } from "@/components/ui/badge";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Card, SectionLabel } from "@/components/ui/card";
import { Field, Input, Select } from "@/components/ui/field";
import { useToast } from "@/components/ui/toast";
import { useChangePassword, usePatchMe } from "@/features/account/api";
import type { User } from "@/features/auth/api";
import { NewPasswordField } from "@/features/auth/components/password-field";
import { browserZone } from "@/lib/time";
import { conflictCurrent, fieldCodes, fieldErrors, messageFor } from "@/lib/errors";

// Profile (frontend-spec 4): name, username, time zone, password. The email is shown, not edited:
// changing it needs a re-verification flow the backend doesn't have yet (decided 2026-10-29).
export const Route = createFileRoute("/_app/settings/profile")({ component: Profile });

function zones(current: string): string[] {
  const all = typeof Intl.supportedValuesOf === "function" ? Intl.supportedValuesOf("timeZone") : [];
  return all.includes(current) ? all : [current, ...all];
}

function Details() {
  const user = useUser();
  const { setUser } = useAuth();
  const patch = usePatchMe();
  const toast = useToast();
  const blank = (u: User) => ({
    firstName: u.firstName,
    lastName: u.lastName,
    username: u.username,
    timeZone: u.timeZone,
  });
  const [f, setF] = useState(() => blank(user));
  const [conflict, setConflict] = useState<User | null>(null);
  useEffect(() => setF(blank(user)), [user]);
  const options = useMemo(() => zones(user.timeZone), [user.timeZone]);
  const errors = fieldErrors(patch.error);
  const here = browserZone();

  const send = (version: number) =>
    patch.mutate(
      {
        version,
        firstName: f.firstName.trim(),
        lastName: f.lastName.trim(),
        username: f.username.trim(),
        timeZone: f.timeZone,
      },
      {
        onSuccess: () => toast({ message: "Saved" }),
        onError: (e) => {
          const cur = conflictCurrent<User>(e);
          if (cur) setConflict(cur);
        },
      },
    );
  const submit = (e: FormEvent) => {
    e.preventDefault();
    send(user.version);
  };
  const dirty = JSON.stringify(f) !== JSON.stringify(blank(user));

  return (
    <Card>
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        <SectionLabel>Profile</SectionLabel>
        {patch.error && !conflictCurrent(patch.error) && !Object.keys(errors).length ? (
          <Banner tone="danger">{messageFor(patch.error)}</Banner>
        ) : null}
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="First name" error={errors.firstName}>
            {(ids) => (
              <Input {...ids} value={f.firstName} onChange={(e) => setF({ ...f, firstName: e.currentTarget.value })} />
            )}
          </Field>
          <Field label="Last name" error={errors.lastName}>
            {(ids) => (
              <Input {...ids} value={f.lastName} onChange={(e) => setF({ ...f, lastName: e.currentTarget.value })} />
            )}
          </Field>
          <Field
            label="Username"
            error={errors.username}
            hint="You can log in with it, and people share courses with it."
          >
            {(ids) => (
              <Input
                {...ids}
                autoComplete="username"
                value={f.username}
                onChange={(e) => setF({ ...f, username: e.currentTarget.value })}
              />
            )}
          </Field>
          <Field
            label="Time zone"
            error={errors.timeZone}
            hint={
              f.timeZone !== here ? (
                <span>
                  This device is on {here}.{" "}
                  <Button variant="link" className="h-auto p-0 text-sm" onClick={() => setF({ ...f, timeZone: here })}>
                    Use it
                  </Button>
                </span>
              ) : (
                "Plans, reminders and deadlines use it."
              )
            }
          >
            {(ids) => (
              <Select {...ids} value={f.timeZone} onChange={(e) => setF({ ...f, timeZone: e.currentTarget.value })}>
                {options.map((z) => (
                  <option key={z} value={z}>
                    {z.replaceAll("_", " ")}
                  </option>
                ))}
              </Select>
            )}
          </Field>
        </div>
        <div className="flex flex-col gap-1.5">
          <span className="text-sm font-medium">Email</span>
          <div className="flex flex-wrap items-center gap-2">
            <span>{user.email}</span>
            {user.emailVerified ? <Badge tone="success">Verified</Badge> : <Badge tone="warning">Not verified</Badge>}
          </div>
          <span className="text-sm text-muted">Changing your email isn't possible yet.</span>
        </div>
        <div className="flex gap-2">
          <Button type="submit" variant="primary" disabled={!dirty || patch.isPending}>
            Save
          </Button>
          {dirty ? (
            <Button onClick={() => setF(blank(user))} disabled={patch.isPending}>
              Cancel
            </Button>
          ) : null}
        </div>
      </form>
      <ConflictDialog
        open={!!conflict}
        onClose={() => setConflict(null)}
        onReload={() => {
          if (conflict) setUser(conflict);
          setConflict(null);
        }}
        onOverwrite={() => {
          if (conflict) send(conflict.version);
          setConflict(null);
        }}
      />
    </Card>
  );
}

function Password() {
  const change = useChangePassword();
  const toast = useToast();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [breached, setBreached] = useState(false);
  const errors = fieldErrors(change.error);

  const send = (acceptBreachedPassword: boolean) =>
    change.mutate(
      { currentPassword: current, newPassword: next, acceptBreachedPassword },
      {
        onSuccess: () => {
          setCurrent("");
          setNext("");
          setBreached(false);
          change.reset();
          toast({ message: "Password changed. Other devices are signed out." });
        },
        onError: (e) => setBreached(fieldCodes(e).password === "breached"),
      },
    );
  const submit = (e: FormEvent) => {
    e.preventDefault();
    send(false);
  };

  return (
    <Card>
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        <SectionLabel>Password</SectionLabel>
        {change.error && !Object.keys(errors).length ? <Banner tone="danger">{messageFor(change.error)}</Banner> : null}
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Current password" error={errors.currentPassword}>
            {(ids) => (
              <Input
                {...ids}
                type="password"
                autoComplete="current-password"
                value={current}
                onChange={(e) => setCurrent(e.currentTarget.value)}
              />
            )}
          </Field>
          <NewPasswordField
            label="New password"
            value={next}
            onChange={(v) => {
              setNext(v);
              setBreached(false);
            }}
            error={errors.password ?? errors.newPassword}
            breached={breached}
            onUseAnyway={() => send(true)}
          />
        </div>
        <p className="m-0 text-sm text-muted">Changing it signs you out on every other device.</p>
        <Button type="submit" variant="primary" className="self-start" disabled={!current || !next || change.isPending}>
          Change password
        </Button>
      </form>
    </Card>
  );
}

function Profile() {
  return (
    <>
      <Details />
      <Password />
    </>
  );
}
