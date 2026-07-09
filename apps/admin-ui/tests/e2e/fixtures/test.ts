/**
 * Shared Playwright `test` for the wt-r3-7 critical-flow suite.
 *
 * Extends the base `test` with a per-test reset of the server-side mock state
 * (`src/lib/e2e-mock-fixtures.ts`). The mock state is a persistent
 * `globalThis`-backed singleton so a mutation survives Next.js dev route
 * recompiles within a test; the flip side is that it also survives ACROSS
 * tests, so each test must start from a pristine fixture. This fixture hits the
 * test-only `POST /api/v1/_e2e/reset` endpoint before the test body (and before
 * any spec `beforeEach`) to guarantee deterministic, leak-free isolation.
 *
 * Every spec imports `{ test, expect }` from here instead of `@playwright/test`.
 */
import { test as base, expect } from "@playwright/test";

export const test = base.extend({
  // Override the `page` fixture so the reset runs during fixture setup, i.e.
  // before the test's own `beforeEach` hooks (which sign the user in).
  //
  // Explicit generous timeout (NOT the 10s `actionTimeout`): this setup POST is
  // the FIRST hit to the catch-all `/api/v1/[...path]` route on a cold/contended
  // dev server (fresh every CI run), where Next compiles the route on-demand —
  // that first compile can exceed 10s and would otherwise flake every suite at
  // its very first test. Route compilation is a one-time cost, so a wide setup
  // budget is correct here; steady-state resets return in single-digit ms.
  page: async ({ page, baseURL }, use) => {
    await page.request.post(`${baseURL}/api/v1/_e2e/reset`, { timeout: 60_000 });
    await use(page);
  },
});

export { expect };
