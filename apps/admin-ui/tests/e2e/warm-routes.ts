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
 * and the run's result was 1 passed, 12 failed — the single passing spec
 * (`customer-overview.spec.ts`) being the only one whose route was in the 200
 * column. So the harness OBSERVED the fault, at second 6 of a 20-minute job,
 * and reported it as OK. (The `ClerkJS: … unable to attribute this request to
 * an instance` error that accompanies these runs is NOT the cause and must not
 * be chased: it is emitted by the deliberate dummy publishable key in
 * `src/app/[locale]/(authenticated)/layout.tsx:39`, and it appears **15× in the
 * PASSING chromium job of that same run and 0× in the failing firefox job** —
 * it is a symptom of the app rendering, i.e. of success.)
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHAT THE PIPED DEV-SERVER STDOUT THEN SHOWED (2026-08-03)
 *
 * With `webServer.stdout: "pipe"` on, run **30781415861** (job 91586663074,
 * chromium) finally explained itself. Next's own log:
 *
 *     GET /en/customer/audit               200 in 1589ms
 *     GET /en/customer/audit/visualization 404 in  589ms (next.js: 519ms)
 *     GET /en/customer/audit/visualization 404 in  146ms (next.js:  15ms)
 *     …
 *     GET /en/customer/billing             200 in 1649ms   <- server still fine
 *     GET /en/admin/tenants                200 in 1612ms
 *     GET /en/admin/ops/op_byok_001        404 in   63ms (next.js:   6ms)
 *
 * Three facts follow, and together they name the fault exactly:
 *
 *   1. **It is not a collapse.** Five routes returned 200 AFTER the first 404.
 *      The "dev server dies ~12 s after boot" reading of the earlier run is
 *      REFUTED; the failure is per-route, not time-based.
 *   2. **The page is never compiled.** A 404 that costs 6-15 ms of `next.js`
 *      time is a route-tree MISS, not a compile error (a broken module graph
 *      is a 500 with an overlay, and a compile of these pages costs ~1.4 s).
 *      Next simply has no entry for the path, and never acquires one.
 *   3. **It is Turbopack-dev-only and intermittent.** The exact same three
 *      routes 404'd on run 30771910241 (a different branch) and served 200 on
 *      runs 30765324783 / 30773211630 / 30773554607 / 30779501435 / 30781832643
 *      — the last of which is the SAME COMMIT, ten minutes later. And in the
 *      failing run's own sibling job, `next build` emitted both of them into
 *      the production route table (`ƒ /[locale]/customer/audit/visualization`,
 *      `ƒ /[locale]/admin/ops/[op_id]`). **The pages are not broken; only the
 *      dev server's lazily-built route tree is**, and when it is short it is
 *      short at the DEEPEST entries (every warmed route at `/[locale]/a/b`
 *      resolved; all three at `/[locale]/a/b/c` did not).
 *
 * So the honest classification of a persistent 404 has two branches, and this
 * file now MEASURES which one it is instead of asserting it in a comment:
 *
 *   - no `page` file resolves the path on disk  → the warm list is wrong. A
 *     harness defect, fatal immediately, no retry, no recovery.
 *   - a `page` file DOES resolve it             → the dev server lost the
 *     entry. Nudge the directory chain Next had to read to find it (a
 *     create+delete inside each directory, which is the invalidation event its
 *     watcher acts on), re-probe once, and fail hard if it is still missing.
 *
 * Neither branch can mask a real defect. A route that genuinely does not exist
 * stays 404 through the recovery and fails; a route whose module graph is
 * broken never 404s in the first place. The recovery only buys back the case
 * where the filesystem says the route exists and the bundler disagrees — which
 * is, by the evidence above, the only case that has ever happened here.
 */
import { unlinkSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { FullConfig } from "@playwright/test";
import { directoryChainFor, resolveAppPage } from "./app-route-index";

/**
 * Every route a spec navigates to (`tests/e2e/**` + the page objects), plus the
 * mock API root the typed clients hit. Keep in sync when a spec adds a route.
 *
 * Every entry MUST resolve to a `page` file under `src/app/` — and that is now
 * CHECKED (`app-route-index.ts`), not merely asserted here, because it is the
 * difference between "this list is stale" and "turbopack lost the route".
 */
export const ROUTES = [
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
 * route that a healthy runner serves in 1.2-2.2 s.
 */
const ATTEMPTS = 4;
const RETRY_BACKOFF_MS = 3_000;
/**
 * ONE re-scan attempt. The recovery is cheap (it only runs on the failure path)
 * and bounded on purpose: if a single forced re-read of the route's own
 * directory chain does not produce the entry, the dev server is broken in a way
 * this harness should report, not keep poking.
 */
const RECOVERIES = 1;
/** Let the watcher coalesce and Next rebuild its tree before re-probing. */
const RESCAN_SETTLE_MS = 3_000;

export interface ProbeResult {
  /** HTTP status, or `null` when the request itself failed. */
  status: number | null;
  /** Human-readable outcome for the log / error report. */
  detail: string;
}

export interface Unresolved {
  route: string;
  detail: string;
}

/**
 * A route counts as RESOLVED on any status except 404 — a 3xx redirect or a
 * 403 still proves Next found and compiled the entry, which is all this file
 * is responsible for. Correctness of what the route renders is the specs' job.
 */
export function isResolved(result: ProbeResult): boolean {
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

/**
 * Force Next to re-read the directories it had to walk to discover `pageFile`.
 *
 * The observed fault is a route tree that is SHORT at a nested entry — Next
 * knows `customer/audit/` (it compiled its page) but not `customer/audit/
 * visualization/`. A directory-entry change is the event a filesystem watcher
 * cannot miss for a directory it is already watching, so a create+delete inside
 * every directory on the chain invalidates exactly the listings that could have
 * come back short, including the one above the missing entry.
 *
 * The probe file is a dotfile with no page/route extension, so it is never a
 * route even for the instant it exists, and it is removed in a `finally`.
 */
export function forceRouteTreeRescan(pageFile: string): void {
  for (const dir of directoryChainFor(pageFile)) {
    const probeFile = join(dir, `.turbopack-rescan-${process.pid}`);
    try {
      writeFileSync(probeFile, "");
    } catch {
      continue;
    } finally {
      try {
        unlinkSync(probeFile);
      } catch {
        /* best effort — the directory event has already fired */
      }
    }
  }
  // Also bump the page file itself: if the watcher DID see the directory but
  // the entry was dropped downstream, a content-change event on the page is the
  // other invalidation Next acts on.
  try {
    const now = new Date();
    utimesSync(pageFile, now, now);
  } catch {
    /* best effort */
  }
}

export interface WarmDeps {
  routes: readonly string[];
  probe: (route: string) => Promise<ProbeResult>;
  /** Filesystem truth: the `page` file serving this route, or `null`. */
  resolvePage: (route: string) => string | null;
  /** Force the dev server to re-discover the routes behind these page files. */
  rescan: (pageFiles: readonly string[]) => Promise<void>;
  log: (line: string) => void;
  sleep: (ms: number) => Promise<void>;
  attempts?: number;
  backoffMs?: number;
  recoveries?: number;
}

/**
 * The warm pass, with every side effect injected so both branches are testable
 * without a dev server. Returns the routes that stayed 404; throws only for the
 * harness-defect branch (a route with no `page` file on disk), which must never
 * be "recovered" from.
 */
export async function runWarmPasses(deps: WarmDeps): Promise<Unresolved[]> {
  const attempts = deps.attempts ?? ATTEMPTS;
  const backoffMs = deps.backoffMs ?? RETRY_BACKOFF_MS;
  const recoveries = deps.recoveries ?? RECOVERIES;

  let pending: readonly string[] = deps.routes;
  let unresolved: Unresolved[] = [];

  for (let pass = 0; pass <= recoveries; pass += 1) {
    unresolved = [];

    // Sequential on purpose: parallel first-compiles on a 2-core runner contend
    // for the same CPU and can push an individual route PAST the fetch timeout,
    // which is the very failure being designed out.
    for (const route of pending) {
      let result: ProbeResult = { status: null, detail: "not attempted" };
      for (let attempt = 1; attempt <= attempts; attempt += 1) {
        result = await deps.probe(route);
        if (isResolved(result)) break;
        if (attempt < attempts) await deps.sleep(backoffMs);
      }

      if (isResolved(result)) {
        // `process.stdout.write`, not `console.log`: the admin-ui eslint config
        // allows only console.warn/error, and neither is honest for routine
        // progress output from a setup script.
        deps.log(`[warm] ${result.detail} ${route}\n`);
      } else {
        deps.log(
          `[warm] UNRESOLVED ${result.detail} ${route} (after ${attempts} attempts)\n`,
        );
        unresolved.push({ route, detail: result.detail });
      }
    }

    if (unresolved.length === 0) return [];

    // Branch 1 — the warm list names a path nothing on disk serves. That is a
    // defect in THIS file, it cannot be recovered from, and retrying it would
    // only delay the honest answer.
    const phantom = unresolved.filter((u) => deps.resolvePage(u.route) === null);
    if (phantom.length > 0) {
      throw new Error(
        `admin-ui e2e harness: the warm list names ${phantom.length} route(s) with no ` +
          `page file under src/app — ${phantom.map((p) => p.route).join(", ")}. ` +
          `This is a defect in tests/e2e/warm-routes.ts (a stale ROUTES entry), ` +
          `NOT a dev-server fault: fix or remove the entry.`,
      );
    }

    if (pass === recoveries) break;

    // Branch 2 — every unresolved route DOES exist on disk, so the dev server
    // lost the entry. Force a re-read of the directories it had to walk.
    const pageFiles = unresolved
      .map((u) => deps.resolvePage(u.route))
      .filter((f): f is string => f !== null);
    deps.log(
      `[warm] ${unresolved.length} route(s) exist on disk but the dev server has no ` +
        `entry for them — forcing a route-tree re-scan and re-probing once.\n`,
    );
    await deps.rescan(pageFiles);
    pending = unresolved.map((u) => u.route);
  }

  return unresolved;
}

export default async function warmRoutes(config: FullConfig): Promise<void> {
  const base =
    config.projects[0]?.use?.baseURL ??
    process.env["E2E_BASE_URL"] ??
    `http://localhost:${process.env["PORT"] ?? 3010}/corelink`;

  const started = Date.now();

  const unresolved = await runWarmPasses({
    routes: ROUTES,
    probe: (route) => probe(`${base}${route}`),
    resolvePage: (route) => resolveAppPage(route),
    rescan: async (pageFiles) => {
      for (const file of pageFiles) forceRouteTreeRescan(file);
      await sleep(RESCAN_SETTLE_MS);
    },
    log: (line) => process.stdout.write(line),
    sleep,
  });

  const elapsed = Date.now() - started;

  if (unresolved.length > 0) {
    const list = unresolved.map((u) => `${u.route} -> ${u.detail}`).join("; ");
    // GitHub workflow command so the failure lands at the TOP of the run's
    // annotations instead of being buried under the specs it would otherwise
    // take down with it.
    if (process.env["CI"]) {
      process.stdout.write(
        `::error title=admin-ui e2e: dev server never resolved ` +
          `${unresolved.length}/${ROUTES.length} app routes::` +
          `The Next dev server behind this suite answered 404 for routes that ` +
          `RESOLVE TO A page.tsx ON DISK (checked, not assumed) and survived a ` +
          `forced route-tree re-scan, so EVERY spec visiting them would have ` +
          `failed against not-found.tsx for a reason unrelated to the code ` +
          `under test. Unresolved: ${list}. ` +
          `This is a turbopack dev route-tree fault, not a spec failure and not ` +
          `a product bug — the same commit's \`next build\` emits these routes ` +
          `into the production route table. Do NOT chase the ClerkJS ` +
          `attribution warning, which is emitted by the deliberate dummy key ` +
          `and also appears in passing runs. The dev server's own stdout is ` +
          `captured in this log ([WebServer] lines); start there.\n`,
      );
    }
    console.error(
      [
        "",
        "[warm] FATAL — the dev server did not resolve the routes this suite needs.",
        `[warm] ${unresolved.length} of ${ROUTES.length} routes stayed unresolved after ${ATTEMPTS} attempts each`,
        "[warm] AND after a forced route-tree re-scan. Each one resolves to a real",
        "[warm] page file under src/app, so this is the dev server, not the list:",
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
