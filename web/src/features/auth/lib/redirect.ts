/** Where to go after logging in: only a path on this site, never another origin (open redirect). */
export function safeRedirect(target: unknown, fallback = "/"): string {
  if (typeof target !== "string" || !target.startsWith("/") || target.startsWith("//") || target.startsWith("/\\")) {
    return fallback;
  }
  if (target.startsWith("/login") || target.startsWith("/signup")) return fallback;
  return target;
}
