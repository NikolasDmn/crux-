import { useMutation } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect } from "react";
import { useAuth } from "@/app/auth";
import { AuthLayout } from "@/components/shell/auth-layout";
import { Button } from "@/components/ui/button";
import { confirmEmail, me } from "@/features/auth/api";
import { t } from "@/lib/t";

export const Route = createFileRoute("/verify")({
  validateSearch: (s: Record<string, unknown>): { token?: string } =>
    typeof s.token === "string" ? { token: s.token } : {},
  component: Verify,
});

function Verify() {
  const { token } = Route.useSearch();
  const { user, setUser } = useAuth();
  const confirm = useMutation({
    mutationFn: async () => {
      await confirmEmail(token ?? "");
      if (user) setUser(await me());
    },
  });
  const { mutate } = confirm;
  useEffect(() => {
    if (token) mutate();
  }, [token, mutate]);

  const failed = !token || confirm.isError;
  return (
    <AuthLayout title={t("auth.verify.title")}>
      <p className="m-0" role="status">
        {failed ? t("auth.verify.failed") : confirm.isSuccess ? t("auth.verify.done") : `${t("common.loading")}…`}
      </p>
      <Button asChild variant={confirm.isSuccess ? "primary" : "secondary"}>
        <Link to={user ? "/" : "/login"}>{user ? t("nav.home") : t("auth.backToLogin")}</Link>
      </Button>
    </AuthLayout>
  );
}
