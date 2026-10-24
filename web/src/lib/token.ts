// The access token lives in memory only, never in storage (frontend-spec 2.1). The refresh cookie is
// HttpOnly and handled by the browser.

type Listener = (token: string | null) => void;

let current: string | null = null;
const listeners = new Set<Listener>();

export function getToken(): string | null {
  return current;
}

export function setToken(token: string | null): void {
  current = token;
  for (const l of listeners) l(token);
}

export function onTokenChange(l: Listener): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}
