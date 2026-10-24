import katex from "katex";
import { describe, expect, it } from "vitest";
import { normalizeDelimiters, repairs } from "./tex";

const renders = (tex: string, displayMode = false) => {
  try {
    katex.renderToString(tex, { throwOnError: true, displayMode });
    return true;
  } catch {
    return false;
  }
};

describe("delimiters", () => {
  it("turns LaTeX delimiters into dollars", () => {
    expect(normalizeDelimiters(String.raw`So \(x^2\) grows.`)).toBe("So $x^2$ grows.");
    expect(normalizeDelimiters(String.raw`Then \[ \sum x \] holds.`)).toBe("Then \n$$\n\\sum x\n$$\n holds.");
  });

  it("leaves code and lone escapes alone", () => {
    const code = "Use `\\(x\\)` or\n```\n\\[y\\]\n```";
    expect(normalizeDelimiters(code)).toBe(code);
    expect(normalizeDelimiters(String.raw`a \( b`)).toBe(String.raw`a \( b`);
    expect(normalizeDelimiters("plain $x$")).toBe("plain $x$");
  });
});

describe("repairs", () => {
  it("rewrites what KaTeX rejects into what it takes", () => {
    const cases = [
      String.raw`\begin{align} a &= b \\ c &= d \end{align}`,
      String.raw`\begin{align*} a &= b \label{eq:1} \end{align*}`,
      String.raw`\begin{equation} E = mc^2 \end{equation}`,
      String.raw`\begin{eqnarray} a &=& b \\ c &<& d \end{eqnarray}`,
      String.raw`\begin{gather} a \\ b \end{gather}`,
      String.raw`\require{cancel} \cancel{x}`,
      String.raw`x = 1 \tag{3}`,
    ];
    for (const tex of cases) {
      expect(renders(tex), tex).toBe(false);
      const [fixed] = repairs(tex);
      expect(fixed, tex).toBeDefined();
      expect(renders(fixed!), fixed).toBe(true);
    }
  });

  it("offers nothing when there is nothing to fix", () => {
    expect(repairs(String.raw`\frac{a}{`)).toEqual([]);
    expect(repairs(String.raw`\ce{H2O}`)).toEqual([]);
  });
});
