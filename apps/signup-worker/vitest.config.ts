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
  },
});
