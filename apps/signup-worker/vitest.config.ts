import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["tests/**/*.test.ts"],
    // Vitest 4 changed `vi.spyOn` to REUSE an already-installed spy instead of
    // creating a fresh one, so a `beforeEach` that re-spies `globalThis.fetch`
    // now accumulates `.mock.calls` across tests (in v3 each re-spy was fresh).
    // Restore spies to their originals after every test so each `beforeEach`
    // re-installs a clean spy — the per-test isolation the suite was written for.
    restoreMocks: true,
    // ── Coverage floor (WP-3, 2026-08-29) ──────────────────────────────────
    // Enforced by `.github/workflows/signup-worker-vitest.yml`, which runs
    // `npx vitest run --coverage`. vitest only evaluates `thresholds` when the
    // coverage provider is enabled, so the workflow flag and this block are a
    // pair — removing either one silently disarms the gate.
    //
    // `include` is set EXPLICITLY rather than left to the default. By default
    // v8 coverage only reports files that a test happened to import, so adding
    // a brand-new untested src file would leave the percentage untouched and
    // the floor would never notice. With `include: src/**/*.ts` a new
    // zero-coverage file drags the total down and trips the floor — measured:
    // the same suite reports 88.23% lines on imported-files-only vs 79.09%
    // across all of src/, i.e. ~9 points of code the default was blind to.
    //
    // Measured 2026-08-29 (provider v8, include src/**/*.ts):
    //   L 79.09  S 77.81  F 76.87  B 67.47
    // Floors sit ~5 points below measured. Ratchet UP as coverage improves.
    coverage: {
      provider: "v8",
      include: ["src/**/*.ts"],
      exclude: ["src/**/*.d.ts"],
      reporter: ["text"],
      thresholds: {
        lines: 74,
        functions: 71,
        branches: 62,
        statements: 72,
      },
    },
  },
});
