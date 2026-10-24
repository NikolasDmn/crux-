import type { Element, Root } from "hast";
import { describe, expect, it } from "vitest";
import { katexHtml, rehypeMath } from "./rehype-math";

const tree = (tex: string, cls: string, wrapPre = false): Root => {
  const code: Element = {
    type: "element",
    tagName: "code",
    properties: { className: [cls] },
    children: [{ type: "text", value: tex }],
  };
  const child: Element = wrapPre ? { type: "element", tagName: "pre", properties: {}, children: [code] } : code;
  return { type: "root", children: [{ type: "element", tagName: "p", properties: {}, children: [child] }] };
};

const first = (t: Root) => ((t.children[0] as Element).children[0] as Element) ?? null;

describe("katexHtml", () => {
  it("renders, repairs, or gives up", () => {
    expect(katexHtml(String.raw`\frac{a}{b}`, false)).toContain("katex");
    expect(katexHtml(String.raw`\begin{align} a &= b \end{align}`, false)).toContain("katex");
    expect(katexHtml(String.raw`\ce{H2O}`, false)).toBeNull();
  });
});

describe("rehypeMath", () => {
  it("replaces maths KaTeX takes with its markup", () => {
    const t = tree("x^2", "math-inline");
    rehypeMath()(t);
    expect(first(t).properties.className).toContain("katex");
  });

  it("leaves a placeholder with the TeX for the rest", () => {
    const t = tree(String.raw`\ce{H2O}`, "math-display");
    rehypeMath()(t);
    expect(first(t)).toMatchObject({
      tagName: "span",
      properties: { dataTex: String.raw`\ce{H2O}`, dataDisplay: "true" },
    });
  });

  it("renders ```math blocks as display maths in place of the <pre>", () => {
    const t = tree("a = b", "language-math", true);
    rehypeMath()(t);
    expect(first(t).properties.className).toContain("katex-display");
  });

  it("ignores other elements", () => {
    const t = tree("x", "language-js");
    rehypeMath()(t);
    expect(first(t).tagName).toBe("code");
  });
});
