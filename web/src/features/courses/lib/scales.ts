// Which grade scales a picker offers (Settings > Study > Grade scales): the built-ins you chose (all
// of them until you choose), your own scales always, and the one a course already uses.

export interface ScaleLike {
  id: string;
  builtIn: boolean;
}

/** `chosen` null: every scale. */
export function shownScales<S extends ScaleLike>(all: S[], chosen: string[] | null, keep?: string | null): S[] {
  if (!chosen) return all;
  const on = new Set(chosen);
  return all.filter((s) => !s.builtIn || on.has(s.id) || s.id === keep);
}

/** The stored choice from settings.extra (null when never set or malformed). */
export function readChosen(extra: unknown): string[] | null {
  const v = (extra as { gradeScales?: unknown } | null)?.gradeScales;
  return Array.isArray(v) && v.every((x) => typeof x === "string") ? (v as string[]) : null;
}
