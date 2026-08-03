/**
 * Playwright `globalSetup` — compile every route the suite visits BEFORE the
 * first assertion runs, and REFUSE to start the suite if the dev server does
 * not actually serve them.
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
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHY THIS FILE ALSO ASSERTS (2026-08-02)
 *
 * The first version of this file logged `res.status` and treated ANY status as
 * success ("Any status is fine — a redirect or a 403 still means the route
 * COMPILED"). That reasoning is right about 3xx and 403 and WRONG about 404,
 * and the difference is the whole ballgame: a 404 from these routes means Next
 * never resolved them, so nothing was warmed and every spec that visits one is
 * about to fail against `not-found.tsx` for a reason that has nothing to do
 * with the code under test.
 *
 * That is exactly what has been happening on `main`. On run 30769355309 (job
 * 91553642199, firefox) this setup printed:
 *
 *     [warm] 200 /sign-in
 *     [warm] 200 /en/customer
 *     [warm] 404 /en/customer/audit          <- and 8 more 404s, ~50 ms apart
 *     [warm] 11 routes compiled in 6445ms    <- reported as success
 *
 * The dev server had stopped resolving App Router routes ~12 s after boot and
 * never recovered: `/en/admin/tenants` was still 404 two minutes later, in
 * `auth.spec.ts`, whose `error-context.md` snapshot is literally
 * `- heading "404 — Page not found" [level=1]`. The correlation with the run's
 * result is exact — **1 passed, 12 failed**, and the single passing spec
 * (`customer-overview.spec.ts`) is the only one whose route was in the
 * 200 column above.
 *
 * So the harness OBSERVED the fault, at second 6 of a 20-minute job, and
 * reported it as OK. Twelve specs then failed one by one on locator timeouts,
 * 33 s each, three attempts apiece — 20 minutes of red that named the wrong
 * culprit. (The `ClerkJS: … unable to attribute this request to an instance`
 * error that accompanies these runs is NOT the cause and must not be chased:
 * it is emitted by the deliberate dummy publishable key in
 * `src/app/[locale]/(authenticated)/layout.tsx:39`, and it appears **15× in the
 * PASSING chromium job of that same run and 0× in the failing firefox job** —
 * it is a symptom of the app rendering, i.e. of success.)
 *
 * This file now polls each route until it RESOLVES and fails the whole run,
 * loudly and immediately, if any route stays 404 within a bounded budget. Two
 * properties matter and neither is negotiable:
 *
 *   - It cannot mask a real defect. The dev-server collapse is PERMANENT
 *     (proven above: the same route was still 404 two minutes and three spec
 *     retries later), so polling cannot turn a genuine failure green. It can
 *     only distinguish "still compiling" from "will never resolve".
 *   - A gate that cannot run says so in red. The failure is a `::error::`
 *     naming the exact routes and statuses, ~30 s in, instead of twelve
 *     misattributed locator timeouts 20 minutes in.
 */
import type { FullConfig } from "@playwright/test";

/**
 * Every route a spec navigates to (`tests/e2e/**` + the page objects), plus the
 * mock API root the typed clients hit. Keep in sync when a spec adds a route.
 *
 * Every entry MUST exist on disk under `src/app/[locale]/(authenticated)/`, so
 * a 404 here is a harness fault, never a product fault — none of these pages
 * calls `notFound()`, and in a healthy run all 11 return 200 (verified on the
 * passing chromium + webkit jobs of run 30769355309). That is what licenses
 * treating 404 as fatal below.
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

/**
 * A 120 s ceiling per attempt: generous enough for a cold turbopack compile on
 * a starved runner, bounded so a genuinely hung route fails the run loudly
 * instead of hanging until the job timeout.
 */
const ATTEMPT_TIMEOUT_MS = 120_000;
/**
 * Retries exist to cover "the route tree is still settling", not to paper over
 * a broken server. 4 attempts × 3 s of backoff is ~9 s of extra tolerance on a
 * route that a healthy runner serves in 1.2-2.2 s, and the observed failure
 * mode does not recover in 120 s let alone 9 s.
 */
const ATTEMPTS = 4;
const RETRY_BACKOFF_MS = 3_000;

interface ProbeResult {
  /** HTTP status, or `null` when the request itself failed. */
  status: number | null;
  /** Human-readable outcome for the log / error report. */
  detail: string;
}

/**
 * A route counts as RESOLVED on any status except 404 — a 3xx redirect or a
 * 403 still proves Next found and compiled the entry, which is all this file
 * is responsible for. Correctness of what the route renders is the specs' job.
 */
function isResolved(result: ProbeResult): boolean {
  return result.status !== null && result.status !== 404;
}

async function probe(url: string): Promise<ProbeResult> {
  try {
    const res = await fetch(url, {
      redirect: "manual",
      signal: AbortSignal.timeout(ATTEMPT_TIMEOUT_MS),
    });
    return { status: res.status, detail: String(res.status) };
  } catch (err) {
    return {
      status: null,
      detail: `request failed — ${err instanceof Error ? err.message : String(err)}`,
    };
  }
}

const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

export default async function warmRoutes(config: FullConfig): Promise<void> {
  const base =
    config.projects[0]?.use?.baseURL ??
    process.env["E2E_BASE_URL"] ??
    `http://localhost:${process.env["PORT"] ?? 3010}/corelink`;

  const started = Date.now();
  const unresolved: { route: string; detail: string }[] = [];

  // Sequential on purpose: parallel first-compiles on a 2-core runner contend
  // for the same CPU and can push an individual route PAST the fetch timeout,
  // which is the very failure being designed out.
  for (const route of ROUTES) {
    const url = `${base}${route}`;
    let result: ProbeResult = { status: null, detail: "not attempted" };

    for (let attempt = 1; attempt <= ATTEMPTS; attempt += 1) {
      result = await probe(url);
      if (isResolved(result)) break;
      if (attempt < ATTEMPTS) await sleep(RETRY_BACKOFF_MS);
    }

    if (isResolved(result)) {
      // `process.stdout.write`, not `console.log`: the admin-ui eslint config
      // allows only console.warn/error, and neither is honest for routine
      // progress output from a setup script.
      process.stdout.write(`[warm] ${result.detail} ${route}\n`);
    } else {
      process.stdout.write(
        `[warm] UNRESOLVED ${result.detail} ${route} (after ${ATTEMPTS} attempts)\n`,
      );
      unresolved.push({ route, detail: result.detail });
    }
  }

  const elapsed = Date.now() - started;

  if (unresolved.length > 0) {
    const list = unresolved.map((u) => `${u.route} -> ${u.detail}`).join("; ");
    // GitHub workflow command so the failure lands at the TOP of the run's
    // annotations instead of being buried under the specs it would otherwise
    // take down with it. `%0A` is the workflow-command newline escape.
    if (process.env["CI"]) {
      process.stdout.write(
        `::error title=admin-ui e2e: dev server never resolved ` +
          `${unresolved.length}/${ROUTES.length} app routes::` +
          `The Next dev server behind this suite answered 404 for routes that ` +
          `exist on disk, so EVERY spec visiting them would have failed against ` +
          `not-found.tsx for a reason unrelated to the code under test. ` +
          `Unresolved: ${list}. ` +
          `This is a harness/dev-server fault, not a spec failure — do NOT chase ` +
          `the ClerkJS attribution warning, which is emitted by the deliberate ` +
          `dummy key and also appears in passing runs. The dev server's own ` +
          `stdout is captured in this log ([WebServer] lines); start there.\n`,
      );
    }
    console.error(
      [
        "",
        "[warm] FATAL — the dev server did not resolve the routes this suite needs.",
        `[warm] ${unresolved.length} of ${ROUTES.length} routes stayed unresolved after ${ATTEMPTS} attempts each:`,
        ...unresolved.map((u) => `[warm]   ${u.route} -> ${u.detail}`),
        "[warm] Refusing to run the suite: the specs would fail on not-found.tsx",
        "[warm] and blame themselves. See the [WebServer] lines above for what",
        "[warm] Next itself reported while it stopped resolving routes.",
        "",
      ].join("\n"),
    );
    throw new Error(
      `admin-ui e2e harness: dev server never resolved ${unresolved.length}/${ROUTES.length} ` +
        `app routes (${list}). The suite was not run.`,
    );
  }

  process.stdout.write(
    `[warm] ${ROUTES.length} routes compiled in ${elapsed}ms\n`,
  );
}
