// Where a material's check questions show: notes are split at their headings (section N = the
// Nth heading and what follows it; text before the first heading is section 0), and a check comes
// after its section; in a PDF a check unlocks once you've reached its page. Position 0, or past
// the last section/page, means at the end.

export interface Placed {
  position: number;
}

/** Markdown split before every #, ## or ### heading, outside code fences. */
export function splitSections(md: string): string[] {
  const out: string[] = [""];
  let fence = false;
  for (const line of md.split("\n")) {
    if (/^\s*(```|~~~)/.test(line)) fence = !fence;
    if (!fence && /^#{1,3}\s/.test(line)) out.push("");
    out[out.length - 1] += `${line}\n`;
  }
  return out.map((s) => s.replace(/\n$/, ""));
}

/** Checks that come right after section `n` of `count` (the last section also takes the end). */
export function afterSection<T extends Placed>(checks: T[], n: number, count: number): T[] {
  const last = count - 1;
  return checks.filter((c) =>
    n === last ? c.position === n || c.position === 0 || c.position > last : n > 0 && c.position === n,
  );
}

/** PDF checks you've reached: a page check once you've seen that page; the end ones at the last page. */
export function reached<T extends Placed>(checks: T[], maxPage: number, pageCount: number | null | undefined): T[] {
  const atEnd = !!pageCount && maxPage >= pageCount;
  return checks.filter((c) =>
    c.position > 0 && (!pageCount || c.position <= pageCount) ? c.position <= maxPage : atEnd,
  );
}

/** "Page 4 of 12": how far you've read, 0–1. */
export function readShare(maxPage: number, pageCount: number | null | undefined): number {
  return pageCount ? Math.min(1, Math.max(0, maxPage / pageCount)) : 0;
}
