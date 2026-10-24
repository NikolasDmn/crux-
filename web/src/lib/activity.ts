// The app-wide user-event queue (frontend-spec 2.2): `track("material.page", {...})` from anywhere;
// batches go to POST /events with the bearer token, keepalive on page hide.

import { BASE } from "./api";
import { EventQueue, type UserEvent } from "./events";
import { getToken } from "./token";

async function send(events: UserEvent[], { keepalive }: { keepalive: boolean }): Promise<boolean> {
  const token = getToken();
  if (!token) return true; // signed out: nothing to attribute them to
  const r = await fetch(`${BASE}/events`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ events }),
    keepalive,
    credentials: "include",
  });
  // 4xx means the batch itself is bad (per-item rejections still answer 200): don't retry forever.
  return r.ok || (r.status >= 400 && r.status < 500 && r.status !== 401 && r.status !== 429);
}

export const activity = new EventQueue(send);

let started = false;
export function startActivity(): void {
  if (started || typeof window === "undefined") return;
  started = true;
  activity.start(window);
}

export function track(type: string, fields: Omit<UserEvent, "clientId" | "type" | "occurredAt"> = {}): void {
  activity.push(type, fields);
}
