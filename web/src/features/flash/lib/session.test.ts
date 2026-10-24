import { describe, expect, it, vi } from "vitest";
import { BATCH, interval, ReviewBuffer, summarize, type Rated, type Review } from "./session";

const store = () => {
  const m = new Map<string, string>();
  return {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => void m.set(k, v),
    removeItem: (k: string) => void m.delete(k),
  } as unknown as Storage;
};

const r = (n: number): Review => ({
  clientId: `c${n}`,
  flashcardId: `f${n}`,
  rating: 3,
  reviewedAt: "2026-10-24T10:00:00Z",
});

describe("review buffer", () => {
  it("sends a full batch at once and keeps nothing after success", async () => {
    const send = vi.fn(async (b: Review[]) => b.map((x) => ({ clientId: x.clientId, status: "applied" })));
    const s = store();
    const buf = new ReviewBuffer(send, undefined, s);
    for (let i = 0; i < BATCH; i++) buf.push(r(i));
    await vi.waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    await vi.waitFor(() => expect(buf.size).toBe(0));
    expect(s.getItem("crux.flash.pending")).toBeNull();
  });

  it("keeps a failed batch, in storage too, and a new buffer picks it up", async () => {
    const s = store();
    const failing = new ReviewBuffer(async () => null, undefined, s);
    failing.push(r(1));
    expect(await failing.flush()).toBe(false);
    expect(failing.size).toBe(1);
    const send = vi.fn(async (b: Review[]) => b.map((x) => ({ clientId: x.clientId, status: "duplicate" })));
    const next = new ReviewBuffer(send, undefined, s);
    expect(next.size).toBe(1);
    expect(await next.drain()).toBe(true);
    expect(send.mock.calls[0]![0]).toEqual([r(1)]);
  });

  it("calls for a refetch when the server says stale", async () => {
    const onStale = vi.fn();
    const buf = new ReviewBuffer(
      async (b) => b.map((x) => ({ clientId: x.clientId, status: "stale" })),
      onStale,
      store(),
    );
    buf.push(r(1));
    await buf.flush();
    expect(onStale).toHaveBeenCalledOnce();
  });

  it("drains in batches and stops on a failure", async () => {
    let calls = 0;
    const buf = new ReviewBuffer(
      async (b) => (++calls === 2 ? null : b.map((x) => ({ clientId: x.clientId, status: "applied" }))),
      undefined,
      store(),
    );
    for (let i = 0; i < BATCH + 5; i++) buf.push(r(i));
    await vi.waitFor(() => expect(buf.size).toBe(5));
    expect(await buf.drain()).toBe(false);
    expect(await buf.drain()).toBe(true);
  });
});

describe("intervals", () => {
  it("reads like the buttons", () => {
    expect(interval(30)).toBe("1 min");
    expect(interval(600)).toBe("10 min");
    expect(interval(5 * 3600)).toBe("5 h");
    expect(interval(3 * 86400)).toBe("3 d");
    expect(interval(65 * 86400)).toBe("2 mo");
    expect(interval(400 * 86400)).toBe("1.1 y");
  });
});

describe("summary", () => {
  it("counts ratings, lapses and sure-but-wrong", () => {
    const rated: Rated[] = [
      { cardId: "a", code: "BSA001", rating: "Again", confidence: "Sure", stateBefore: "Review" },
      { cardId: "b", code: "BSA002", rating: "Again", confidence: null, stateBefore: "New" },
      { cardId: "c", code: "BSA003", rating: "Good", confidence: "Sure", stateBefore: "Review" },
    ];
    const s = summarize(rated);
    expect(s.count).toBe(3);
    expect(s.byRating).toEqual({ Again: 2, Hard: 0, Good: 1, Easy: 0 });
    expect(s.lapsed.map((x) => x.code)).toEqual(["BSA001"]);
    expect(s.sureWrong.map((x) => x.code)).toEqual(["BSA001"]);
  });
});
