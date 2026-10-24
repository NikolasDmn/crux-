import "katex/dist/katex.min.css";
import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import rehypeSanitize, { defaultSchema } from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { cn } from "@/lib/cn";

// Every prompt, explanation, card, note and AI message renders through this (frontend-spec 2.4):
// markdown + $…$ / $$…$$ maths, sanitised (no raw HTML), then KaTeX (with MathML for screen readers).
// An expression KaTeX rejects shows its source in the danger colour; the MathJax fallback comes
// with the accessibility pass (build step 9).

const schema = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    code: [...(defaultSchema.attributes?.code ?? []), ["className", /^language-./, "math-inline", "math-display"]],
  },
};

export function Markdown({ children, className }: { children: string; className?: string }) {
  return (
    <div className={cn("crux-prose", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[
          [rehypeSanitize, schema],
          [rehypeKatex, { throwOnError: false, output: "htmlAndMathml" }],
        ]}
      >
        {children}
      </ReactMarkdown>
    </div>
  );
}
