// All UI strings go through t() with English keys (frontend-spec 0), so a translation can be added
// later without touching components. Missing keys show the key itself.

import { en } from "./en";

export type Key = keyof typeof en;

/** `t("home.greeting", { name: "Nick" })` fills `{name}` placeholders. */
export function t(key: Key, vars: Record<string, string | number> = {}): string {
  const s: string = en[key] ?? key;
  return s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

/** Plural by count: `plural(3, "lecture")` → "3 lectures". */
export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}
