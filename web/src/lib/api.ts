// The API client (frontend-spec 2.2): generated types, bearer token from memory, a silent refresh on
// 401 (once, shared by concurrent requests), and typed errors. Only `features/*/api.ts` call it.

import createClient, { type Middleware } from "openapi-fetch";
import { ulid } from "ulid";
import type { components, paths } from "./api.gen";
import { getToken, setToken } from "./token";

export type Schemas = components["schemas"];
export const BASE = "/api/v1";

export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly details: Record<string, unknown> | null;
  readonly requestId: string | null;

  constructor(
    status: number,
    code: string,
    message: string,
    details: Record<string, unknown> | null,
    requestId: string | null,
  ) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.details = details;
    this.requestId = requestId;
  }

  /** From a failed response body (`{"error": {code, message, details, requestId}}`, spec 3.1). */
  static from(status: number, body: unknown): ApiError {
    const e = (body as { error?: Schemas["ErrorDetail"] } | null)?.error;
    if (e && typeof e.code === "string") {
      return new ApiError(
        status,
        e.code,
        e.message,
        (e.details as Record<string, unknown> | null) ?? null,
        e.requestId ?? null,
      );
    }
    const code = status === 404 ? "not_found" : status >= 500 ? "internal" : "unknown";
    return new ApiError(status, code, `Request failed (${status}).`, null, null);
  }
}

// ---------- refresh ----------

let refreshing: Promise<Schemas["SessionResponse"] | null> | null = null;
let onSignedOut: () => void = () => {};

/** Called once at start-up: where to go when the session can't be refreshed. */
export function setSignedOutHandler(fn: () => void): void {
  onSignedOut = fn;
}

/** `POST /auth/refresh` with the HttpOnly cookie. Concurrent callers share one request. */
/** The new session (its user included), or null when there is none. */
export function refresh(): Promise<Schemas["SessionResponse"] | null> {
  refreshing ??= (async () => {
    try {
      const r = await fetch(`${BASE}/auth/refresh`, { method: "POST", credentials: "include" });
      if (!r.ok) {
        setToken(null);
        return null;
      }
      const body = (await r.json()) as Schemas["SessionResponse"];
      setToken(body.accessToken);
      return body;
    } catch {
      return null;
    } finally {
      queueMicrotask(() => {
        refreshing = null;
      });
    }
  })();
  return refreshing;
}

const NO_REFRESH = ["/auth/login", "/auth/signup", "/auth/refresh", "/auth/logout", "/auth/password/"];
const copies = new WeakMap<Request, Request>();

const auth: Middleware = {
  onRequest({ request }) {
    const token = getToken();
    if (token) request.headers.set("authorization", `Bearer ${token}`);
    copies.set(request, request.clone());
    return request;
  },
  async onResponse({ request, response }) {
    const path = new URL(request.url, location.origin).pathname.replace(BASE, "");
    if (response.status !== 401 || NO_REFRESH.some((p) => path.startsWith(p))) return response;
    if (!(await refresh())) {
      onSignedOut();
      return response;
    }
    // Retry once with the fresh token.
    const again = copies.get(request) ?? request;
    const headers = new Headers(again.headers);
    headers.set("authorization", `Bearer ${getToken()}`);
    return fetch(new Request(again, { headers }));
  },
};

export const api = createClient<paths>({ baseUrl: BASE, credentials: "include" });
api.use(auth);

/** The data of an openapi-fetch result, or throws its {@link ApiError}. */
export async function unwrap<T>(p: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<T> {
  const { data, error, response } = await p;
  if (!response.ok) throw ApiError.from(response.status, error);
  return data as T;
}

/** A fresh `Idempotency-Key` (made when a create form opens, spec 2.2). */
export function idempotencyKey(): string {
  return ulid();
}

/** `If-Match` for an edit of a resource at `version` (spec 2.2). */
export function ifMatch(version: number): { "If-Match": number } {
  return { "If-Match": version };
}
