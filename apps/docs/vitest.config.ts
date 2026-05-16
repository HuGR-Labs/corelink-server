import { defineConfig } from "vitest/config";

/**
 * Vitest config for the CoreLink docs site.
 *
 * Tests run in node, with globals enabled. Structural-invariant tests
 * (sidebar shape, locale presence, navbar links, robots.txt, etc.) live
 * under `tests/`. Pure-library unit tests (e.g. pricing formulas) live
 * alongside their source in `src/**` and end in `.test.ts`.
 */
export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    include: ["tests/**/*.test.ts", "src/**/*.test.ts"],
    reporters: "default",
  },
});
