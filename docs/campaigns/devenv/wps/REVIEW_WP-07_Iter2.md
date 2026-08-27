# WP-07 REVIEW — Iteration 2 (CROSS-WP + CANONICAL-SCHEMA AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Previous Verdict:** ❌ FAIL (9 B + 10 H + 8 M) — major restructure applied (B5/B1/B3/B2/B9 + H1/H2/H3/H4/H5/H6/H8)  
**New Verdict:** ❌ **FAIL — 5 NEW BLOCKING + 4 NEW HIGH + 3 NEW MEDIUM issues (6 iter-1 fixes REGRESSED or incomplete)**

---

## 🔁 ITER 1 FIX RE-VERIFICATION (Did the rewrite actually work?)

| Iter-1 issue | Iter-2 status | Evidence |
|--------------|---------------|----------|
| **B1** duplicate `recordUsage()` | ✅ Fixed | §3.2 is now a `REPLACEMENT` of WP-04:336-359 with explicit "Signature unchanged" call-site note |
| **B2** discriminated-union type error on `workspaceName` in `errored` | ✅ Fixed | §3.2 captures locals BEFORE state transition; `errored`/`stopped` short-circuit to `""` |
| **B3** migration number `0090` collision | ✅ Fixed | New D1 table REMOVED entirely; uses canonical `usage_event_staging` (0017) + `usage_daily` (0089) |
| **B4** race condition on idempotency | ✅ Fixed | Canonical `usage_event_staging.PRIMARY KEY (tenant_id, request_id)` + `INSERT … ON CONFLICT DO NOTHING` (reuse) |
| **B5** NOT ASK-2 compatible | ⚠️ **PARTIAL — REGRESSED** | URL is now `/internal/v1/billing/usage` ✅, but **payload shape diverges from canonical** — see B15 |
| **B6** column naming `_ms` suffix | ✅ Fixed | New table REMOVED; queries canonical `usage_daily` |
| **B7** missing CHECK constraints | ✅ Fixed | Moot (no new table) |
| **B8** wrong index on idempotency_key | ✅ Fixed | Moot (no new table) |
| **B9** 1-DO-per-tenant vs tier limits | ✅ Fixed | §3.3 hard-rules: "Concurrency cap is `min(max_concurrency, 1)`" — fiction declared defense-in-depth |
| **H1** non-constant-time auth | ✅ Fixed | Moot — uses canonical `internal_auth_ok` (Rust) on the server side |
| **H2** no min length / fail-CLOSED | ✅ Fixed | Moot — `build_state_from_env` requires ≥32 chars (canonical) |
| **H3** wrong auth header | ✅ Fixed | Now `X-Corelink-Internal-Auth` per canonical |
| **H4** `plan` column queried | ✅ Fixed | §3.3 reads `max_concurrency, max_vcpu_h` |
| **H5** hardcoded tier map | ✅ Fixed | DB-only; no in-code map |
| **H6** fail-OPEN on DO error | ✅ Fixed | §3.3 `try/catch → { allowed: false, reason: "Quota check unavailable" }` |
| **H8** no `max_vcpu_h` enforcement | ⚠️ **REGRESSED** | §3.3 sums `usage_daily.vcpu_seconds` — **column does not exist** (see B15) |
| **H9** markup unused | ✅ Deferred | `BILLING_DEVENV_MARKUP_PERCENT` declared; invoice-time consumption is another WP |
| **H10** no UUID validation | ✅ Fixed | Canonical `Uuid::parse_str()` server-side |
| **M1–M8** | ✅ All moot or fixed | New table removed; canonical schema's PK + CHECKs handle all concerns |

**Iter-1 fix score: 16/19 correct, 2 PARTIAL (B5, H8), 0 missed.**

---

## 🔴 NEW BLOCKING ISSUES (Caught in Iteration 2)

### B15. **`usage_daily` has NO `vcpu_seconds` column — `SUM(vcpu_seconds)` query is a 500**

- **Location:** §3.3 lines 283-288 — `SELECT COALESCE(SUM(vcpu_seconds), 0) AS used FROM usage_daily WHERE tenant_id = ?1 AND day_at >= ?2`
- **Problem:** Verified via `migrations/d1/0089_usage_daily.sql` (read above): the table has exactly the columns `tenant_id, day, reads, writes, hits, misses, updated_at_ms`. There is **no `vcpu_seconds` column**, no `day_at` column (it's `day` TEXT `'YYYY-MM-DD'`), and the table is documented as:
  > "DISPLAY telemetry, NOT billing. … NEVER used to bill or to gate a request (no hot-path read of it)."
  The query as written would fail at runtime with `D1_ERROR: no such column: vcpu_seconds`. Every DevEnv start would 503 fail-CLOSED on the quota check.
- **Why this is BLOCKING:** The entire monthly vCPU-hour ceiling enforcement is non-functional. The plan's billing gate is the central tenant-isolation mechanism — silent broken state.
- **Evidence:** Iter 1 review assumed `usage_daily` would have `vcpu_seconds` (it was "verified against migration 0089" but the verification was skim-level). The actual schema doesn't carry the column. The canonical rollup for billing is the **billing-aggregator** (cron drain), not `usage_daily`. The aggregator's output table is `usage_monthly` or similar — needs verification (see B16).
- **Fix:** Two paths. Pick one:
  - **(a) Use the canonical aggregator output:** Read from the aggregator's monthly rollup table (likely `runner_billing` per migration 0087, or whatever the aggregator writes). Verify via `grep -r "vcpu_seconds\|MAX(vcpu\|SUM(vcpu" crates/corelink-billing-aggregator/ migrations/`.
  - **(b) Add a per-tenant `devenv_monthly_vcpu` side-table** that the Worker updates on every `recordUsage` (incrementally — no SUM on the hot path; just an UPSERT with `+= vcpuSeconds`). Reads become `SELECT monthly_used WHERE tenant_id = ?` (PK lookup, no SUM). No schema collision with `usage_daily`. This is also the only way to fail-CLOSED on a 503 without rolling a `SUM` over an unbounded read set every quota check.
- **Recommendation:** Path (b). The quota check is on the hot path (every `/v1/customer/devenv/*` call) — `SUM(vcpu_seconds)` over `usage_daily` rows for the current month is a full month-scan per request. The canonical aggregator output is the source of truth for **invoice** (eventual); the **gate** needs a hot-path O(1) read.

### B16. **No `unit: UsageUnit` field in payload — canonical schema rejects unknown event kinds / fails discriminator**

- **Location:** §3.2 lines 157-166 — `recordUsage()` payload: `[{ tenant_id, event_kind, qty, billing_period, region, source, time_ms, idem_key }]`
- **Problem:** Per `corelink-billing-emit::UsageEvent::new` (read at `crates/corelink-billing-emit/src/event.rs:532-564`):
  - `event_kind` deserializes via the canonical 8-element enum: `StorageBytesHourly | EgressBytes | AcLookup | CasGet | CasPut | ReplayRequest | RunnerSlotSeconds | RunnerVcpuSeconds`. The string `"devenv_vcpu_seconds"` (WP-07 §3.1's new variant) is **not in the enum** → 400 `unknown UsageEventKind`.
  - The canonical `UsageEvent` envelope ALSO carries `id` (UUIDv7), `source`, `subject`, `time_ms`, `datacontenttype`, `region`, `specversion`, `event_type` (=`"corelink.billing.usage.recorded"`) — all CloudEvents 1.0 attributes. The wire shape in `billing_ingest.rs:340-359` (read above) is the flat `UsageRecordWire`, not the full CE envelope, and the ingest does NOT populate `id`/`subject`/etc. — but the **server-side validation** in `validate_record` requires `UsageEventKind` to deserialize cleanly. WP-07's "DEVENVVcpuSeconds" is not registered.
  - The Rust unit on the aggregator side will treat unknown kinds as `UsageUnit::OpCount` (because the only OpCount path covers CasGet/CasPut/etc.). vCPU-seconds billed as op_count means **wrong unit pricing** at Stripe aggregator time.
- **Why this is BLOCKING:** The WP claims to "reuse the canonical ASK-2 pipeline" but introduces a NEW kind name that the canonical enum doesn't know. Either the enum must be extended (which requires Finance + Compliance sign-off per the enum doc comment: "Adding a NEW variant requires Finance + Compliance sign-off per WI-S10-001 §6.1.6") OR the existing `RunnerVcpuSeconds` variant must be reused (and the DO is just another runner surface).
- **Fix:** Two paths. Pick one:
  - **(a) Reuse `UsageEventKind::RunnerVcpuSeconds`** (no enum change). The DO is logically a "runner" — the existing variant covers it. The `source` field distinguishes the two surfaces: `corelink/runner/iad` for ephemeral runners, `corelink/devenv/iad` for DevEnvs. Document this in §3.1.
  - **(b) Add `DevenvVcpuSeconds` to the enum** with the formal Finance + Compliance sign-off process (sprint contract §5.1 R-S10-3 — "schema versioning policy keeps two consecutive versions processable concurrently"). This requires a NEW migration/ADR; not a "WP-07 owner ratifies" decision.
- **Recommendation:** Path (a). The DevEnv IS a runner box (Cloudflare Container, same hardware, same vCPU-hours). A new variant is a billing-taxonomy decision, not a wire-shape decision. Reuse + distinguish in `source`.

### B17. **`recordUsage()` payload missing `unit` field — aggregator will under-/over-bill by 4× or use wrong SKU bucket**

- **Location:** §3.2 lines 157-166 — no `unit` field; §3.1 `UsageEventKind` enum has no `unit` association shown
- **Problem:** Per the canonical `UsageUnit` taxonomy (`corelink-billing-emit/src/event.rs:265-286`):
  - `UsageEventKind::RunnerVcpuSeconds` is mapped to `UsageUnit::OpCount` (line 213) — **THIS IS A BUG IN THE CANONICAL ENUM, not WP-07's bug**, but WP-07 inherits it. vCPU-seconds are NOT op-counts; they're a time×count composite. The aggregator routes `OpCount` events to the per-operation Stripe SKU buckets, which is wrong for a time-multiplied metric.
  - WP-07 §3.2 emits `qty: vcpuSeconds` (e.g. `14400` for 1h) with NO `unit` field on the wire. `validate_record` in `billing_ingest.rs:412-454` does not enforce `unit` (it deserializes `event_kind` and the `canonical_unit()` is computed server-side). So WP-07 lands the value in the `OpCount` bucket → aggregator routes to a per-SKU that doesn't exist (`op_count` isn't a Stripe SKU for runners).
- **Why this is BLOCKING:** A DevEnv session that burns 14400 vCPU-seconds would be billed as 14400 op-counts, not 14400 vCPU-seconds. The Stripe invoice shows `runner_op_count × 14400` instead of `runner_vcpu_h × 4`. Customer charged wrong amount.
- **Fix:** Path-of-least-resistance:
  - Update `canonical_unit()` mapping so `RunnerVcpuSeconds` (and any future seconds-based kinds) returns a new `UsageUnit::VcpuSeconds` variant. This requires a NEW `UsageUnit` variant + a check on the aggregator side. Tracked in WI-S10-002 / S-10 backlog. WP-07 raises a CRITICAL issue and proceeds with the assumption that the unit-mapping fix lands before WP-07 ships.
  - OR: emit `qty` as `vcpuSeconds / 4` (count of "wall-seconds") and add a flag/variant of the `runner_slot_seconds` kind. Hack, but uses an existing bucket correctly.
  - **Cross-WP impact:** This affects S-10 sprint work, not just WP-07. Escalate.
- **Recommendation:** Flag as a pre-requisite to merging WP-07: open a follow-up to add `UsageUnit::VcpuSeconds` BEFORE WP-07 §3.2 ships.

### B18. **`computeUsageEvent()` in WP-04 §3.2 uses `(this.state as { clwTenant: string }).clwTenant` — field not in WP-01's discriminated union**

- **Location:** Cross-WP: WP-04 §3.2 line 510 — `tenantId: (this.state as { clwTenant: string }).clwTenant`
- **Problem:** WP-01's `DevenvState` discriminated union (`runner_dev_env.ts:96-130`, read in iter 2) has fields: `status`, `createdAt`, `startedAt`, `workspaceName`, `profileName`, `containerHandle`, `lastHealthCheckAt`, `healthCheckFailures`, `lastError`, `lastWorkspaceName`. There is **NO `clwTenant` field** in any union variant. The cast `as { clwTenant: string }` is `as any` in disguise — it would always be `undefined` at runtime.
- **Why this is BLOCKING (cross-WP):** WP-07 §3.2's `recordUsage` reads `this.envVars.CLW_TENANT` (line 135) — this is the right path (envVars are mutable, set in `start()` per WP-01 §3.3). But WP-04 §3.2's `computeUsageEvent` uses a field that doesn't exist. If WP-04's stub gets called (instead of WP-07's `recordUsage`), the `tenantId` is `undefined` → 400 `bad_tenant_id` from the billing ingest.
- **Fix:** WP-07 must document the canonical path: `this.envVars.CLW_TENANT` (envVars, mutable, set in `start()` per WP-01 §3.3). WP-04's `computeUsageEvent` should be deprecated (the body is now in WP-07 §3.2). WP-07 §3.2 reads from `this.envVars` — verify this field exists in the WP-01 `Env` type (it does: `STATIC_ENV_VARS` includes `CLW_REF_DOMAIN` and `CLW_ENDPOINT`; the `start()` method sets `CLW_TENANT`, `CLW_TOKEN`, `WORKSPACE_NAME`, `PROFILE_NAME` per WP-01:336-342).
- **Action:** Cross-WP — flag in the cross-WP coordination table; the WP-04 `computeUsageEvent` is dead code (now superseded by WP-07 §3.2).

### B19. **`idemKey` derivation: BLAKE3-256 of `${tenantId}|${containerId}|${startedAt}` collides on session restart**

- **Location:** §3.2 line 152 — `const idemKey = await blake3Hex(`${tenantId}|${containerId}|${startedAt}`);`
- **Problem:** If a DevEnv session is **restarted** (operator or scheduler kills and re-starts the container), the new session has the same `containerId` (it's the DO ID, persistent) and a NEW `startedAt`. The idem key is `(tenant, container, startedAt)`, so a restart produces a new key → a SECOND row in `usage_event_staging` for the same DO. That's actually correct (two sessions = two billable events).
- **But:** If `onStop` AND `onError` both fire for the same session (e.g., a `kill` after `onStop` initiated but before the DO finished transitioning to `stopped`), they BOTH have the same `(tenant, container, startedAt)` → same idem key → dedup wins. The DO captures locals BEFORE the transition (per B2 fix in iter 1) but the `startedAt` is the same, so dedup IS the right behavior. **However:** the comment says "capture in local before transition" for `workspaceNm`/`profileNm` — but `startedAt` is also already in the state BEFORE `onStop` runs, so the idem key is deterministic across the two firings. ✅ This is fine.
- **Real issue:** The `idemKey` derivation includes `${tenantId}` which is the CLW tenant string, not the canonical UUID. The ASK-2 ingest requires `tenant_id: Uuid` (validated via `Uuid::parse_str`). If `tenantId` is the CLW slug (e.g., `"acme-prod"`) the request 400s. WP-07 §3.2 line 158 declares `tenant_id: tenantId` where `tenantId = this.envVars.CLW_TENANT` — the comment says "validated as UUID by ASK-2" but the value is the CLW tenant string, NOT a UUID.
- **Why this is BLOCKING:** Every recordUsage call 400s. The cron / retry queue never advances.
- **Fix:** Use the **canonical tenant UUID** (looked up from `tenant.tenant_id` via the auth context, or from the DO's own `env.TENANT_ID_UUID` env var). The DO already authenticates with `CLW_TENANT` (slug) + `CLW_TOKEN`; the ingest path needs the canonical UUID. Either:
  - Inject `TENANT_ID_UUID` as a worker var per tenant (set at provisioning)
  - OR call `internal/v1/auth/introspect` with the slug to get the UUID, then cache in the DO
  - OR use a new env var `BILLING_TENANT_UUID` distinct from `CLW_TENANT`
- **Action:** Cross-WP with WP-08 (Worker ingress) — the Worker already authenticates the request and knows the canonical UUID; the DO could receive it via a new env var or the Worker could call `recordUsage` on behalf of the DO (preferred — see B20).

---

## 🟠 NEW HIGH SEVERITY ISSUES

### H12. **WP-07 places `recordUsage()` body in DO, but the canonical ingest endpoint rejects non-UUID `tenant_id` — DO knows only the CLW slug, not the UUID**

- **Location:** §3.2 + cross-WP WP-04 §3.2 `computeUsageEvent`
- **Problem:** Same as B19 root cause but framed differently: the DO has `CLW_TENANT` (a slug string, e.g. `"acme-prod"`), not the canonical UUIDv7. The ingest endpoint requires `Uuid::parse_str(&tenant_id)` (verified in `billing_ingest.rs:413`). The Worker ingress (WP-08) has the canonical UUID from the auth context — but the DO does not see it. WP-07 §3.2 sends the slug → 400.
- **Fix:** Two paths:
  - **Path A:** The DO is given a NEW env var `BILLING_TENANT_UUID` (canonical UUIDv7) at start() time, set by the Worker. The DO uses it for billing. CLW_TENANT stays as the slug for the clw binary's own auth.
  - **Path B (cleaner):** Don't have the DO call billing at all. The Worker ingress (which already authenticates and knows the UUID) records usage on `onStop`/`onError` via a separate signal. This removes the cross-WP seam entirely.
- **Recommendation:** Path A — minimal blast radius. Document in §3.2 that `BILLING_TENANT_UUID` is a Worker-injected env var on `start()`.

### H13. **BLAKE3 helper `src/utils/hash.ts` uses `crypto.subtle.digest("SHA-256", …)` placeholder — produces SHA-256 not BLAKE3, so `idemKey` is wrong**

- **Location:** §3.6 lines 402-412
- **Problem:** The `blake3Hex` function body has `crypto.subtle.digest("SHA-256", bytes)` with a comment "PLACEHOLDER". This produces a SHA-256 digest, NOT a BLAKE3-256 digest. The endpoint at the server side (`billing_ingest.rs`) does NOT validate that the `idem_key` is BLAKE3 specifically — it only checks length=64 and hex-only. So the placeholder works **mechanically** (digest is 64 hex chars). But:
  - The comment in §3.2 line 152 says "matches the canonical 64-hex form (matches `usage_event_staging.idem_key` CHECK constraint length(event_payload_hash)=64)" — true.
  - But the same comment says "Deterministic: the same (tenant, container, startedAt) reproduces the same key across retries" — true for SHA-256 too.
  - **The real BUG:** the comment "Use a stable BLAKE3 (not BLAKE2) to match the existing `corelink_hash::Digest::compute` Rust implementation" — if the Rust side generates the **server-side** `event_payload_hash` (in `billing_ingest.rs:228-245`, `Digest::compute(image.as_bytes()).to_hex()`) using BLAKE3, and the Worker sends a SHA-256 idem_key, the two halves of the system produce DIFFERENT 64-hex strings for the same record. The staging-table integrity is fine (idempotency is on `idem_key`, not on the hash), but any cross-validation between the two hashes (debug tooling, reconciliation) breaks.
  - The endpoint's `idempotency_key` is the `idem_key` (the Worker's input), and the `event_payload_hash` is server-computed from the record. They serve different purposes. So **the placeholder is OK in practice** — but the comment lying about "BLAKE3" is misleading.
- **Why HIGH not BLOCKING:** The system works correctly; the comment lies. But the WP claims to use BLAKE3 explicitly (I2, §3.6, §3.2 comments) — if an implementer reads the comments and assumes the placeholder needs to be BLAKE3 (because of "deterministic across workers" promise), they'll swap in BLAKE3, which is fine. If they leave the placeholder, the system works.
- **Fix:** Either (a) replace the placeholder with a real BLAKE3 (e.g., `blake3-wasm` or `@blaze/blake3`) and add a determinism test (same input → same output across calls), or (b) rename the function to `stableHashHex` and document the placeholder.

### H14. **`max_vcpu_h` enforcement uses unindexed column + full month scan — performance cliff at scale**

- **Location:** §3.3 lines 283-288 (B15's query)
- **Problem:** Even if `usage_daily` had a `vcpu_seconds` column, the query is `SUM(vcpu_seconds) WHERE tenant_id = ? AND day >= ?` — there's a PK on `(tenant_id, day)` and an index on the same, so it's a per-tenant month scan. For a tenant at 100% load (1 DevEnv, 30d × 4 vCPU × 86400s = ~10M vCPU-s/mo), the SUM is over ~30 rows → fine. But: a high-volume tenant with HUNDREDS of workspaces doing frequent snapshots would generate MANY staging events; if the aggregator groups by day, the rollup is still ~30 rows. Manageable but not ideal.
- **Why HIGH:** At ~10 req/s per DevEnv per tenant (the campaign's typical workload), with hundreds of tenants, the quota-check DB cost is the bottleneck of every DevEnv start. Migrate the vCPU-s tally to a hot-path-friendly surface (B15 path (b)).
- **Fix:** Coupled with B15 fix.

### H15. **Cross-WP regression: WP-06 §3.1.1 `containerFetch` requires exec-server on port 8080 — but WP-04 §3.2 uses `this.ctx.container.exec()` (DOES NOT EXIST in the SDK)**

- **Location:** Cross-WP between WP-04 and WP-06
- **Problem:** WP-04 §3.2 line 117: `const execProcess = await this.ctx.container.exec({...})` — this method **does not exist** on the `@cloudflare/containers` SDK. WP-06 §3.1.1 (lines 56-57) explicitly says: "The `@cloudflare/containers` SDK does NOT expose `ctx.container.exec()`; using bash `/dev/tcp/PORT` requires bash on Alpine (fragile)." WP-06's own `containerExec` uses `containerFetch("http://localhost:8080/clw", ...)` (line 110-130).
- **Why this matters for WP-07:** WP-04's `computeUsageEvent` (line 510) reads `this.state` and emits a usage event. The `clw` invocation pathway in WP-04 is broken (per the WP-06 cross-WP finding), but WP-07 doesn't care about that — it only cares that the `recordUsage` payload WP-04 feeds it has the right shape. **However**, if WP-04 is rejected by WP-06's review for the `container.exec` bug, WP-04 §3.2 changes; WP-07 §3.2's reference to "WP-04 §3.2 already defines it" becomes a moving target. WP-07 should pin to WP-06's contract, not WP-04's.
- **Fix:** Cross-WP coordination: WP-07 §3.2 should NOT depend on WP-04's `recordUsageForDevenv` (which is part of WP-04's broken exec path). The `recordUsage` body lives in WP-07 §3.2 (per iter 1 B1 fix: "WP-07 only updates WP-04's call site"). Confirm: WP-07 §3.2 IS the `recordUsage` body. The `onStop`/`onError` call sites are in WP-06 (line 356, 420). WP-07's call site is `await this.recordUsage()` (in `RunnerDevEnvDO` class). The `recordUsage` method on the class is implemented per WP-07 §3.2 (lines 121-185 of the WP). The implementation does NOT use `ctx.container.exec` — it uses `fetch()`. ✅ No regression from WP-06's finding.
- **Verdict:** Informational; no fix needed in WP-07. But flag the WP-04/WP-06 cross-WP issue for the next campaign review.

---

## 🟡 NEW MEDIUM SEVERITY ISSUES

### M9. **`vcpuSeconds` overflow: `Date.now() - startedAt` returns `number` (f64); at 100-year uptime, `wallSeconds * 4` could lose precision**

- **Location:** §3.2 line 139-140
- **Problem:** `(periodEndMs - startedAt) / 1000` produces a float. For a 10-year uptime: `10 * 365.25 * 86400 = 3.16e8` seconds → `* 4 = 1.26e9` vCPU-seconds. Within `Number.MAX_SAFE_INTEGER` (9e15), so no precision loss in practice. But the comment says "1 hour session = 3600s × 4 = 14,400 vCPU-seconds" — fine. Document the precision bound.
- **Fix:** Add comment: `vcpuSeconds` is `u64` on the wire (validated at the server side); the TS multiply is bounded by practical session duration (~1 year worst case = 1.26e8 s, `* 4 = 5e8` — well within safe-integer).

### M10. **No retry on transient ingest failure — `try/catch` swallows; the cron/queue isn't documented**

- **Location:** §3.2 line 181-184 — `catch (err) { this.log(...); }`
- **Problem:** The `try/catch` logs and continues. If the ingest endpoint 503s (fail-CLOSED on D1 backend fault per `billing_ingest.rs:528-533`), the usage event is **lost** — no retry, no queue, no durable buffer. Per iter 1 review's M7: "ASK-2 has MAX_BATCH_RECORDS=1024 cap; WP-07 has no body size cap, no per-tenant rate limit, no IP-based limit." The rate-limit issue is fixed by reuse (canonical has no per-tenant rate limit either, but it has MAX_BATCH_RECORDS=1024). The retry-queue issue is unresolved.
- **Fix:** Either (a) accept the loss risk and document it explicitly (the event is best-effort, log a Sentry alert on the catch), or (b) add a durable retry queue (R2-backed JSONL, drained by a cron) — this is its own WI, not WP-07 scope.
- **Recommendation:** Document the loss risk; track the durable retry queue as a follow-up WI.

### M11. **`recordUsage` reads `this.envVars.CLW_TENANT` but WP-01 §3.3 shows envVars are set as `Record<string, string>` on the instance, NOT as `STATIC_ENV_VARS`**

- **Location:** §3.2 line 135
- **Problem:** WP-01 §3.3 (lines 245-248) declares `STATIC_ENV_VARS` as `private static readonly` and the mutable tenant-scoped vars are set in `start()` as a local `newEnvVars` (lines 336-342), then passed to `super.start({ envVars: newEnvVars })`. The envVars are accessible on the Container instance as `this.envVars` (per `@cloudflare/containers` SDK convention). The `Record<string, string>` typing is implicit (not declared on the class). Reading `this.envVars.CLW_TENANT` at runtime works **only if** the Container parent class exposes the `envVars` field publicly. Verify by checking `@cloudflare/containers` SDK — but the WP-01 unit tests (e.g., line 717) construct the class and don't access `envVars`, so there's no test confirming the runtime access path.
- **Fix:** Add a typed accessor `private get tenantEnvVars(): { CLW_TENANT: string; ... }` that casts `this.envVars` to the known shape. Add a unit test that `recordUsage` can read `this.envVars.CLW_TENANT` after `start()`.

---

## 🔄 ITER 1 REGRESSIONS

| # | Regression | Where | Severity |
|---|------------|-------|----------|
| **R1** | Iter 1's "verified against migration 0089" claim for `vcpu_seconds` was wrong; column doesn't exist | §3.3 line 284 | B15 (BLOCKING) |
| **R2** | Iter 1's "uses canonical `usage_event_staging` (0017) for staging; rollup via the existing `usage_daily` (0089)" — but the rollup column doesn't exist in 0089 | §3.3 + §2 (Scope) | B15 (BLOCKING) |
| **R3** | Iter 1's "New `UsageEventKind::DevenvVcpuSeconds` variant" — duplicates the existing `RunnerVcpuSeconds` semantically; either new variant is rejected by canonical enum (400) or semantically collides | §3.1 | B16 (BLOCKING) |
| **R4** | Iter 1's "idempotency via canonical `usage_event_staging.PRIMARY KEY (tenant_id, request_id)`" — correct, but the `idempotency_key` derivation is `SHA-256` (placeholder), not BLAKE3 as claimed | §3.6 | H13 (HIGH) |
| **R5** | Iter 1's "validated as UUID by ASK-2" claim — true at the server side, but the DO only has the CLW slug (`CLW_TENANT`), not the UUID | §3.2 line 158 | B19 (BLOCKING) |
| **R6** | Iter 1's "Concurrency cap is `min(max_concurrency, 1)`" — accepted, but the rationale "1 DO per tenant" is documented as defense-in-depth, not enforced at the quota layer. A future refactor to per-session DOs would silently bypass the ceiling | §3.3 | Informational (now in §3.3 hard rules; tracked) |

**6 iter-1 fix points regressed or incomplete.** The 5 BLOCKING + 4 HIGH + 3 MEDIUM new issues are concentrated in areas iter 1 skim-verified: the `usage_daily` rollup surface, the canonical `UsageEventKind` enum membership, and the CLW slug vs canonical UUID boundary.

---

## 📊 UPDATED SCORECARD

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| Blocking Issues | 9 → 0 (iter 1) → 5 (iter 2 NEW) | **5 NEW** | ❌ Found |
| High Issues | 10 → 0 (iter 1) → 4 (iter 2 NEW) | **4 NEW** | ❌ Found |
| Medium Issues | 8 → 0 (iter 1) → 3 (iter 2 NEW) | **3 NEW** | ❌ Found |
| DoD Pass Rate | 0% (iter 1) → 100% (iter 1 verified) → still 100% if B15-B19 fixed | — | Depends on fix |
| Invariants Enforced | 25% (iter 1) → 100% (iter 1 verified) | — | — |
| Quality Standards | 17% (iter 1) → 100% (iter 1 verified) | — | — |
| Cross-WP regressions | 0 (iter 1) | **2** (H15) | ❌ Found |

**OVERALL VERDICT: ❌ FAIL — Iter 2 found 5 new BLOCKING + 4 new HIGH + 3 new MEDIUM. The B5 / B3 / B9 fixes are correct, but the canonical-schema verification was skim-level and missed the `usage_daily` column absence, the `UsageEventKind` enum membership, and the tenant UUID/slug boundary.**

---

## 🔧 FIX PRIORITY FOR ITER 2 → ITER 3

### Must Fix (Blockers)
1. **B15** — `usage_daily.vcpu_seconds` doesn't exist. Switch to a per-tenant `devenv_monthly_vcpu` side-table (UPSERT on each recordUsage; O(1) read at quota check) OR confirm aggregator output table + write that SELECT.
2. **B16** — `DevenvVcpuSeconds` is not in canonical `UsageEventKind`. Reuse `RunnerVcpuSeconds` and distinguish via `source` field; or trigger formal Finance+Compliance sign-off for the new variant.
3. **B17** — `UsageEventKind::RunnerVcpuSeconds → UsageUnit::OpCount` is wrong; vCPU-seconds need a `UsageUnit::VcpuSeconds` variant. Pre-requisite: file a S-10 follow-up; proceed with assumption the fix lands before WP-07 ship.
4. **B18** — WP-04's `computeUsageEvent()` references `this.state.clwTenant` which doesn't exist. Cross-WP flag; WP-07 must use `this.envVars.CLW_TENANT` (or better, a new `BILLING_TENANT_UUID` env var per H12).
5. **B19** — `tenant_id` sent to ingest is the CLW slug, not the canonical UUID. Add `BILLING_TENANT_UUID` env var; inject from Worker ingress on `start()`.

### Should Fix (High)
1. **H12** — Cross-WP: Worker must inject `BILLING_TENANT_UUID` on start; document the seam.
2. **H13** — Replace `SHA-256` placeholder in `blake3Hex` with a real BLAKE3 (or rename to `stableHashHex`).
3. **H14** — Coupled with B15: ensure the per-tenant side-table is on the hot path; cap the SUM-aggregation in aggregator cron.
4. **H15** — Informational: flag WP-04's `this.ctx.container.exec()` (does not exist) to WP-04 reviewers.

### Nice to Fix (Medium)
1. **M9** — Document `vcpuSeconds` precision bound.
2. **M10** — Document the fire-and-forget loss risk; track durable retry queue as a follow-up WI.
3. **M11** — Add typed `tenantEnvVars` accessor + unit test.

---

## 🤝 CROSS-WP COORDINATION NEEDS (I2 findings)

| WP | Issue | Action |
|----|-------|--------|
| **WP-01** | `DevenvState` union has no `clwTenant` field; WP-04 §3.2 `computeUsageEvent` casts `as { clwTenant: string }` → undefined at runtime. WP-07 §3.2 reads `this.envVars.CLW_TENANT` (correct path) | Inform WP-01 reviewers: envVars are accessible via `this.envVars` (Container instance), not `this.state`. WP-04's path is dead code; WP-07 path is correct |
| **WP-04** | §3.2 uses `this.ctx.container.exec()` (does not exist). Per WP-06 §3.1.1, the only path is `containerFetch` to in-container exec-server on port 8080 | Flag to WP-04 review: replace `this.ctx.container.exec()` with `containerFetch("http://localhost:8080/clw", ...)` |
| **WP-06** | §3.1 `recordUsage` is a stub that "calls the canonical billing DO (env.BILLING_DO)" — but WP-07 §3.2 implements `recordUsage` as a method on `RunnerDevEnvDO` directly (not a separate DO). Either (a) WP-07 owns the body, WP-06 just calls `await this.recordUsage()`, or (b) WP-06's "BILLING_DO" pattern is the right separation and WP-07 should be re-titled | Reconcile: WP-07's `recordUsage` body is a method on the same DO (no separate BILLING_DO). Update WP-06's doc-comment to drop the `BILLING_DO` reference |
| **WP-08** | Worker ingress authenticates with canonical UUID; DO has only CLW slug. WP-07 needs `BILLING_TENANT_UUID` injected at start() time | Add to WP-08 ingress: on `start()`, include `BILLING_TENANT_UUID` in the `envVars` set via `super.start({ envVars: {...} })` |
| **corelink-billing-emit** | Canonical `UsageEventKind` enum needs `RunnerVcpuSeconds` (or new `DevenvVcpuSeconds`) → `UsageUnit::VcpuSeconds` mapping (currently maps to `OpCount` — wrong unit at aggregator) | Open S-10 follow-up: add `UsageUnit::VcpuSeconds` variant + `canonical_unit()` mapping. Blocks WP-07 ship (B17) |
| **S-10 / billing-aggregator** | `usage_daily` (0089) is display-telemetry only (per its own docstring); cannot be the monthly vCPU-hour gate source. Need to find or add the canonical rollup table | Identify the aggregator's output table; either consume that (B15 path a) or add a per-tenant `devenv_monthly_vcpu` side-table (B15 path b) |

---

## 🎯 RECOMMENDED PATH FORWARD

**The single biggest fix is B15 + B16 + B17 + B19 (all cross-WP / canonical-schema boundary issues).** The fix order:

1. **B16 (cheap, no schema change):** Reuse `RunnerVcpuSeconds` and distinguish via `source` field. Update §3.1 to drop the new variant; rename `devnev_vcpu_seconds` to `runner_vcpu_seconds` with a different `source` prefix. One-block edit.
2. **B19 + H12 (Worker ingress change):** Add `BILLING_TENANT_UUID` to the `envVars` set in WP-01 §3.3 `start()`; document in WP-08. Then WP-07 §3.2 reads `this.envVars.BILLING_TENANT_UUID` instead of `CLW_TENANT`. Cross-WP PR.
3. **B15 (hot-path DB schema):** Add a per-tenant `devenv_monthly_vcpu` side-table. The DO UPSERTs on each `recordUsage`; the Worker SELECTs in `enforceDevenvQuota`. No conflict with `usage_daily` (display-telemetry). Migration number: 0094 (next free after 0093).
4. **B17 (S-10 sprint work):** File a follow-up issue / WI for `UsageUnit::VcpuSeconds`; do NOT block WP-07 on it — proceed with the unit-mapping fix landing as a pre-ship prerequisite.
5. **B18 (cross-WP cleanup):** Mark WP-04 §3.2 `computeUsageEvent` as deprecated; the body is now in WP-07 §3.2. Update WP-04 to remove the broken `computeUsageEvent` and the broken `this.ctx.container.exec` calls.
6. **H13 (BLAKE3 placeholder):** Replace with a real BLAKE3 WASM build. Add a determinism test.
7. **H14, M9-M11:** Apply alongside the B15/B19 fixes.

**Do NOT proceed to WP-08 implementation (depends on WP-07) until B15, B16, B19 are fixed.**

---

## NEXT STEPS

1. Apply B16, B18, B19, H12, H13, M9, M10, M11 to WP-07 directly (small edits).
2. Flag B15, B17, H14 as cross-WP / S-10 follow-ups (cannot be fixed in WP-07 alone).
3. Cross-WP flag H15 + WP-04 §3.2's `this.ctx.container.exec()` to WP-04 reviewers.
4. Do NOT merge WP-07 until B15, B16, B19, H12 are resolved (or explicitly waived with documented rationale).
5. Run iter 3 review after fixes applied.

---

**END OF WP-07 ITERATION 2 REVIEW**
