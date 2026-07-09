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
  page: async ({ page, baseURL }, use) => {
    await page.request.post(`${baseURL}/api/v1/_e2e/reset`);
    await use(page);
  },
});

export { expect };
