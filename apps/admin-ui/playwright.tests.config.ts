/**
 * Playwright config for the wt-r3-7 critical-flow suite under `tests/e2e/`.
 *
 * Separate from the original `playwright.config.ts` (which targets
 * `playwright/e2e/`) so the new suite can iterate independently and the
 * legacy fixme'd specs are not blockers for the ship gate.
 *
 * Browsers:
 *   - chromium (default, required to pass locally + CI)
 *   - firefox / webkit (enabled when PROJECTS env var includes them — CI matrix)
 *
 * Mock strategy: see tests/e2e/fixtures/users.ts and the catch-all route at
 * src/app/api/v1/[...path]/route.ts. Backend calls are routed to /api/v1/*
 * via NEXT_PUBLIC_CORELINK_API_URL=/api so SSR + client fetches both hit the
 * deterministic mock.
 */
import { defineConfig, devices } from "@playwright/test";

const PORT = Number(process.env["PORT"] ?? 3010);
const BASE_URL = process.env["E2E_BASE_URL"] ?? `http://localhost:${PORT}`;
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
  testDir: "./tests/e2e",
  testMatch: /.*\.spec\.ts$/,
  fullyParallel: false,
  forbidOnly: !!process.env["CI"],
  retries: process.env["CI"] ? 2 : 0,
  workers: 1,
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
  webServer: process.env["E2E_BASE_URL"]
    ? undefined
    : {
        command: `pnpm dev --port ${PORT}`,
        url: BASE_URL,
        reuseExistingServer: !process.env["CI"],
        timeout: 180_000,
        env: {
          PORT: String(PORT),
          NEXT_TELEMETRY_DISABLED: "1",
          NEXT_PUBLIC_E2E_TEST_MODE: "1",
          NEXT_PUBLIC_CORELINK_API_URL: "/api",
        },
      },
});
