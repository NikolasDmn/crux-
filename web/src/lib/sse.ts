// EventSource streams with a single-use ticket (frontend-spec 2.3): the inbox, file processing and
// import progress. A dropped stream reconnects with a fresh ticket after 1, 2, 5, then 10 s.

export const BACKOFF_MS = [1000, 2000, 5000, 10000] as const;

export type Handlers = Record<string, (data: unknown) => void>;

export interface StreamDeps {
  ticket: () => Promise<string>;
  open: (url: string) => EventSource;
  setTimer: (fn: () => void, ms: number) => unknown;
  clearTimer: (t: unknown) => void;
}

export interface Stream {
  close(): void;
}

function withTicket(url: string, ticket: string): string {
  return `${url}${url.includes("?") ? "&" : "?"}ticket=${encodeURIComponent(ticket)}`;
}

/** Opens `url` and keeps it open until `close()`. Each handler gets the event's JSON data. */
export function openStream(url: string, handlers: Handlers, deps: StreamDeps): Stream {
  let source: EventSource | null = null;
  let timer: unknown = null;
  let attempt = 0;
  let closed = false;

  const retry = () => {
    if (closed) return;
    const ms = BACKOFF_MS[Math.min(attempt, BACKOFF_MS.length - 1)]!;
    attempt += 1;
    timer = deps.setTimer(connect, ms);
  };

  async function connect() {
    if (closed) return;
    let ticket: string;
    try {
      ticket = await deps.ticket();
    } catch {
      retry();
      return;
    }
    if (closed) return;
    const s = deps.open(withTicket(url, ticket));
    source = s;
    s.onopen = () => {
      attempt = 0;
    };
    s.onerror = () => {
      s.close();
      if (source === s) source = null;
      retry();
    };
    for (const [name, fn] of Object.entries(handlers)) {
      s.addEventListener(name, (ev) => {
        const raw = (ev as MessageEvent<string>).data;
        let data: unknown = raw;
        try {
          data = JSON.parse(raw);
        } catch {
          /* plain text event */
        }
        fn(data);
      });
    }
  }

  void connect();
  return {
    close() {
      closed = true;
      if (timer !== null) deps.clearTimer(timer);
      source?.close();
      source = null;
    },
  };
}
