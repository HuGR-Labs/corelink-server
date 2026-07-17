/**
 * Playwright config — production-surface regression suite.
 *
 * Distinct from:
 *   - playwright.config.ts        → localhost legacy suite (`playwright/e2e/`)
 *   - playwright.tests.config.ts  → localhost critical-flow suite (`tests/e2e/`)
 *
 * This config drives the specs under `./e2e/` against LIVE production URLs
 * (no `webServer` auto-start). It is the deploy canary that protects against
 * regressions like:
 *
 *   - Cloudflare Pages secrets missing → /sign-up returns 500
 *   - get.corelink.io route binding dropped → install one-liner 404
 *   - Docs build broke /legal/* or /pricing → public surface dead
 *
 * Env overrides:
 *   E2E_BASE_URL              app URL          (default https://humangr.com/corelink)
 *   E2E_DOCS_URL              docs URL         (default https://docs.corelink.humangr.com)
 *   E2E_INSTALL_URL           install Worker   (default https://get.corelink.io)
 *   E2E_AUTH_STORAGE_STATE    path to Playwright storageState.json from a real
 *                             signed-in operator. When set, enables the
 *                             auth-gated /welcome DOM block in signup-welcome.spec.ts.
 *   PROJECTS                  comma-list of browsers (default `chromium`)
 *
 * Retries are capped at 2 per the agent charter — flaky tests > 2 retries
 * are a bug, not a configuration knob.
 */
import { defineConfig, devices } from "@playwright/test";

const BASE_URL = process.env["E2E_BASE_URL"] ?? "https://humangr.com/corelink";
const PROJECTS_ENV = (process.env["PROJECTS"] ?? "chromium")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);

const allProjects = [
  { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  { name: "firefox", use: { ...devices["Desktop Firefox"] } },
  { name: "webkit", use: { ...devices["Desktop Safari"] } },
];

export default defineConfig({
  testDir: "./e2e",
  testMatch: /.*\.spec\.ts$/,
  // Production endpoints — keep workers low to avoid hammering the live edge.
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env["CI"],
  retries: process.env["CI"] ? 2 : 0,
  reporter: process.env["CI"]
    ? [["github"], ["html", { open: "never" }], ["list"]]
    : [["list"]],
  timeout: 60_000,
  expect: { timeout: 10_000 },
  use: {
    baseURL: BASE_URL,
    trace: "on-first-retry",
    video: "retain-on-failure",
    screenshot: "only-on-failure",
    actionTimeout: 10_000,
    navigationTimeout: 30_000,
  },
  projects: allProjects.filter((p) => PROJECTS_ENV.includes(p.name)),
  // NO webServer — this suite drives live production URLs.
});
