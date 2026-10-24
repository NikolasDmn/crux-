import { ApiError, idempotencyKey, ifMatch, unwrap } from "./api";
import { cn } from "./cn";
import { plural, t } from "./t";
import { applyTheme, getTheme, isDark } from "./theme";
import { getToken, onTokenChange, setToken } from "./token";

it("fills placeholders and pluralises", () => {
  expect(t("home.greeting", { name: "Nick" })).toBe("Hello, Nick");
  expect(t("invite.shared", { who: "mara_k" })).toBe("mara_k shared {course} with you");
  expect(plural(1, "lecture")).toBe("1 lecture");
  expect(plural(3, "lecture")).toBe("3 lectures");
  expect(plural(2, "quiz", "quizzes")).toBe("2 quizzes");
});

it("merges class names", () => {
  const hidden: string | false = Math.random() > 2 && "hidden";
  expect(cn("px-2 text-sm", hidden, "px-4")).toBe("text-sm px-4");
});

it("switches themes and remembers the choice", () => {
  applyTheme("dark");
  expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  expect(getTheme()).toBe("dark");
  expect(isDark()).toBe(true);
  applyTheme("system");
  expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  expect(getTheme()).toBe("system");
});

it("keeps the access token in memory and notifies", () => {
  const seen: (string | null)[] = [];
  const off = onTokenChange((x) => seen.push(x));
  setToken("abc");
  setToken(null);
  off();
  setToken("later");
  expect(seen).toEqual(["abc", null]);
  expect(getToken()).toBe("later");
  setToken(null);
});

it("unwraps results and throws typed errors", async () => {
  const ok = new Response("{}", { status: 200 });
  await expect(unwrap(Promise.resolve({ data: { a: 1 }, response: ok }))).resolves.toEqual({ a: 1 });
  const bad = new Response("{}", { status: 409 });
  const e = await unwrap(
    Promise.resolve({ error: { error: { code: "conflict", message: "m" } }, response: bad }),
  ).catch((x) => x);
  expect(e).toBeInstanceOf(ApiError);
  expect((e as ApiError).code).toBe("conflict");
  expect(idempotencyKey()).toMatch(/^[0-9A-Z]{26}$/);
  expect(ifMatch(3)).toEqual({ "If-Match": "3" });
});
