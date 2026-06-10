/**
 * Playwright config for the STANDALONE public-flip authenticated smoke.
 *
 * Deliberately separate from apps/admin-ui's three Playwright configs —
 * this harness must be runnable from a clean checkout with only npm and
 * two env vars (SMOKE_USER_EMAIL / SMOKE_USER_PASSWORD), with no
 * workspace install and no webServer.
 *
 * Targets LIVE production by default (override via SMOKE_BASE_URL).
 * Workers = 1 and fullyParallel = false: one real user session, serial flow.
 * Retries = 0: a smoke against prod must not paper over flakes — a red run
 * is a signal, not a retry candidate.
 */
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testMatch: "authenticated-smoke.spec.ts",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 120_000,
  reporter: [["list"]],
  use: {
    ...devices["Desktop Chrome"],
    baseURL: process.env["SMOKE_BASE_URL"] ?? "https://corelink-app.humangr.com",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
});
