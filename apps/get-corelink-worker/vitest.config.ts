/**
 * Vitest config for `get-corelink-worker`.
 *
 * Test strategy (mirrors `worker/vitest.config.mts`):
 *   - Tests run in the standard Node.js environment, NOT against the
 *     real workerd runtime. The worker is a pure static-string render
 *     with zero DO / KV / R2 / D1 / outbound-fetch dependencies, so
 *     Node + a thin Env stub exercises 100% of the code paths.
 *   - `@cloudflare/vitest-pool-workers` is installed for parity with
 *     the rest of the monorepo but is NOT wired up — the pnpm
 *     workspace miniflare resolution issue documented in
 *     `worker/vitest.config.mts` §17-22 applies here too.
 *
 * Coverage: not enforced via thresholds (single small file, tests
 * already cover every branch). The CI gate is a green test run.
 */

import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["tests/**/*.test.ts"],
    globals: true,
  },
  resolve: {
    extensions: [".ts", ".js"],
  },
});
