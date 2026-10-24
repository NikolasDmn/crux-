import { createFileRoute, Link } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";
import { AuthLayout } from "@/components/shell/auth-layout";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { resetPassword } from "@/features/auth/api";
import { NewPasswordField } from "@/features/auth/components/password-field";
import { fieldCodes, fieldErrors, messageFor } from "@/lib/errors";
import { t } from "@/lib/t";

export const Route = createFileRoute("/reset")({
  validateSearch: (s: Record<string, unknown>): { token?: string } =>
    typeof s.token === "string" ? { token: s.token } : {},
  component: Reset,
});

function Reset() {
  const { token } = Route.useSearch();
  const [password, setPassword] = useState("");
  const [breached, setBreached] = useState(false);
  const [fieldError, setFieldError] = useState<string | undefined>();
  const [error, setError] = useState<string | null>(token ? null : t("auth.verify.failed"));
  const [done, setDone] = useState(false);
  const [busy, setBusy] = useState(false);

  const send = async (acceptBreachedPassword: boolean) => {
    if (!token) return;
    setBusy(true);
    setError(null);
    try {
      await resetPassword({ token, password, acceptBreachedPassword });
      setDone(true);
    } catch (err) {
      const fields = fieldErrors(err);
      setBreached(fieldCodes(err).password === "breached");
      setFieldError(fields.password);
      if (!fields.password) setError(fields.token ? t("auth.verify.failed") : messageFor(err));
    } finally {
      setBusy(false);
    }
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    void send(false);
  };

  return (
    <AuthLayout
      title={t("auth.reset.title")}
      footer={
        <Link to="/login" className="text-text underline-offset-4 hover:underline">
          {t("auth.backToLogin")}
        </Link>
      }
    >
      {done ? (
        <p className="m-0">{t("auth.reset.done")}</p>
      ) : (
        <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
          {error ? <Banner tone="danger">{error}</Banner> : null}
          <NewPasswordField
            value={password}
            onChange={(v) => {
              setPassword(v);
              setBreached(false);
            }}
            error={fieldError}
            breached={breached}
            onUseAnyway={() => void send(true)}
          />
          <Button type="submit" variant="primary" disabled={busy || !token || !password}>
            {t("auth.reset.submit")}
          </Button>
        </form>
      )}
    </AuthLayout>
  );
}
