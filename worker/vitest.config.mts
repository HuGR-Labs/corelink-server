/**
 * Vitest configuration for the CoreLink worker package.
 *
 * Test strategy:
 * - Unit tests run in the standard Node.js environment.
 *   These test pure functions: route matching, auth middleware, CORS, error
 *   envelopes, timing-pad, and the DO state machine. They do NOT require
 *   a live Cloudflare Workers runtime.
 *
 * - Workerd integration tests (`*.miniflare.test.ts`) run in a REAL workerd
 *   runtime via the separate `vitest.miniflare.config.mts` — programmatic
 *   Miniflare v4 loading the freshly-built bundle by `scriptPath` (+ nodejs_compat).
 *   They cover DO dispatch / cross-tenant isolation / CORS end-to-end, which the
 *   node pool can't exercise. Excluded from THIS (node) config below; both run in
 *   the `worker-vitest` CI gate.
 *
 * Coverage provider: istanbul (v8 requires node:inspector which is not
 * available in the Workers runtime; istanbul works on any JS runtime).
 *
 * (Historical note: an older comment here claimed vitest-pool-workers couldn't be
 * used because miniflare resolved as v3-vs-v4 — that was never true in this tree
 * (only miniflare v4 resolves). The real past breakage was a stale `dist/index.js`
 * loaded as a string; the miniflare harness now always rebuilds + loads via
 * scriptPath. See vitest.miniflare.config.mts.)
 */

import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["tests/**/*.test.ts"],
    // *.miniflare.test.ts run under vitest.miniflare.config.mts (workerd pool), not the node pool.
    exclude: ["tests/**/*.miniflare.test.ts"],
    globals: true,
    coverage: {
      provider: "istanbul",
      include: ["src/**/*.ts"],
      exclude: [
        "src/**/*.d.ts",
      ],
      thresholds: {
        // Global thresholds across all src files combined.
        // index.ts achieves ~95% coverage; durable_object.ts achieves ~41%
        // because the Cloudflare Container API (container.start(), getTcpPort(),
        // container.running, container.destroy()) is only accessible inside
        // the CF workerd runtime — these paths CANNOT be exercised in Node.js.
        // The wrangler dev --local smoke test (pnpm test:wrangler) validates
        // the runtime path. Combined threshold reflects testable-in-Node.js code.
        lines: 65,
        functions: 70,
        branches: 50,
        statements: 65,
        // Per-file overrides: index.ts must hit 70% (it's the primary shim).
        // durable_object.ts container-dependent paths are excluded from threshold.
        "src/index.ts": {
          lines: 90,
          functions: 95,
          branches: 75,
          statements: 90,
        },
        "src/durable_object.ts": {
          // Lower threshold acknowledges CF Container runtime dependency:
          // ~52% of the file requires state.container (only in CF workerd).
          // The testable Node.js paths (state machine, health probe, stop,
          // alarm skeleton, persistence, constant-time compare) ARE covered.
          lines: 35,
          functions: 60,
          branches: 25,
          statements: 35,
        },
        "src/rollout_controller.ts": {
          // RolloutController is a Phase B stub (WASM bridge deferred to Phase C).
          // The class exists solely to satisfy the wrangler.toml DO binding export
          // requirement. Full implementation and tests land in Phase C.
          lines: 0,
          functions: 0,
          branches: 0,
          statements: 0,
        },
      },
      reporter: ["text", "json"],
    },
    setupFiles: ["./tests/setup.ts"],
  },
  resolve: {
    extensions: [".ts", ".js", ".mts", ".mjs"],
  },
});
