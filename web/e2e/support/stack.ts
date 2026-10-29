import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, request, type APIRequestContext, type APIResponse, type Page } from "@playwright/test";

// Talking to the stack directly: the demo admin (from `make seed`) hands out invites, Mailpit holds
// the verification emails, and each journey signs up its own users so runs never collide.

export const REPO = fileURLToPath(new URL("../../../", import.meta.url));
export const SAMPLE_PDF = `${REPO}scripts/fixtures/sample.pdf`;
const MAILPIT = process.env.CRUX_E2E_MAILPIT ?? "http://localhost:8025";
/** Unique per run, so names never clash with an earlier run's leftovers. */
export const RUN = Date.now().toString(36);

export interface User {
  username: string;
  email: string;
  password: string;
  firstName: string;
}

export function baseURL(): string {
  return process.env.CRUX_E2E_BASE_URL ?? `http://localhost:${process.env.CRUX_HTTP_PORT ?? "8000"}`;
}

/** A new user's details (not signed up yet). */
export function newUser(tag: string): User {
  const username = `e2e-${tag}-${RUN}`;
  return {
    username,
    email: `${username}@crux.test`,
    password: `Pw-${RUN}-${Math.random().toString(36).slice(2, 10)}`,
    firstName: tag[0]!.toUpperCase() + tag.slice(1),
  };
}

function adminPassword(): string {
  if (process.env.CRUX_E2E_ADMIN_PASSWORD) return process.env.CRUX_E2E_ADMIN_PASSWORD;
  const line = readFileSync(`${REPO}deploy/secrets/demo-accounts.txt`, "utf8")
    .split("\n")
    .find((l) => l.startsWith("demo-admin"));
  const pw = line?.trim().split(/\s+/)[2];
  if (!pw) throw new Error("No demo-admin in deploy/secrets/demo-accounts.txt: run `make seed` first");
  return pw;
}

/** An API context signed in as `login` (the access token in the Authorization header). */
export async function apiAs(login: string, password: string): Promise<APIRequestContext> {
  const anon = await request.newContext({ baseURL: baseURL() });
  const res = await anon.post("/api/v1/auth/login", { data: { login, password } });
  expect(res.status(), `log in as ${login}`).toBe(200);
  const { accessToken } = (await res.json()) as { accessToken: string };
  await anon.dispose();
  return request.newContext({
    baseURL: `${baseURL()}/api/v1/`,
    extraHTTPHeaders: { authorization: `Bearer ${accessToken}` },
  });
}

let admin: APIRequestContext | null = null;

export async function createInvite(): Promise<string> {
  admin ??= await apiAs("demo-admin@crux.test", adminPassword());
  const res = await admin.post("admin/invites", { data: { expiresInDays: 1 } });
  expect(res.ok(), "create an invite").toBeTruthy();
  return ((await res.json()) as { code: string }).code;
}

/** Signs up through the API and verifies the email, as a returning user would be. */
export async function signedUp(tag: string): Promise<User> {
  const u = newUser(tag);
  const ctx = await request.newContext({ baseURL: baseURL() });
  const signup = async () =>
    ctx.post("/api/v1/auth/signup", {
      data: {
        inviteCode: await createInvite(),
        email: u.email,
        username: u.username,
        firstName: u.firstName,
        lastName: "Tester",
        password: u.password,
        timeZone: "Europe/Amsterdam",
        acceptBreachedPassword: true,
      },
    });
  let res = await signup();
  if (res.status() === 429) {
    // A full run signs up more users than the hourly limit per IP allows.
    unlock();
    res = await signup();
  }
  expect(res.status(), `sign up ${u.username}`).toBe(200);
  await ctx.post("/api/v1/auth/verify-email/confirm", { data: { token: await verifyToken(u.email) } });
  await ctx.dispose();
  return u;
}

/** The token from the newest verification email to `email` (Mailpit). */
export async function verifyToken(email: string): Promise<string> {
  const mail = await request.newContext({ baseURL: MAILPIT });
  let token: string | undefined;
  await expect
    .poll(
      async () => {
        const list = await mail.get("/api/v1/search", { params: { query: `to:"${email}"`, limit: "5" } });
        const msgs = ((await list.json()) as { messages: { ID: string; Subject: string }[] }).messages;
        for (const m of msgs) {
          const body = (await (await mail.get(`/api/v1/message/${m.ID}`)).json()) as { Text: string };
          token = /\/verify\?token=([\w-]+)/.exec(body.Text)?.[1];
          if (token) return true;
        }
        return false;
      },
      { message: `a verification email to ${email}`, timeout: 30_000 },
    )
    .toBe(true);
  await mail.dispose();
  return token!;
}

/** Signs the page in (the refresh cookie lands in the page's context) and opens Home. */
export async function logIn(page: Page, u: User): Promise<void> {
  const res = await page.request.post("/api/v1/auth/login", { data: { login: u.username, password: u.password } });
  expect(res.status(), `log in as ${u.username}`).toBe(200);
  await page.goto("/");
  await expect(page.getByRole("navigation").first()).toBeVisible();
}

/**
 * Runs a stack command from the repo root: CRUX_E2E_<name> if set (e.g. through WSL on Windows),
 * else `fallback`. `{file}` and the like are filled from `vars`.
 */
export function stackCommand(name: string, fallback: string, vars: Record<string, string> = {}): string {
  const cmd = (process.env[`CRUX_E2E_${name}`] ?? fallback).replace(/\{(\w+)\}/g, (_, k: string) => vars[k] ?? "");
  return execSync(cmd, { cwd: REPO, encoding: "utf8" });
}

/** A course with one lecture and one MC question, made through the API (for journeys that need one). */
export async function seededCourse(api: APIRequestContext, code: string) {
  const ok = async (p: Promise<APIResponse>) => {
    const res = await p;
    expect(res.ok(), `${res.url()} ${res.status()}`).toBeTruthy();
    return (await res.json()) as { id: string };
  };
  const course = await ok(api.post("courses", { data: { name: `Course ${code}`, code } }));
  const lecture = await ok(api.post(`courses/${course.id}/components`, { data: { code: "L1", name: "Basics" } }));
  await ok(
    api.post(`courses/${course.id}/questions`, {
      data: {
        componentId: lecture.id,
        type: "MC",
        content: {
          prompt: `Seeded question ${code}`,
          keyReliability: "Official",
          options: [
            { text: "Right", isCorrect: true },
            { text: "Wrong", isCorrect: false },
          ],
        },
      },
    }),
  );
  return { courseId: course.id, lectureId: lecture.id };
}

/** Clears the login and signup rate limits: `make unlock`, or CRUX_E2E_UNLOCK ("off" skips). */
export function unlock(): void {
  if (process.env.CRUX_E2E_UNLOCK === "off") return;
  stackCommand("UNLOCK", "make unlock");
}
