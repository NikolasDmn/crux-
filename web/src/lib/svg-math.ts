// MathJax's SVG string into DOM nodes this page may show. The CSP has no 'unsafe-inline' for
// styles, so `style` attributes are moved to the CSSOM (allowed); anything that could run script
// or link out is dropped, although the TeX packages in lib/mathjax.ts make none of it.

const TAGS = new Set([
  "svg",
  "g",
  "path",
  "rect",
  "line",
  "polyline",
  "polygon",
  "ellipse",
  "circle",
  "text",
  "title",
  "defs",
]);

/** The `<svg>` inside MathJax's output, cleaned; `null` when there is none. */
export function svgFrom(html: string, doc: Document = document): SVGSVGElement | null {
  const parsed = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const svg = parsed.querySelector("svg");
  if (!svg) return null;
  const styles: [Element, string][] = [];
  const walk = (el: Element) => {
    for (const child of [...el.children]) {
      if (!TAGS.has(child.tagName.toLowerCase())) child.remove();
      else walk(child);
    }
    for (const a of [...el.attributes]) {
      const name = a.name.toLowerCase();
      if (name.startsWith("on") || name === "href" || name === "xlink:href" || name === "src") {
        el.removeAttribute(a.name);
      } else if (name === "style") {
        styles.push([el, a.value]);
        el.removeAttribute(a.name);
      }
    }
  };
  walk(svg);
  const out = doc.importNode(svg, true) as unknown as SVGSVGElement;
  // Re-apply the moved styles on the imported copy, in the same document order.
  const copies = [out, ...out.querySelectorAll("*")];
  const originals = [svg, ...svg.querySelectorAll("*")];
  for (const [el, css] of styles) {
    const target = copies[originals.indexOf(el)] as (Element & ElementCSSInlineStyle) | undefined;
    if (target) target.style.cssText = css;
  }
  return out;
}
