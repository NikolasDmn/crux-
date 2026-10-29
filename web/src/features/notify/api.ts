import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { api, BASE, unwrap, type Schemas } from "@/lib/api";
import { openStream } from "@/lib/sse";

export type InboxItem = Schemas["InboxItem"];
export type KindPreference = Schemas["KindPreference"];
export type Preferences = Schemas["PreferencesView"];
export type PreferenceChange = Schemas["PreferenceChange"];

export const keys = {
  inbox: ["notify", "inbox"] as const,
  unread: ["notify", "unread"] as const,
  prefs: ["notify", "preferences"] as const,
};

const PAGE = 30;

export function useUnreadCount() {
  return useQuery({
    queryKey: keys.unread,
    queryFn: async () =>
      (await unwrap(api.GET("/notify/inbox", { params: { query: { unread: true, limit: 1 } } }))).unread,
  });
}

/** The inbox, newest first, a cursor page at a time (frontend-spec 4, Inbox). */
export function useInbox(unreadOnly: boolean) {
  return useInfiniteQuery({
    queryKey: [...keys.inbox, unreadOnly],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) =>
      unwrap(
        api.GET("/notify/inbox", {
          params: { query: { limit: PAGE, ...(unreadOnly ? { unread: true } : {}), ...(pageParam ? { cursor: pageParam } : {}) } },
        }),
      ),
    getNextPageParam: (last) => last.nextCursor ?? null,
  });
}

function invalidateInbox(qc: ReturnType<typeof useQueryClient>) {
  void qc.invalidateQueries({ queryKey: keys.inbox });
  void qc.invalidateQueries({ queryKey: keys.unread });
}

export function useMarkRead() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.POST("/notify/{notification_id}/read", { params: { path: { notification_id: id } } })),
    onSuccess: () => invalidateInbox(qc),
  });
}

export function useMarkAllRead() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.POST("/notify/read-all")),
    onSuccess: () => invalidateInbox(qc),
  });
}

export function usePreferences() {
  return useQuery({ queryKey: keys.prefs, queryFn: () => unwrap(api.GET("/notify/preferences")) });
}

export function useSetPreferences() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (preferences: PreferenceChange[]) =>
      unwrap(api.PUT("/notify/preferences", { body: { preferences } })),
    onSuccess: (view) => qc.setQueryData(keys.prefs, view),
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
