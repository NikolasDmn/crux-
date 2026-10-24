import { createFileRoute, redirect } from "@tanstack/react-router";
import { AppShell } from "@/components/shell/app-shell";

// Every signed-in route sits under this layout; signed-out visitors go to /login (frontend-spec 2.1).
export const Route = createFileRoute("/_app")({
  beforeLoad: ({ context, location }) => {
    if (!context.auth.user) {
      throw redirect({ to: "/login", search: location.href === "/" ? {} : { redirect: location.href } });
    }
  },
  component: AppShell,
});
