# Frontend Stabilization Plan (max-rigor)

**Owner mandate:** stabilize the admin-ui frontend *for good*, with maximum rigor.
**Author:** corelink-server TL · **Status:** APPROVED to execute (post-launch sequencing) · **Created:** 2026-07-16

> **The throughline principle (non-negotiable):** every workstream below MUST ship
> with a **gate that welds the failure mode shut** — a test, smoke, or deploy check
> that makes the specific way we got burned *impossible to recur silently*. A fix
> without a gate is not "done" here; it is a patch that will regress. "Stabilized"
> is defined at the bottom and is measured by the gates, not by vibes.

---

## Why the frontend keeps breaking — root diagnosis (evidence, not theory)

Observed directly this session (2026-07-16), three independent root causes:

1. **The stack fights its host.** Next 15 + `@opennextjs/cloudflare` (Workers) + `basePath` + a
   dual surface (`corelink-app.humangr.com` root **and** `humangr.com/corelink` path) is an
   uncommon, fragile combination. Every touch reopened a class of bug: the SSR-500s that forced the
   Pages→Worker migration (`admin-ui-deploy.yml` header), `basePath` 404ing every route, the
   standalone-build hack Lighthouse needs (`lighthouserc.cjs`). OpenNext runtime semantics ≠ `next dev`
   semantics → "green locally, broken in prod."
2. **No gate verifies the customer-facing app actually renders.** The app was **unrouted / 404ing at
   `humangr.com/corelink` while CI was green** (worker `routes: []`, `basePath` uncommitted). Zero
   real users masked it. This is the central failure: *you can ship a broken frontend and nothing tells you*.
3. **Config ≠ deployed reality, and the test signal is poisoned.** Routes declared in
   `apps/admin-ui/wrangler.toml` that the CI token cannot create (`CF_API_TOKEN` = Workers Scripts
   only, no Routes); a ~200-file migration left uncommitted in the tree; subdomain↔path drift; stale
   `E2E_BASE_URL` defaults. And the e2e signal itself lies: `@sentry/core _optionalChain` breaks the
   Playwright critical-flows **repo-wide** (`admin-ui-sentry-critical-flows-broken`), so red/green is meaningless.

---

## Workstreams (ranked by stability ROI)

### WS-1 — Post-deploy synthetic smoke that PAGES on a broken customer surface 🥇
*The single highest-leverage stabilizer. Would have caught every frontend break this session.*

- **Problem/evidence:** the app rendered 404/marketing-catch-all at the canonical URL for an unknown
  duration with green CI. `monitoring/synthetic/probes.yml` exists but was not pointed at the real
  rendered surface.
- **Change:** a synthetic probe suite that hits the **live** canonical surface and asserts *rendered
  reality*, not just HTTP 200:
  - `GET https://humangr.com/corelink/en/pricing` → 200 **and** body contains the live tier prices
    (`$15/$35/$50/$149`) **and** Next hydration markers (`_next/static`).
  - `GET https://humangr.com/corelink/sign-up` → 200 **and** Clerk publishable-key / mount markers present.
  - `GET https://humangr.com/corelink/_next/static/<a real chunk>` → `content-type: application/javascript` (NOT html).
  - `POST https://humangr.com/corelink/api/checkout/session` (unauth) → 3xx to `/corelink/sign-in` (NOT apex, NOT 405).
- **The gate (the weld):** runs on a schedule (≤5 min) **and** as the final step of every admin-ui
  deploy; **fails the deploy and PAGES** (PagerDuty, `post-golive-alerting-todo`) on any assertion miss.
  A deploy is not "successful" until the real surface is proven rendering.
- **Acceptance:** kill a route binding in staging → probe RED + page fires < 5 min. Restore → GREEN.
- **Effort:** M. **Owner-gated bits:** PD delivery must be verified (`post-golive-alerting-todo` is OPEN).

### WS-2 — Fix `@sentry/core _optionalChain` at the root (un-poison the e2e signal) 🥈
*While critical-flows is always-red on Sentry, the whole e2e gate is noise — it masked the basePath break.*

- **Problem/evidence:** Playwright critical-flows (audit-trail / dsr-approve / tenant-overview) fail
  **repo-wide** on a `@sentry/core` `_optionalChain` boot error, pre-dating any migration; currently a
  standing documented `--admin` reason. Green/red carries no information.
- **Root cause (to confirm):** `@sentry/nextjs` v10 client instrumentation boot under the OpenNext/
  Turbopack build hits `_optionalChain` on a null path (`instrumentation-client` / `register()`), or a
  transitive `@sentry/core` version skew.
- **Change:** reproduce deterministically, fix at root (init guard / version pin / conditional
  instrumentation for the test build) — **not** an `--admin` waiver.
- **The gate (the weld):** critical-flows must be **fully green with zero `--admin`** on a clean run;
  add a boot-smoke assertion that the Sentry client initializes without throwing.
- **Acceptance:** `playwright.tests.config.ts` chromium suite → 0 failed on `main`, no documented-flake carve-out.
- **Effort:** M (could be a rabbit hole — timeboxed spike first).

### WS-3 — CI deploy token scope + verify-after-deploy (config == reality) 🥉

- **Problem/evidence:** `wrangler deploy` "succeeds" but creates **no zone routes** (token lacks
  Workers Routes: Edit) → `corelink-admin-ui` had `routes: []`; routes were hand-bound via the CF API.
- **Change:** (a) broaden the CI `CF_API_TOKEN` to include **Zone → Workers Routes: Edit** so
  `wrangler deploy` reconciles `wrangler.toml` routes itself (no manual binding); (b) the admin-ui
  deploy job asserts, post-deploy, that the declared routes exist and resolve to the worker.
- **The gate (the weld):** deploy job **hard-fails** if any `wrangler.toml` route is absent from the
  live zone after deploy. No silent route drift.
- **Acceptance:** remove a route from the zone by hand → next deploy re-creates it (or fails loudly).
- **Effort:** S (code) + **OWNER** (token scope in the CF dashboard + GH secret).

### WS-4 — One canonical surface; freeze the stack (stop host churn)

- **Problem/evidence:** every host migration (Pages→Worker) and the dual surface reintroduced
  instability. The `corelink-app.humangr.com` → `humangr.com/corelink` migration (PR #804) collapses it
  to one surface.
- **Change:** land #804 (one canonical surface, subdomain retired); then a written **stack-freeze ADR**:
  no host/framework migration without an ADR that names the failure modes and the rollback. `basePath`,
  OpenNext, and the single canonical URL are frozen defaults.
- **The gate (the weld):** a `validate_docs_reality`-style check that fails if `corelink-app.humangr.com`
  (bare host) reappears in code/config, and a CODEOWNERS gate on `next.config.ts` basePath / host lists.
- **Acceptance:** grep gate green; one surface live + smoked (WS-1).
- **Effort:** S (mostly done by #804 + a gate + ADR).

### WS-5 — Separate PR-gates (deterministic, local) from post-deploy smokes (live)

- **Problem/evidence:** `playwright prod-surface` (live-URL) is a **PR-blocking** gate that targets a
  surface that only exists **after** deploy → un-green-able pre-cutover (a chicken-egg that forces `--admin`).
  Meanwhile the deploy itself is trusted with no live check.
- **Change:** PR gates run only the **deterministic local** suites (unit + critical-flows against a local
  server); the **live** suites (`prod-surface`, WS-1 smoke) move to **post-deploy** triggers.
- **The gate (the weld):** PR CI has **zero** live-URL dependencies (fast, deterministic); the live
  contract is enforced by WS-1 after deploy. Honest signals both places.
- **Acceptance:** a PR can go 100% green with the app not yet deployed; a bad deploy still gets caught (WS-1).
- **Effort:** S–M (workflow re-wiring).

---

## Sequencing & ownership

| # | Workstream | When | Effort | Owner-gated part |
|---|---|---|---|---|
| WS-4 | One surface + freeze | **Now** (rides #804 launch) | S | — |
| WS-1 | Smoke-that-pages | **Fast-follow #1** (post-launch) | M | verify PD paging delivery |
| WS-2 | Kill @sentry poison | Fast-follow #2 | M | — |
| WS-3 | Token scope + verify-deploy | Fast-follow #3 | S | CF token scope + GH secret |
| WS-5 | PR-gate vs smoke split | Fast-follow #4 | S–M | — |

Each WS ships as its own branch → PR → merge (one concern each), reviewed via `/techlead`, green on
its own weld-gate before merge. No WS is "done" without its gate.

## Definition of "stabilized" (measured, not claimed)

The frontend is stabilized when **all four are simultaneously true**:
1. A broken customer surface **pages within 5 minutes** and **fails the deploy** (WS-1).
2. The e2e signal is **honest** — critical-flows green with **zero standing `--admin`** (WS-2, WS-5).
3. Declared routing == live routing, enforced at deploy time (WS-3).
4. One canonical surface, host/framework frozen behind an ADR gate (WS-4).

Until all four hold, "the frontend works" remains a claim, not a measurement.
