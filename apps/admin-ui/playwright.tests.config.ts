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
import { APP_BASE_PATH } from "./src/lib/route-matcher";

const PORT = Number(process.env["PORT"] ?? 3010);
// The app is mounted under `basePath` (`/corelink`, see next.config.ts), so the
// local dev server serves every route — including the root landing page used as
// the webServer readiness probe — under that prefix. Bake the base path into the
// localhost default so `webServer.url` resolves to a real 200 (root at `/` 404s
// under basePath) and Playwright's `baseURL` origin is correct. When
// `E2E_BASE_URL` is provided externally (e.g. the prod-surface run passes the
// full `https://humangr.com/corelink`) it ALREADY carries the base path — use it
// verbatim, never re-append (the spec `goto` prefixer keys off the same constant).
const BASE_URL =
  process.env["E2E_BASE_URL"] ?? `http://localhost:${PORT}${APP_BASE_PATH}`;
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
  // The shown-once PAT reveal modal copies the token via
  // navigator.clipboard.writeText. The `clipboard-read`/`clipboard-write`
  // permissions are a CHROMIUM-only concept — passing them to WebKit/Firefox
  // throws `Unknown permission: clipboard-write` on newPage and fails the whole
  // project. Grant them ONLY to chromium; on webkit/firefox the copy path is
  // exercised without an explicit grant (the modal tolerates a rejected write).
  projects: allProjects
    .filter((p) => PROJECTS_ENV.includes(p.name))
    .map((p) =>
      p.name === "chromium"
        ? { ...p, use: { ...p.use, permissions: ["clipboard-read", "clipboard-write"] } }
        : p,
    ),
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
          // The app is mounted under `basePath` (`/corelink`), so the dev server
          // serves the catch-all mock at `/corelink/api/v1/*` — a bare `/api`
          // 404s (Next does NOT auto-prefix `fetch()`, only framework links). The
          // typed clients (admin/customer/dsr) read this env for BOTH the SSR
          // absolute origin (`http://127.0.0.1:PORT${API}`) and the client-side
          // relative base, so prefixing it here makes every data fetch land on
          // the mock under basePath. Kept as a single source of truth via
          // `APP_BASE_PATH`; prod builds set an absolute cross-origin API URL.
          NEXT_PUBLIC_CORELINK_API_URL: `${APP_BASE_PATH}/api`,
        },
      },
});
