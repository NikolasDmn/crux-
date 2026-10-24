// Course colours (frontend-spec 3.1, 5.2). The colour is only an accent; anything drawn on it gets
// a readable foreground by contrast (≥ 4.5:1 where possible).

export interface CourseColor {
  name: string;
  hex: string;
  softLight: string;
  softDark: string;
}

export const COURSE_COLORS: readonly CourseColor[] = [
  { name: "Orange", hex: "#f76b15", softLight: "#fde7d9", softDark: "#3a2416" },
  { name: "Blue", hex: "#3e63dd", softLight: "#e1e7fb", softDark: "#1b2240" },
  { name: "Teal", hex: "#12a594", softLight: "#d9f2ee", softDark: "#10302c" },
  { name: "Purple", hex: "#8e4ec6", softLight: "#ede1f7", softDark: "#2a1d38" },
  { name: "Pink", hex: "#e5559f", softLight: "#fae0ef", softDark: "#3a1a2e" },
  { name: "Green", hex: "#30a46c", softLight: "#dcf2e5", softDark: "#13301f" },
  { name: "Amber", hex: "#e2a336", softLight: "#fbefd6", softDark: "#36290f" },
  { name: "Slate", hex: "#8b8d98", softLight: "#ececee", softDark: "#26272b" },
];

export const DEFAULT_COURSE_COLOR = COURSE_COLORS[0]!;

const INK = "#1c1c1a";
const WHITE = "#ffffff";

function channel(c: number): number {
  const s = c / 255;
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

/** Parses `#rgb` or `#rrggbb`; null when it isn't a hex colour. */
export function parseHex(hex: string): [number, number, number] | null {
  const m = /^#?([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  let h = m[1]!;
  if (h.length === 3) h = [...h].map((c) => c + c).join("");
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
}

export function luminance(hex: string): number {
  const rgb = parseHex(hex);
  if (!rgb) return 0;
  const [r, g, b] = rgb.map(channel) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG contrast ratio, 1–21. */
export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

/** Ink or white, whichever reads better on `bg`. */
export function readableOn(bg: string): string {
  return contrast(INK, bg) >= contrast(WHITE, bg) ? INK : WHITE;
}

/** The palette entry for a stored course colour; unknown values fall back to the default. */
export function courseColor(hex: string | null | undefined): CourseColor {
  const h = (hex ?? "").toLowerCase();
  return (
    COURSE_COLORS.find((c) => c.hex === h) ?? (parseHex(h) ? { ...DEFAULT_COURSE_COLOR, hex: h } : DEFAULT_COURSE_COLOR)
  );
}
