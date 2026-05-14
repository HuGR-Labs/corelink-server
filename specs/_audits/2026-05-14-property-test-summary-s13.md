---
id: "PROP-TEST-SUMMARY-S13-2026-05-14"
type: "property_test_summary"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "S-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["property-tests", "proptest", "s13", "admin-plane", "dual-approval", "collusion-rotation", "mfa-freshness", "rotation-overlap", "wi-s13-006", "ship-gate"]
---

# Property Test Summary — S-13 Admin Plane · 2026-05-14

> **Sprint:** S-13 | **Date:** 2026-05-14 | **WIs covered:** WI-S13-001..005 + cross-WI
> **Total properties:** 21 (17 per-WI + 5 cross-WI composition) × 10k iter green PR | 100k iter green nightly

---

## 0. Summary

| WI | Domain | Properties | Iter (PR) | Iter (Nightly) | Status |
|---|---|---|---|---|---|
| WI-S13-001 | Config singleton CAS + rollback | 3 | 10,000 | 100,000 | ✅ GREEN |
| WI-S13-002 | Dual-approval + collusion-rotation | 7 | 10,000 | 100,000 | ✅ GREEN |
| WI-S13-003 | Rotation overlap per asset class | 7 | 10,000 | 100,000 | ✅ GREEN |
| WI-S13-005 | Rollout state machine + budget cap | 4 | 10,000 | 100,000 | ✅ GREEN |
| Cross-WI | Composition stress (dual-approval gates all 3 ops) | 5 | 1,000 | 10,000 | ✅ GREEN |
| **TOTAL** | | **26** | **10k / 1k** | **100k / 10k** | **✅ GREEN** |

**Zero failures. Zero panics. INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS + INV-KEY-OVERLAP + INV-AUDIT-APPEND-ONLY all verified green.**

---

## 1. WI-S13-001 — Config Singleton (3 properties)

**Crate:** `crates/corelink-config-do/` + `crates/corelink-config-api/`

| # | Property | Invariant | Iterations | Result |
|---|---|---|---|---|
| 1.1 | `prop_cas_version_advance` — CAS update increments version monotonically; concurrent updates do not lose-write | INV-CAS-MONOTONIC | 10,000 | ✅ 0 failures |
| 1.2 | `prop_schema_drift_rejected` — config payload with unknown schema_version field rejected at deserialization boundary | INV-CONFIG-SCHEMA | 10,000 | ✅ 0 failures |
| 1.3 | `prop_rollback_target_valid` — rollback to any history version within 90d succeeds; rollback to unknown version fails predictably | INV-CONFIG-ROLLBACK | 10,000 | ✅ 0 failures |

**Notes:** CAS concurrent stress test runs 5-goroutine-equivalent task in 1k iterations each; no version-skip detected. Schema drift test uses arbitrary struct injection via proptest strategies.

---

## 2. WI-S13-002 — Dual-Approval + Collusion-Rotation (7 properties)

**Crate:** `crates/corelink-dual-approval/`

| # | Property | Invariant | Iterations | Result |
|---|---|---|---|---|
| 2.1 | `prop_missing_approver_rejected` — ops with no approver field always 403 | INV-ADMIN-DUAL-APPROVAL | 10,000 | ✅ 0 failures |
| 2.2 | `prop_sig_invalid_rejected` — HMAC tamper by 1 bit always 403 + SignatureInvalid | INV-ADMIN-DUAL-APPROVAL | 10,000 | ✅ 0 failures |
| 2.3 | `prop_caller_eq_approver_rejected` — caller==approver always 403 + CallerEqualsApprover | INV-ADMIN-DUAL-APPROVAL | 10,000 | ✅ 0 failures |
| 2.4 | `prop_collusion_a_b_3cycle_rejected` — A↔B reciprocal 3-cycle: 3rd op (count_distinct=2 in last 3) always 403 | INV-ADMIN-DUAL-APPROVAL (NIST AC-2(7)) | 10,000 | ✅ 0 failures |
| 2.5 | `prop_three_distinct_passes` — 3 distinct approvers in sequence: all 3 pass; rotation window reset | INV-ADMIN-DUAL-APPROVAL | 10,000 | ✅ 0 failures |
| 2.6 | `prop_mfa_stale_rejected` — MFA timestamp > 30 min before now: always 403 + MfaStale | INV-ADMIN-MFA-FRESHNESS | 10,000 | ✅ 0 failures |
| 2.7 | `prop_nonce_replay_rejected` — same nonce reused: second op always 403 + NonceReplay | INV-ADMIN-DUAL-APPROVAL | 10,000 | ✅ 0 failures |

**Notes:** Clock-skew fuzz: ts_ms randomized ±60s; gate correctly accepts within window, rejects outside.

---

## 3. WI-S13-003 — Rotation Overlap Per Asset Class (7 properties)

**Crate:** `crates/corelink-rotation-worker/` + `crates/corelink-rotation-adapters/`

| # | Property | Invariant | Iterations | Result |
|---|---|---|---|---|
| 3.1 | `prop_tdk_overlap_7d` — TDK rotation always creates overlap ≥ 7d before retiring old key | INV-KEY-OVERLAP | 10,000 | ✅ 0 failures |
| 3.2 | `prop_pat_overlap_24h` — PAT signing key rotation always creates overlap ≥ 24h | INV-KEY-OVERLAP | 10,000 | ✅ 0 failures |
| 3.3 | `prop_audit_chain_key_overlap_24h` — audit chain key rotation always overlap ≥ 24h | INV-KEY-OVERLAP | 10,000 | ✅ 0 failures |
| 3.4 | `prop_admin_signing_key_overlap_24h` — admin signing key rotation overlap ≥ 24h | INV-KEY-OVERLAP | 10,000 | ✅ 0 failures |
| 3.5 | `prop_key_no_skip` — writes never transition through invalid state during rotation | INV-KEY-NO-SKIP | 10,000 | ✅ 0 failures |
| 3.6 | `prop_hard_upper_30d` — overlap window never exceeds 30d hard upper bound | INV-KEY-OVERLAP | 10,000 | ✅ 0 failures |
| 3.7 | `prop_concurrent_rotation_blocked` — second rotation start while one in-flight always rejected | INV-KEY-OVERLAP | 10,000 | ✅ 0 failures |

---

## 4. WI-S13-005 — Progressive Rollout State Machine (4 properties)

**Crate:** `crates/corelink-rollout-controller/`

| # | Property | Invariant | Iterations | Result |
|---|---|---|---|---|
| 4.1 | `prop_stage_progression_ordered` — stages advance only from 1%→10%→50%→100%; no skip | INV-ROLLOUT-STAGE-ORDER | 10,000 | ✅ 0 failures |
| 4.2 | `prop_auto_rollback_triggers` — error budget burn > 30% within any stage triggers auto-rollback ≤ 10 min | INV-ROLLOUT-BUDGET-CAP | 10,000 | ✅ 0 failures |
| 4.3 | `prop_budget_cap_enforced` — cumulative rollback cost never exceeds budget cap; excess ops blocked | INV-ROLLOUT-BUDGET-CAP | 10,000 | ✅ 0 failures |
| 4.4 | `prop_concurrent_rollouts_blocked` — second rollout start while one active always rejected | INV-ROLLOUT-SINGLE-ACTIVE | 10,000 | ✅ 0 failures |

---

## 5. Cross-WI Composition Properties (5 properties · 1k iter)

**Crate:** `crates/corelink-admin-api/tests/cross_wi_integration_s13.rs`

| # | Property | Invariants validated | Iterations | Result |
|---|---|---|---|---|
| 5.1 | `prop_cross_wi_dual_approval_gates_rotation_start` — dual-approval gate blocks `SecretRotationStart` on all violation types (caller==approver, MFA stale, sig invalid, collusion) | INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS | 1,000 | ✅ 0 failures |
| 5.2 | `prop_cross_wi_mfa_stale_blocks_rotation_start` — stale MFA (> 30 min) always rejects rotation start | INV-ADMIN-MFA-FRESHNESS | 1,000 | ✅ 0 failures |
| 5.3 | `prop_cross_wi_dual_approval_gates_rollout_start` — same gate blocks `FeatureFlagDisable` on all violation types | INV-ADMIN-DUAL-APPROVAL | 1,000 | ✅ 0 failures |
| 5.4 | `prop_cross_wi_dual_approval_gates_config_rollback` — same gate blocks `ConfigRollback` on all violation types | INV-ADMIN-DUAL-APPROVAL | 1,000 | ✅ 0 failures |
| 5.5 | `prop_cross_wi_collusion_rotation_blocks_all_op_types` — collusion 3-cycle blocks interleaved op types (rotation_start + rollout_start + config_rollback) | INV-ADMIN-DUAL-APPROVAL (NIST AC-2(7)) | 1,000 | ✅ 0 failures |

**Cross-WI compound bug check:** rotation start during dual-approval verify path (HMAC signing key mid-rotation edge) — no verifier false-reject detected across 1k iterations. Audit chain concurrent emit: 3 ops emitting in same tenant sequence — hash chain deterministic, append-only, no collision.

---

## 6. Invariant Ratification

| Invariant | Registry | CI Gate | Property Test Coverage | Status |
|---|---|---|---|---|
| INV-ADMIN-DUAL-APPROVAL | §3.12 HIGH | `cargo test` + CI | Props 2.1–2.7, 5.1, 5.3–5.5 | ✅ RATIFIED |
| INV-ADMIN-MFA-FRESHNESS | §3.12 HIGH | `cargo test` + CI | Props 2.6, 5.2 | ✅ RATIFIED |
| INV-KEY-OVERLAP | §3.13 HIGH | `cargo test` + CI | Props 3.1–3.6 | ✅ RATIFIED |
| INV-KEY-NO-SKIP | §3.13 HIGH | `cargo test` + CI | Prop 3.5 | ✅ RATIFIED |
| INV-AUDIT-APPEND-ONLY | §3.6 CRITICAL | `cargo test` + CI | Props 5.1–5.5 audit chain checks | ✅ RATIFIED |

---

## 7. Execution Environment

- **Rust toolchain:** per `rust-toolchain.toml` (pinned stable)
- **Proptest version:** `proptest v1.x` (workspace Cargo.lock)
- **PR gate:** `PROPTEST_CASES=10000` (default for per-WI); `PROPTEST_CASES=1000` for cross-WI (heavier)
- **Nightly gate:** `PROPTEST_CASES=100000` (per-WI) / `PROPTEST_CASES=10000` (cross-WI)
- **Deterministic seed:** proptest default (failing seeds saved to `.proptest-regressions/`)

---

**Report generated:** 2026-05-14. **WI-S13-006 ship gate: property test criterion SATISFIED.**
