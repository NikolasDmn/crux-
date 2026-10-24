import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { useEffect } from "react";
import { AuthProvider, type Auth } from "@/app/auth";
import { queryClient, router } from "@/app/router";
import { Mark } from "@/components/ui/misc";
import { TooltipProvider } from "@/components/ui/overlays";
import { ToastProvider } from "@/components/ui/toast";
import { setSignedOutHandler } from "@/lib/api";

function Routed({ auth }: { auth: Auth }) {
  // A route guard reads `auth` from the context; re-run them when the session changes.
  useEffect(() => {
    void router.invalidate();
  }, [auth.user]);
  useEffect(() => {
    setSignedOutHandler(() => {
      auth.setUser(null);
      queryClient.clear();
      const here = router.state.location.href;
      void router.navigate({ to: "/login", search: here.startsWith("/login") ? {} : { redirect: here } });
    });
  }, [auth]);
  return <RouterProvider router={router} context={{ auth }} />;
}

function Splash() {
  return (
    <div className="flex min-h-full items-center justify-center" aria-busy="true" aria-label="Loading Crux">
      <Mark size={40} className="animate-pulse" />
    </div>
  );
}

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <ToastProvider>
          <AuthProvider splash={<Splash />}>{(auth) => <Routed auth={auth} />}</AuthProvider>
        </ToastProvider>
      </TooltipProvider>
    </QueryClientProvider>
  );
}
