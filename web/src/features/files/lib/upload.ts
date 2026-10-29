// The upload pipeline's pure parts (frontend-spec 5.1): states, the hash, and the same-origin rule
// for signed URLs.

export type UploadPhase =
  | { phase: "hashing" }
  | { phase: "starting" }
  | { phase: "uploading"; loaded: number; total: number }
  | { phase: "completing" }
  | { phase: "scanning" }
  | { phase: "extracting" }
  | { phase: "ready"; pageCount: number | null }
  | { phase: "failed"; message: string; quota?: boolean };

export const ACCEPT = "application/pdf";
export const MAX_BYTES = 100 * 1024 * 1024;

/** Checks before any network: type and size. `null` when fine. */
export function precheck(file: { name: string; size: number; type: string }): string | null {
  const pdf = file.type === "application/pdf" || file.name.toLowerCase().endsWith(".pdf");
  if (!pdf) return "Only PDFs can be uploaded for now.";
  if (file.size <= 0) return "This file is empty.";
  if (file.size > MAX_BYTES) return `Files must be under ${MAX_BYTES / (1024 * 1024)} MB.`;
  return null;
}

/** Question images (the editor's "Upload image"): what the files service accepts. */
export const IMAGE_ACCEPT = "image/png,image/jpeg,image/webp";
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;

export function imagePrecheck(file: { name: string; size: number; type: string }): string | null {
  if (!/\.(png|jpe?g|webp)$/i.test(file.name)) return "Use a PNG, JPEG or WebP image.";
  if (file.size <= 0) return "This file is empty.";
  if (file.size > MAX_IMAGE_BYTES) return `Images must be under ${MAX_IMAGE_BYTES / (1024 * 1024)} MB.`;
  return null;
}

/** Signed URLs are signed for the gateway's host; send them to this origin, path and query kept,
 *  so the request stays same-origin (CSP connect-src 'self', B2-1). */
export function sameOrigin(url: string, origin: string): string {
  const u = new URL(url, origin);
  return `${u.pathname}${u.search}`;
}

/** Hex SHA-256 in a worker when there is one, else on this thread. */
export async function sha256(file: Blob, makeWorker?: () => Worker): Promise<string> {
  if (makeWorker && typeof Worker !== "undefined") {
    const w = makeWorker();
    try {
      return await new Promise<string>((resolve, reject) => {
        w.onmessage = (e: MessageEvent<{ hex?: string; error?: string }>) =>
          e.data.hex ? resolve(e.data.hex) : reject(new Error(e.data.error ?? "hash failed"));
        w.onerror = (e) => reject(new Error(e.message));
        w.postMessage(file);
      });
    } finally {
      w.terminate();
    }
  }
  const digest = await crypto.subtle.digest("SHA-256", await bytes(file));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** The blob's bytes; FileReader where `Blob.arrayBuffer` is missing (older engines, jsdom). */
function bytes(blob: Blob): Promise<ArrayBuffer | Uint8Array> {
  if (typeof blob.arrayBuffer === "function") return blob.arrayBuffer();
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    // Copied into this realm's Uint8Array: a reader's buffer can come from another one.
    r.onload = () => resolve(Uint8Array.from(new Uint8Array(r.result as ArrayBuffer)));
    r.onerror = () => reject(r.error);
    r.readAsArrayBuffer(blob);
  });
}

/** What a file's scan/extract outcome means for the user. */
export function failureText(reason: string): string {
  switch (reason) {
    case "infected":
      return "This file failed the virus scan and was removed.";
    case "failed":
      return "The scan couldn't finish. Try uploading again.";
    default:
      return "Crux couldn't read this PDF's text. You can still view it.";
  }
}

/** One line for the status chip. */
export function phaseLabel(p: UploadPhase): string {
  switch (p.phase) {
    case "hashing":
      return "Preparing…";
    case "starting":
      return "Starting…";
    case "uploading":
      return `Uploading ${p.total ? Math.round((p.loaded / p.total) * 100) : 0}%`;
    case "completing":
      return "Finishing upload…";
    case "scanning":
      return "Scanning for viruses…";
    case "extracting":
      return "Reading the text…";
    case "ready":
      return p.pageCount ? `Ready · ${p.pageCount} pages` : "Ready";
    case "failed":
      return p.message;
  }
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(n < 10 * 1024 * 1024 ? 1 : 0)} MB`;
  return `${(n / 1024 ** 3).toFixed(1)} GB`;
}
