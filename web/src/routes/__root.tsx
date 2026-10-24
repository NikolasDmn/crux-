import type { QueryClient } from "@tanstack/react-query";
import { createRootRouteWithContext, Link, Outlet, useRouter, type ErrorComponentProps } from "@tanstack/react-router";
import type { Auth } from "@/app/auth";
import { Button } from "@/components/ui/button";
import { ApiError } from "@/lib/api";
import { messageFor } from "@/lib/errors";
import { t } from "@/lib/t";

export interface RouterContext {
  auth: Auth;
  queryClient: QueryClient;
}

function NotFound() {
  return (
    <div className="flex flex-col items-start gap-3 py-16">
      <h1 className="m-0 text-xl font-semibold">{t("common.notFound")}</h1>
      <Button asChild>
        <Link to="/">{t("nav.home")}</Link>
      </Button>
    </div>
  );
}

/** Global error boundary (spec 5.7): Reload, and the request id when there is one. */
function ErrorView({ error, reset }: ErrorComponentProps) {
  const router = useRouter();
  if (error instanceof ApiError && error.status === 404) return <NotFound />;
  return (
    <div role="alert" className="flex flex-col items-start gap-3 py-16">
      <h1 className="m-0 text-xl font-semibold">{t("error.title")}</h1>
      <p className="m-0 text-muted">{messageFor(error)}</p>
      {error instanceof ApiError && error.requestId ? (
        <p className="m-0 font-mono text-xs text-muted">{t("common.requestId", { id: error.requestId })}</p>
      ) : null}
      <Button
        onClick={() => {
          reset();
          void router.invalidate();
        }}
      >
        {t("common.reload")}
      </Button>
    </div>
  );
}

export const Route = createRootRouteWithContext<RouterContext>()({
  component: Outlet,
  notFoundComponent: NotFound,
  errorComponent: ErrorView,
});
