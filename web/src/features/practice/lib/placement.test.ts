import { describe, expect, it } from "vitest";
import { afterSection, reached, readShare, splitSections } from "./placement";

describe("sections", () => {
  it("splits at headings, keeping the intro as section 0", () => {
    expect(splitSections("Intro\n\n## Mean\nAdd.\n\n## Median\nMiddle.")).toEqual([
      "Intro\n",
      "## Mean\nAdd.\n",
      "## Median\nMiddle.",
    ]);
    expect(splitSections("# One\nx")).toEqual(["", "# One\nx"]);
  });

  it("ignores headings inside code fences and deeper headings", () => {
    expect(splitSections("## A\n```\n## not a heading\n```\n#### deep\n## B")).toHaveLength(3);
  });

  it("puts checks after their section, the rest at the end", () => {
    const checks = [{ position: 1 }, { position: 0 }, { position: 9 }, { position: 2 }];
    expect(afterSection(checks, 1, 3)).toEqual([{ position: 1 }]);
    expect(afterSection(checks, 2, 3)).toEqual([{ position: 0 }, { position: 9 }, { position: 2 }]);
    expect(afterSection(checks, 0, 3)).toEqual([]);
  });
});

describe("pdf", () => {
  const checks = [{ position: 2 }, { position: 5 }, { position: 0 }, { position: 99 }];

  it("unlocks page checks as you read and the end ones on the last page", () => {
    expect(reached(checks, 1, 6)).toEqual([]);
    expect(reached(checks, 3, 6)).toEqual([{ position: 2 }]);
    expect(reached(checks, 6, 6)).toEqual(checks);
  });

  it("without a page count, page checks still unlock and the end waits", () => {
    expect(reached(checks, 5, null)).toEqual([{ position: 2 }, { position: 5 }]);
  });

  it("measures how far you've read", () => {
    expect(readShare(3, 12)).toBe(0.25);
    expect(readShare(20, 12)).toBe(1);
    expect(readShare(3, null)).toBe(0);
  });
});
