// The maths step of <Markdown> (frontend-spec 2.4), in place of rehype-katex: KaTeX first, then
// KaTeX on a repaired copy (lib/tex.ts). What still fails becomes a placeholder span carrying the
// TeX, which <Markdown> renders with MathJax, loaded only then.

import type { Element, ElementContent, Root } from "hast";
import { fromHtmlIsomorphic } from "hast-util-from-html-isomorphic";
import { toText } from "hast-util-to-text";
import katex from "katex";
import { SKIP, visitParents } from "unist-util-visit-parents";
import { repairs } from "@/lib/tex";

/** The placeholder's attribute; its value is the TeX source. */
export const PENDING_ATTR = "data-tex";

/** KaTeX HTML for `tex`, trying the repairs in turn; `null` when none of them renders. */
export function katexHtml(tex: string, displayMode: boolean): string | null {
  for (const t of [tex, ...repairs(tex)]) {
    try {
      return katex.renderToString(t, { displayMode, throwOnError: true, output: "htmlAndMathml" });
    } catch {
      // Next candidate.
    }
  }
  return null;
}

export function rehypeMath() {
  return (tree: Root) => {
    visitParents(tree, "element", (element: Element, parents) => {
      const classes = Array.isArray(element.properties.className) ? element.properties.className : [];
      const inline = classes.includes("math-inline");
      const display = classes.includes("math-display");
      const fenced = classes.includes("language-math");
      if (!inline && !display && !fenced) return;

      let parent = parents[parents.length - 1];
      let scope: Element = element;
      let displayMode = display;
      // ```math blocks: replace the <pre> and render as display.
      if (fenced && element.tagName === "code" && parent?.type === "element" && parent.tagName === "pre") {
        scope = parent;
        parent = parents[parents.length - 2];
        displayMode = true;
      }
      if (!parent) return;

      const tex = toText(scope, { whitespace: "pre" });
      const html = katexHtml(tex, displayMode);
      const result: ElementContent[] = html
        ? (fromHtmlIsomorphic(html, { fragment: true }).children as ElementContent[])
        : [
            {
              type: "element",
              tagName: "span",
              properties: { dataTex: tex, dataDisplay: displayMode ? "true" : "false" },
              children: [],
            },
          ];
      parent.children.splice(parent.children.indexOf(scope), 1, ...result);
      return SKIP;
    });
  };
}
