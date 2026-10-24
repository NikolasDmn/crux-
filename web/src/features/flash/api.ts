import { useQuery } from "@tanstack/react-query";
import { api, unwrap } from "@/lib/api";

export const keys = { queue: (courseId?: string) => ["flash", "queue", courseId ?? "all"] as const };

/** Cards due now, across courses (sidebar count). */
export function useDueCount() {
  return useQuery({
    queryKey: keys.queue(),
    queryFn: async () => (await unwrap(api.GET("/flash/queue"))).dueCount,
    staleTime: 60_000,
  });
}
