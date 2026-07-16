/**
 * Shared Playwright `test` for the WI-S16-007 legacy suite (`playwright/e2e/`).
 *
 * Extends the base `test` with basePath (`/corelink`) navigation awareness.
 * The app is mounted under `basePath` (see next.config.ts), so the dev server
 * serves every route under `/corelink`. The specs call `page.goto("/en/…")` /
 * `"/"` — absolute-path forms that Playwright resolves against `baseURL`'s
 * ORIGIN only (a leading-slash path replaces the whole path), which would drop
 * the mount prefix and 404 every navigation. Re-attach it here, ONCE — exactly
 * as Next auto-prefixes framework-generated links — so every `goto` call site
 * (including the data-driven a11y/screenshot/locale sweeps) stays prefix-free
 * and the base path is a single source of truth (`APP_BASE_PATH`).
 *
 * Every spec imports `{ test, expect }` from here instead of `@playwright/test`.
 */
import { test as base, expect } from "@playwright/test";
import { APP_BASE_PATH } from "../../src/lib/route-matcher";

export const test = base.extend({
  page: async ({ page }, use) => {
    const originalGoto = page.goto.bind(page);
    page.goto = ((url: string, options?: Parameters<typeof originalGoto>[1]) => {
      const target =
        typeof url === "string" &&
        url.startsWith("/") &&
        url !== APP_BASE_PATH &&
        !url.startsWith(`${APP_BASE_PATH}/`)
          ? `${APP_BASE_PATH}${url}`
          : url;
      return originalGoto(target, options);
    }) as typeof page.goto;
    await use(page);
  },
});

export { expect };
