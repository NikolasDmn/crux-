import { createFileRoute, Link, redirect, useNavigate } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";
import { useAuth } from "@/app/auth";
import { AuthLayout } from "@/components/shell/auth-layout";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Field, Input } from "@/components/ui/field";
import { login } from "@/features/auth/api";
import { safeRedirect } from "@/features/auth/lib/redirect";
import { messageFor } from "@/lib/errors";
import { t } from "@/lib/t";

export const Route = createFileRoute("/login")({
  validateSearch: (s: Record<string, unknown>): { redirect?: string } =>
    typeof s.redirect === "string" ? { redirect: s.redirect } : {},
  beforeLoad: ({ context, search }) => {
    if (context.auth.user) throw redirect({ to: safeRedirect(search.redirect) });
  },
  component: Login,
});

function Login() {
  const { redirect: target } = Route.useSearch();
  const { setUser } = useAuth();
  const navigate = useNavigate();
  const [form, setForm] = useState({ login: "", password: "" });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      setUser(await login(form));
      void navigate({ to: safeRedirect(target) });
    } catch (err) {
      setError(messageFor(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <AuthLayout
      title={t("auth.login.title")}
      footer={
        <Link to="/signup" className="text-text underline-offset-4 hover:underline">
          {t("auth.login.invite")}
        </Link>
      }
    >
      <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
        {error ? <Banner tone="danger">{error}</Banner> : null}
        <Field label={t("auth.login.login")}>
          {(ids) => (
            <Input
              {...ids}
              name="login"
              autoComplete="username"
              required
              autoFocus
              value={form.login}
              onChange={(e) => setForm({ ...form, login: e.currentTarget.value })}
            />
          )}
        </Field>
        <Field label={t("auth.login.password")}>
          {(ids) => (
            <Input
              {...ids}
              name="password"
              type="password"
              autoComplete="current-password"
              required
              value={form.password}
              onChange={(e) => setForm({ ...form, password: e.currentTarget.value })}
            />
          )}
        </Field>
        <Button type="submit" variant="primary" disabled={busy || !form.login || !form.password}>
          {t("auth.login.submit")}
        </Button>
        <Link to="/forgot" className="self-start text-sm text-muted underline-offset-4 hover:text-text hover:underline">
          {t("auth.login.forgot")}
        </Link>
      </form>
    </AuthLayout>
  );
}
