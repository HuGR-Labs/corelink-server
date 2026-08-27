# WP-10 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)
**Date:** 2026-08-26
**Verdict:** ❌ **FAIL — 9 BLOCKING ISSUES, 11 HIGH SEVERITY ISSUES, 7 MEDIUM SEVERITY ISSUES**

---

## Executive summary

WP-10 is the GA gate WP. It plans a 2-week internal dogfood, a k6 load test,
seven runbooks, ten user-guide pages, and a sign-off pack. The structure is
right; the ground-truth alignment is wrong on **every** artifact the WP
intends to create or run. None of the seven runbooks it lists would land in
the directory the WP names, the k6 script has at least three syntax/semantic
errors and ignores the repo's own load-test patterns, the
`devenvOpenApiSpec` it claims to "auto-generate from" does not produce the
route shape WP-10 assumes, the `tests/load/launch-day-projection.rs` it
references elsewhere does not exist, the pricing page it says it owns lives
in `marketing/launch/`, the tier-select wiring is being built in
`corelink-tier-selection` (not the surface WP-10 implies), and the OKF wiki
has zero DevEnv concepts — so the campaign launches without a code-grounded
architecture record. The single biggest miss is **the WP does not know about
the existing GA-gate machinery** (`specs/_compliance/GA-GATE-CRITERIA.md`
already has 59 criteria across 6 tracks; `RB-GA-CUTOVER.md`, `RB-GA-LAUNCH-
ROLLBACK.md`, `RB-LAUNCH-WAR-ROOM-COORDINATION.md`, and
`marketing/launch/LAUNCH-CHECKLIST-V2.md` are already AUTHORITATIVE — WP-10
duplicates their scope without referencing them).

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **Runbook directory path is wrong — `runbooks/devenv/` does not exist; convention is `specs/_runbooks/RB-DEVENV-*.md`**
- **Location:** §5.3 Runbooks, lines 243-254
- **Problem:** The WP plans `runbooks/devenv/devenv-stuck-starting.md` etc. (repo-root `runbooks/`). The repo has **no** `runbooks/` directory at the root (verified: `ls /Users/gustavoschneiter/Documents/HuGR/corelink-server/runbooks` → not found). All runbooks live under `specs/_runbooks/RB-*.md` (58 of them, e.g. `RB-CHAOS-CAMPAIGN.md`, `RB-DSR-GDPR.md`, `RB-PENTEST-FINDING-RESPONSE.md`).
- **Real pattern:** `specs/_runbooks/RB-DEVENV-STUCK-STARTING.md`, `RB-DEVENV-WONT-STOP.md`, etc. — must follow the `RB-` ID + `ACTIVE`/`FROZEN` frontmatter that the existing 58 runbooks share (see `RB-GA-CUTOVER.md:1-15` for the canonical frontmatter).
- **Downstream impact:** GA-GATE-O03 ("All SLOs have runbooks (100%)") and GA-GATE-O06 ("All 47 runbooks `doc_status: FROZEN` or `ACTIVE` (none `DRAFT`)") are validated by directory sweep — wrong path = invisible to the validator = GA-GATE RED at T-7d.
- **Fix:** Replace §5.3 with the canonical path + ID + frontmatter. Cross-link to `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` and `RB-LAUNCH-WAR-ROOM-COORDINATION.md` (already exist; do not duplicate).

### B2. **k6 script: 4+ syntax/semantic errors; will not run as written**
- **Location:** §4.2 Load Test Script (k6), lines 122-205
- **Problems:**
  1. `devenv.vnc_url.replace('https://', 'wss://')` is called **before** the `check()` on line 159 confirms the create succeeded. When `r.status === 201` fails (i.e. `createRes.json()` throws on 4xx/5xx), k6 aborts the whole iter with `TypeError: cannot read properties of undefined (reading 'vnc_url')`. Real k6 scripts gate downstream on the `check()` first (e.g. `if (check(...)) { ... }`) and skip the rest on fail.
  2. The script runs **two** `ws.connect` calls inside a sync `for` loop, but `ws.connect` is asynchronous — the loop fires the second `ws.connect` BEFORE the first one's `socket.on('open')` callback fires, with no way to coordinate close/cleanup. Real k6 WS scripts either use the callback form to nest (`ws.connect(..., function(socket) { ... ws.connect(...) })`) or use the promise-based `ws.connect(url).then(...)` form (`k6 >= 0.49`). As written, **the second connection may close before the first opens** and the `setTimeout(30s)` racing is undefined.
  3. `http.del` is the legacy alias for `http.delete`; in current k6 (`>= 0.50`) it prints a deprecation warning. Use `http.delete`.
  4. `headers['x-corelink-tenant-id']` is never read by the Worker route — auth is `Authorization: Bearer <PAT>` (the container extracts tenant from `x-internal-tenant` from internal auth, not from a customer header). Putting the tenant ID in a header is meaningless and will fail the tenant-isolation invariant on a multi-tenant load test (the script then exercises whatever single tenant the test token happens to belong to, defeating the "50 tenants, 1 DevEnv each" scenario on line 113).
  5. `tenantId = \`loadtest-${__VU}-${__ITER}\`` is generated **but never used downstream** — the create uses `workspace_name: \`loadtest-${__VU}-${__ITER}\`` (line 156) and the auth is a single `__ENV.TEST_TOKEN` shared across all VUs. So "50 tenants" is really "1 tenant, 50 workspaces, all sharing one PAT" — not a tenant-isolation test at all, and guaranteed to trip `INV-TENANT-ISOLATION` (R1-7).
  6. `options.thresholds: { http_req_duration: ['p(95)<5000'] }` measures ALL HTTP calls including the WS upgrade HTTP handshake, which has its own threshold (`ws_connecting: ['p(95)<3000']`) — these thresholds are not nested and the script has no `scenarios:` block, so the 4-stage `stages` array ramps the **same VU mix** through 0→10→50→100→0 instead of the 50-tenant + 100-WS mix the WP claims in §4.1 (which says 50 tenants AND 100 WS as two separate scenarios).
  7. The script does not call the established k6 libs the repo uses: no `recordClassA`/`recordClassB` (COGS attribution lives in `tests/load/k6/lib/cogs.js`; every other k6 script imports it), no Prometheus remote-write output (`--out experimental-prometheus-rw`), no `DURATION` env-var composition, no `K6_ENDURANCE_CONFIRM=yes` safety rail (the repo's hard rule for any `DURATION > 30s` per `endurance-24h.js:127-131`).
- **Real patterns:** `tests/load/k6/cas-write-read.js` (5-min Zipfian CAS), `tests/load/k6/scenarios/endurance-24h-w22.js` (1000 RPS over 24h with safety rails), `tests/load/k6/signup-orchestration.js`. The runner is `scripts/run_24h_endurance.sh` which the WP ignores.
- **Fix:** Replace §4.2 with: (a) **two distinct scenarios** (`concurrent_devenvs`, `websocket_storm`); (b) call `ws.connect` async-correctly; (c) gate on `check()`; (d) per-VU `K6_AUTH_BEARER` (50 distinct staging PATs); (e) `http.delete`; (f) `K6_ENDURANCE_CONFIRM=yes` safety rail; (g) `K6_PROMETHEUS_RW_SERVER_URL`; (h) `recordClassA/recordClassB`; (i) `scenarios:` block instead of bare `stages:`. Reference `scripts/run_24h_endurance.sh` and a new `scripts/run_devenv_load.sh` wrapper.

### B3. **References a non-existent load test (`tests/load/launch-day-projection.rs`)**
- **Location:** Implicit (cross-WP contract), and the absence is a problem
- **Problem:** The DevEnv campaign plan and `marketing/launch/LAUNCH-CHECKLIST-V2.md:62` (`L3 T-24h 09:00 PT — Staging traffic verification — Confirm staging traffic = projected launch-day load (tests/load/launch-day-projection.rs); zero new SEV-2 since dry-run`) and `specs/_compliance/GA-GATE-CRITERIA.md:111` (`GA-GATE-O09 — Capacity headroom — load test tests/load/launch-day-projection.rs green at T-1 d`) BOTH reference a Rust load test at `tests/load/launch-day-projection.rs`. **This file does not exist** (verified: `ls tests/load/` → only `k6/`, `fixtures/`, `README.md`). The campaign plan owns creating it (it sits between WP-09 and WP-10), but WP-10's §4 load-test plan never mentions it, never scopes it, and never assigns it. **GA-GATE-O09 is therefore UNSCOPED — DEFER trigger.**
- **Fix:** Add §4.4 "Capacity Headroom Load Test" that scopes the `tests/load/launch-day-projection.rs` ownership (size, fixture, RPS profile, sign-off), with a direct citation to `GA-GATE-CRITERIA.md:GA-GATE-O09` and `LAUNCH-CHECKLIST-V2.md:L3`. Without this, GA cannot go.

### B4. **k6 WebSocket scenario: routes returned are HTTP upgrade endpoints, not noVNC-native WS — and the script's `wss://` rewrite skips the Worker entirely**
- **Location:** §4.2 lines 162-183
- **Problem:** `POST /v1/customer/devenv` returns `vnc_url: ${env.API_BASE_URL}/v1/customer/devenv/vnc` (`WP-08:171`). The Worker route at `GET /v1/customer/devenv/vnc` is a Cloudflare Worker path that proxies to the DO `stub.fetch(request)` (`WP-08:266-274`). The k6 script does `vnc_url.replace('https://', 'wss://')` — which IS correct for the Worker-proxied WS, **BUT** noVNC and ttyd speak a WS subprotocol (`binary`, `base64`) that the DO `proxyWebSocket()` MUST forward verbatim (per `WP-01:B5` and `WP-05`'s contract). The k6 script's `socket.on('message', (msg) => { /* Handle messages */ })` is an empty no-op that never asserts the subprotocol is forwarded — meaning the script could be passing while the DO silently strips the noVNC binary framing, breaking every real noVNC client in prod.
- **Fix:** Add an explicit VNC `subprotocols: ['binary']` parameter to `ws.connect` (noVNC's default), and a `socket.on('message', (msg) => check(socket, { 'vnc subprotocol forwarded': () => msg.length > 0 }))` assertion. For ttyd, no subprotocol; assert a banner message arrives within 5s.

### B5. **No link to existing GA-gate machinery; the WP re-implements scope that already lives at `specs/_compliance/GA-GATE-CRITERIA.md`**
- **Location:** §6 GA Release Checklist, lines 286-340
- **Problem:** §6 has its own 12-row "Engineering Sign-Offs", 10-row "Quality Gates", 7-row "Infrastructure Readiness", 4-row "Rollback Plan". `specs/_compliance/GA-GATE-CRITERIA.md` already has **59 criteria** (15 engineering + 12 security + 10 operations + 8 customer + 8 legal + 6 launch) with concrete pass/fail metrics, evidence, and owners; `RB-GA-CUTOVER.md` has the 12-step pre-cutover checklist; `RB-GA-LAUNCH-ROLLBACK.md` has the rollback tree; `marketing/launch/LAUNCH-CHECKLIST-V2.md` has the T-7d..T+72h rows; `LAUNCH-CHECKLIST-V2.md` ALSO has a T-24h row that demands the load test be green (`L3 — Staging traffic verification`). WP-10 §6 **does not cite any of these docs** and silently overlaps three of them, with no owner arbitration. In the worst case, the §6.1 sign-off pack is the only one a TL sees — and the 59 GA-GATE criteria are unevaluated, so the launch-day Go/No-Go is uninformed.
- **Fix:** §6 becomes a **DERIVATION** page: cite `GA-GATE-CRITERIA.md` §1-§6 as the authoritative checklist; show only the DevEnv-campaign-specific rows (e.g. "DevEnv load test green" = pointer to GA-GATE-O09; "DevEnv runbooks 7×RB-DEVENV-*" = pointer to GA-GATE-O03/O06); cite `RB-GA-CUTOVER.md` §0 pre-cutover + `RB-GA-LAUNCH-ROLLBACK.md` §5 triggers; cite `LAUNCH-CHECKLIST-V2.md` L3 for staging traffic. The 4-row "Rollback Plan" in §6.4 should be a cross-link to `RB-GA-LAUNCH-ROLLBACK.md` (which has 8 trigger rows), not a parallel mini-rollback tree.

### B6. **Pricing page ownership is misrouted — it lives in `marketing/launch/`, not "owned by All"**
- **Location:** §6.1 Engineering Sign-Offs (line 300) and §2 Scope (line 22)
- **Problem:** §2 lists "Pricing page and marketing assets" as In Scope, and §6.1 assigns "Documentation (WP-10) | All". But the pricing page (tiers + Stripe checkout) is already owned end-to-end at `crates/corelink-container/src/routes/tier_select.rs` (the orchestration) + `crates/corelink-tier-selection/` (the checkout crate, per `CLAUDE.md` "Keystone in flight: the `tier_select.rs` checkout backend") + `marketing/launch/PILOT-LANDING-PAGE-COPY.md` (the marketing copy) + `marketing/corelink-feature-catalog.html` (the public feature catalog with 4 pricing-relevant cards under "Subscription lifecycle"). WP-10 calling "pricing page" In Scope with "All" as owner contradicts every one of these and risks duplicate ownership. Tier semantics: **tier is a separate paid axis from runners** (ratified 2026-06-13, per `CHANGELOG.md:432` and migration `0070_runners_entitlement.sql`); the DevEnv-as-Runner campaign touches the runner axis, not the cache tier axis. So a "DevEnv pricing" page is a NEW artifact — not the same as the existing cache pricing page.
- **Fix:** Rename §2's "Pricing page" to "DevEnv-tier pricing addendum (runner axis only)" and explicitly scope it as a `marketing/launch/DEVENV-PRICING.md` companion doc that does NOT touch `tier_select.rs` (cache tier) or `corelink-billing-stripe/` (Stripe materializer). The Runner tier price is the runner-axis price, derived from the existing `runners_entitlement` table. Cite the runner-vs-cache separation in the WP so a future reader does not merge them.

### B7. **Container OOM failure-injection test is wrong (`docker kill -s KILL`)**
- **Location:** §4.3 Failure Injection Tests, line 211
- **Problem:** `docker kill -s KILL` from inside the Cloudflare Container environment is **not how OOM manifests in prod** — the Container runs in a Cloudflare microVM; you don't have `docker` access from outside the Worker. The correct OOM injection is the Cloudflare Container's memory limit (`MEMORY_LIMIT_MB` env / wrangler `limits.memory_mb`) configured to 64 MiB on a workload that allocates more, OR the in-container `stress-ng --vm 1 --vm-bytes 80M --timeout 60s` (Linux). The WP's failure-injection test is literally not executable against the deployed environment; a dogfooder reading this WP cannot run it.
- **Fix:** Replace with the actual mechanism: (a) memory-pressure via a deliberate workload running under `MEMORY_LIMIT_MB=64`, or (b) in-container `stress-ng --vm 1 --vm-bytes 80M --timeout 60s` executed via `this.execClw()` / `this.ctx.container.exec`. Also: the WP-06 lifecycle's `onError` hook is supposed to fire on OOM and attempt an emergency snapshot — but the WP-10 test does not assert that `onError` fires (the table column is "Expected Behavior", not "onError fires", so a passing test is consistent with `onError` never firing).

### B8. **D1 migrations and CF Container quota are not in scope but are gate-blocking**
- **Location:** §6.3 Infrastructure Readiness (lines 320-330)
- **Problem:** §6.3 lists "D1 migrations applied to prod" and "Cloudflare Container quota increased". The DevEnv campaign introduces **new D1 tables** (per the campaign plan: runner_entitlements pivot, DevEnv state mirror) and **a new CF Container product surface** that has its own per-account quota. GA-GATE-E15 demands "All 43+ D1 migrations applied to staging via `wrangler d1 migrations apply`; each migration has a dry-run rollback test (`RB-D1-MIGRATION-APPLY`); zero pending in `migrations/applied.json`". The WP does not own those migrations, does not own the quota-increase ticket, and does not own the rollback-test plumbing. They will silently be unowned on T-0.
- **Fix:** Either (a) explicitly cross-link these rows to the upstream owner and the GA-GATE-E15 evidence path (`RB-D1-MIGRATION-APPLY` + `migrations/applied.json`), or (b) drop them and replace with a single row "DevEnv-specific infra (D1 + CF Container quota) — see `WP-04 clw_Integration` §owner matrix" with a direct link.

### B9. **Snapshot integrity test in §3.2 is non-deterministic as written**
- **Location:** §3.2 Dogfood Checklist, line 54
- **Problem:** "Snapshot integrity | Byte-identical | Stop → Start → diff workspace" — the Workspace contains `~/.claude/`, `~/.codex/`, browser caches, `node_modules/`, `target/`, etc. that DO change between Stop and Start (timestamps, pids, lock files, audit logs). "Byte-identical" is unmeetable in practice; the WP-06 spec uses a content-hash of the user-data partition, not byte-equality of the whole workspace. A dogfooder running this check will see thousands of diff lines and mark the campaign FAIL on day 1.
- **Fix:** Replace with the WP-06 invariant: "Snapshot integrity | SHA-256 of `/workspace/user` matches pre-snapshot hash | Verify via `clw devenv verify-snapshot <id>`". Cite the exact tool + hash command the dogfooder runs.

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **Runbook template (lines 258-282) drops the `forensics-backlink` + frontmatter the repo mandates**
- **Location:** §5.4 Runbook Template
- **Problem:** The template has no YAML frontmatter, no `id`/`type`/`doc_status`/`audit_status`/`version`/`created`/`updated`/`owner`/`final_approver`/`tags` fields. Every existing 58 runbook has them (e.g. `RB-GA-CUTOVER.md:1-15`). The validator at `scripts/validate_specs.py` frontmatter-sweeps docs and will mark these as schema violations (CLAUDE.md gate: `validate_specs.py → 463/0`).
- **Fix:** Show the canonical frontmatter in §5.4 with the 7 mandated fields and the `<!-- forensics-backlink -->` directive (per `RB-GA-CUTOVER.md:17-19`).

### H2. **Load test is missing custom metrics and Prometheus remote-write**
- **Location:** §4.2 k6 script
- **Problem:** Every other k6 script in the repo (`cas-write-read.js`, `endurance-24h.js`, `endurance-24h-w22.js`) imports `Counter`/`Trend`/`Gauge`/`Rate` from `k6/metrics` and exports them via `--out experimental-prometheus-rw` to `K6_PROMETHEUS_RW_SERVER_URL`. The WP-10 script uses ZERO custom metrics — so the load test produces no PromQL-queryable signal in Grafana, and the T-24h `LAUNCH-CHECKLIST-V2.md:L3` "zero new SEV-2" check has no data. The COGS attribution lib `tests/load/k6/lib/cogs.js` is also ignored, so R2 op-class counts and the predicted $cost are not measured for the DevEnv workload.
- **Fix:** Add `Counter('devenv_create_total')`, `Trend('devenv_ws_connect_ms')`, `Counter('devenv_resize_total')`, `Counter('devenv_snapshot_total')`, `Gauge('devenv_active_dos')`, and the `recordClassA`/`recordClassB` calls. Wire `--out experimental-prometheus-rw`. Reference `lib/cogs.js`.

### H3. **No script wrapper for the k6 load test (no `scripts/run_devenv_load.sh`)**
- **Location:** Appendix B (lines 401-409)
- **Problem:** Appendix B is a raw `k6 run --env BASE_URL=...` invocation with no `RESULT_DIR`, no summary-export path, no run-meta JSON, no analyzer, no `smoke|dressrun|nightly|full` mode flag. The repo's canonical pattern is `scripts/run_24h_endurance.sh` (verified: 163 lines, four modes, summary export, analyzer). WP-10's Appendix B is a copy-paste-able one-liner, which is what makes ad-hoc runs leak untriaged data into the local box and break reproducibility.
- **Fix:** Add `scripts/run_devenv_load.sh` with `smoke|dressrun|nightly|full` modes mirroring `run_24h_endurance.sh:40-90`, summary export to `tests/load/results/<UTC-stamp>-<mode>/`, and an `analyze_devenv_load.py` companion. Then Appendix B is just `bash scripts/run_devenv_load.sh <mode>`.

### H4. **The `tier_select.rs` reference is to a different (cache-tier) keystone**
- **Location:** §6.1 row "Billing (WP-07)" + §2 In Scope
- **Problem:** Per `CLAUDE.md`, "Keystone in flight: the `tier_select.rs` checkout backend" — this is the **CACHE-TIER** checkout, not the DevEnv/RUNNER pricing. The Runner pricing is a separate paid axis (`runners_entitlement` table, migration `0070`). The WP-10 sign-off pack puts "Billing (WP-07) | corelink-server" — fine — but the §2 In Scope "Pricing page" line doesn't disambiguate, and a future reader will conflate the two. The `marketing/corelink-feature-catalog.html` "Subscription lifecycle" cards (lines 1207-1395) explain this in great detail and the WP-10 should cite the relevant card.
- **Fix:** Explicit cross-link to `marketing/corelink-feature-catalog.html#active-subscription-tier-resolution` and the runner-vs-cache separation; rename "Pricing page" to "DevEnv runner-tier pricing addendum" in both §2 and §6.1.

### H5. **Dogfood participants' "Hours/Day" is unrealistic for a 2-week sustained test (6h, 6h, 8h, 6h, 4h = 30h/week per engineer)**
- **Location:** §3.1 Participants & Workloads
- **Problem:** 6h/day * 5 days * 2 weeks = 60h of dogfood per engineer, on TOP of their normal sprint work. The CLAUDE.md says the team is founder + small; burning 30h/week/engineer for two weeks on DevEnv dogfood is a sprint-blocker. Realistic dogfood allocations for a 5-engineer team are ≤2h/day per engineer on the dogfooded surface (i.e. one specific feature path), with the rest of the day on normal sprint work. A dogfood program where every engineer spends 6-8h/day on DevEnv is a re-org, not a dogfood.
- **Fix:** Cap "Hours/Day" at 2h, add a "Dogfood surface" column (each engineer dogfoods ONE specific feature path: cold-start, browser persistence, terminal reliability, build caching, multi-device). Total dogfood = 10h/engineer * 2 weeks = 100h team-wide, which is what one engineer-week is — a 2-week campaign for the WHOLE team's dogfood should be ~100-200h team-total, not 600h.

### H6. **No `tests/load/launch-day-projection.rs` sign-off path is mentioned**
- **Location:** §4 + §6
- **Problem:** Cross-WP gap (also see B3). The launch-day-projection test is cited in TWO pre-existing docs (`LAUNCH-CHECKLIST-V2.md:L3` and `GA-GATE-CRITERIA.md:GA-GATE-O09`) but WP-10 §4 has no §4.4 for it, and §6 Quality Gates row "Load test thresholds met" does not name this test.
- **Fix:** Add §4.4 + cite in §6 Quality Gates row 5.

### H7. **The 50-tenant scenario contradicts the CF Durable Object's per-account limits**
- **Location:** §4.1 Test Scenarios row "Concurrent DevEnvs" (line 113)
- **Problem:** "50 tenants, 1 DevEnv each, all running" = 50 active Containers per account. The Cloudflare Containers Beta quota is **100 concurrent instances per account** (per CF docs as of 2026-08). The next scenario ("100 WebSocket connections (mixed vnc/tty/code) per DO" — line 114) takes ONE of those 50 and loads 100 WS on it, which is fine for WS but is undefined against the DO's outbound connection limit (Cloudflare DOs have a 32 KiB outbound-WS-message cap, 6 active TCP sockets outbound, and a 30s wall-time per `fetch()` — none of which the script's success criteria assert).
- **Fix:** Add an explicit CF Container + DO constraint callout: "50 active Containers must stay below the account's per-region Container cap (CF quota, currently 100). WS connection count per DO limited by CF DO outbound-TCP cap. The 50-tenant test must verify `cf:container:concurrent_instances < 90%` at the account level before iteration begins." Cite the CF quota.

### H8. **No tier-promotion pathway from dogfood → billing in the WP**
- **Location:** §3 + §6
- **Problem:** Dogfooders are presumably on the **free cache tier** (per `tier_selections('free','active')` seed). A dogfooder hitting a "Tier limit reached" wall (e.g. `quota.ts` 429) is not in scope — but the WP does not call this out, so the dogfood report is meaningless when a P1 surfaces as "quota issue" instead of a real product bug. The WP needs a "Dogfood tier: free cache + 1-tenant Runner promo (max_concurrency=1, max_vcpu=2)" row in §3.1, with a clear escalation path to seed-test tier-promo creds via `seedTenantEntitlements`.
- **Fix:** Add "Dogfood Tier" column to §3.1, default `free + runner-promo`; cite `apps/signup-worker/src/lib/d1.ts::seedTenantEntitlements` and the `0070_runners_entitlement` migration for the runner-promo shape.

### H9. **Missing CHANGELOG entry requirement is unstated**
- **Location:** §6 Quality Gates (line 305-318) — absent
- **Problem:** Per `CLAUDE.md` (the root `CLAUDE.md`, not the WP-level one): "feat:/fix: commits **require a CHANGELOG.md `[Unreleased]` entry** (changelog gate)". The DevEnv campaign will produce 30-50 `feat:` and `fix:` commits across WP-01..WP-10. WP-10's release-gate does not assert the changelog gate is closed.
- **Fix:** Add a "CHANGELOG `[Unreleased]` row count ≥ N" row to §6 Quality Gates (mirror `pre-merge-gate-check.sh` which already enforces this per-PR; for the campaign, the gate is "all WP-01..WP-10 features/fixes have a `[Unreleased]` entry").

### H10. **No signing policy on the OpenAPI spec — published unauthenticated at `docs.corelink.humangr.com/api/devenv`**
- **Location:** §5.2 API Reference (OpenAPI), line 237-241
- **Problem:** The spec is auto-generated and "Published at `https://docs.corelink.humangr.com/api/devenv`". This URL has no auth — but the spec documents authenticated routes (line 436-452 in WP-08: `security: [{ bearerAuth: [] }]`) and the response shapes include error schemas. The spec is a fingerprint of the API surface. GA-GATE-S07 (`RFC 9116 security.txt live`) and the `disclosure.md` workflow require attack-surface minimization; an unauthenticated OpenAPI dump is an over-share (CWE-200 surface enumeration).
- **Fix:** Either (a) gate the published spec behind the same `clerkAuthMiddleware` as the docs site (defer to `WP-08` §3.3), or (b) add a "Published scope: customer-authenticated, requires Clerk session" row to §5.2 with a link to the auth gate. Pick one — the current text is silent.

### H11. **No `CHECKLIST.md` for dogfooders to print/copy; the report template is markdown, not a checklist**
- **Location:** §3.3 Dogfood Reporting Template (lines 58-91)
- **Problem:** The template is a markdown report — a dogfooder has to copy-paste into a fresh file every day. A printable one-page checklist is what dogfooders actually use (cross off items live, screenshot, attach). The WP should ship a `.txt` or `.pdf` "print-and-tick" version OR a `clw dogfood check` subcommand.
- **Fix:** Add an Appendix D "Print-Ready Dogfood Checklist" (1 page) with ☐ boxes for each row of §3.2 + a "Date: __ Engineer: __" header.

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **Two §8 sections; the second is the sign-off, not "post-launch metrics"**
- **Location:** §8 Post-Launch Metrics (line 358) AND §8 Sign-Off (line 374)
- **Problem:** Two sections both numbered §8. The first is "Post-Launch Metrics (First 30 Days)", the second is "Sign-Off". Markdown will render both as §8. The sign-off should be §9 (and the appendices §10).
- **Fix:** Renumber §8 → §9, §9 → §10.

### M2. **"NPS score > 50" post-launch metric has no measurement plan**
- **Location:** §8 Post-Launch Metrics (line 371)
- **Problem:** "NPS score | > 50 | Survey" — there's no survey tool listed, no cadence, no segment (all customers? paying only? new signups?). "Survey" is not an implementation.
- **Fix:** Cite `marketing/launch/PILOT-EMAIL-BLAST.md` or the `lighthouse-kit` for the survey mechanism (it exists at `marketing/lighthouse-kit/`) and the cadence (Day +3 / Day +30 NPS pulse).

### M3. **`Mobile Safari` access in §3.2 row "Multi-device access" has no iOS version specified**
- **Location:** §3.2 (line 55)
- **Problem:** "iPhone Safari + Desktop Chrome" — but noVNC in iOS Safari has known issues with the WebSocket binary subprotocol (Safari < 16 does not support `binary`; Safari 16+ does but with quirks). The dogfood checklist should pin an iOS version (iOS 17+ recommended) and assert a known-good WebSocket subprotocol behavior.
- **Fix:** Add iOS version to the row; cite the noVNC iOS compatibility matrix.

### M4. **"60s between iterations" is unrealistic for "rapid start/stop"**
- **Location:** §4.1 row "Rapid start/stop" (line 116)
- **Problem:** "100 create/stop cycles in 1 hour" = 36s per cycle on average. CF Container cold start is 5-30s; if cold start is 25s, the test has 11s for stop+create-overhead, which is below the 5s warm-resume SLO floor. The threshold is incompatible with the SLO it claims to verify.
- **Fix:** Either relax to 50 cycles/hr OR require 50%+ warm-resume (sustain on the same DO without container teardown — i.e. `stop`/`start` without deprovisioning the Container, which is a separate DO API method).

### M5. **Failure-injection tests lack a "blast radius" / "rollback" column**
- **Location:** §4.3 (lines 207-215)
- **Problem:** Each row has Method + Expected Behavior. None has a rollback-from-test step. Injecting OOM on a shared account's DO without rollback is a self-inflicted incident.
- **Fix:** Add "Rollback" column with the explicit teardown step (e.g. "kill stress-ng, verify DO recovers; if not, `cf-do-evict <do-id>` via SRE-OC").

### M6. **No `doc_status: FROZEN` / `ACTIVE` enforcement for the docs/devenv/ subtree the WP creates**
- **Location:** §5.1
- **Problem:** Per `GA-GATE-CRITERIA.md:GA-GATE-O06` and the `validate_specs.py` schema, every doc in the campaign needs `doc_status: FROZEN|ACTIVE` frontmatter. The `docs/devenv/` subtree the WP creates is not mentioned in any frontmatter enforcement; the new pages will be `doc_status: DRAFT` by default and fail the GA-gate sweep.
- **Fix:** Add a row in §5.1: "Frontmatter: every file in `docs/devenv/` MUST ship with `doc_status: ACTIVE` (or `FROZEN` for the API reference). Verified by `python3 scripts/validate_specs.py` (target 463/0 → 473/0 after WP-10 lands)."

### M7. **No link from §4 to the existing `RB-CHAOS-CAMPAIGN.md` + `RB-CHAOS-CATALOG.md`**
- **Location:** §4
- **Problem:** The failure-injection tests in §4.3 are functionally chaos experiments. The repo already has `specs/_runbooks/RB-CHAOS-CAMPAIGN.md` and `RB-CHAOS-CATALOG.md` (per `ls specs/_runbooks/`) with documented blast radius + recovery time per `GA-GATE-CRITERIA.md:GA-GATE-O10`. The WP-10 failure-injection table does not cite them; the chaos experiment for "DevEnv" should be ADDED to `RB-CHAOS-CATALOG.md` and the dogfood/load-test plan should EXECUTE one of its rows, not invent new ones.
- **Fix:** Add to §4.3: "These tests are DevEnv entries for `RB-CHAOS-CATALOG.md`; each row's blast radius + recovery time recorded per `GA-GATE-CRITERIA.md:GA-GATE-O10`."

---

## 📋 DoD Gap Analysis

The DoD for WP-10 is implicit in the WP's own 6 numbered sections. Checking each:

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | Dogfood program scoped (2 weeks, ≥5 engineers, ≥40h each) | ❌ **Fail** | 5 engineers but 30h/week each (H5) — blocks sprint work |
| 2 | Dogfood checklist per engineer (10 rows) | ⚠️ **Partial** | 10 rows present, but "Byte-identical" snapshot is non-deterministic (B9) |
| 3 | Dogfood gate criteria (6 rows) | ✅ Pass | Lines 95-103 |
| 4 | Load test scenarios (6 rows) | ❌ **Fail** | k6 script broken (B2), missing scenarios block, no Prometheus export (H2) |
| 5 | k6 script provided | ❌ **Fail** | 7+ defects (B2); does not match repo patterns |
| 6 | Failure-injection tests (5 rows) | ❌ **Fail** | OOM test unexecutable (B7); no blast radius (M5); no RB-CHAOS link (M7) |
| 7 | User guide subtree (10 pages) | ⚠️ **Partial** | Listed but no frontmatter enforcement (M6) |
| 8 | API reference auto-generated | ✅ Pass | Cites `devenvOpenApiSpec` (WP-08:347) |
| 9 | Runbook list (7 runbooks) | ❌ **Fail** | Wrong path (B1), wrong template (H1) |
| 10 | Runbook template with frontmatter | ❌ **Fail** | Plain markdown, no YAML (H1) |
| 11 | GA Engineering Sign-Offs (12 rows) | ⚠️ **Partial** | Overlaps `GA-GATE-CRITERIA.md` 59 rows without citation (B5) |
| 12 | Quality Gates (10 rows) | ⚠️ **Partial** | Missing CHANGELOG gate (H9), missing `launch-day-projection.rs` (B3/H6), missing tier-promotion (H8) |
| 13 | Infrastructure Readiness (7 rows) | ⚠️ **Partial** | Unowned rows for D1 + CF Container (B8) |
| 14 | Rollback Plan (4 rows) | ⚠️ **Partial** | Mini-rollback duplicates `RB-GA-LAUNCH-ROLLBACK.md` (B5) |
| 15 | Launch Timeline (8 milestones) | ✅ Pass | T-14d → T+7d (lines 344-356) |
| 16 | Post-Launch Metrics (9 rows) | ⚠️ **Partial** | NPS measurement plan missing (M2) |
| 17 | Sign-off table (8 roles) | ✅ Pass | Lines 377-386 |
| 18 | Appendix: Onboarding | ✅ Pass | Lines 393-399 |
| 19 | Appendix: Load test exec | ❌ **Fail** | One-liner, no `run_devenv_load.sh` (H3) |
| 20 | Appendix: Emergency contacts | ✅ Pass | Lines 412-418 |

**DoD Score: 4/20 PASS, 7/20 PARTIAL, 9/20 FAIL**

---

## 📋 Invariants Verification

| Invariant | Enforced? | Verdict |
|-----------|-----------|---------|
| INV-DOGFOOD-NO-DATA-LOSS: 0 data loss incidents in dogfood | ✅ Cited in §3.4 row | **ENFORCED** (as a check) |
| INV-LOAD-TEST-THRESHOLDS: p95 < 30s cold, < 5s warm | ✅ Cited | **ENFORCED** |
| INV-SNAPSHOT-INTEGRITY: workspace content-hash matches pre-snapshot | ❌ WP says "byte-identical" which is wrong | **NOT ENFORCED** (B9) |
| INV-RUNBOOK-COMPLETE: every DevEnv failure mode has a runbook | ❌ Wrong dir (B1) + no frontmatter (H1) | **NOT ENFORCED** |
| INV-TIER-PROMO: dogfooders are on `free + runner-promo` | ❌ Not stated (H8) | **NOT ENFORCED** |
| INV-OBSERVABILITY: load test emits PromQL-queryable signal | ❌ No Prometheus export (H2) | **NOT ENFORCED** |
| INV-COGS-ATTRIBUTION: load test records R2 op-classes | ❌ `lib/cogs.js` not imported (H2) | **NOT ENFORCED** |
| INV-CHAOS-CATALOG-COVERED: DevEnv entries in `RB-CHAOS-CATALOG.md` | ❌ Not linked (M7) | **NOT ENFORCED** |
| INV-SIGNOFF-PACK: 8 TLs sign before T-0 | ✅ Cited | **ENFORCED** |
| INV-CHANGELOG-COMPLETE: every feat:/fix: has `[Unreleased]` entry | ❌ Not in Quality Gates (H9) | **NOT ENFORCED** |
| INV-CF-QUOTA-HEADROOM: 50 active Containers < 90% of 100/account | ❌ Not stated (H7) | **NOT ENFORCED** |

**Invariants Enforced: 3/11 (27%)** — INSUFFICIENT

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| Pattern-consistency: scripts mirror `run_24h_endurance.sh` | ❌ WP has no `run_devenv_load.sh` (H3) | **FAIL** |
| Pattern-consistency: runbooks follow `RB-*.md` frontmatter | ❌ Template is plain markdown (H1) | **FAIL** |
| Pattern-consistency: k6 scripts use `lib/cogs.js` | ❌ Not imported (H2) | **FAIL** |
| Pattern-consistency: docs have `doc_status: ACTIVE\|FROZEN` | ❌ Not enforced (M6) | **FAIL** |
| Cross-doc consistency: GA-gate cited from `GA-GATE-CRITERIA.md` | ❌ Re-implemented, not cited (B5) | **FAIL** |
| Cross-doc consistency: runbooks live in `specs/_runbooks/` | ❌ Wrong path (B1) | **FAIL** |
| Realism: dogfood workload fits a 5-engineer team | ❌ 30h/week/engineer (H5) | **FAIL** |
| Reproducibility: load test is scriptable + repeatable | ⚠️ Raw k6 invocation (H3) | **PARTIAL** |
| Observability: load test emits Prometheus | ❌ (H2) | **FAIL** |
| Coherence: sign-off pack aligns with the 59-criteria GA-GATE | ❌ (B5) | **FAIL** |

**Quality Standards: 0.5/10 MET** — INSUFFICIENT

---

## 📋 Self-Check Points Analysis

The WP has no explicit self-check section. Reconstructing from the implied
gates:

### Self-Check 1: k6 script is runnable as written
- [ ] Compiles / parses in `k6 inspect` — ❌ Multiple semantic errors (B2)
- [ ] Respects established repo patterns — ❌ Ignores `lib/cogs.js`, Prometheus, env-var composition (H2)
- [ ] Has safety rails for long DURATION — ❌ No `K6_ENDURANCE_CONFIRM` (B2)
- [ ] Has scripted wrapper — ❌ No `run_devenv_load.sh` (H3)
- [ ] Captures results in `tests/load/results/` — ❌ No summary-export

**Verdict: 0/5 PASS**

### Self-Check 2: Runbooks land in the right place with the right shape
- [ ] Path = `specs/_runbooks/RB-DEVENV-*.md` — ❌ (B1)
- [ ] Frontmatter includes all 11 fields — ❌ (H1)
- [ ] Each has a dry-run drill within 30d of T-0 — ⚠️ Implied, not scheduled
- [ ] Cross-linked to existing runbooks (`RB-CHAOS-CAMPAIGN`, `RB-LAUNCH-WAR-ROOM-COORDINATION`) — ❌ (M7)
- [ ] Cross-linked to existing chaos catalog — ❌ (M7)

**Verdict: 0/5 PASS, 1/5 PARTIAL, 4/5 FAIL**

### Self-Check 3: GA sign-off is authoritative
- [ ] Derives from `GA-GATE-CRITERIA.md` 59-criteria — ❌ (B5)
- [ ] Cites `RB-GA-CUTOVER.md` pre-cutover checklist — ❌ (B5)
- [ ] Cites `RB-GA-LAUNCH-ROLLBACK.md` triggers — ❌ (B5)
- [ ] Cites `LAUNCH-CHECKLIST-V2.md` T-24h row L3 — ❌ (B5)
- [ ] Sign-off pack enumerates the 8 TLs + Sign-off cadence + retention

**Verdict: 0/5 PASS, 1/5 PARTIAL, 4/5 FAIL**

### Self-Check 4: No cross-WP ownership gap
- [ ] `tests/load/launch-day-projection.rs` owned somewhere — ❌ (B3, H6)
- [ ] D1 migration rollback tests owned — ⚠️ Cited `RB-D1-MIGRATION-APPLY` but not assigned
- [ ] CF Container quota increase owned — ❌ (B8)
- [ ] Pricing addendum owned — ❌ (B6, H4)
- [ ] Runner-tier pricing distinct from cache-tier — ❌ (B6, H4)

**Verdict: 0/5 PASS, 1/5 PARTIAL, 4/5 FAIL**

### Self-Check 5: Documentation subtree will pass `validate_specs.py`
- [ ] Every file has `doc_status: ACTIVE\|FROZEN` frontmatter — ❌ (M6)
- [ ] Cross-links to OKF wiki — ❌ (no DevEnv concept exists in OKF)
- [ ] Cross-links to related runbooks — ❌
- [ ] API reference published behind auth — ❌ (H10)
- [ ] Tier-promotion path documented — ❌ (H8)

**Verdict: 0/5 PASS, 5/5 FAIL**

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 9 | 0 | **-9** |
| High Issues | 11 | 0 | **-11** |
| Medium Issues | 7 | 0 | **-7** |
| DoD Pass Rate | 20% | 100% | **-80%** |
| Invariants Enforced | 27% | 100% | **-73%** |
| Quality Standards | 5% | 100% | **-95%** |
| Self-Check Pass | 0% | 100% | **-100%** |

**OVERALL VERDICT: ❌ FAIL — Requires major rework before sign-off**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers)
1. B1 — Move runbook paths to `specs/_runbooks/RB-DEVENV-*.md`
2. B2 — Rewrite the k6 script (7 defects)
3. B3 — Add §4.4 scoping `tests/load/launch-day-projection.rs`
4. B4 — Add WS subprotocol assertions to k6
5. B5 — Convert §6 to DERIVATION from existing GA-gate machinery
6. B6 — Disambiguate DevEnv pricing addendum from cache-tier pricing
7. B7 — Replace `docker kill` with `MEMORY_LIMIT_MB=64` + `stress-ng`
8. B8 — Cross-link D1 + CF Container quota to upstream owner
9. B9 — Replace "byte-identical" with content-hash

### Should Fix (High)
1. H1 — Canonical runbook frontmatter in §5.4
2. H2 — Custom metrics + Prometheus + `lib/cogs.js`
3. H3 — `scripts/run_devenv_load.sh` wrapper
4. H4 — Cache vs runner axis citation
5. H5 — Cap dogfood to 2h/engineer/day
6. H6 — Cite `launch-day-projection.rs` in §4.4 and §6
7. H7 — Add CF Container + DO constraint callout
8. H8 — Dogfood tier (free + runner-promo) row
9. H9 — CHANGELOG `[Unreleased]` gate row
10. H10 — Auth-gate or scoped-publish decision for OpenAPI
11. H11 — Print-ready checklist appendix

### Nice to Fix (Medium)
1. M1 — Renumber §8
2. M2 — NPS measurement plan
3. M3 — iOS version pin
4. M4 — Fix rapid start/stop cycle math
5. M5 — Blast-radius / rollback column
6. M6 — `doc_status` enforcement for `docs/devenv/`
7. M7 — Link to `RB-CHAOS-CAMPAIGN.md` / `RB-CHAOS-CATALOG.md`

---

## Cross-WP Coordination Needs

- **WP-08** — `devenvOpenApiSpec` shape (vnc_url, tty_url, code_url strings) does not include error schemas for `409 already_active` (the runner-axis conflict) or `429 quota_exceeded`. WP-08 §3.3 needs a small extension for the runner-axis error surface.
- **WP-07** — Billing meter for runner-axis usage is owned by WP-07; WP-10's load test exercises runner flow but does not assert billing emissions. WP-07's `tests/load/k6/stripe-webhook-burst.js` could be extended with a "after devenv start, expect Stripe metered event" assertion.
- **WP-04** — clw integration supplies the `clw devenv` subcommands; WP-10's dogfood onboarding (`Appendix A`) calls `clw auth login && clw devenv create` but WP-04 does not list these subcommands in its public surface. Verify in WP-04 that the CLI surface exposes them.
- **WP-06** — DO lifecycle owns the `onError` hook that B7's failure injection is supposed to trigger. WP-06 must assert `onError` fires on OOM (it doesn't currently per `WP-01:B1-B3` and the eventual WP-06 hook).
- **WP-09** — Dashboard UI exposes the same `/v1/customer/devenv/*` routes; the load test's "100 WS connections" scenario assumes the dashboard renders WS-up state. WP-09 must not N+1 the create endpoint on dashboard load (it does per `WP-09:155-188` — a side-effect that WP-10's 100-WS test would amplify).
- **Marketing/launch/LAUNCH-CHECKLIST-V2.md** — Already references the (non-existent) `tests/load/launch-day-projection.rs`. WP-10 should NOT silently own it; raise to TechLead that this is a campaign-plan-level gap that must be scoped in WP-09-or-WP-10 (B3, H6).

---

## NEXT STEPS

1. **Apply all BLOCKING fixes** (B1-B9) — non-negotiable
2. **Apply all HIGH fixes** (H1-H11) — required for GA gate passage
3. **Apply at least 70% of MEDIUM fixes** (M1-M7)
4. **Re-verify all 20 DoD items pass**
5. **Re-verify all 11 invariants enforced in code or doc**
6. **Re-verify all 10 quality standards met**
7. **Open a cross-WP issue** for `tests/load/launch-day-projection.rs` ownership
8. **Open a cross-WP issue** for OKF wiki — add DevEnv concepts (none exist today)
9. **Proceed to Iteration 2 review**

**Do NOT sign off WP-10 until it passes review AND the cross-WP gaps (launch-day-projection, OKF concepts) are owned.**

---

**END OF WP-10 ITERATION 1 REVIEW**
