import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { api, BASE, unwrap } from "@/lib/api";
import { openStream } from "@/lib/sse";

export const keys = { inbox: ["notify", "inbox"] as const, unread: ["notify", "unread"] as const };

export function useUnreadCount() {
  return useQuery({
    queryKey: keys.unread,
    queryFn: async () =>
      (await unwrap(api.GET("/notify/inbox", { params: { query: { unread: true, limit: 1 } } }))).unread,
  });
}

/** A single-use ticket for an EventSource stream (spec 2.3). */
export async function sseTicket(): Promise<string> {
  return (await unwrap(api.POST("/sse-ticket"))).ticket;
}

/** Keeps the unread count and inbox live over the notification stream while signed in. */
export function useNotifyStream(enabled: boolean) {
  const qc = useQueryClient();
  useEffect(() => {
    if (!enabled) return;
    const s = openStream(
      `${BASE}/notify/stream`,
      {
        notification: () => {
          void qc.invalidateQueries({ queryKey: keys.unread });
          void qc.invalidateQueries({ queryKey: keys.inbox });
        },
      },
      {
        ticket: sseTicket,
        open: (url) => new EventSource(url),
        setTimer: (fn, ms) => window.setTimeout(fn, ms),
        clearTimer: (t) => window.clearTimeout(t as number),
      },
    );
    return () => s.close();
  }, [enabled, qc]);
}
