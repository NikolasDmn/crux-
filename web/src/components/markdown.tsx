import "katex/dist/katex.min.css";
import type { ComponentProps, ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { MathJaxMath } from "@/components/math";
import { cn } from "@/lib/cn";
import { rehypeMath } from "@/lib/rehype-math";
import { normalizeDelimiters } from "@/lib/tex";

// Every prompt, explanation, card, note and AI message renders through this (frontend-spec 2.4):
// markdown + maths (`$…$`, `$$…$$`, and LaTeX's `\(…\)` / `\[…\]`), sanitised (no raw HTML).
// Maths goes to KaTeX (with MathML for screen readers), then KaTeX on a repaired copy, then
// MathJax, loaded only for a page that needs it; the source shows quietly if all of them fail.

const schema = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    code: [...(defaultSchema.attributes?.code ?? []), ["className", /^language-./, "math-inline", "math-display"]],
  },
};

/** rehypeMath's placeholder for maths KaTeX rejected; any other span renders as itself. */
function Span({ node: _, ...p }: ComponentProps<"span"> & { node?: unknown }) {
  const tex = (p as Record<string, unknown>)["data-tex"];
  if (typeof tex === "string") {
    return <MathJaxMath tex={tex} display={(p as Record<string, unknown>)["data-display"] === "true"} />;
  }
  return <span {...p} />;
}

export function Markdown({
  children,
  className,
  link,
}: {
  children: string;
  className?: string;
  /** Renders a link itself (e.g. the AI panel's `#p-12` citation chips); `undefined` keeps the default. */
  link?: (href: string | undefined, children: ReactNode) => ReactNode | undefined;
}) {
  const A = ({ node: _, ...p }: ComponentProps<"a"> & { node?: unknown }) => link?.(p.href, p.children) ?? <a {...p} />;
  return (
    <div className={cn("crux-prose", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[[rehypeSanitize, schema], rehypeMath]}
        components={link ? { span: Span, a: A } : { span: Span }}
      >
        {normalizeDelimiters(children)}
      </ReactMarkdown>
    </div>
  );
}
