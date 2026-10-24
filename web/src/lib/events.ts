// User events (`material.open`, `material.page`, `notes.step`, `lecture.focus`, …) are batched and
// sent every 10 s, when the tab is hidden, and on page hide (frontend-spec 2.2). Each carries a
// `clientId`, so a resend after a failure is harmless (the server dedupes).
//
// On page hide the batch goes with `fetch(…, {keepalive: true})` rather than `sendBeacon`: the
// events endpoint needs the bearer token, which a beacon can't carry.

import { ulid } from "ulid";

export interface UserEvent {
  clientId: string;
  type: string;
  occurredAt: string;
  courseId?: string;
  componentId?: string;
  payload?: Record<string, unknown>;
}

export type Send = (events: UserEvent[], opts: { keepalive: boolean }) => Promise<boolean>;

export const FLUSH_MS = 10_000;
/** The server accepts at most this many per batch. */
export const MAX_BATCH = 100;

export class EventQueue {
  private queue: UserEvent[] = [];
  private sending = false;

  constructor(
    private readonly send: Send,
    private readonly now: () => Date = () => new Date(),
  ) {}

  push(type: string, fields: Omit<UserEvent, "clientId" | "type" | "occurredAt"> = {}): UserEvent {
    const e: UserEvent = { clientId: ulid(), type, occurredAt: this.now().toISOString(), ...fields };
    this.queue.push(e);
    return e;
  }

  get size(): number {
    return this.queue.length;
  }

  /** Sends what's queued; failed events go back to the front of the queue. */
  async flush(keepalive = false): Promise<void> {
    if (this.sending || !this.queue.length) return;
    this.sending = true;
    const batch = this.queue.splice(0, MAX_BATCH);
    try {
      const ok = await this.send(batch, { keepalive });
      if (!ok) this.queue.unshift(...batch);
    } catch {
      this.queue.unshift(...batch);
    } finally {
      this.sending = false;
    }
  }

  /** Flushes on a timer, when the tab hides and when the page goes. Returns a stop function. */
  start(win: Window = window): () => void {
    const t = win.setInterval(() => void this.flush(), FLUSH_MS);
    const onHide = () => {
      if (win.document.visibilityState === "hidden") void this.flush(true);
    };
    const onPageHide = () => void this.flush(true);
    win.document.addEventListener("visibilitychange", onHide);
    win.addEventListener("pagehide", onPageHide);
    return () => {
      win.clearInterval(t);
      win.document.removeEventListener("visibilitychange", onHide);
      win.removeEventListener("pagehide", onPageHide);
    };
  }
}
