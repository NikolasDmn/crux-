import { BACKOFF_MS, openStream, type StreamDeps } from "./sse";

class FakeSource {
  static all: FakeSource[] = [];
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  listeners: Record<string, ((ev: MessageEvent<string>) => void)[]> = {};
  constructor(readonly url: string) {
    FakeSource.all.push(this);
  }
  addEventListener(name: string, fn: (ev: MessageEvent<string>) => void) {
    (this.listeners[name] ??= []).push(fn);
  }
  emit(name: string, data: string) {
    for (const fn of this.listeners[name] ?? []) fn({ data } as MessageEvent<string>);
  }
  close() {
    this.closed = true;
  }
}

function deps(tickets: (string | Error)[]) {
  const timers: { fn: () => void; ms: number }[] = [];
  let n = 0;
  const d: StreamDeps = {
    ticket: async () => {
      const t = tickets[Math.min(n++, tickets.length - 1)]!;
      if (t instanceof Error) throw t;
      return t;
    },
    open: (url) => new FakeSource(url) as unknown as EventSource,
    setTimer: (fn, ms) => timers.push({ fn, ms }),
    clearTimer: () => {},
  };
  return { d, timers };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  FakeSource.all = [];
});

it("opens with a ticket and parses JSON events", async () => {
  const { d } = deps(["t1"]);
  const got: unknown[] = [];
  openStream("/api/v1/notify/stream", { notification: (x) => got.push(x), ready: (x) => got.push(x) }, d);
  await tick();
  const s = FakeSource.all[0]!;
  expect(s.url).toBe("/api/v1/notify/stream?ticket=t1");
  s.emit("ready", "{}");
  s.emit("notification", '{"id":"n1"}');
  s.emit("notification", "not json");
  expect(got).toEqual([{}, { id: "n1" }, "not json"]);
});

it("reconnects with a fresh ticket and backs off 1, 2, 5, 10 s", async () => {
  const { d, timers } = deps(["t1", "t2", "t3", "t4", "t5", "t6"]);
  openStream("/api/v1/files/f/events?x=1", {}, d);
  await tick();
  for (let i = 0; i < 5; i++) {
    FakeSource.all.at(-1)!.onerror?.();
    timers.at(-1)!.fn();
    await tick();
  }
  expect(timers.map((t) => t.ms)).toEqual([...BACKOFF_MS, 10000]);
  expect(FakeSource.all.map((s) => s.url.split("ticket=")[1])).toEqual(["t1", "t2", "t3", "t4", "t5", "t6"]);
  expect(FakeSource.all[0]!.url).toContain("?x=1&ticket=");
});

it("resets the back-off once a connection opens, and retries a failed ticket", async () => {
  const { d, timers } = deps([new Error("down"), "t1", "t2"]);
  openStream("/s", {}, d);
  await tick();
  expect(timers[0]!.ms).toBe(1000);
  timers[0]!.fn();
  await tick();
  const s = FakeSource.all[0]!;
  s.onopen?.();
  s.onerror?.();
  expect(timers[1]!.ms).toBe(1000);
});

it("stops on close", async () => {
  const { d, timers } = deps(["t1", "t2"]);
  const stream = openStream("/s", {}, d);
  await tick();
  stream.close();
  expect(FakeSource.all[0]!.closed).toBe(true);
  FakeSource.all[0]!.onerror?.();
  expect(timers).toHaveLength(0);
});
