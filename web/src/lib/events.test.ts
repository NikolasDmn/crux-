import { EventQueue, MAX_BATCH, type UserEvent } from "./events";

const at = () => new Date("2026-10-24T12:00:00Z");

it("batches events with client ids and sends them", async () => {
  const sent: UserEvent[][] = [];
  const q = new EventQueue(async (b) => (sent.push(b), true), at);
  q.push("material.page", { componentId: "L1", payload: { page: 3 } });
  q.push("notes.step", { courseId: "C1", payload: { step: 2 } });
  await q.flush();
  expect(sent).toHaveLength(1);
  expect(sent[0]!.map((e) => e.type)).toEqual(["material.page", "notes.step"]);
  expect(sent[0]![0]!.clientId).toMatch(/^[0-9A-Z]{26}$/);
  expect(sent[0]![0]!.occurredAt).toBe("2026-10-24T12:00:00.000Z");
  expect(q.size).toBe(0);
  await q.flush();
  expect(sent).toHaveLength(1);
});

it("keeps events after a failed or rejected send, oldest first", async () => {
  let ok = false;
  const q = new EventQueue(async () => {
    if (!ok) throw new Error("offline");
    return true;
  }, at);
  const first = q.push("a");
  await q.flush();
  q.push("b");
  expect(q.size).toBe(2);
  ok = true;
  const sent: string[] = [];
  const q2 = new EventQueue(async (b) => (sent.push(...b.map((e) => e.clientId)), false), at);
  q2.push("x");
  await q2.flush();
  expect(q2.size).toBe(1);
  expect(first.clientId).toHaveLength(26);
});

it("sends at most one batch at a time, MAX_BATCH events each", async () => {
  const sizes: number[] = [];
  const q = new EventQueue(async (b) => (sizes.push(b.length), true), at);
  for (let i = 0; i < MAX_BATCH + 5; i++) q.push("lecture.focus");
  await Promise.all([q.flush(), q.flush()]);
  await q.flush();
  expect(sizes).toEqual([MAX_BATCH, 5]);
});

it("flushes with keepalive when the tab hides or the page goes", async () => {
  const calls: boolean[] = [];
  const q = new EventQueue(async (_b, o) => (calls.push(o.keepalive), true), at);
  const stop = q.start(window);
  q.push("a");
  Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
  document.dispatchEvent(new Event("visibilitychange"));
  await Promise.resolve();
  q.push("b");
  window.dispatchEvent(new Event("pagehide"));
  await Promise.resolve();
  stop();
  expect(calls).toEqual([true, true]);
});
