// What the user may do in a course (spec 4.2): Owner > Editor > Viewer.

export type Role = "Owner" | "Editor" | "Viewer";

export function canEdit(role: string | undefined): boolean {
  return role === "Owner" || role === "Editor";
}

export function isOwner(role: string | undefined): boolean {
  return role === "Owner";
}

/** Course codes the API takes: letters and digits, up to 12 (spec 5.1). */
export function normalizeCode(code: string): string {
  return code
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, "")
    .slice(0, 12);
}
