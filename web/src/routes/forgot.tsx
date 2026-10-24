import { createFileRoute, Link } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";
import { AuthLayout } from "@/components/shell/auth-layout";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Field, Input } from "@/components/ui/field";
import { forgot } from "@/features/auth/api";
import { messageFor } from "@/lib/errors";
import { t } from "@/lib/t";

export const Route = createFileRoute("/forgot")({ component: Forgot });

function Forgot() {
  const [email, setEmail] = useState("");
  const [state, setState] = useState<"idle" | "busy" | "sent">("idle");
  const [error, setError] = useState<string | null>(null);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setState("busy");
    setError(null);
    try {
      await forgot(email);
      setState("sent");
    } catch (err) {
      setError(messageFor(err));
      setState("idle");
    }
  };
  return (
    <AuthLayout
      title={t("auth.forgot.title")}
      footer={
        <Link to="/login" className="text-text underline-offset-4 hover:underline">
          {t("auth.backToLogin")}
        </Link>
      }
    >
      {state === "sent" ? (
        <p className="m-0">{t("auth.forgot.sent")}</p>
      ) : (
        <form className="flex flex-col gap-4" onSubmit={submit} noValidate>
          <p className="m-0 text-muted">{t("auth.forgot.body")}</p>
          {error ? <Banner tone="danger">{error}</Banner> : null}
          <Field label={t("auth.signup.email")}>
            {(ids) => (
              <Input
                {...ids}
                type="email"
                autoComplete="email"
                required
                autoFocus
                value={email}
                onChange={(e) => setEmail(e.currentTarget.value)}
              />
            )}
          </Field>
          <Button type="submit" variant="primary" disabled={state === "busy" || !email}>
            {t("auth.forgot.submit")}
          </Button>
        </form>
      )}
    </AuthLayout>
  );
}
