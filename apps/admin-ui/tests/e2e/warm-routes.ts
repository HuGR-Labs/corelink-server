/**
 * Playwright `globalSetup` — compile every route the suite visits BEFORE the
 * first assertion runs.
 *
 * Why this exists: the critical-flow suite runs against `next dev`, where Next
 * compiles a route **on its first request**. On a 2-core hosted runner that
 * first-compile lands INSIDE a spec's 30 s locator timeout, so the suite failed
 * on innocent code — and because whichever route happened to be hit first won
 * the race, a DIFFERENT spec failed each run (`h1#audit-heading`,
 * `h1#op-heading`, `h1#page-heading`). That is why the gate read as "flaky"
 * rather than "misconfigured": it blocked #923 four times on 2026-08-01 while
 * the same specs passed locally in 16-57 s.
 *
 * Why NOT `next build` + `next start` (the obvious fix, and the one that was
 * tried first): the suite's synthetic-session auth bypass is DOUBLE-gated —
 * `src/middleware.ts` requires `NEXT_PUBLIC_E2E_TEST_MODE === "1"` **AND**
 * `NODE_ENV !== "production"`, precisely so a stray env var can never disable
 * auth in a production deploy. `next start` sets `NODE_ENV=production`, so the
 * bypass switches off and EVERY spec fails on the 403 panel. That guard is
 * correct and must not be weakened to make a test suite pass — so the suite
 * stays in dev mode and we remove the compile from the critical path instead.
 *
 * Playwright runs `globalSetup` AFTER `webServer` is up, so a plain fetch of
 * each route here is enough to trigger (and wait for) its compilation. The
 * cost is paid once, off the assertion clock.
 */
import type { FullConfig } from "@playwright/test";

/**
 * Every route a spec navigates to (`tests/e2e/**` + the page objects), plus the
 * mock API root the typed clients hit. Keep in sync when a spec adds a route:
 * a missing entry is not a failure, only a route that pays its compile inside
 * the first assertion that touches it.
 */
const ROUTES = [
  "/sign-in",
  "/en/customer",
  "/en/customer/audit",
  "/en/customer/audit/visualization",
  "/en/customer/billing",
  "/en/customer/keys",
  "/en/customer/team",
  "/en/admin/audit",
  "/en/admin/tenants",
  "/en/admin/ops/op_byok_001",
  "/en/admin/ops/op_dsr_001",
];

export default async function warmRoutes(config: FullConfig): Promise<void> {
  const base =
    config.projects[0]?.use?.baseURL ??
    process.env["E2E_BASE_URL"] ??
    `http://localhost:${process.env["PORT"] ?? 3010}/corelink`;

  const started = Date.now();
  // Sequential on purpose: parallel first-compiles on a 2-core runner contend
  // for the same CPU and can push an individual route PAST the fetch timeout,
  // which is the very failure being designed out.
  for (const route of ROUTES) {
    const url = `${base}${route}`;
    try {
      // A 120 s ceiling per route: generous enough for a cold turbopack
      // compile on a starved runner, bounded so a genuinely hung route fails
      // the run loudly instead of hanging until the job timeout.
      const res = await fetch(url, {
        redirect: "manual",
        signal: AbortSignal.timeout(120_000),
      });
      // Any status is fine — a redirect or a 403 still means the route
      // COMPILED, which is the only thing being warmed here. Correctness is
      // the specs' job, not this file's.
      // `process.stdout.write`, not `console.log`: the admin-ui eslint config
      // allows only console.warn/error, and neither is honest for routine
      // progress output from a setup script.
      process.stdout.write(`[warm] ${res.status} ${route}\n`);
    } catch (err) {
      // Never fail the run on a warm miss: the spec that owns this route will
      // report a real, readable failure. Warming is an optimisation, not a gate.
      const msg = err instanceof Error ? err.message : String(err);
      console.warn(`[warm] SKIPPED ${route} — ${msg}`);
    }
  }
  process.stdout.write(
    `[warm] ${ROUTES.length} routes compiled in ${Date.now() - started}ms\n`,
  );
}
