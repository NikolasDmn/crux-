import * as pdfjs from "pdfjs-dist";

// One PDF page on a canvas, for the crop picker. Imported lazily (pdf.js stays out of the editor's
// own chunk). Same options as the viewer: fonts drawn as paths, the CSP allows no data: fonts.

pdfjs.GlobalWorkerOptions.workerSrc = new URL("pdfjs-dist/build/pdf.worker.min.mjs", import.meta.url).toString();

export interface RenderedPage {
  pageCount: number;
  /** CSS pixels per PDF point: divide a box on the canvas by it to get points. */
  scale: number;
  width: number;
  height: number;
}

export async function renderPage(
  url: string,
  n: number,
  canvas: HTMLCanvasElement,
  width: number,
): Promise<RenderedPage> {
  const doc = await pdfjs.getDocument({
    url,
    isEvalSupported: false,
    disableFontFace: true,
    standardFontDataUrl: "/pdfjs/standard_fonts/",
    cMapUrl: "/pdfjs/cmaps/",
    cMapPacked: true,
  }).promise;
  try {
    const page = await doc.getPage(Math.min(Math.max(1, n), doc.numPages));
    const scale = width / page.getViewport({ scale: 1 }).width;
    const viewport = page.getViewport({ scale });
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.floor(viewport.width * ratio);
    canvas.height = Math.floor(viewport.height * ratio);
    canvas.style.width = `${viewport.width}px`;
    canvas.style.height = `${viewport.height}px`;
    await page.render({
      canvasContext: canvas.getContext("2d")!,
      viewport,
      transform: ratio !== 1 ? [ratio, 0, 0, ratio, 0, 0] : undefined,
    }).promise;
    return { pageCount: doc.numPages, scale, width: viewport.width, height: viewport.height };
  } finally {
    void doc.destroy();
  }
}
