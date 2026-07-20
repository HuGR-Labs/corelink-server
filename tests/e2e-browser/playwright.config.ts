import { defineConfig, devices } from "@playwright/test";

/**
 * Real-browser e2e vs PROD humangr.com/corelink. No webServer — we drive the
 * DEPLOYED app with a REAL Clerk session (via @clerk/testing), the only path
 * that produces a token the prod worker's Clerk verification accepts (the
 * headless FAPI mint is rejected 401; a browser session is not).
 */
export default defineConfig({
  testDir: "./specs",
  globalSetup: "./global.setup.ts",
  timeout: 120_000,
  expect: { timeout: 20_000 },
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: process.env["CORELINK_APP_URL"] ?? "https://humangr.com",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
});
