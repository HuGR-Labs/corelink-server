/**
 * Playwright config — WI-S16-007 ship gate.
 *
 * Targets:
 *   - chromium (mandatory, runs in PR CI smoke + scheduled full)
 *   - firefox  (CI matrix, optional via PROJECTS env var)
 *   - webkit   (CI matrix, optional)
 *
 * Tests live under ./playwright/e2e/. Fixtures + mocks under ./playwright/fixtures/.
 * Tests are FE-only: every backend `/v1/*` call is mocked via route.fulfill().
 *
 * Trace / video / screenshot strategy follows spec contract S-16 §15 + WI-S16-007 §6:
 *   - trace: 'on-first-retry'  (cheap by default, full on flake)
 *   - video: 'retain-on-failure'
 *   - screenshot: 'only-on-failure'
 *
 * Webserver: we start `pnpm dev` against port 3000 unless E2E_BASE_URL is set
 * (allowing `pnpm e2e` to run against a pre-warmed `pnpm build && pnpm start`).
 */

import { defineConfig, devices } from "@playwright/test";

const PORT = Number(process.env["PORT"] ?? 3000);
const BASE_URL = process.env["E2E_BASE_URL"] ?? `http://localhost:${PORT}`;

// Allow trimming the project matrix from CI via env (e.g. PROJECTS=chromium for fast PRs).
const PROJECTS_ENV = (process.env["PROJECTS"] ?? "chromium").split(",").map((s) => s.trim());

const allProjects = [
  {
    name: "chromium",
    use: { ...devices["Desktop Chrome"] },
  },
  {
    name: "firefox",
    use: { ...devices["Desktop Firefox"] },
  },
  {
    name: "webkit",
    use: { ...devices["Desktop Safari"] },
  },
];

export default defineConfig({
  testDir: "./playwright/e2e",
  fullyParallel: true,
  forbidOnly: !!process.env["CI"],
  retries: process.env["CI"] ? 2 : 0,
  workers: process.env["CI"] ? 2 : undefined,
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
  // Only auto-start the dev server when running locally; CI uses a pre-warmed server.
  webServer: process.env["E2E_BASE_URL"]
    ? undefined
    : {
        command: "pnpm dev",
        url: BASE_URL,
        reuseExistingServer: !process.env["CI"],
        timeout: 120_000,
        env: {
          PORT: String(PORT),
          NEXT_TELEMETRY_DISABLED: "1",
          // Force test-mode auth fixtures (see playwright/fixtures/clerk.ts).
          NEXT_PUBLIC_E2E_TEST_MODE: "1",
        },
      },
});
