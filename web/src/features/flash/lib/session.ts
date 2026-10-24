// A flashcard session (frontend-spec 5.4): ratings advance the card at once and post in batches
// (20, or every 15 s, and on leaving), each with a clientId so a resend is harmless. Unsent ratings
// wait in sessionStorage. A `stale` result means the queue moved on elsewhere: refetch it.

import type { Rating } from "@/lib/keyboard";

export interface Review {
  clientId: string;
  flashcardId: string;
  rating: number;
  reviewedAt: string;
  durationMs?: number | null;
  confidence?: string | null;
}

export type ReviewStatus = "applied" | "stale" | "duplicate" | "unknown";
export type Send = (
  reviews: Review[],
  opts: { keepalive: boolean },
) => Promise<{ clientId: string; status: string }[] | null>;

export const BATCH = 20;
export const FLUSH_MS = 15_000;
const KEY = "crux.flash.pending";

export class ReviewBuffer {
  private queue: Review[];
  private sending = false;

  constructor(
    private readonly send: Send,
    private readonly onStale: () => void = () => undefined,
    private readonly store: Storage | undefined = safeSession(),
  ) {
    this.queue = load(this.store);
  }

  get size(): number {
    return this.queue.length;
  }

  /** Queues a rating; sends at once when a batch is full. */
  push(r: Review): void {
    this.queue.push(r);
    save(this.store, this.queue);
    if (this.queue.length >= BATCH) void this.flush();
  }

  /** Sends up to one batch; what fails stays queued (and stored) for the next try. */
  async flush(keepalive = false): Promise<boolean> {
    if (this.sending || !this.queue.length) return true;
    this.sending = true;
    const batch = this.queue.slice(0, BATCH);
    try {
      const results = await this.send(batch, { keepalive });
      if (!results) return false;
      const sent = new Set(batch.map((r) => r.clientId));
      this.queue = this.queue.filter((r) => !sent.has(r.clientId));
      save(this.store, this.queue);
      if (results.some((r) => r.status === "stale")) this.onStale();
      return true;
    } catch {
      return false;
    } finally {
      this.sending = false;
    }
  }

  /** Everything still queued, batch after batch (end of a session). */
  async drain(): Promise<boolean> {
    while (this.queue.length) {
      if (!(await this.flush())) return false;
    }
    return true;
  }

  /** Timer, hidden tab and page hide. Returns a stop function. */
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

function safeSession(): Storage | undefined {
  try {
    return window.sessionStorage;
  } catch {
    return undefined;
  }
}

function load(store: Storage | undefined): Review[] {
  try {
    const v: unknown = JSON.parse(store?.getItem(KEY) ?? "[]");
    return Array.isArray(v) ? (v as Review[]) : [];
  } catch {
    return [];
  }
}

function save(store: Storage | undefined, q: Review[]): void {
  try {
    if (q.length) store?.setItem(KEY, JSON.stringify(q));
    else store?.removeItem(KEY);
  } catch {
    // Storage blocked: ratings still go out with the next flush.
  }
}

export const RATING_NUMBER: Record<Rating, number> = { Again: 1, Hard: 2, Good: 3, Easy: 4 };

/** "1 min", "10 min", "5 h", "3 d", "2 mo", "1.2 y" for a rating button. */
export function interval(seconds: number): string {
  const m = seconds / 60;
  if (m < 60) return `${Math.max(1, Math.round(m))} min`;
  const h = m / 60;
  if (h < 24) return `${Math.round(h)} h`;
  const d = h / 24;
  if (d < 30) return `${Math.round(d)} d`;
  if (d < 365) return `${Math.round(d / 30)} mo`;
  return `${(d / 365).toFixed(1)} y`;
}

export interface Rated {
  cardId: string;
  /** The review's clientId: a slip from it points back at this rating. */
  clientId?: string;
  courseId?: string;
  code: string;
  rating: Rating;
  confidence: string | null;
  /** The card's state before this rating (Review + Again = a lapse). */
  stateBefore: string;
}

export interface Summary {
  count: number;
  byRating: Record<Rating, number>;
  /** Cards you knew (in Review) and forgot this time. */
  lapsed: Rated[];
  /** Rated Again while sure: the misconceptions worth a slip. */
  sureWrong: Rated[];
}

export function summarize(rated: Rated[]): Summary {
  const byRating: Record<Rating, number> = { Again: 0, Hard: 0, Good: 0, Easy: 0 };
  for (const r of rated) byRating[r.rating]++;
  return {
    count: rated.length,
    byRating,
    lapsed: rated.filter((r) => r.rating === "Again" && r.stateBefore === "Review"),
    sureWrong: rated.filter((r) => r.rating === "Again" && r.confidence === "Sure"),
  };
}
