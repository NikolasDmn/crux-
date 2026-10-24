import { QueryClient } from "@tanstack/react-query";
import { createRouter } from "@tanstack/react-router";
import { ApiError } from "@/lib/api";
import { routeTree } from "@/routeTree.gen";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      // A 4xx won't fix itself; retry only network errors and 5xx, twice.
      retry: (n, e) => n < 2 && !(e instanceof ApiError && e.status < 500),
      refetchOnWindowFocus: true,
    },
  },
});

export const router = createRouter({
  routeTree,
  context: { auth: undefined!, queryClient },
  defaultPreload: "intent",
  scrollRestoration: true,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
