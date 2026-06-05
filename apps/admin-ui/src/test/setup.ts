// jest-dom matchers: extend this package's vitest `expect` directly (the
// `/vitest` entry binds the wrong hoisted vitest in this monorepo — see
// apps/admin-ui/vitest.setup.ts).
import * as jestDomMatchers from "@testing-library/jest-dom/matchers";
import { expect } from "vitest";

expect.extend(jestDomMatchers);
