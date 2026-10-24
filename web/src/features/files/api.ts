import { useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError, BASE, unwrap, type Schemas } from "@/lib/api";
import { messageFor } from "@/lib/errors";
import { openStream, type Stream } from "@/lib/sse";
import { sseTicket } from "@/features/notify/api";
import { failureText, precheck, sameOrigin, sha256, type UploadPhase } from "./lib/upload";

export type FileView = Schemas["FileView"];

export function fileUrl(id: string) {
  return unwrap(api.GET("/files/{file_id}/url", { params: { path: { file_id: id } } }));
}

export function pageText(id: string, page: number) {
  return unwrap(api.GET("/files/{file_id}/pages/{page}/text", { params: { path: { file_id: id, page } } }));
}

export function getFile(id: string) {
  return unwrap(api.GET("/files/{file_id}", { params: { path: { file_id: id } } }));
}

/** PUT to the signed URL with progress (fetch can't report upload progress). */
function put(
  url: string,
  headers: Record<string, string>,
  file: File,
  onProgress: (loaded: number) => void,
  signal: AbortSignal,
) {
  return new Promise<void>((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("PUT", sameOrigin(url, location.origin));
    for (const [k, v] of Object.entries(headers)) xhr.setRequestHeader(k, v);
    xhr.upload.onprogress = (e) => onProgress(e.loaded);
    xhr.onload = () => (xhr.status < 300 ? resolve() : reject(new Error(`upload failed (${xhr.status})`)));
    xhr.onerror = () => reject(new TypeError("network"));
    signal.addEventListener("abort", () => xhr.abort());
    xhr.send(file);
  });
}

/** Follows scanning → extracting → ready over SSE (spec 5.1). Resolves once the scan is Clean
 *  (`onClean`, so a material can be made right away) and again when extraction ends. */
function follow(fileId: string, set: (p: UploadPhase) => void, onClean: () => void): Stream {
  let clean = false;
  const s = openStream(
    `${BASE}/files/${fileId}/events`,
    {
      scanned: (d) => {
        const status = (d as { status?: string }).status;
        if (status === "Clean") {
          clean = true;
          set({ phase: "extracting" });
          onClean();
        }
      },
      extracted: (d) => set({ phase: "ready", pageCount: (d as { pageCount?: number | null }).pageCount ?? null }),
      failed: (d) => {
        const reason = (d as { reason?: string; status?: string }).reason ?? (d as { status?: string }).status ?? "";
        // An extract failure after a clean scan still leaves a usable PDF.
        set(clean ? { phase: "ready", pageCount: null } : { phase: "failed", message: failureText(reason) });
      },
      done: () => s.close(),
    },
    {
      ticket: sseTicket,
      open: (url) => new EventSource(url),
      setTimer: (fn, ms) => window.setTimeout(fn, ms),
      clearTimer: (t) => window.clearTimeout(t as number),
    },
  );
  return s;
}

export interface Upload {
  key: string;
  name: string;
  size: number;
  state: UploadPhase;
  file?: FileView;
}

/**
 * Uploads PDFs one by one (spec 5.1): hash in a worker, start, skip the PUT when the file is already
 * there, PUT with progress, complete, then follow the scan. `onClean(file, name)` runs once a file
 * is safe to attach (e.g. create the material).
 */
export function useUploads({
  courseId,
  onClean,
}: {
  courseId?: string;
  onClean: (file: FileView, name: string) => void;
}) {
  const [uploads, setUploads] = useState<Upload[]>([]);
  const streams = useRef<Stream[]>([]);
  const aborts = useRef<AbortController[]>([]);
  const cleanRef = useRef(onClean);
  cleanRef.current = onClean;

  useEffect(
    () => () => {
      streams.current.forEach((s) => s.close());
      aborts.current.forEach((a) => a.abort());
    },
    [],
  );

  const add = useCallback(
    (files: File[]) => {
      for (const file of files) {
        const key = `${file.name}-${file.size}-${Date.now()}-${Math.random()}`;
        const set = (state: UploadPhase, fv?: FileView) =>
          setUploads((us) => us.map((u) => (u.key === key ? { ...u, state, ...(fv ? { file: fv } : {}) } : u)));
        const bad = precheck(file);
        setUploads((us) => [
          ...us,
          {
            key,
            name: file.name,
            size: file.size,
            state: bad ? { phase: "failed", message: bad } : { phase: "hashing" },
          },
        ]);
        if (bad) continue;
        const abort = new AbortController();
        aborts.current.push(abort);
        void (async () => {
          try {
            const hash = await sha256(
              file,
              () => new Worker(new URL("./lib/hash.worker.ts", import.meta.url), { type: "module" }),
            );
            set({ phase: "starting" });
            const started = await unwrap(
              api.POST("/files/uploads", {
                body: {
                  fileName: file.name,
                  sizeBytes: file.size,
                  sha256: hash,
                  courseId: courseId ?? null,
                  purpose: "Original",
                },
              }),
            );
            let fv = started.file;
            if (!started.alreadyPresent && started.uploadUrl) {
              set({ phase: "uploading", loaded: 0, total: file.size }, fv);
              await put(
                started.uploadUrl,
                started.uploadHeaders as Record<string, string>,
                file,
                (loaded) => set({ phase: "uploading", loaded, total: file.size }),
                abort.signal,
              );
              set({ phase: "completing" });
              fv = await unwrap(api.POST("/files/{file_id}/complete", { params: { path: { file_id: fv.id } } }));
            }
            if (fv.scanStatus === "Clean") {
              // Already scanned (a re-upload of the same bytes, B7-7).
              set(
                {
                  phase: fv.extractStatus === "Pending" ? "extracting" : "ready",
                  pageCount: fv.pageCount ?? null,
                } as UploadPhase,
                fv,
              );
              cleanRef.current(fv, file.name);
              if (fv.extractStatus === "Pending")
                streams.current.push(
                  follow(
                    fv.id,
                    (p) => set(p),
                    () => {},
                  ),
                );
              return;
            }
            set({ phase: "scanning" }, fv);
            const done = fv;
            streams.current.push(
              follow(
                fv.id,
                (p) => set(p),
                () => cleanRef.current(done, file.name),
              ),
            );
          } catch (e) {
            if (abort.signal.aborted) return;
            set({
              phase: "failed",
              message:
                e instanceof ApiError
                  ? messageFor(e)
                  : e instanceof Error && e.message.startsWith("upload failed")
                    ? "The upload didn't go through. Try again."
                    : messageFor(e),
            });
          }
        })();
      }
    },
    [courseId],
  );

  const dismiss = useCallback((key: string) => setUploads((us) => us.filter((u) => u.key !== key)), []);
  return { uploads, add, dismiss };
}
