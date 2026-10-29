import { useMutation, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError, BASE, unwrap, type Schemas } from "@/lib/api";
import { messageFor } from "@/lib/errors";
import type { Material } from "@/features/lectures/api";
import { quotaDetail } from "./lib/storage";
import { openStream, type Stream } from "@/lib/sse";
import { sseTicket } from "@/features/notify/api";
import { failureText, imagePrecheck, precheck, sameOrigin, sha256, type UploadPhase } from "./lib/upload";

export type FileView = Schemas["FileView"];

export function fileUrl(id: string) {
  return unwrap(api.GET("/files/{file_id}/url", { params: { path: { file_id: id } } }));
}

/** Saves a file through its signed URL (on this origin, so the CSP allows it). */
export async function downloadFile(id: string, name?: string): Promise<void> {
  const { url } = await fileUrl(id);
  const a = document.createElement("a");
  a.href = sameOrigin(url, location.origin);
  a.download = name ?? "";
  document.body.append(a);
  a.click();
  a.remove();
}

export function pageText(id: string, page: number) {
  return unwrap(api.GET("/files/{file_id}/pages/{page}/text", { params: { path: { file_id: id, page } } }));
}

export function getFile(id: string) {
  return unwrap(api.GET("/files/{file_id}", { params: { path: { file_id: id } } }));
}

export function thumbnailUrl(id: string, page: number) {
  return unwrap(api.GET("/files/{file_id}/pages/{page}/thumbnail", { params: { path: { file_id: id, page } } }));
}

// Storage saver (frontend-spec 4: /settings/storage and the course's Storage section).

export type StorageView = Schemas["StorageView"];
const storageKey = ["files", "storage"] as const;

export function useStorage() {
  return useQuery({ queryKey: storageKey, queryFn: () => unwrap(api.GET("/files/storage")) });
}

export function useStorageSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Schemas["StorageSettings"]) => unwrap(api.PATCH("/files/storage/settings", { body })),
    onSuccess: (v) => qc.setQueryData(storageKey, v),
  });
}

/** A material's purge state changed: its lecture list, the viewer and the quota view. */
function materialChanged(qc: QueryClient, m: Material) {
  qc.setQueryData<Material[]>(["lecture", m.componentId, "materials"], (list) =>
    list?.map((x) => (x.id === m.id ? m : x)),
  );
  void qc.invalidateQueries({ queryKey: ["material", m.id] });
  void qc.invalidateQueries({ queryKey: storageKey });
}

export const RESTORED = "Restored: your questions and notes are reconnected";

/** A re-upload of a purged original brought the same file back (spec 5.1). files tells core over the
 *  bus, so show it present now and refetch once core has caught up. */
export function markRestored(qc: QueryClient, fileId: string) {
  const present = (m: Material) =>
    m.fileId === fileId ? { ...m, originalStatus: "Present", originalPurgeAt: null } : m;
  qc.setQueriesData<Material[]>({ queryKey: ["lecture"] }, (list) => (Array.isArray(list) ? list.map(present) : list));
  qc.setQueriesData<{ material: Material }>({ queryKey: ["material"] }, (d) =>
    d?.material ? { ...d, material: present(d.material) } : d,
  );
  void qc.invalidateQueries({ queryKey: storageKey });
  setTimeout(() => {
    void qc.invalidateQueries({ queryKey: ["lecture"] });
    void qc.invalidateQueries({ queryKey: ["material"] });
  }, 3000);
}

export function useFreeMaterial() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.POST("/materials/{id}/free-space", { params: { path: { id } } })),
    onSuccess: (m) => materialChanged(qc, m),
  });
}

export function useCancelFree() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.POST("/materials/{id}/free-space/cancel", { params: { path: { id } } })),
    onSuccess: (m) => materialChanged(qc, m),
  });
}

export function useFreeCourse(courseId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.POST("/courses/{id}/free-space", { params: { path: { id: courseId } } })),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["lecture"] });
      void qc.invalidateQueries({ queryKey: ["material"] });
      void qc.invalidateQueries({ queryKey: storageKey });
    },
  });
}

/** A question image for a course (the editor's "Upload image"): returns its file id, to save as the
 *  question's `imageFileId`. It's scanned like any upload; the question shows it meanwhile. */
export async function uploadQuestionImage(courseId: string, file: File): Promise<string> {
  const wrong = imagePrecheck(file);
  if (wrong) throw new Error(wrong);
  const started = await unwrap(
    api.POST("/files/uploads", {
      body: {
        fileName: file.name,
        sizeBytes: file.size,
        sha256: await sha256(file),
        courseId,
        purpose: "QuestionImage",
      },
    }),
  );
  if (started.uploadUrl) {
    await put(started.uploadUrl, started.uploadHeaders ?? {}, file, () => undefined, new AbortController().signal);
  }
  return (await unwrap(api.POST("/files/{file_id}/complete", { params: { path: { file_id: started.file.id } } }))).id;
}

/** A question image cut from a page of one of the course's PDFs (box in PDF points). */
export async function cropQuestionImage(
  courseId: string,
  input: { fileId: string; page: number; box: { x: number; y: number; w: number; h: number } },
): Promise<string> {
  return (
    await unwrap(api.POST("/courses/{id}/question-images/crop", { params: { path: { id: courseId } }, body: input }))
  ).fileId;
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
  check,
}: {
  courseId?: string;
  onClean: (file: FileView, name: string) => void;
  /** Runs on the hash before anything is sent; a message stops the upload (e.g. "not the same PDF"). */
  check?: (sha256: string) => string | null;
}) {
  const [uploads, setUploads] = useState<Upload[]>([]);
  const streams = useRef<Stream[]>([]);
  const aborts = useRef<AbortController[]>([]);
  const cleanRef = useRef(onClean);
  cleanRef.current = onClean;
  const checkRef = useRef(check);
  checkRef.current = check;

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
            const refused = checkRef.current?.(hash);
            if (refused) {
              set({ phase: "failed", message: refused });
              return;
            }
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
              ...(quotaDetail(e) ? { quota: true } : {}),
              message:
                e instanceof ApiError
                  ? quotaDetail(e)
                    ? `${messageFor(e)} ${quotaDetail(e)}.`
                    : messageFor(e)
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
