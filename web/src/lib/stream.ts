// Server-sent events read from a fetch response body (frontend-spec 2.3): the AI chat reply is the
// body of a POST, which EventSource can't send.

export interface SseEvent {
  event: string;
  data: unknown;
}

/** Yields each event of a `text/event-stream` body; `data` is parsed JSON when it is JSON. */
export async function* readEvents(body: ReadableStream<Uint8Array>): AsyncGenerator<SseEvent> {
  const reader = body.pipeThrough(new TextDecoderStream()).getReader();
  let buf = "";
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (value) buf += value;
      // Events end with a blank line; tolerate \r\n.
      buf = buf.replace(/\r\n/g, "\n");
      let end: number;
      while ((end = buf.indexOf("\n\n")) >= 0) {
        const block = buf.slice(0, end);
        buf = buf.slice(end + 2);
        const ev = parse(block);
        if (ev) yield ev;
      }
      if (done) break;
    }
    const last = parse(buf.trim());
    if (last) yield last;
  } finally {
    reader.releaseLock();
  }
}

function parse(block: string): SseEvent | null {
  if (!block) return null;
  let event = "message";
  const data: string[] = [];
  for (const line of block.split("\n")) {
    if (line.startsWith(":")) continue; // comment / keep-alive
    const i = line.indexOf(":");
    const field = i < 0 ? line : line.slice(0, i);
    const value = i < 0 ? "" : line.slice(i + 1).replace(/^ /, "");
    if (field === "event") event = value;
    else if (field === "data") data.push(value);
  }
  if (!data.length) return null;
  const raw = data.join("\n");
  try {
    return { event, data: JSON.parse(raw) };
  } catch {
    return { event, data: raw };
  }
}
