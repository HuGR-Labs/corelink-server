# rt-nuclear cycle-2 — triage & fix-wave (2026-06-18, autonomous run)

Workflow `corelink-redteam-nuclear` (run `wf_b6ea5150-cee`): 414 agents, 4 rounds, 18 trust
boundaries, 55 candidates → **12 confirmed exploitable** (7 high, 5 medium) after 5-skeptic
refutation (≥3/5 to survive). ⚠️ **Rounds 3-4 were heavily API-rate-limited** (≈60 hunt
agents failed on throttle) — deep second-order/composed-kill-chain coverage is PARTIAL; a
re-run when throttle clears is warranted for completeness.

TL cold-verifies each against code (AP-5 — never trust the agent self-report) before fixing.
Status legend: ✅ fixed+verified · 🔧 fixing · 📋 designed (owner-aware / pending build) · ❓ cold-verify pending.

| # | Sev | Reach | TL verdict | Status |
|---|---|---|---|---|
| 7 | high | authed | **REAL** — `handle_keys_revoke`/`_list` (customer.rs) lack the `requires_cache_write` gate that `handle_keys_create` has → a `cas:r` token revokes/enumerates ANY tenant credential | ✅ gate added to both |
| 2 | high | free-tenant | **REAL (cold-verified)** — OCI `OciMoatStore::finalize_upload` persisted bytes BEFORE the caller's `verify_against_bytes`; mismatch left a poisoned slot (no rollback port) → intra-tenant digest-lie | ✅ verify moved INTO the store before `moat.put` (reuses `OciDigest::verify_against_bytes`; fail-closed, no rollback needed) + regression test |
| 3 | high | free-tenant | **REAL (cold-verified)** — native CAS/AC (cas.rs:552, ac.rs:516) charge the `$`-ceiling on reads UNCONDITIONALLY; OCI (oci.rs:789) wrapped it in `if is_write` → OCI reads bypass it | ✅ write-only wrapper removed |
| 1 | high | free-tenant | **REAL (amplifier confirmed)** — no per-tenant PAT cap (customer_d1 create has no COUNT) + shared 16-permit Argon2id pool not per-tenant-fair → 200 distinct PATs flood → cross-tenant native 503 | 🔧 PAT cap + per-tenant Argon2 sub-limit |
| 4 | high | free-tenant | **REAL (cold-verified)** — `handle_events` (turbo_v8.rs:866) shared `GlobalPutBudgetGuard` with PUT (a deliberate C5 OOM choice) → events flood/slow-body starved real writes | ✅ events decoupled to its OWN `EventsBudgetGuard` budget (cross-plane starvation fixed; OOM bound preserved) |
| 5 | high | free-tenant | nuclear-confirmed (5/5) — 8 accounts pin the 512 MiB global OCI in-flight ceiling (per-tenant slice too large vs global) | 📋 reserve global headroom / smaller per-tenant slice |
| 6 | high | authed | nuclear-confirmed (4/5) — adapter writes (cargo/npm/brew/pip) don't thread the per-tier cap header → #297 seeding gap on the adapter plane | 📋 thread cap into adapter CasWriteRequest |
| 8 | med | free-tenant | partially addressed by #4 — slow-body `/events` now holds only the SEPARATE events budget (can't touch PUT); the residual (a slow-body holding an *events* permit) needs a server body-read timeout layer | 🔧 #4 isolates it; body-timeout = follow-up |
| 9 | med | free-tenant | nuclear-confirmed (4/5) — `/events` has no per-tenant concurrency cap (the PUT sibling does) | 📋 per-tenant `/events` guard |
| 10 | med | authed | **cold-verify NARROWS this** — `checkStorageQuota` (quota.ts:344) SUMs actual `bytes_used` vs the RESOLVED (downgraded) cap on every PAT-gated write → native + cargo/npm/brew/pip are MITIGATED; residual is **OCI-only** (OCI bypasses the Worker gate, so a *downgraded* tenant over-stores via OCI until a native write reseeds) | 📋 OCI-only: thread the resolved cap into the container OCI write path (or run a Worker SUM gate for OCI) |
| 11 | med | authed | nuclear-confirmed (3/5) — multi-region fanout double-counts the monthly request quota (OVER-count — conservative, customer-unfavorable, not a bypass) | 📋 gate quota block on `!isFanout` (worker) |
| 12 | med | leaked-secret | nuclear-confirmed (5/5) — leaked `PAT_SIGNING_KEY` → valid-HMAC nonexistent-tenant PATs force Argon2id work → adapter-pool starvation (depends on a leaked secret) | 📋 isolate OCI verify pool + per-tenant Argon2 cap (overlaps #1) |

## Fix-wave plan (themed minimal PRs)
- **PR A — customer-plane authz (#7):** ✅ `requires_cache_write` gate on revoke+list. + regression test. (done, pending cargo-verify)
- **PR B — billing (#3):** remove the `if is_write` wrapper on the OCI `$`-ceiling gate (oci.rs ~789) so reads are charged like the native plane. Small, clear.
- **PR C — DoS shared-pool fairness (#1/#4/#5/#8/#9/#12):** per-tenant PAT mint cap (customer_d1) + per-tenant Argon2id sub-limit (adapter_pat) + decouple Turbo `/events` from the PUT budget + body/concurrency caps on `/events` + reserve OCI global headroom + isolate the OCI verify pool. Cohesive but the largest; do in careful sub-steps.
- **PR D — byte-accounting adapter plane (#6/#10):** thread the server-trusted per-tier cap into the adapter (cargo/npm/brew/pip/OCI) `CasWriteRequest` so seeding + downgrade-reconcile run (closes the #297 gap on the adapter plane).
- **PR E — OCI cache-poisoning (#2):** verify-before-commit (split assemble/commit in the BlobStore port, or add `delete_blob` rollback). Careful port change.
- **#11 (multi-region over-count):** worker `index.ts` — gate the quota block on `!isFanout`. Lowest priority (over-count, not a bypass).

## TL cold-verify nuance on the REMAINING findings (why fixed-clean vs owner-aware)
Fixed this run were the clean, clear-cut, low-blast-radius "missing-check" bugs (#7 missing scope
gate, #3 missing read-charge, #4 wrong shared budget). Cold-verifying the rest revealed they are
**not** clean bug-fixes — each rebalances a *deliberate* design or hot-path, so each is an
owner-aware decision, not a madrugada auto-fix:
- **#1 / #12 (Argon2id per-tenant fairness):** the real fix (two-tier global+per-tenant semaphore in
  `adapter_pat.rs`) sits on the **PAT-verification hot path that runs on EVERY native/adapter request**
  — highest blast radius (a bug breaks all auth). Needs careful concurrency design + testing, not a rush.
  (#1's PAT-mint-cap amplifier is a clean complementary add: `SELECT COUNT(*)` cap in `customer_d1` create.)
- **#5 (OCI in-flight ceiling):** the per-tenant cap EXISTS (`global/8` = 64 MiB, deliberate); the fix
  is a **tuning trade-off** (smaller slice resists the 8-account saturation but throttles legit large
  container pushes) — needs real OCI push-size data to pick the value.
- **#6 / #10 (adapter byte-accounting):** the adapter NOT seeding the cap is a **deliberate fail-closed
  posture** (`adapter_cache.rs:260-273` — "absence of a cap is never treated as unlimited"; a fresh
  tenant is seeded on its first native write). #6 is an availability/UX limitation (adapter-only tenant
  must do one native write first), NOT a quota-evasion (a fresh tenant's adapter write fails CLOSED — it
  cannot over-store). **#10 — cold-verified + NARROWED:** `checkStorageQuota` (quota.ts:306/344) SUMs
  actual `bytes_used` vs the RESOLVED (downgraded) cap on EVERY PAT-gated write, so the downgrade IS
  enforced on the native + cargo/npm/brew/pip planes regardless of the container's stale row. The ONLY
  residual is **OCI** (its pass-through bypasses the Worker quota gate), where a *downgraded* tenant
  could over-store until a native write reseeds the container row. Fix scope is therefore OCI-only:
  thread the resolved cap into the container OCI write path, or add a Worker-side SUM gate for OCI.
- **#2 (OCI digest-lie poisoning):** REAL (verify-after-persist, no rollback port) but intra-tenant; the
  correct fix (verify-BEFORE-commit) is a **BlobStore port split** (assemble vs commit) across prod+fake — a careful multi-file port change.
- **#8 / #9:** #4 already isolated events from writes; residual = a server **body-read timeout** layer
  (#8) + a per-tenant `/events` counter (#9) — events-pool-only impact now (low).
- **#11:** an OVER-count (double-charges multi-region tenants — customer-*unfavorable*, not a bypass);
  worker `index.ts` `!isFanout` gate. Lowest priority; fixing it stops over-charging customers.

**Recommendation:** greenlight a focused, CI-healthy fix-wave for #1/#12 (auth hot-path — highest value,
needs care) and #2 (poisoning), with workload input for #5; #6/#10 need the Worker-gate behavior
confirmed first; #11 is a quick customer-favorable cleanup.

## Constraint note
Self-hosted Linux CI is DOWN + the Mac cancels long jobs (see `ci-infra-state` memory) → these
Rust fixes are LOCAL-verified (`cargo check`/targeted tests) + `--admin` merged with the documented
infra reason. Fixes that can't be safely completed+verified tonight are left as precise 📋 designs
above for an owner-aware follow-up — never merged unverified (rigor compact).
