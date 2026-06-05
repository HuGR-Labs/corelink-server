// Augment Vitest's `expect` with the jest-dom + jest-axe matchers we use.
//
// jest-dom ships `@testing-library/jest-dom/vitest`, but its own augmentation
// imports `TestingLibraryMatchers` as a *named* import from an `export =`
// namespace module — which resolves to nothing under this project's
// (moduleResolution: bundler, esModuleInterop, skipLibCheck) config, so the
// matchers silently never attach to `Assertion`. We re-create the augmentation
// here with an `import()` type-query, which resolves the namespace member
// correctly and pulls in the full matcher set (and errors loudly if it can't).
import "vitest";

type JestDomMatchers<T> = import("@testing-library/jest-dom/matchers").TestingLibraryMatchers<
  unknown,
  T
>;

declare module "vitest" {
  interface Assertion<T = unknown> extends JestDomMatchers<T> {
    toHaveNoViolations(): T;
  }
  interface AsymmetricMatchersContaining extends JestDomMatchers<unknown> {
    toHaveNoViolations(): unknown;
  }
}
