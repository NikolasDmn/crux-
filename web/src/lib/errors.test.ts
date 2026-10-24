import { ApiError } from "./api";
import { conflictCurrent, fieldCodes, fieldErrors, messageFor, retryAfter } from "./errors";

const err = (status: number, code: string, details: Record<string, unknown> | null = null) =>
  ApiError.from(status, { error: { code, message: "server text", details, requestId: "r1" } });

describe("ApiError.from", () => {
  it("reads the error envelope", () => {
    const e = err(422, "validation_failed", { fields: [] });
    expect([e.status, e.code, e.requestId]).toEqual([422, "validation_failed", "r1"]);
  });
  it("falls back when the body isn't an envelope", () => {
    expect(ApiError.from(404, null).code).toBe("not_found");
    expect(ApiError.from(502, "<html>").code).toBe("internal");
    expect(ApiError.from(418, {}).code).toBe("unknown");
  });
});

describe("messageFor", () => {
  it("never reveals whether a resource exists", () => {
    expect(messageFor(err(404, "not_found"))).toBe("Not found, or you don't have access.");
  });
  it("counts down rate limits", () => {
    const e = err(429, "rate_limited", { retryAfterSecs: 42 });
    expect(retryAfter(e)).toBe(42);
    expect(messageFor(e)).toBe("Too many attempts. Try again in 42 s.");
    expect(messageFor(err(429, "rate_limited"))).toMatch(/Try again in a moment/);
  });
  it("uses the server message for unknown codes and a network line for fetch failures", () => {
    expect(messageFor(err(400, "brand_new_code"))).toBe("server text");
    expect(messageFor(new TypeError("Failed to fetch"))).toMatch(/connection/);
    expect(messageFor("??")).toMatch(/our side/);
  });
});

describe("field errors", () => {
  const e = err(422, "validation_failed", {
    fields: [
      { field: "password", code: "breached", message: "Seen in a breach." },
      { field: "password", code: "too_short", message: "Too short." },
      { field: "extra.studyEnd", code: "invalid", message: "End at least an hour after the start." },
    ],
  });
  it("maps the first message per field, dotted fields kept", () => {
    expect(fieldErrors(e)).toEqual({
      password: "Seen in a breach.",
      "extra.studyEnd": "End at least an hour after the start.",
    });
    expect(fieldCodes(e).password).toBe("breached");
  });
  it("is empty for other errors", () => {
    expect(fieldErrors(err(404, "not_found"))).toEqual({});
    expect(fieldCodes(new Error("x"))).toEqual({});
  });
  it("returns the current resource from a conflict", () => {
    expect(conflictCurrent(err(409, "conflict", { current: { version: 4 } }))).toEqual({ version: 4 });
    expect(conflictCurrent(err(409, "conflict"))).toBeNull();
  });
});
