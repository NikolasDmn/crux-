import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Markdown } from "./markdown";

const texToSvg = vi.fn();
vi.mock("@/lib/mathjax", () => ({ texToSvg: (tex: string, display: boolean) => texToSvg(tex, display) }));

afterEach(() => texToSvg.mockReset());

describe("Markdown maths", () => {
  it("renders KaTeX for ordinary maths, LaTeX delimiters included", () => {
    const { container } = render(<Markdown>{String.raw`Mean \(\bar{x}\) and $$s^2$$`}</Markdown>);
    expect(container.querySelectorAll(".katex")).toHaveLength(2);
    expect(texToSvg).not.toHaveBeenCalled();
  });

  it("repairs a display environment used inline", () => {
    const { container } = render(<Markdown>{String.raw`$\begin{align} a &= b \end{align}$`}</Markdown>);
    expect(container.querySelector(".katex")).not.toBeNull();
  });

  it("hands what KaTeX can't render to MathJax", async () => {
    texToSvg.mockReturnValue('<svg viewBox="0 0 1 1"><path d="M0 0"/></svg>');
    const { container } = render(<Markdown>{String.raw`Water: $\ce{H2O}$`}</Markdown>);
    await waitFor(() => expect(container.querySelector("svg")).not.toBeNull());
    expect(texToSvg).toHaveBeenCalledWith(String.raw`\ce{H2O}`, false);
    expect(screen.getByRole("img", { name: String.raw`\ce{H2O}` })).toBeInTheDocument();
  });

  it("shows the source quietly when MathJax fails too", async () => {
    texToSvg.mockImplementation(() => {
      throw new Error("Undefined control sequence");
    });
    const { container } = render(<Markdown>{String.raw`$\nope{x}$`}</Markdown>);
    await waitFor(() => expect(container.querySelector(".crux-math-source")).not.toBeNull());
    expect(container.querySelector(".crux-math-source")?.textContent).toBe(String.raw`\nope{x}`);
    expect(container.querySelector(".katex-error")).toBeNull();
  });
});
