import { createFileRoute, Link, redirect, useNavigate } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";
import { useAuth } from "@/app/auth";
import { AuthLayout } from "@/components/shell/auth-layout";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Field, Input } from "@/components/ui/field";
import { signup } from "@/features/auth/api";
import { NewPasswordField } from "@/features/auth/components/password-field";
import { fieldCodes, fieldErrors, messageFor } from "@/lib/errors";
import { browserZone } from "@/lib/time";
import { t } from "@/lib/t";

// Signup only with an invite (frontend-spec 2.1); a course-share invite also accepts the share.
export const Route = createFileRoute("/signup")({
  validateSearch: (s: Record<string, unknown>): { invite?: string } =>
    typeof s.invite === "string" ? { invite: s.invite } : {},
  beforeLoad: ({ context }) => {
    if (context.auth.user) throw redirect({ to: "/" });
  },
  component: Signup,
});

function Signup() {
  const { invite } = Route.useSearch();
  const { setUser } = useAuth();
  const navigate = useNavigate();
  const [f, setF] = useState({
    inviteCode: invite ?? "",
    email: "",
    username: "",
    firstName: "",
    lastName: "",
    password: "",
  });
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [breached, setBreached] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const set = (k: keyof typeof f) => (v: string) => {
    setF((x) => ({ ...x, [k]: v }));
    if (k === "password") setBreached(false);
  };

  const send = async (acceptBreachedPassword: boolean) => {
    setBusy(true);
    setError(null);
    try {
      setUser(await signup({ ...f, timeZone: browserZone(), acceptBreachedPassword }));
      void navigate({ to: "/" });
    } catch (err) {
      const fields = fieldErrors(err);
      setErrors(fields);
      setBreached(fieldCodes(err).password === "breached");
      if (!Object.keys(fields).length) setError(messageFor(err));
    } finally {
      setBusy(false);
    }
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    void send(false);
  };
  const text = (k: Exclude<keyof typeof f, "password">, label: string, auto: string, type = "text") => (
    <Field label={label} error={errors[k]}>
      {(ids) => (
        <Input
          {...ids}
          type={type}
          autoComplete={auto}
          required
          value={f[k]}
          onChange={(e) => set(k)(e.currentTarget.value)}
        />
      )}
    </Field>
  );

  return (
    <AuthLayout
      title={t("auth.signup.title")}
      footer={
        <Link to="/login" className="text-text underline-offset-4 hover:underline">
          {t("auth.signup.haveAccount")}
        </Link>
      }
    >
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        {error ? <Banner tone="danger">{error}</Banner> : null}
        {text("inviteCode", t("auth.signup.invite"), "off")}
        <div className="grid grid-cols-2 gap-3">
          {text("firstName", t("auth.signup.firstName"), "given-name")}
          {text("lastName", t("auth.signup.lastName"), "family-name")}
        </div>
        {text("email", t("auth.signup.email"), "email", "email")}
        {text("username", t("auth.signup.username"), "username")}
        <NewPasswordField
          value={f.password}
          onChange={set("password")}
          error={errors.password}
          breached={breached}
          onUseAnyway={() => void send(true)}
        />
        <Button type="submit" variant="primary" disabled={busy}>
          {t("auth.signup.submit")}
        </Button>
      </form>
    </AuthLayout>
  );
}
