import { describe, expect, it } from "vitest";
import { svgFrom } from "./svg-math";

describe("svgFrom", () => {
  it("keeps the drawing, moves styles to the CSSOM and drops anything active", () => {
    const html =
      '<mjx-container class="MathJax"><svg style="vertical-align: -0.5ex;" viewBox="0 0 10 10" onload="x()">' +
      '<g fill="currentColor"><path d="M0 0L1 1" onclick="y()"/><a href="javascript:z()"><path d="M2 2"/></a>' +
      "<script>bad()</script><foreignObject><div>hi</div></foreignObject></g></svg></mjx-container>";
    const svg = svgFrom(html)!;
    expect(svg.tagName.toLowerCase()).toBe("svg");
    expect(svg.getAttribute("style")).toContain("vertical-align");
    expect(svg.style.verticalAlign).toBe("-0.5ex");
    expect(svg.hasAttribute("onload")).toBe(false);
    expect(svg.querySelectorAll("path")).toHaveLength(1);
    expect(svg.querySelector("path")!.hasAttribute("onclick")).toBe(false);
    expect(svg.querySelector("a, script, foreignObject")).toBeNull();
  });

  it("returns null without an svg", () => {
    expect(svgFrom("<span>nothing</span>")).toBeNull();
  });
});
