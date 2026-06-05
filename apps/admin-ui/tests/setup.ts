// jest-dom matchers: extend this package's vitest `expect` directly. The
// `@testing-library/jest-dom/vitest` entry extends the wrong (hoisted 4.1.7)
// vitest in this monorepo — see vitest.setup.ts for the full explanation.
import * as jestDomMatchers from "@testing-library/jest-dom/matchers";
import { afterEach, expect } from "vitest";
import { cleanup } from "@testing-library/react";

expect.extend(jestDomMatchers);

afterEach(() => {
  cleanup();
});
