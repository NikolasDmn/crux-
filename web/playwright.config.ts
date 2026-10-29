import { defineConfig, devices } from "@playwright/test";

// The Playwright suite (frontend-spec 9) runs against a running Compose stack with `make seed`,
// AI via the fake provider: `make up-ai seed`, then `npm run e2e`. The gateway serves dist/, so
// build first (`make web`), or point CRUX_E2E_BASE_URL at `npm run dev` (http://localhost:5173).
// The journeys share one stack and build on each other within a file, so they run one at a time.
const port = process.env.CRUX_HTTP_PORT ?? "8000";

export default defineConfig({
  testDir: "e2e",
  globalSetup: "./e2e/global-setup.ts",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: process.env.CRUX_E2E_BASE_URL ?? `http://localhost:${port}`,
    actionTimeout: 10_000,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    timezoneId: "Europe/Amsterdam",
    locale: "en-GB",
  },
  projects: [
    {
      name: "desktop",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } },
      testIgnore: /a11y/,
    },
    // Every screen through axe, in light and dark, at 1280 and 375 px (spec 2.7, 9).
    { name: "a11y", testMatch: /a11y/, use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } } },
  ],
});
