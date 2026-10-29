import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium, expect, test } from "@playwright/test";
import { logIn, signedUp } from "./support/stack";

// Spec 6: Lighthouse on Home (desktop): Performance >= 90, Accessibility 100. Run against the
// production build the gateway serves (`make web`, no CRUX_E2E_BASE_URL); it's skipped against the
// dev server, whose unbundled modules say nothing about performance.
test("Lighthouse on Home", async () => {
  test.skip(!!process.env.CRUX_E2E_BASE_URL?.includes(":5173"), "the dev server isn't the production build");
  test.setTimeout(180_000);
  const user = await signedUp("lighthouse");
  const port = 9300 + Math.floor(Math.random() * 500);
  // A persistent context is the browser's default one, where Lighthouse opens its tab: it then has
  // the sign-in cookie (an isolated context would leave Lighthouse measuring the login page).
  const context = await chromium.launchPersistentContext(mkdtempSync(join(tmpdir(), "crux-lh-")), {
    args: [`--remote-debugging-port=${port}`],
    baseURL: test.info().project.use.baseURL,
  });
  try {
    const page = await context.newPage();
    await logIn(page, user);
    // A cold load: keep the sign-in cookie, drop the cached bundle.
    const cdp = await context.newCDPSession(page);
    await cdp.send("Network.clearBrowserCache");
    const { default: lighthouse } = await import("lighthouse");
    const result = await lighthouse(page.url(), {
      port,
      output: "json",
      logLevel: "error",
      onlyCategories: ["performance", "accessibility"],
      formFactor: "desktop",
      screenEmulation: { mobile: false, width: 1350, height: 940, deviceScaleFactor: 1, disabled: false },
      throttlingMethod: "devtools",
      // Same tab, same session: the refresh cookie is in this browser.
      disableStorageReset: true,
    });
    expect(result!.lhr.finalDisplayedUrl, "measured Home, signed in").not.toContain("/login");
    const categories = result!.lhr.categories;
    const score = (k: string) => Math.round((categories[k]?.score ?? 0) * 100);
    const failing = Object.values(result!.lhr.audits)
      .filter((a) => a.score !== null && a.score < 1 && a.scoreDisplayMode === "binary")
      .map((a) => a.id);
    console.log(`Lighthouse: performance ${score("performance")}, accessibility ${score("accessibility")}`);
    const a = result!.lhr.audits;
    for (const id of [
      "first-contentful-paint",
      "largest-contentful-paint",
      "total-blocking-time",
      "cumulative-layout-shift",
      "speed-index",
    ]) {
      console.log(`  ${id}: ${a[id]?.displayValue} (score ${a[id]?.score})`);
    }
    console.log(
      `  LCP element: ${JSON.stringify((a["largest-contentful-paint-element"]?.details as { items?: unknown[] })?.items?.[0] ?? null).slice(0, 300)}`,
    );
    console.log(
      `  lcp phases: ${JSON.stringify((a["largest-contentful-paint-element"]?.details as { items?: unknown[] })?.items?.[1] ?? null).slice(0, 400)}`,
    );
    console.log(
      `  bootup: ${a["bootup-time"]?.displayValue}; main thread: ${a["mainthread-work-breakdown"]?.displayValue}`,
    );
    console.log(`  chains: ${JSON.stringify(a["critical-request-chains"]?.details).slice(0, 200)}`);
    const reqs = (a["network-requests"]?.details as { items?: Record<string, unknown>[] })?.items ?? [];
    reqs.sort((x, y) => Number(y.networkEndTime) - Number(x.networkEndTime));
    for (const r of reqs.slice(0, 10)) {
      console.log(
        `  req ${String(r.url).replace(/^https?:\/\/[^/]+/, "")} ${r.transferSize}B/${r.resourceSize}B ${r.resourceType} ${Math.round(Number(r.networkEndTime))}ms`,
      );
    }
    console.log(
      `  blocking: ${JSON.stringify((a["render-blocking-resources"]?.details as { items?: unknown[] })?.items ?? []).slice(0, 300)}`,
    );
    for (const o of Object.values(a).filter((x) => x.details?.type === "opportunity" && (x.numericValue ?? 0) > 100)) {
      console.log(`  opportunity ${o.id}: ${o.displayValue}`);
    }
    expect(score("accessibility"), `accessibility audits failing: ${failing.join(", ")}`).toBe(100);
    expect(score("performance")).toBeGreaterThanOrEqual(90);
  } finally {
    await context.close();
  }
});
