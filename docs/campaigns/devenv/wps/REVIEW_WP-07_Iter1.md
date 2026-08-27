# WP-07 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Verdict:** ❌ **FAIL — 9 BLOCKING ISSUES, 10 HIGH SEVERITY ISSUES, 8 MEDIUM SEVERITY ISSUES**

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **DUPLICATE `recordUsage()` IMPLEMENTATION — CONFLICT WITH WP-04 + WP-06**
- **Location:** §3.3 line 195 (`private async recordUsage()`)
- **Problem:** `recordUsage()` is **already defined** in `WP-04_clw_Integration.md:336-359` AND called by `onStop` (WP-06:253) and `onError` (WP-06:307). WP-07 §3.3 redefines a **second** `recordUsage()` — three definitions of the same method.
- **Evidence:**
  - WP-04:336 — `private async recordUsage(): Promise<void> { … }` (URL `${this.env.BILLING_INGEST_URL}/v1/usage`, no idempotency_key)
  - WP-06:253 / 307 — both call `await this.recordUsage();`
  - WP-07:195 — second definition with `idempotencyKey` and new endpoint
- **Why this is BLOCKING:** TypeScript will not compile (duplicate method). If both kept, one silently shadows the other. WP-04's version has NO idempotency key — it would double-bill on retry. Three WPs cannot each define the same method.
- **Fix:** WP-07 §3.3 must be **REMOVED entirely**. `recordUsage()` belongs in WP-04 (which already defines it). WP-07 only needs to define the D1 schema, the Worker endpoint, and the quota guard. Add a "**Note:** `recordUsage()` itself is defined in WP-04 §3.2; WP-07 only defines the destination D1 schema and Worker ingest endpoint."

### B2. **`recordUsage()` Body Has Type Errors Against WP-01's Discriminated Union State**
- **Location:** §3.3 line 216-217: `workspace_name: this.state.workspaceName, profile_name: this.state.profileName`
- **Problem:** Per `WP-01_RunnerDevEnvDO_Skeleton.md:96-130`, `DevenvState` is a discriminated union where `workspaceName` and `profileName` exist **only** in `starting | running | stopping` variants. The `stopped` and `errored` variants have **no** `workspaceName` field. `recordUsage()` is called from `onStop` and `onError` (WP-06:253, 307) — `onError` runs on the `errored` variant, where `this.state.workspaceName` is **type-illégal**.
- **Fix:** Either (a) widen the state union to always carry these fields, or (b) call `recordUsage()` BEFORE transitioning to `errored` (capture the workspace/profile in a local before `transitionState("errored", …)`). Document which approach.

### B3. **D1 Migration Number `0090` Already Used**
- **Location:** §3.1 line 43: `migrations/d1/0090_runner_devenv_usage.sql`
- **Problem:** `migrations/d1/0090_dsr_tickets.sql` **already exists** (verified via `ls migrations/d1/`). The repo's forward-only migration runner would fail with "duplicate migration".
- **Fix:** Use a fresh number, e.g. `0094_runner_devenv_usage.sql` (0093 is the latest existing). Verify with `ls migrations/d1/ | tail -5` before assigning.

### B4. **Idempotency Check Has Race Condition (No DB-Level UNIQUE)**
- **Location:** §3.2 line 112-114 (SELECT then INSERT pattern)
- **Problem:** The code does `SELECT … WHERE idempotency_key = ?` then `INSERT …` without a UNIQUE constraint. Two concurrent `recordUsage()` calls (e.g., onStop + onError firing in race, or a DO restart) can both pass the SELECT and both INSERT, producing duplicate rows. This **double-bills the tenant**.
- **Why the index alone is not enough:** The `idx_runner_devenv_usage_idempotency` index is non-unique. The existing canonical pattern (ASK-2, `usage_event_staging`) uses `PRIMARY KEY (tenant_id, request_id)` to make the dedup coordinate structural.
- **Fix:** Either:
  - (a) `CREATE UNIQUE INDEX … ON runner_devenv_usage (tenant_id, idempotency_key)` and switch INSERT to `INSERT … ON CONFLICT (tenant_id, idempotency_key) DO NOTHING`, OR
  - (b) Make `idempotency_key` the PRIMARY KEY alone (scoped per tenant via the table being per-tenant), OR
  - (c) Best: **adopt the canonical ASK-2 pattern** — stage into the existing `usage_event_staging` table (migration 0017) so a single canonical dedup pipeline exists. See B5.

### B5. **NOT ASK-2 Compatible — Reinvents the Wheel**
- **Location:** §3.1 (table), §3.2 (endpoint), §3.3 (payload)
- **Problem:** The WP claims "ASK-2 compatible" but is **completely incompatible** with the existing canonical billing ingest (`crates/corelink-container/src/routes/billing_ingest.rs`):
  - **Endpoint path:** WP-07 = `/internal/v1/billing/devenv/usage` ≠ ASK-2 = `/internal/v1/billing/usage`
  - **Auth header:** WP-07 = `Authorization: Bearer <key>` ≠ ASK-2 = `X-Corelink-Internal-Auth: <key>` (fail-CLOSED, constant-time compare via `internal_auth_ok`, ≥32 chars, route not mounted if absent/short)
  - **Auth compare:** WP-07 = `token !== env.BILLING_DEVENV_INGEST_AUTH_KEY` (timing-LEAK — string `!==` is not constant-time)
  - **Request shape:** WP-07 = `{ tenant_id, container_id, workspace_name, profile_name, vcpu_seconds, period_start_ms, period_end_ms, idempotency_key }` ≠ ASK-2 = `[{ tenant_id, event_kind: "runner_slot_seconds", qty: u64, billing_period: "YYYY-MM", region: 3-char, source, time_ms: u64, idem_key: 64-hex BLAKE3 }]`
  - **Storage:** WP-07 = brand-new `runner_devenv_usage` table ≠ ASK-2 = canonical `usage_event_staging` (migration 0017, `PRIMARY KEY (tenant_id, request_id)`, CHECK constraints, hash chain)
  - **Tenant ID validation:** WP-07 = `typeof === "string"` ≠ ASK-2 = `Uuid::parse_str()` (canonical UUIDv7)
  - **Idempotency key shape:** WP-07 = free-text string ≠ ASK-2 = 64-char BLAKE3 hex (canonicalized to lowercase)
  - **Mark of "compatibility":** The WP also introduces a SECOND secret `BILLING_DEVENV_INGEST_AUTH_KEY` instead of reusing `BILLING_INGEST_AUTH_KEY` — split blast radius but doubles the secret surface
- **Why this is BLOCKING:** A second, parallel billing pipeline violates INV-BILLING-NO-LOSS / INV-BILLING-NO-DUP. Two ingestion paths = two sets of records = reconciliation drift. The ASK-2 path has full audit chain (BLAKE3), tenant UUID validation, region-tagged drain, and aggregator rollup — none of which the new path has.
- **Fix:** Reuse ASK-2. Add a new `event_kind` value (e.g., `UsageEventKind::DevenvVcpuSeconds`) to the canonical enum (`corelink_billing_emit::UsageEventKind`). Map `vcpu_seconds` → `qty: u64`, `period_start_ms` → `time_ms`, `tenant_id + container_id + period_start_ms` → BLAKE3-derived `idem_key`, workspace/profile → `source`. Mount at the existing `/internal/v1/billing/usage`. The Worker calls the existing endpoint. **Delete the new D1 table and the new endpoint entirely.**

### B6. **D1 Schema Column Naming Violates Repo Convention**
- **Location:** §3.1 lines 60-67: `period_start_ms`, `period_end_ms`, `created_at_ms` use `_ms` suffix
- **Problem:** Per `migrations/d1/0017_usage_event_idem.sql:60-64` and the Lote 10.7bis P0-3 column-drift lesson documented in the repo:
  > "No `_ms` column-name suffix per Lote 10.7bis P0-3 column-drift lesson; instead `*_at` Unix epoch ms."
  - Existing migrations use `emitted_at`, `created_at_ms` only in earlier migrations but new ones prefer `*_at`
  - Migration 0087 `runner_billing` uses `created_at_ms`/`updated_at_ms` (older pattern), but 0089 `usage_daily` likely uses `*_at` — must verify
- **Fix:** Rename to `period_start_at`, `period_end_at`, `created_at` (drop `_ms`). Verify with `grep "_at_ms\|_at INTEGER" migrations/d1/`.

### B7. **D1 Schema Missing CHECK Constraints**
- **Location:** §3.1, no CHECK clauses
- **Problem:** The schema relies on application-layer validation alone. Per `0017_usage_event_idem.sql:122-146`, every existing migration uses inline CHECKs. Invariants I4 (`period_end_ms > period_start_ms`) and I5 (`vcpu_seconds >= 0`) are NOT enforced at storage layer — bad data is a clean INSERT.
- **Fix:** Add:
  ```sql
  CHECK (vcpu_seconds >= 0),
  CHECK (period_end_at > period_start_at),
  CHECK (length(tenant_id) > 0),
  CHECK (length(idempotency_key) > 0),
  ```

### B8. **Index `idx_runner_devenv_usage_idempotency WHERE idempotency_key != ''` is Wrong**
- **Location:** §3.1 line 74-75
- **Problem:** `idempotency_key TEXT NOT NULL` is already a constraint (NOT NULL means it's never empty NULL — but actually TEXT can be empty string `''`). The partial index condition `idempotency_key != ''` is also fine for SQLite. BUT the bigger problem: the index is on `idempotency_key` **alone**, not `(tenant_id, idempotency_key)`. Two tenants could collide on the same key (or one tenant's key collides with another's). Also: if the key is generated as `${ctx.id}:${startedAt}` and `ctx.id` is unique per DO, the collision is impossible — **but** the index still must be `(tenant_id, idempotency_key)` to support tenant-scoped lookups.
- **Fix:** Either drop the index (the PK `(tenant_id, usage_id)` plus a UNIQUE on `(tenant_id, idempotency_key)` is enough), or make it a UNIQUE composite index. Also: see B4 — prefer the canonical ASK-2 staging table.

### B9. **Per-Tenant Concurrency Quota is Mathematically Impossible to Exceed**
- **Location:** §3.4 lines 282-285: `const currentCount = isRunning ? 1 : 0; // Per-tenant = 1 DO`
- **Problem:** The DO namespace `RUNNER_DEVENV_DO` is keyed per-tenant (`idFromName(tenantId)`), so at most ONE DevEnv per tenant can ever exist. The comment says "Per-tenant = 1 DO" but the tier limits claim `runner_pro = 2`, `runner_team = 4`, `runner_scale = 8`, `runner_max = 16`. **These limits are physically unenforced** — a Pro tenant can never run 2 DevEnvs because the DO namespace has one DO per tenant.
- **Either:**
  - (a) The architecture is wrong: DO namespace should be keyed per-session, not per-tenant, OR
  - (b) The tier limits are wrong: should be `runner_starter=1, runner_pro=1, …` (tier only affects other dimensions like vCPU size / idle timeout), OR
  - (c) The campaign should document "tier limits apply to vCPU-hours/month" (cf. `max_vcpu_h` in migration 0072) NOT concurrency
- **Fix:** Pick one. Most likely: tier limits govern `max_vcpu_h` (already exists) and per-tenant DevEnv count is always 1 (the design). Drop the misleading "Pro=2, Team=4, Scale=8, Max=16" numbers and refer to `max_vcpu_h` for billing tiers.

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **Auth Compare is Not Constant-Time (Timing Oracle)**
- **Location:** §3.2 line 96: `if (token !== env.BILLING_DEVENV_INGEST_AUTH_KEY)`
- **Problem:** String `!==` short-circuits on the first differing byte — measurable timing leak allows remote extraction of the auth key byte-by-byte. Per `billing_ingest.rs:36-37`: "The compare reuses the exact constant-time, length-padded gate from `internal_auth_ok`."
- **Fix:** Import the constant-time compare from the Rust side, or in TS use a manual length-padded compare (HMAC-based constant-time). Easiest: route this through the existing ASK-2 endpoint (see B5) so the constant-time gate is reused.

### H2. **No `BILLING_DEVENV_INGEST_AUTH_KEY` Min Length / Fail-CLOSED Mount**
- **Location:** §3.2 + §3.6 (env var declared but no validation)
- **Problem:** ASK-2 pattern: if the auth key is absent or < 32 chars, the route is **NOT mounted** (warn log) — fail-CLOSED. WP-07 declares the env var but the route is mounted regardless of its presence, so a missing key → 401 always (fail-OPEN route is mounted).
- **Fix:** Mirror ASK-2: at Worker boot, check `env.BILLING_DEVENV_INGEST_AUTH_KEY?.length >= 32`; if not, do not mount the route (and log a clear warning).

### H3. **Auth Header Mismatch — `Authorization: Bearer` is Wrong Convention**
- **Location:** §3.2 line 92: `if (!authHeader || !authHeader.startsWith("Bearer "))`
- **Problem:** The existing internal auth pattern uses `X-Corelink-Internal-Auth` (single header, no scheme). The `Authorization: Bearer` form is a customer-facing convention; using it for service-to-service is inconsistent and risks confusion with PATs.
- **Fix:** Use `X-Corelink-Internal-Auth` header (no scheme prefix). Or: reuse ASK-2 entirely (B5).

### H4. **`getTenantRunnerTier` Queries Wrong Column**
- **Location:** §3.4 line 304: `SELECT plan FROM runners_entitlement WHERE tenant_id = ?1 LIMIT 1`
- **Problem:** `runners_entitlement` (migration 0070) has columns `tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h` (added in 0072). The `plan` column is **INFORMATIONAL ONLY** per migration 0070:32-36:
  > "plan TEXT — informational label for the entitlement source/plan (nullable; the runners side owns its own custom plan semantics)."
  Quota enforcement must read **`max_concurrency`**, not `plan`. Also, the query uses `LIMIT 1` on a `tenant_id PRIMARY KEY` lookup — redundant.
- **Fix:** `SELECT max_concurrency, max_vcpu_h FROM runners_entitlement WHERE tenant_id = ?1` (PK lookup, no LIMIT). Or, given the per-tenant-DO architecture (B9), the quota check is moot — the design only allows 1 DO per tenant.

### H5. **Tier Limit Hardcoded Mapping Disagrees With DB**
- **Location:** §3.4 lines 263-269: `limits: Record<string, { maxConcurrent }>` mapping
- **Problem:** The tier→maxConcurrent map is hardcoded in Worker code, duplicating the source-of-truth in `runners_entitlement.max_concurrency`. If the operator changes a tier limit via D1, the Worker still enforces the old number. Two sources of truth → drift.
- **Fix:** Read the limit from `runners_entitlement.max_concurrency`. Drop the hardcoded map.

### H6. **`enforceDevenvQuota` Fail-OPENs on DO Error (Quota Bypass)**
- **Location:** §3.4 line 295-298: `catch { return { allowed: true }; }`
- **Problem:** A DO outage or stub error → `{ allowed: true }` — the quota guard **bypasses itself on failure**. An attacker (or a misbehaving client) can intentionally induce DO errors to bypass the limit.
- **Fix:** Fail-CLOSED: `catch { return { allowed: false, reason: "Quota check unavailable" }; }` with a 503 response (not 403).

### H7. **`enforceDevenvQuota` Accepts Unused `request: Request`**
- **Location:** §3.4 line 254
- **Problem:** The `request` parameter is never used. Dead arg. Either use it (e.g., for tenant-from-header fallback) or drop it.
- **Fix:** Drop the `request` param. Or document why it's there.

### H8. **No `max_vcpu_h` Enforcement Despite Migration 0072 Adding It**
- **Location:** Entire WP
- **Problem:** Migration 0072 (`runners_entitlement.max_vcpu_h`) added a per-tenant monthly vCPU-hour ceiling. WP-07 only enforces concurrency, not the monthly compute ceiling. A tenant on `runner_starter` (100 vCPU-h/mo per the campaign plan) can run continuously within their concurrency cap and blow through the ceiling.
- **Fix:** Add a monthly vCPU-hour aggregator (separate table or use `usage_daily` at migration 0089). On each `recordUsage`, sum the tenant's vCPU-seconds for the current month and reject when ≥ `max_vcpu_h * 3600`. (This is also why B5 is BLOCKING — the canonical ASK-2 pipeline already has the rollup surface.)

### H9. **Inconsistent Pricing Calculation (No Actual Rate Computation)**
- **Location:** §3.7 lines 380-399
- **Problem:** §3.7 says "CoreLink markup: 50% (configurable via BILLING_DEVENV_MARKUP_PERCENT)" but no code applies the markup. The math is shown in comments only. The `BILLING_DEVENV_MARKUP_PERCENT` env var is declared (§3.6) but no handler reads it.
- **Fix:** Either (a) remove §3.7 if invoice generation is in another WP (the Scope says "Out of Scope: Stripe invoice generation"), or (b) add a pricing helper that computes `customer_price = cf_cost * (1 + markup/100)`. Note `UsageEventKind` should also carry a `unit_cost_micro_usd` so the aggregator handles pricing.

### H10. **No `tenant_id` UUID Validation on the Worker**
- **Location:** §3.2 line 162-163: `if (!payload.tenant_id || typeof payload.tenant_id !== "string")`
- **Problem:** Only checks "is a non-empty string". `tenant_id` is supposed to be a canonical UUID. ASK-2 uses `Uuid::parse_str()` to fail-CLOSED on malformed IDs. A `"not-a-uuid"` tenant_id is currently accepted.
- **Fix:** Add `UUIDv7 regex` check: `/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i`. Or: route through ASK-2 (B5).

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **Idempotency Key Uses `:` Without Length Validation**
- **Location:** §3.3 line 211: `${this.ctx.id}:${this.state.startedAt}`
- **Problem:** The format is not validated as a string, not normalized (case-folded), and `startedAt` is just a number — but a negative or zero `startedAt` would yield a degenerate key. Mostly cosmetic since `startedAt` is set to `Date.now()`.
- **Fix:** Document the canonical format and validate length (e.g., 32-256 chars).

### M2. **D1 Table Lacks Reporting Indexes**
- **Location:** §3.1, only 2 indexes
- **Problem:** No index on `(tenant_id, workspace_name)` for per-workspace billing reports, no index on `(tenant_id, created_at)` for time-range queries. `usage_daily` (migration 0089) likely rolls this up — but if the new table is kept, indexes for WP-09 dashboard queries are needed.
- **Fix:** Add `CREATE INDEX idx_runner_devenv_usage_tenant_workspace ON runner_devenv_usage(tenant_id, workspace_name)`. Or: drop the new table and use canonical `usage_daily`.

### M3. **No Retention / TTL on `runner_devenv_usage`**
- **Location:** §3.1 (table is unbounded)
- **Problem:** The table grows by 1 row per session per tenant, forever. No `expires_at` column, no GC strategy. For a 10-year retention policy this is fine, but document the retention.
- **Fix:** Either add a TTL/partition strategy OR a `R2 archive after 90 days` policy (mirroring `usage_event_staging` → R2 Object Lock 7y).

### M4. **Idempotency Key Check Uses Non-Indexed SELECT**
- **Location:** §3.2 line 112-114: `SELECT usage_id FROM runner_devenv_usage WHERE idempotency_key = ?1`
- **Problem:** The index is on `idempotency_key` ALONE, not `(tenant_id, idempotency_key)`. A lookup like this on a busy tenant table would full-scan. The check is also racy with the INSERT (B4).
- **Fix:** Combine into `INSERT … ON CONFLICT (tenant_id, idempotency_key) DO NOTHING` and report dedup via `RETURNING`.

### M5. **Pricing Section Shows Wrong Math**
- **Location:** §3.7 line 392: `720h × $0.0864 = $62.21` → `With 50% markup: $93.32/month`
- **Problem:** 50% markup on $62.21 = $93.315, not $93.32. Minor. But more importantly, the monthly estimate uses 720h (24×30 = 720) which is **continuous 24/7 usage**, not realistic. A "Starter tier (1 DevEnv, 30m idle timeout): ~$15-20/mo typical usage" implies <12h/day. The numbers should match.
- **Fix:** Drop the misleading monthly estimate, or anchor to a realistic 8h/day × 30 = 240h/month. Better: remove the pricing section since "Out of Scope: Stripe invoice generation (existing pipeline)".

### M6. **Response Body Missing `usage_id` on Duplicate**
- **Location:** §3.2 line 117: `return { ok: true, duplicate: true }`
- **Problem:** The duplicate response doesn't include the existing `usage_id` — the caller (DO) has no way to know which row already exists. Mostly cosmetic since the DO doesn't use the id, but inconsistent with the success path.
- **Fix:** `return { ok: true, duplicate: true, usage_id: existing.usage_id }`.

### M7. **No Rate Limiting on `/internal/v1/billing/devenv/usage`**
- **Location:** §3.2 entire handler
- **Problem:** A misbehaving or malicious caller can hammer the endpoint. ASK-2 has `MAX_BATCH_RECORDS=1024` cap; WP-07 has no body size cap, no per-tenant rate limit, no IP-based limit.
- **Fix:** Add a `MAX_BODY_BYTES` cap (16 KiB) and a per-tenant rate limit (e.g., 10 RPS via existing ratelimit layer).

### M8. **Inconsistent Naming with WP-04 (`period_start` vs `period_start_ms`)**
- **Location:** §3.1 (uses `period_start_ms`) vs WP-04:352 (uses `period_start`)
- **Problem:** The two WPs disagree on field name. WP-06 also implicitly uses these fields. Downstream code will be confused.
- **Fix:** Pick one (the more precise `period_start_ms` since the value IS milliseconds). If WP-07 is kept, propagate the rename to WP-04.

---

## 📋 DoD Gap Analysis

The DoD has 10 items. Checking each:

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|---------|
| 1 | D1 migration `0090_runner_devenv_usage.sql` applied | ❌ **FAIL** | Migration number collides with `0090_dsr_tickets.sql` (B3) |
| 2 | `recordUsage()` posts to billing ingest endpoint | ❌ **FAIL** | `recordUsage()` already defined in WP-04 — duplicate (B1) |
| 3 | Billing ingest endpoint validates payload + idempotency | ⚠️ **Partial** | Validates shape but auth is not constant-time, key not tenant-scoped, INSERT not idempotent at DB level (B4, H1) |
| 4 | D1 table populated with correct fields | ⚠️ **Partial** | Missing CHECK constraints (B7), column naming non-conformant (B6) |
| 5 | Per-tenant concurrency limit enforced | ❌ **FAIL** | Architecture has 1 DO/tenant, so limit is structurally always 1 (B9). Pro/Team/Scale/Max numbers are aspirational fiction |
| 6 | Markup applied at invoice generation | ❌ **FAIL** | No code applies the markup; env var declared but unused (H9) |
| 7 | Idempotency key prevents double-billing | ❌ **FAIL** | Race condition (B4), not enforced at DB layer |
| 8 | vCPU-seconds calculation correct | ⚠️ **Partial** | `vcpu_seconds = wall_seconds × 4` correct for standard-4, but `Math.floor` rounds down 0.5s sessions to 0 and skips (documented but not justified) |
| 9 | Billing ingest auth key required | ⚠️ **Partial** | Auth required, but compare is not constant-time (H1), no min length (H2), header convention wrong (H3) |
| 10 | Usage events survive DO restart | ⚠️ **Partial** | `onError` is called on crash, `recordUsage()` is fire-and-forget, but if the network call hangs and DO is killed, the event is lost. No durable retry queue. |

**DoD Score: 0/10 PASS, 4/10 PARTIAL, 6/10 FAIL**

---

## 📋 Invariants Verification

| Invariant | Enforced in Code? | Verdict |
|-----------|-------------------|---------|
| I1: `vcpu_seconds = wall_seconds × 4` | ✅ Yes (line 203) | **ENFORCED** |
| I2: `idempotency_key = container_id:period_start_ms` unique per session | ⚠️ Generated correctly, but no DB UNIQUE constraint (B4) | **NOT ENFORCED** |
| I3: Duplicate idempotency key → single D1 record | ❌ Race allows double-insert (B4) | **NOT ENFORCED** |
| I4: `period_end_ms > period_start_ms` always | ⚠️ Validated in handler, no DB CHECK (B7) | **PARTIAL** |
| I5: `vcpu_seconds ≥ 0` always | ⚠️ Validated in handler, no DB CHECK (B7) | **PARTIAL** |
| I6: Per-tenant active DevEnvs ≤ tier limit | ❌ Architecture makes this 1 always (B9) | **NOT ENFORCED** |
| I7: Billing ingest auth key validated on every request | ⚠️ Yes, but not constant-time (H1) | **PARTIAL** |
| I8: Usage recording never fails DO lifecycle (fire-and-forget) | ✅ Yes (`try/catch` logs and swallows) | **ENFORCED** |

**Invariants Enforced: 2/8 (25%)** — INSUFFICIENT

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| Idempotency: composite key | ❌ Not at DB level (B4) | — |
| Audit Trail: usage_id, vcpu_seconds, tenant logged | ⚠️ In log, not in D1 audit chain (B5) | — |
| Fire-and-Forget | ✅ Yes | line 242-245 |
| Per-Tenant Isolation: `WHERE tenant_id = ?` | ⚠️ Idempotency check doesn't include `tenant_id` (B8) | — |
| Tier Enforcement: checked at Worker ingress | ⚠️ Yes, but architectural (B9) | — |
| Markup Transparency: env var + visible on invoice | ❌ Env var declared, not used (H9) | — |

**Quality Standards: 1/6 MET** — INSUFFICIENT

---

## 📋 Self-Check Points Analysis

The WP has 3 self-check points. Checking each:

### Self-Check 1: Idempotency Correctness
- [x] Idempotency key = `container_id:period_start_ms` — ✅ format correct
- [x] Billing ingest checks `idempotency_key` before insert — ⚠️ race condition (B4)
- [x] Duplicate key → returns 200 with `duplicate: true` — ⚠️ but no `usage_id` returned (M6)
- [x] `onStop` and `onError` both use same key format — ❌ `onError` runs on `errored` state where `workspaceName` doesn't exist (B2)
- [x] Retry-safe: network timeout → retry → same key → no duplicate — ❌ DB doesn't enforce uniqueness (B4)

**Verdict: 1/5 PASS, 2/5 PARTIAL, 2/5 FAIL**

### Self-Check 2: vCPU-Second Calculation Accuracy
- [x] standard-4 = 4 vCPU (Cloudflare docs) — ✅
- [x] `vcpu_seconds = wall_seconds × 4` — ✅
- [x] `wall_seconds = (period_end_ms - period_start_ms) / 1000` — ✅
- [x] 1 hour session = 3600s × 4 = 14,400 vCPU-seconds — ✅
- [x] Minimum session: 1 second = 4 vCPU-seconds — ⚠️ `Math.floor` rounds 0.5s sessions to 0 (line 205 skips)

**Verdict: 4/5 PASS, 1/5 PARTIAL**

### Self-Check 3: Tier Limit Enforcement
- [x] Starter: 1 concurrent DevEnv — ✅ (structural, B9)
- [x] Pro: 2 concurrent — ❌ impossible per architecture (B9)
- [x] Team: 4 concurrent — ❌ impossible per architecture (B9)
- [x] Scale: 8 concurrent — ❌ impossible per architecture (B9)
- [x] Max: 16 concurrent — ❌ impossible per architecture (B9)
- [x] Checked at Worker ingress (before DO spawn) — ⚠️ checked AFTER tenant DO lookup (B6 fail-open)
- [x] 403 with clear error message when limit reached — ✅

**Verdict: 2/7 PASS, 1/7 PARTIAL, 4/7 FAIL**

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 9 | 0 | **-9** |
| High Issues | 10 | 0 | **-10** |
| Medium Issues | 8 | 0 | **-8** |
| DoD Pass Rate | 0% | 100% | **-100%** |
| Invariants Enforced | 25% | 100% | **-75%** |
| Quality Standards | 17% | 100% | **-83%** |
| Self-Check Pass | 50% | 100% | **-50%** |

**OVERALL VERDICT: ❌ FAIL — Requires major rework before sign-off**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers)
1. **B1** — REMOVE §3.3 entirely; `recordUsage()` belongs in WP-04 (already defined)
2. **B5** — DELETE the new endpoint and table; reuse canonical ASK-2 (`/internal/v1/billing/usage` + `usage_event_staging`)
3. **B3** — Renumber migration to `0094_runner_devenv_usage.sql` (or DELETE if B5 is applied)
4. **B4** — Add DB UNIQUE constraint on `(tenant_id, idempotency_key)` OR use ASK-2 PRIMARY KEY
5. **B9** — Reconcile tier limits with 1-DO-per-tenant architecture (drop the Pro/Team/Scale/Max concurrency numbers, or document them as `max_vcpu_h` instead)
6. **B2** — Fix `recordUsage` access to `this.state.workspaceName` against discriminated union

### Should Fix (High)
1. H1, H2, H3 — Auth must be constant-time, ≥32 char min, fail-CLOSED mount, `X-Corelink-Internal-Auth` header
2. H4 — Read `max_concurrency` from DB, not `plan`
3. H5 — Drop hardcoded tier map, read from DB
4. H6 — Fail-CLOSED on DO error
5. H8 — Enforce `max_vcpu_h` monthly ceiling (0072)
6. H9, H10 — Wire markup + UUID validation

### Nice to Fix (Medium)
1. M1, M4, M6, M7 — Idempotency + rate limit polish
2. M2, M3 — Indexes + retention
3. M5 — Fix pricing math
4. M8 — Pick `period_start_ms` and propagate to WP-04

---

## 🤝 CROSS-WP COORDINATION NEEDS

1. **WP-04** owns `recordUsage()` already — WP-07 must NOT redefine it. If WP-07 migrates to ASK-2 pattern (B5), the WP-04 definition also needs updating. **Coordination: edit WP-04 §3.2 to use the canonical ASK-2 endpoint.**

2. **WP-06** already calls `recordUsage()` in `onStop` (line 253) and `onError` (line 307). No change to WP-06 needed IF the WP-04 implementation is updated.

3. **WP-01** state union is the source of truth for `DevenvState`. WP-07 §3.3 breaks the type (B2). Either widen the union or capture fields before transitioning to `errored`.

4. **WP-08** (ingress) already imports `enforceDevenvQuota` from `./devenv_guard` (line 63). If WP-07 is restructured to use the canonical ASK-2 path AND the architectural concurrency limits are dropped, WP-08 §3.1-3.3 also needs review (the `quota.allowed` check is moot if max is always 1).

5. **WP-09** (Dashboard UI) consumes billing data. If WP-07 reuses ASK-2 staging → `usage_daily` (migration 0089) → rollup, WP-09 doesn't need to know about the new `runner_devenv_usage` table.

6. **Ask-2 / billing_ingest.rs** (`crates/corelink-container/src/routes/billing_ingest.rs`): the canonical `UsageEventKind` enum needs a new variant (e.g., `DevenvVcpuSeconds`). Owner: corelink-server TL (current WP-07 owner — natural fit).

---

## 🎯 RECOMMENDED PATH FORWARD

**The single biggest fix is B5: reuse the canonical ASK-2 pipeline.** This collapses:
- B1 (duplicate recordUsage) — just delete §3.3
- B2 (state union type error) — moot, no recordUsage in WP-07
- B3 (migration number collision) — moot, no new table
- B4 (race condition) — solved by `usage_event_staging.PRIMARY KEY (tenant_id, request_id)`
- B5 itself — by definition
- B6 (column naming) — moot
- B7 (missing CHECKs) — moot
- B8 (wrong index) — moot
- H1, H2, H3 (auth) — reused from `internal_auth_ok` + `BILLING_INGEST_AUTH_KEY`
- H4, H5 (tier limit source) — separate fix (B9)
- H8 (`max_vcpu_h` enforcement) — still needed, but uses canonical aggregator
- H9 (markup unused) — can be deferred to invoice WP
- H10 (UUID validation) — reused from `Uuid::parse_str`

**The "B5 path" reduces 9 BLOCKING + 10 HIGH issues to ~3 BLOCKING + 2 HIGH.** B9 (architecture mismatch) and H4/H5/H6 (quota guard correctness) remain as separate fixes.

---

## NEXT STEPS

1. **Apply B5** — delete §3.1 D1 migration, §3.2 endpoint, §3.3 recordUsage; replace with a §3.x "Reuse ASK-2" section that documents the new `UsageEventKind::DevenvVcpuSeconds` variant + the Worker-side call into the existing `/internal/v1/billing/usage` endpoint.
2. **Apply B9** — drop the "tier limits per concurrency" map; document the actual model (1 DO per tenant, `max_vcpu_h` per tier governs monthly compute).
3. **Apply B2** — capture `workspaceName`/`profileName` into locals before `transitionState("errored", ...)` in `onError`, OR widen the state union.
4. **Apply H1, H2, H3** — auth conventions if any new endpoint is added.
5. **Apply H6** — fail-CLOSED quota guard.
6. **Apply M5, M6, M7, M8** — minor polish.
7. **Coordinate with WP-04** — its `recordUsage()` must also be updated to use the canonical endpoint.

**Do NOT proceed to WP-08 implementation (depends on WP-07) until WP-07 passes review.**

---

**END OF WP-07 ITERATION 1 REVIEW**
