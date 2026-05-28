/**
 * Vitest configuration for miniflare v4 programmatic integration tests.
 *
 * These tests run in the standard Node.js environment but spin up a real
 * miniflare v4 / workerd instance programmatically in beforeAll/afterAll.
 * This satisfies the R2 mandate: "miniflare integration tests for DO are
 * mandatory" — the worker and DO are executed inside workerd, not Node.js.
 *
 * Note: @cloudflare/vitest-pool-workers (vitest-pool-workers plugin) has a
 * runtime compatibility issue with vitest@4.1.7 — the cloudflare:test
 * `@vitest/expect` module resolution fails inside workerd when pool-workers
 * 0.16.10 bundles its own version. We use the programmatic miniflare v4 API
 * instead, which achieves the same workerd-backed test guarantee.
 *
 * Tests matching `tests/*.miniflare.test.ts` are included here.
 *
 * Run:
 *   cd worker && npx vitest run --config vitest.miniflare.config.mts
 */

import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["tests/**/*.miniflare.test.ts"],
    globals: true,
    testTimeout: 60_000,
    hookTimeout: 90_000,
    setupFiles: ["./tests/setup.ts"],
  },
  resolve: {
    extensions: [".ts", ".js", ".mts", ".mjs"],
  },
});
