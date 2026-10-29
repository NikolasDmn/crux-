import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chromium, expect, test } from "@playwright/test";
import { logIn, signedUp } from "./support/stack";

// Spec 6: Lighthouse on Home (desktop), cold load: Performance >= 90, Accessibility 100. Opt-in
// (CRUX_E2E_LIGHTHOUSE=1) and against the production build the gateway serves (`make web`).
// Status 2026-10-29: accessibility 100; performance ~65-70 on the local stack. The heading paints
// only once the entry, the session refresh, the route chunks (and their many small imports) and
// the font have arrived one after another, over the local gateway's plain HTTP/1.1. Not yet fixed.
test("Lighthouse on Home", async () => {
  test.skip(!process.env.CRUX_E2E_LIGHTHOUSE, "opt-in: set CRUX_E2E_LIGHTHOUSE=1");
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
      throttlingMethod: "simulate",
      disableStorageReset: true, // keeps the sign-in cookie
    });
    const lhr = result!.lhr;
    expect(lhr.finalDisplayedUrl, "measured Home, signed in").not.toContain("/login");
    const score = (k: string) => Math.round((lhr.categories[k]?.score ?? 0) * 100);
    const a = lhr.audits;
    console.log(
      `Lighthouse: performance ${score("performance")}, accessibility ${score("accessibility")}; ` +
        `FCP ${a["first-contentful-paint"]?.displayValue}, LCP ${a["largest-contentful-paint"]?.displayValue}`,
    );
    const failing = Object.values(a)
      .filter((x) => x.score !== null && x.score < 1 && x.scoreDisplayMode === "binary")
      .map((x) => x.id);
    expect(score("accessibility"), `accessibility audits failing: ${failing.join(", ")}`).toBe(100);
    expect(score("performance")).toBeGreaterThanOrEqual(90);
  } finally {
    await context.close();
  }
});
