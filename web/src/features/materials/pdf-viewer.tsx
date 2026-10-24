import "pdfjs-dist/web/pdf_viewer.css";
import { Minus, Plus } from "lucide-react";
import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentProxy } from "pdfjs-dist";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/misc";

// pdf.js viewer (frontend-spec 0, 4): pages render as they scroll into view, each with a text layer
// so text can be selected (Ask AI about it arrives with step 8). Reports the page in view.
// Embedded fonts are drawn as paths (disableFontFace): loading them as data: URLs would need the CSP
// to allow font-src data:.

pdfjs.GlobalWorkerOptions.workerSrc = new URL("pdfjs-dist/build/pdf.worker.min.mjs", import.meta.url).toString();

function Page({
  doc,
  n,
  width,
  zoom,
  onVisible,
}: {
  doc: PDFDocumentProxy;
  n: number;
  width: number;
  zoom: number;
  onVisible: (n: number, ratio: number) => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const text = useRef<HTMLDivElement>(null);
  const [near, setNear] = useState(n <= 2);
  const [size, setSize] = useState<{ w: number; h: number } | null>(null);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    // Render a page once it is within 800 px; report how much of it is on screen separately.
    const nearIo = new IntersectionObserver((es) => es.some((e) => e.isIntersecting) && setNear(true), {
      rootMargin: "800px 0px",
    });
    const seenIo = new IntersectionObserver((es) => es.forEach((e) => onVisible(n, e.intersectionRatio)), {
      threshold: [0, 0.1, 0.25, 0.5, 0.75, 1],
    });
    nearIo.observe(el);
    seenIo.observe(el);
    return () => {
      nearIo.disconnect();
      seenIo.disconnect();
    };
  }, [n, onVisible]);

  useEffect(() => {
    if (!near || !width) return;
    let cancelled = false;
    let task: { cancel: () => void } | null = null;
    void (async () => {
      const page = await doc.getPage(n);
      if (cancelled) return;
      const base = page.getViewport({ scale: 1 });
      const viewport = page.getViewport({ scale: (width / base.width) * zoom });
      setSize({ w: viewport.width, h: viewport.height });
      const c = canvas.current;
      if (!c) return;
      const ratio = window.devicePixelRatio || 1;
      c.width = Math.floor(viewport.width * ratio);
      c.height = Math.floor(viewport.height * ratio);
      // The text layer doesn't need the canvas: build it alongside, so text is selectable at once
      // (and even when painting waits for animation frames, as in a background tab).
      const t = text.current;
      if (t) {
        t.replaceChildren();
        t.style.setProperty("--scale-factor", String(viewport.scale));
        const layer = new pdfjs.TextLayer({ textContentSource: page.streamTextContent(), container: t, viewport });
        void layer.render().catch((e: unknown) => console.warn("pdf text layer", e));
      }
      const render = page.render({
        canvasContext: c.getContext("2d")!,
        viewport,
        transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : undefined,
      });
      task = render;
      await render.promise.catch((e: unknown) => {
        if ((e as { name?: string })?.name !== "RenderingCancelledException") console.warn("pdf render", e);
      });
    })();
    return () => {
      cancelled = true;
      task?.cancel();
    };
  }, [doc, n, near, width, zoom]);

  return (
    <div
      ref={box}
      data-page={n}
      className="relative mx-auto bg-white shadow-[0_0_0_1px_var(--border)]"
      style={size ? { width: size.w, height: size.h } : { width: width * zoom, height: width * zoom * 1.3 }}
    >
      <canvas
        ref={canvas}
        className="block"
        style={size ? { width: size.w, height: size.h } : undefined}
        aria-hidden="true"
      />
      <div ref={text} className="textLayer" />
      <span className="sr-only">Page {n}</span>
    </div>
  );
}

export function PdfViewer({
  url,
  initialPage = 1,
  onPage,
}: {
  url: string;
  initialPage?: number;
  onPage?: (n: number) => void;
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const [doc, setDoc] = useState<PDFDocumentProxy | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [width, setWidth] = useState(0);
  const [zoom, setZoom] = useState(1);
  const [page, setPage] = useState(initialPage);
  const onPageRef = useRef(onPage);
  onPageRef.current = onPage;

  useEffect(() => {
    let live = true;
    const task = pdfjs.getDocument({
      url,
      isEvalSupported: false,
      disableFontFace: true,
      // Same-origin copies (vite.config.ts): fonts a PDF doesn't embed, and CJK character maps.
      standardFontDataUrl: "/pdfjs/standard_fonts/",
      cMapUrl: "/pdfjs/cmaps/",
      cMapPacked: true,
    });
    task.promise.then(
      (d) => live && setDoc(d),
      () => live && setError("This PDF couldn't be opened."),
    );
    return () => {
      live = false;
      void task.destroy();
    };
  }, [url]);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    // Measure now (layout is available even when no frames run, e.g. a background tab), then follow
    // resizes.
    setWidth(Math.min(900, el.clientWidth));
    const ro = new ResizeObserver(([e]) => setWidth(Math.min(900, Math.floor(e!.contentRect.width))));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    if (doc && initialPage > 1) wrap.current?.querySelector(`[data-page="${initialPage}"]`)?.scrollIntoView();
  }, [doc, initialPage]);

  // The page shown is the one with the largest share on screen.
  const ratios = useRef(new Map<number, number>());
  const visible = useRef((n: number, ratio: number) => {
    ratios.current.set(n, ratio);
    let best = n;
    for (const [p, r] of ratios.current) if (r > (ratios.current.get(best) ?? 0)) best = p;
    if ((ratios.current.get(best) ?? 0) <= 0) return;
    setPage((cur) => {
      if (cur !== best) onPageRef.current?.(best);
      return best;
    });
  }).current;

  const go = (n: number) => {
    const target = Math.max(1, Math.min(doc?.numPages ?? 1, n));
    wrap.current?.querySelector(`[data-page="${target}"]`)?.scrollIntoView({ behavior: "smooth" });
  };

  if (error) return <p className="text-danger-text">{error}</p>;
  return (
    <div className="flex flex-col gap-3">
      <div className="sticky top-0 z-10 flex items-center gap-2 border-b border-border bg-bg/95 py-2 backdrop-blur">
        <label className="flex items-center gap-2 text-sm">
          <span className="sr-only">Page</span>
          <input
            type="number"
            min={1}
            max={doc?.numPages ?? 1}
            value={page}
            onChange={(e) => go(Number(e.currentTarget.value))}
            className="h-8 w-16 rounded-md border border-border bg-surface px-2 text-center font-mono tnum"
          />
          <span className="text-muted">of {doc?.numPages ?? "…"}</span>
        </label>
        <div className="ml-auto flex items-center gap-1">
          <Button
            size="iconSm"
            variant="ghost"
            aria-label="Zoom out"
            onClick={() => setZoom((z) => Math.max(0.5, +(z - 0.25).toFixed(2)))}
          >
            <Minus />
          </Button>
          <span className="w-12 text-center font-mono text-sm tnum">{Math.round(zoom * 100)}%</span>
          <Button
            size="iconSm"
            variant="ghost"
            aria-label="Zoom in"
            onClick={() => setZoom((z) => Math.min(3, +(z + 0.25).toFixed(2)))}
          >
            <Plus />
          </Button>
        </div>
      </div>
      <div ref={wrap} className="flex flex-col items-center gap-4 overflow-x-auto pb-8">
        {!doc ? (
          <Skeleton className="h-[70vh] w-full max-w-[900px]" />
        ) : (
          Array.from({ length: doc.numPages }, (_, i) => (
            <Page key={i + 1} doc={doc} n={i + 1} width={width} zoom={zoom} onVisible={visible} />
          ))
        )}
      </div>
    </div>
  );
}
