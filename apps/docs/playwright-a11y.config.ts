/**
 * Playwright a11y sweep config — WI-S18-005 deliverable 3.
 *
 * WCAG 2.2 AA via @axe-core/playwright; zero serious / critical violations
 * allowed across all routes. Per spec contract S-18 §5.6 R-S18-14 +
 * Quality Standard 14.s18.4.
 *
 * Pages exercised live in `playwright/a11y-sweep.spec.ts`. Locally:
 *
 *   pnpm exec playwright test --config playwright-a11y.config.ts
 *
 * The dev server must be running on http://localhost:3000 (Docusaurus
 * `pnpm start`). CI workflow `.github/workflows/docs-a11y.yml` boots it
 * automatically via `webServer.command`.
 */

import { defineConfig, devices } from "@playwright/test";

const BASE_URL = process.env.DOCS_BASE_URL ?? "http://localhost:3000";

export default defineConfig({
  testDir: "./playwright",
  testMatch: /a11y-.*\.spec\.ts/,
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: [
    ["list"],
    ["html", { outputFolder: "playwright-report/a11y", open: "never" }],
    ["json", { outputFile: "playwright-report/a11y-results.json" }],
  ],
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: process.env.SKIP_WEBSERVER
    ? undefined
    : {
        command: "pnpm start --no-open --port 3000",
        url: BASE_URL,
        reuseExistingServer: !process.env.CI,
        timeout: 180_000,
      },
});
