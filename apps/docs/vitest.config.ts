import { defineConfig } from "vitest/config";

/**
 * Vitest config for the CoreLink docs site.
 *
 * Tests run in node, with globals enabled. Tests live in `tests/` and
 * assert structural invariants over the Docusaurus configuration
 * (sidebar shape, locale presence, navbar links, robots.txt, etc.).
 */
export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    include: ["tests/**/*.test.ts"],
    reporters: "default",
  },
});
