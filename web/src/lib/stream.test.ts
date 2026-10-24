import { readEvents } from "./stream";

function body(chunks: string[]): ReadableStream<Uint8Array> {
  const enc = new TextEncoder();
  return new ReadableStream({
    start(c) {
      for (const ch of chunks) c.enqueue(enc.encode(ch));
      c.close();
    },
  });
}

async function all(chunks: string[]) {
  const out = [];
  for await (const e of readEvents(body(chunks))) out.push(e);
  return out;
}

it("reads the chat stream's events, split anywhere", async () => {
  const wire =
    'event: delta\ndata: {"text":"Var(X̄) = "}\n\nevent: delta\ndata: {"text":"σ²/n [p.3]"}\n\n: ping\n\nevent: citation\ndata: {"fileId":"F","page":3}\n\nevent: done\ndata: {"messageId":7,"servedBy":null}\n\n';
  const pieces = [wire.slice(0, 7), wire.slice(7, 40), wire.slice(40, 41), wire.slice(41)];
  expect(await all(pieces)).toEqual([
    { event: "delta", data: { text: "Var(X̄) = " } },
    { event: "delta", data: { text: "σ²/n [p.3]" } },
    { event: "citation", data: { fileId: "F", page: 3 } },
    { event: "done", data: { messageId: 7, servedBy: null } },
  ]);
});

it("handles CRLF, multi-line data, plain text and a missing final blank line", async () => {
  expect(await all(["event: error\r\ndata: line one\r\ndata: line two\r\n\r\n", 'data: {"x":1}'])).toEqual([
    { event: "error", data: "line one\nline two" },
    { event: "message", data: { x: 1 } },
  ]);
});
