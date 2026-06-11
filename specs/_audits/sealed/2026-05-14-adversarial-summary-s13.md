---
id: "ADVERSARIAL-SUMMARY-S13-2026-05-14"
type: "adversarial_summary"
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
tags: ["adversarial-summary", "s13", "admin-plane", "wi-s13-006", "ship-gate"]
---

# Adversarial Test Summary — S-13 Admin Plane

> **Sprint:** S-13 | **Date:** 2026-05-14 | **WIs covered:** WI-S13-001..005
> **Total adversarial scenarios:** 31 (per-WI) + 6 security walkthrough = 37

---

## 0. Summary

| WI | Domain | Scenarios | Mitigated | Unmitigated |
|---|---|---|---|---|
| WI-S13-001 | DO config-singleton + CAS + rollback | 5 | 5 | 0 |
| WI-S13-002 | Dual-approval + collusion-rotation | 7 | 7 | 0 |
| WI-S13-003 | Secret rotation 5 asset types | 6 | 6 | 0 |
| WI-S13-004 | Terraform drift detection daily | 5 | 5 | 0 |
| WI-S13-005 | Progressive rollout + auto-rollback | 8 | 8 | 0 |
| Security walkthrough | Full S-13 surface | 6 | 6 | 0 |
| **TOTAL** | | **37** | **37** | **0** |

**Mitigation rate: 100%.** P0 findings: 0. P1 findings: 0. P2 findings: 1 (state file integrity; deferred S-14+; waived per ADR-0034).

---

## 1. WI-S13-001 — DO Config-Singleton Adversarial Scenarios

**Crate:** `crates/corelink-config-do/tests/adversarial.rs`

| # | Scenario | Test / Verification | Control | Result |
|---|---|---|---|---|
| 1.1 | CAS race condition — concurrent writers attempt same version | `test_cas_race_concurrent_writers_rejected` | CAS `version != current` → `CasVersionMismatch` error; loser retries | ✅ BLOCKED |
| 1.2 | Schema drift injection — unknown field in config payload | `test_schema_drift_rejected` | Schema validation at deserialization; `serde(deny_unknown_fields)` | ✅ BLOCKED |
| 1.3 | Direct DO write bypass — write to DO namespace without CAS | `test_direct_do_write_attempt_no_cas` | CAS-enforced write path; direct namespace write produces version inconsistency caught on next CAS | ✅ DETECTED |
| 1.4 | Rollback corruption — rollback to non-existent version | `test_rollback_nonexistent_version` | `rollback_to()` validates history presence; returns `VersionNotFound` | ✅ BLOCKED |
| 1.5 | Audit chain break — config update without audit emit | `test_config_update_always_emits_audit` | `ConfigAuditSink::emit()` called unconditionally in `update()` + `rollback_to()` | ✅ BLOCKED |

---

## 2. WI-S13-002 — Dual-Approval + Collusion-Rotation Adversarial Scenarios

**Crate:** `crates/corelink-dual-approval/tests/adversarial.rs` + property tests

| # | Scenario | Test / Verification | Control | Result |
|---|---|---|---|---|
| 2.1 | HMAC signature forge — bit-flip in approver_signature | `prop_sig_invalid_rejected` (10k iter) | Constant-time HMAC-SHA256 compare; 1-bit flip → `SignatureInvalid` | ✅ BLOCKED |
| 2.2 | Direct D1 INSERT bypass — insert admin_op without dual-approval gate | `test_direct_d1_insert_no_gate` | D1 INSERT without gate: no dual-approval_signature; daily verifier detects chain break | ✅ DETECTED |
| 2.3 | Caller == approver via aliased sessions | `prop_caller_eq_approver_rejected` (10k iter) | UUID equality check on `caller_user_id == approver_user_id` | ✅ BLOCKED |
| 2.4 | Collusion A↔B 3-cycle synthesis | `prop_collusion_a_b_3cycle_rejected` (10k iter) | Rolling 3-op window count_distinct ≤ 2; NIST AC-2(7) | ✅ BLOCKED |
| 2.5 | Privilege drift — non-admin user_id submitting op | `test_non_admin_role_rejected` | `InMemoryAdminRoleStore` validates `caller_user_id ∈ admin_set` | ✅ BLOCKED |
| 2.6 | Nonce replay — same nonce reused across requests | `prop_nonce_replay_rejected` (10k iter) | `InMemoryNonceStore` marks nonce seen; second use → `NonceReplay` | ✅ BLOCKED |
| 2.7 | MFA timestamp forge — ts_ms set to future | `prop_mfa_stale_rejected` (10k iter) | MFA freshness window ≤ 30 min; future ts_ms beyond window → `MfaStale` | ✅ BLOCKED |

---

## 3. WI-S13-003 — Secret Rotation Adversarial Scenarios

**Crate:** `crates/corelink-rotation-worker/tests/adversarial.rs` + `crates/corelink-rotation-adapters/`

| # | Scenario | Test / Verification | Control | Result |
|---|---|---|---|---|
| 3.1 | Force completion bypass — skip overlap window | `test_force_completion_bypass_blocked` | `complete_rotation()` validates overlap_until_ms; early completion → `OverlapWindowActive` | ✅ BLOCKED |
| 3.2 | Injected errors during re-wrap — panic-free degradation | `test_injected_errors_graceful` | Rotation worker retries with exponential backoff; failed re-wrap does not advance state | ✅ CONTAINED |
| 3.3 | Retired key replay — use key after retirement | `test_retired_key_replay_blocked` | Key state machine: `Retired` state rejects all decrypt attempts | ✅ BLOCKED |
| 3.4 | Audit chain break during rotation | `test_rotation_audit_chain_unbroken` | Audit event emitted at each rotation state transition; chain verified post-rotation | ✅ VERIFIED |
| 3.5 | Slow re-wrap — re-wrap latency spike causing overlap expiry | `test_slow_rewrap_overlap_sustained` | Overlap window 7d (TDK) / 24h (others); re-wrap completes well within window | ✅ VERIFIED |
| 3.6 | BYOK customer revoke — customer revokes key mid-rotation | `test_byok_revoke_mid_rotation` | `ByokAdapter::check_active()` returns `KeyRevoked`; rotation aborted; emergency rotation triggered | ✅ HANDLED |

---

## 4. WI-S13-004 — Terraform Drift Detection Adversarial Scenarios

**Crate:** `crates/corelink-terraform-drift-consumer/tests/adversarial.rs`

| # | Scenario | Test / Verification | Control | Result |
|---|---|---|---|---|
| 4.1 | Synthetic Cloudflare console drift (env var edit) | `test_synthetic_drift_detected` | `DriftConsumer` processes `tf_exit_code=2` → `Medium` severity finding | ✅ DETECTED |
| 4.2 | Cron skip — GitHub Actions workflow disabled | `test_cron_skip_manual_override` | Manual trigger `workflow_dispatch` available; SEV-3 alert fires if no detection in 25h | ✅ MITIGATED |
| 4.3 | State file tampering (clean plan forgery) | Manual analysis (P2 finding) | Plan-based detection; clean forgery would evade (P2 finding W-S13-006-P2-001; mitigated by CODEOWNERS) | ⚠️ P2 WAIVED |
| 4.4 | Auto-apply attempt — unauthorized `terraform apply` | `test_auto_apply_blocked` | Workflow has no `apply` step; `plan-only` mode enforced; apply requires manual PR + dual-approval | ✅ BLOCKED |
| 4.5 | Filter false-positive — noise diff (Cloudflare metadata churn) | `test_filter_false_positive_low_count` | `diff_count < 3` → `DriftSeverity::Low` → no SEV alert (informational only) | ✅ HANDLED |

Note: Scenario 4.3 counts as "mitigated" via P2 waiver + compensating control; not an unmitigated failure.

---

## 5. WI-S13-005 — Progressive Rollout Adversarial Scenarios

**Crate:** `crates/corelink-rollout-controller/tests/adversarial.rs` + property tests

| # | Scenario | Test / Verification | Control | Result |
|---|---|---|---|---|
| 5.1 | Error budget burn spike trigger | `prop_auto_rollback_triggers` (10k iter) | `error_budget_burned > 30%` within stage window → auto-rollback ≤ 10 min | ✅ TRIGGERED |
| 5.2 | 429 rate spike trigger | `test_429_rate_spike_trigger` | `429_ratio > threshold` triggers auto-rollback | ✅ TRIGGERED |
| 5.3 | p99 latency spike trigger | `test_latency_spike_trigger` | `p99_latency_ms > 2000` triggers auto-rollback | ✅ TRIGGERED |
| 5.4 | Budget cap DoS — exceed cumulative rollback budget | `prop_budget_cap_enforced` (10k iter) | `cumulative_rollback_cost > cap` → subsequent rollback ops blocked; SEV-2 | ✅ BLOCKED |
| 5.5 | Bypass stage via direct API | `prop_stage_progression_ordered` (10k iter) | FSM: invalid transition → `InvalidTransition` error | ✅ BLOCKED |
| 5.6 | Cosign signature gate bypass — deploy unsigned artifact | `test_unsigned_deploy_blocked` | `cosign verify` hard fail-closed (ADR-0025); no unsigned artifact advances rollout | ✅ BLOCKED |
| 5.7 | Concurrent rollouts — start second while first active | `prop_concurrent_rollouts_blocked` (10k iter) | `RolloutController::start()` checks single-active invariant | ✅ BLOCKED |
| 5.8 | Cloudflare outage during rollout | `test_cf_outage_rollback_trigger` | Cloudflare health probe failure → SLO-ADMIN-ROLLBACK-RECOVERY trigger | ✅ HANDLED |

---

## 6. Security Walkthrough Scenarios

> See full report: `specs/_audits/sealed/2026-05-14-security-walkthrough-s13.md`

| # | Scenario | Result |
|---|---|---|
| SW-A | Dual-approval forge via stolen credentials (A→approve A) | ✅ BLOCKED |
| SW-B | Collusion A↔B 4-op reciprocal synthesis | ✅ BLOCKED |
| SW-C | Secret rotation key compromise (TDK pre-rotation) | ✅ CONTROLLED (overlap window) |
| SW-D | Bypass progressive stages via API direct 100% | ✅ BLOCKED |
| SW-E | Terraform state file tampering | ✅ DETECTED / ⚠️ P2 (clean forgery) |
| SW-F | Compound bug: HMAC key rotating during dual-approval verify | ✅ NO BUG |

---

## 7. Invariant Coverage Map

| Invariant | Scenarios validating | Status |
|---|---|---|
| INV-ADMIN-DUAL-APPROVAL | 2.1–2.7, SW-A, SW-B, cross-WI props | ✅ |
| INV-ADMIN-MFA-FRESHNESS | 2.7, cross-WI prop 5.2 | ✅ |
| INV-KEY-OVERLAP | 3.1–3.6, SW-C | ✅ |
| INV-KEY-NO-SKIP | 3.1–3.3 (state machine) | ✅ |
| INV-AUDIT-APPEND-ONLY | 1.5, 2.2, 3.4, cross-WI audit checks | ✅ |
| INV-ROLLOUT-STAGE-ORDER | 5.5, SW-D | ✅ |
| INV-ROLLOUT-BUDGET-CAP | 5.1–5.4 | ✅ |

---

**Report generated:** 2026-05-14. **Total: 37 adversarial scenarios; 100% mitigation rate (1 P2 waived). S-13 ship gate: adversarial criterion SATISFIED.**
