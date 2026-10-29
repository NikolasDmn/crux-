import { useQueries } from "@tanstack/react-query";
import { Crop, ImagePlus, X } from "lucide-react";
import { useEffect, useRef, useState, type PointerEvent } from "react";
import { Banner } from "@/components/ui/banner";
import { Button } from "@/components/ui/button";
import { Field, Input, Select } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/misc";
import { Dialog } from "@/components/ui/overlays";
import { cropQuestionImage, fileUrl, uploadQuestionImage } from "@/features/files/api";
import { IMAGE_ACCEPT, sameOrigin } from "@/features/files/lib/upload";
import { keys as lectureKeys, useLectures } from "@/features/lectures/api";
import { api, unwrap } from "@/lib/api";
import { messageFor } from "@/lib/errors";
import type { RenderedPage } from "../lib/pdf-page";

// The question editor's image (frontend-spec 4): upload one (PNG, JPEG or WebP) or crop it from a
// page of one of the course's PDFs. Either way it's a question image of this course, saved with the
// question as its imageFileId.

export function ImageField({
  courseId,
  value,
  onChange,
}: {
  courseId: string;
  value: string | null;
  onChange: (fileId: string | null) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [cropping, setCropping] = useState(false);

  const upload = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    setError(null);
    try {
      onChange(await uploadQuestionImage(courseId, file));
    } catch (e) {
      setError(e instanceof Error && !("status" in e) ? e.message : messageFor(e));
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <span className="text-sm font-medium">Image</span>
      <div className="flex flex-wrap items-center gap-2">
        <input
          ref={input}
          type="file"
          accept={IMAGE_ACCEPT}
          className="sr-only"
          tabIndex={-1}
          aria-hidden="true"
          onChange={(e) => void upload(e.currentTarget.files?.[0])}
        />
        <Button size="sm" disabled={busy} onClick={() => input.current?.click()}>
          <ImagePlus aria-hidden="true" />
          {busy ? "Uploading…" : value ? "Replace image" : "Upload image"}
        </Button>
        <Button size="sm" disabled={busy} onClick={() => setCropping(true)}>
          <Crop aria-hidden="true" />
          Crop from a PDF page
        </Button>
        {value ? (
          <Button size="sm" variant="ghost" onClick={() => onChange(null)}>
            <X aria-hidden="true" />
            Remove image
          </Button>
        ) : null}
        <span className="text-sm text-muted">PNG, JPEG or WebP up to 5 MB.</span>
      </div>
      {error ? <Banner tone="danger">{error}</Banner> : null}
      {cropping ? (
        <CropDialog
          courseId={courseId}
          onClose={() => setCropping(false)}
          onCropped={(id) => {
            onChange(id);
            setCropping(false);
          }}
        />
      ) : null}
    </div>
  );
}

/** The course's PDF materials, across lectures. */
function useCoursePdfs(courseId: string) {
  const lectures = useLectures(courseId);
  const lists = useQueries({
    queries: (lectures.data ?? []).map((l) => ({
      queryKey: lectureKeys.materials(l.id),
      queryFn: async () => unwrap(api.GET("/components/{id}/materials", { params: { path: { id: l.id } } })),
    })),
  });
  const pdfs: { fileId: string; label: string }[] = [];
  for (const [i, l] of (lectures.data ?? []).entries()) {
    for (const m of lists[i]?.data ?? []) {
      if (m.fileId && m.originalStatus !== "Purged") pdfs.push({ fileId: m.fileId, label: `${l.code} ${m.title}` });
    }
  }
  return { pdfs, loading: lectures.isPending || lists.some((q) => q.isPending) };
}

type Box = { x: number; y: number; w: number; h: number };

function CropDialog({
  courseId,
  onClose,
  onCropped,
}: {
  courseId: string;
  onClose: () => void;
  onCropped: (fileId: string) => void;
}) {
  const { pdfs, loading } = useCoursePdfs(courseId);
  const [fileId, setFileId] = useState("");
  const [page, setPage] = useState("1");
  const [rendered, setRendered] = useState<RenderedPage | null>(null);
  const [renderError, setRenderError] = useState<string | null>(null);
  // In canvas pixels; null = the whole page.
  const [box, setBox] = useState<Box | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // A state, not a ref: the dialog's content mounts after this component, and the page renders
  // once the canvas exists.
  const [canvas, setCanvas] = useState<HTMLCanvasElement | null>(null);
  const drag = useRef<{ x: number; y: number } | null>(null);
  const chosen = fileId || pdfs[0]?.fileId || "";
  const n = Math.max(1, Number(page) || 1);

  useEffect(() => {
    if (!chosen || !canvas) return;
    let live = true;
    setRendered(null);
    setRenderError(null);
    setBox(null);
    void (async () => {
      try {
        const [{ renderPage }, url] = await Promise.all([import("../lib/pdf-page"), fileUrl(chosen)]);
        const r = await renderPage(sameOrigin(url.url, window.location.origin), n, canvas, 520);
        if (live) setRendered(r);
      } catch {
        if (live) setRenderError("This page couldn't be shown.");
      }
    })();
    return () => {
      live = false;
    };
  }, [chosen, n, canvas]);

  const at = (e: PointerEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return {
      x: Math.min(Math.max(0, e.clientX - r.left), r.width),
      y: Math.min(Math.max(0, e.clientY - r.top), r.height),
    };
  };

  const save = async () => {
    if (!rendered) return;
    const b = box ?? { x: 0, y: 0, w: rendered.width, h: rendered.height };
    setSaving(true);
    setError(null);
    try {
      const s = rendered.scale;
      onCropped(
        await cropQuestionImage(courseId, {
          fileId: chosen,
          page: n,
          box: { x: b.x / s, y: b.y / s, w: b.w / s, h: b.h / s },
        }),
      );
    } catch (e) {
      setError(messageFor(e));
      setSaving(false);
    }
  };

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title="Crop from a PDF page"
      description="Drag over the part of the page to use. Without a selection, the whole page is used."
      className="w-[min(600px,calc(100vw-32px))]"
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={!rendered || saving} onClick={() => void save()}>
            {saving ? "Cropping…" : box ? "Use this area" : "Use the whole page"}
          </Button>
        </>
      }
    >
      {error ? <Banner tone="danger">{error}</Banner> : null}
      {!loading && !pdfs.length ? (
        <p className="m-0 text-muted">This course has no PDFs yet. Upload slides to a lecture first.</p>
      ) : (
        <>
          <div className="grid gap-3 sm:grid-cols-[1fr_6rem]">
            <Field label="PDF">
              {(ids) => (
                <Select {...ids} value={chosen} onChange={(e) => setFileId(e.currentTarget.value)}>
                  {pdfs.map((p) => (
                    <option key={p.fileId} value={p.fileId}>
                      {p.label}
                    </option>
                  ))}
                </Select>
              )}
            </Field>
            <Field label="Page" hint={rendered ? `of ${rendered.pageCount}` : undefined}>
              {(ids) => (
                <Input
                  {...ids}
                  type="number"
                  min={1}
                  max={rendered?.pageCount}
                  value={page}
                  onChange={(e) => setPage(e.currentTarget.value)}
                />
              )}
            </Field>
          </div>
          {renderError ? <Banner tone="danger">{renderError}</Banner> : null}
          <div
            className="relative self-center touch-none select-none border border-border bg-white"
            onPointerDown={(e) => {
              if (!rendered) return;
              e.currentTarget.setPointerCapture(e.pointerId);
              drag.current = at(e);
              setBox(null);
            }}
            onPointerMove={(e) => {
              const start = drag.current;
              if (!start) return;
              const p = at(e);
              setBox({
                x: Math.min(start.x, p.x),
                y: Math.min(start.y, p.y),
                w: Math.abs(p.x - start.x),
                h: Math.abs(p.y - start.y),
              });
            }}
            onPointerUp={() => {
              drag.current = null;
              setBox((b) => (b && b.w > 4 && b.h > 4 ? b : null));
            }}
          >
            {!rendered && !renderError ? <Skeleton className="absolute inset-0" /> : null}
            <canvas ref={setCanvas} className="block" aria-label={`Page ${n}`} />
            {box ? (
              <div
                aria-hidden="true"
                className="pointer-events-none absolute border-2 border-[color:var(--course,var(--focus-ring))] bg-[color:var(--course,var(--focus-ring))]/10"
                style={{ left: box.x, top: box.y, width: box.w, height: box.h }}
              />
            ) : null}
          </div>
        </>
      )}
    </Dialog>
  );
}
