import AxeBuilder from "@axe-core/playwright";
import { expect, type Page } from "@playwright/test";

/** Spec 9: axe on the screen as it is now, with 0 serious or critical violations (WCAG 2.2 AA). */
export async function expectAccessible(page: Page, screen: string): Promise<void> {
  const result = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
    // pdf.js's text layer is transparent on purpose: it's there for selection, over the drawn page.
    .exclude(".textLayer")
    .analyze();
  const bad = result.violations
    .filter((v) => v.impact === "serious" || v.impact === "critical")
    .map((v) => `${v.id} (${v.impact}): ${v.help}\n  ${v.nodes.map((n) => n.target.join(" ")).join("\n  ")}`);
  expect(bad, `axe on ${screen}`).toEqual([]);
}
