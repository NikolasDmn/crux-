// Theme toggle (frontend-spec 0): system / light / dark. The choice is a per-browser convenience in
// localStorage; "system" removes `data-theme` so prefers-color-scheme decides.

export type Theme = "system" | "light" | "dark";
const KEY = "crux.theme";

export function getTheme(): Theme {
  try {
    const t = localStorage.getItem(KEY);
    return t === "light" || t === "dark" ? t : "system";
  } catch {
    return "system";
  }
}

export function applyTheme(t: Theme, root: HTMLElement = document.documentElement): void {
  if (t === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", t);
  try {
    if (t === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, t);
  } catch {
    /* storage blocked: the choice lasts for this page only */
  }
}

/** Whether the page currently renders dark. */
export function isDark(root: HTMLElement = document.documentElement): boolean {
  const t = root.getAttribute("data-theme");
  if (t) return t === "dark";
  return typeof matchMedia === "function" && matchMedia("(prefers-color-scheme: dark)").matches;
}
