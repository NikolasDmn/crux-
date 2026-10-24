// KaTeX's stylesheet: it also hides the MathML copy that screen readers read.
import "katex/dist/katex.min.css";
import { fromHtmlIsomorphic } from "hast-util-from-html-isomorphic";
import { toJsxRuntime } from "hast-util-to-jsx-runtime";
import { useEffect, useMemo, useRef, useState } from "react";
import { Fragment, jsx, jsxs } from "react/jsx-runtime";
import { katexHtml } from "@/lib/rehype-math";
import { svgFrom } from "@/lib/svg-math";
import { cn } from "@/lib/cn";

// Maths KaTeX couldn't render, drawn by MathJax (lazy, frontend-spec 2.4 and 6). When MathJax
// can't either, the source shows quietly in mono with a note, not as a red error.
export function MathJaxMath({ tex, display }: { tex: string; display: boolean }) {
  const box = useRef<HTMLSpanElement>(null);
  const [state, setState] = useState<"loading" | "done" | "failed">("loading");

  useEffect(() => {
    let live = true;
    setState("loading");
    import("@/lib/mathjax")
      .then(({ texToSvg }) => {
        const svg = svgFrom(texToSvg(tex, display));
        if (!live) return;
        if (!svg || !box.current) return setState("failed");
        svg.setAttribute("aria-hidden", "true");
        box.current.replaceChildren(svg);
        setState("done");
      })
      .catch(() => live && setState("failed"));
    return () => {
      live = false;
    };
  }, [tex, display]);

  if (state === "failed") {
    return (
      <code
        className={cn("crux-math-source", display && "block overflow-x-auto py-1")}
        title="This maths couldn't be rendered; showing its source"
      >
        {tex}
      </code>
    );
  }
  return (
    <span
      className={cn(display ? "my-3 block overflow-x-auto text-center" : "inline-block align-baseline")}
      role="img"
      aria-label={tex}
      aria-busy={state === "loading"}
    >
      <span ref={box} />
      {state === "loading" ? <span className="font-mono text-sm text-muted">{tex}</span> : null}
    </span>
  );
}

/** One expression outside markdown (a key rule as display maths): KaTeX, else MathJax. */
export function TexMath({ tex, display = false }: { tex: string; display?: boolean }) {
  // Through React, not innerHTML: KaTeX's inline styles then go via the CSSOM, which the CSP allows.
  const katex = useMemo(() => {
    const html = katexHtml(tex, display);
    return html === null ? null : toJsxRuntime(fromHtmlIsomorphic(html, { fragment: true }), { Fragment, jsx, jsxs });
  }, [tex, display]);
  if (katex === null) return <MathJaxMath tex={tex} display={display} />;
  return <span className={display ? "block" : undefined}>{katex}</span>;
}
