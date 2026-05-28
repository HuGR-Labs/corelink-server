---
id: "ASVS-CHECKLIST-S13"
type: "audit"
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
tags: ["checklist", "owasp-asvs", "ssdf", "nist", "compliance", "s13", "admin-plane", "wi-s13-006"]
references:
  - "specs/04_sprints/S13/_spec_contract.md"
  - "specs/04_sprints/S13/work_items/WI-S13-006-property-tests-mfa-freshness-rotation-overlap-rb-fm-205-prr.md"
---

# OWASP ASVS V4 + V5 + V6 + V7 + V14 + SSDF + NIST SP 800-53 + NIST SP 800-57 Checklist — S-13

> **Sprint:** S-13 Admin Plane | **Date:** 2026-05-14
> **Scope:** DO config-singleton + dual-approval + secret rotation + terraform drift + progressive rollout
> **Self-assessment by:** Gustavo Schneiter (Owner / Final Approver)

---

## Summary

| Framework | Items checked | Items passing | Notes |
|---|---|---|---|
| OWASP ASVS V4 (Authentication) | 8 | 8 | MFA freshness + admin step-up + dual-approval |
| OWASP ASVS V5 (Validation) | 5 | 5 | Config schema + payload validation |
| OWASP ASVS V6 (Cryptography) | 7 | 7 | Secret rotation + HMAC + constant-time compare |
| OWASP ASVS V7 (Error Handling + Logging) | 6 | 6 | Audit chain + structured logs |
| OWASP ASVS V14 (Configuration) | 6 | 6 | DO config + terraform drift + progressive rollout |
| SSDF (NIST SP 800-218) | 6 | 6 | Secure development practices |
| NIST SP 800-53 AC-2(1) / AC-2(7) / AC-6(1) / AU-2 | 8 | 8 | Access control + audit |
| NIST SP 800-57 Pt 1 Rev 5 §5.3 | 5 | 5 | Key rotation policy |
| **TOTAL** | **51** | **51** | **100% PASS** |

---

## 1. OWASP ASVS V4 — Authentication

| # | Item | Requirement | Implementation | Status |
|---|---|---|---|---|
| V4.1 | Admin step-up authentication required for destructive ops | All destructive ops require MFA re-authentication within 30 min window | `DualApprovalGate::verify()` checks `mfa_ts_ms` freshness; `CTRL-AUTH-010` enforced | ✅ PASS |
| V4.2 | MFA freshness window enforced | MFA timestamp validated; stale MFA ≥ 30 min rejected | `INV-ADMIN-MFA-FRESHNESS`; `prop_mfa_stale_rejected` 10k green | ✅ PASS |
| V4.3 | Admin role verification before privileged op | Only `admin_role` users can submit ops | `InMemoryAdminRoleStore::is_admin()` checked per request | ✅ PASS |
| V4.4 | Session nonce replay prevention | Nonces are single-use; replay rejected | `InMemoryNonceStore`; `prop_nonce_replay_rejected` 10k green | ✅ PASS |
| V4.5 | Clock skew tolerance ≤ 60s | Timestamp validation accepts ±60s from server clock | `ts_ms` within `[now - MAX_SKEW, now + MAX_SKEW]` per gate | ✅ PASS |
| V4.6 | Separation of duties enforced | Caller ≠ approver mandatory | `DualApprovalError::CallerEqualsApprover`; UUID equality check | ✅ PASS |
| V4.7 | Collusion detection (NIST AC-2(7)) | Rolling 3-op window enforces 3 distinct approvers | `CollusionStore` rolling window; `CollusionRotation` error | ✅ PASS |
| V4.8 | Admin op audit trail mandatory | All admin ops emit audit event before response | `AdminOpAuditSink::emit()` called unconditionally; INV-AUDIT-APPEND-ONLY | ✅ PASS |

---

## 2. OWASP ASVS V5 — Validation

| # | Item | Requirement | Implementation | Status |
|---|---|---|---|---|
| V5.1 | Config payload schema validation | All config updates validate schema version + field types | `ConfigPayload::validate()` + `serde(deny_unknown_fields)` | ✅ PASS |
| V5.2 | Rate limit config bounds validation | `refill_rate` + `burst` within allowed range | `RateLimitTunable` bounds checked; 0-rate rejected | ✅ PASS |
| V5.3 | Rollout percentage bounds | Stage percentages: 1, 10, 50, 100 only | FSM enum; arbitrary % rejected as `InvalidTransition` | ✅ PASS |
| V5.4 | Drift event payload validation | `DriftPlanEvent` fields validated; no injection via summary string | `plan_summary` treated as opaque string; no eval/exec | ✅ PASS |
| V5.5 | Admin op payload max size | Op payload bounded; no unbounded memory allocation | `op_payload: Vec<u8>` with max-size check | ✅ PASS |

---

## 3. OWASP ASVS V6 — Cryptography

| # | Item | Requirement | Implementation | Status |
|---|---|---|---|---|
| V6.1 | HMAC-SHA256 for admin op signing | Approved cryptographic algorithm for HMAC | `hmac::Hmac<sha2::Sha256>`; no MD5/SHA1 | ✅ PASS |
| V6.2 | Constant-time HMAC comparison | Side-channel resistance in signature verify | `subtle::ConstantTimeEq`; no early-exit comparison | ✅ PASS |
| V6.3 | Secret rotation overlap per asset class | Canonical overlap windows enforced | INV-KEY-OVERLAP: TDK 7d / PAT 24h / audit 24h / admin-signing 24h / BYOK 7d | ✅ PASS |
| V6.4 | No key reuse across rotation | Old key retired after overlap; new key used exclusively post-overlap | `KeyState::Retired` → decrypt rejects; `KeyState::Active` for new ops | ✅ PASS |
| V6.5 | Admin signing key not logged or serialized to audit | Admin signing key bytes never in audit events | `AdminSigningKey` implements no `Serialize`; audit sink receives computed HMAC only | ✅ PASS |
| V6.6 | BYOK customer key revocation path | Customer revoke triggers emergency rotation | `ByokAdapter::check_active()` → `KeyRevoked` → emergency rotation | ✅ PASS |
| V6.7 | Key material bounded in memory | Keys zeroized on drop | `AdminSigningKey` wraps `ZeroizeOnDrop` bytes | ✅ PASS |

---

## 4. OWASP ASVS V7 — Error Handling + Logging

| # | Item | Requirement | Implementation | Status |
|---|---|---|---|---|
| V7.1 | Admin op audit chain integrity | All audit events chained via hash; chain verifiable | `AuditChainProcessor`; INV-AUDIT-APPEND-ONLY; INV-OBS-AUDIT-CHAIN-INTEGRITY | ✅ PASS |
| V7.2 | Structured audit log format | Audit events in structured JSON (CloudEvents) | `AdminOpAuditEvent` CloudEvent envelope; `serde_jcs` canonical JSON | ✅ PASS |
| V7.3 | No secrets in audit logs | Admin signing key / TDK / PAT never in audit payload | `op_payload` is caller-controlled; redaction macros for sensitive fields | ✅ PASS |
| V7.4 | Error responses do not leak internal state | `DualApprovalError` variants are public-safe; no stack traces | `thiserror` derive; no internal path/state in error message | ✅ PASS |
| V7.5 | Admin op denied events always emitted | Rejections (403 path) emit audit event before returning error | Audit emit precedes error return in `verify()` | ✅ PASS |
| V7.6 | Audit chain daily verifier | Admin chain hash verified daily; break triggers SEV-2 | `AuditChainVerifier` cron; `admin.audit_chain.verify_failure` metric | ✅ PASS |

---

## 5. OWASP ASVS V14 — Configuration

| # | Item | Requirement | Implementation | Status |
|---|---|---|---|---|
| V14.1 | DO config-singleton per-region | Single-instance config with CAS atomic update | `corelink-config-do`; DO class `ConfigSingleton`; CAS `(version, payload)` | ✅ PASS |
| V14.2 | Config propagation ≤ 5s edge globally | Config change propagated to all Workers within SLO | DO pub-sub change events; `SLO-ADMIN-CONFIG-PROPAGATION ≤ 5s p99` | ✅ PASS |
| V14.3 | Config rollback available ≤ 5 min | Rollback to any version last 90d via API | `store.rollback_to()`; `SLO-ADMIN-ROLLBACK-RECOVERY ≤ 5 min` | ✅ PASS |
| V14.4 | Terraform drift detection daily | IaC vs runtime drift detected ≤ 24h | `corelink-terraform-drift-consumer`; daily GitHub Actions cron | ✅ PASS |
| V14.5 | Progressive rollout 4-stage with auto-rollback | Deployments go through 1%→10%→50%→100%; bad deploys auto-rollback | `corelink-rollout-controller`; error budget burn auto-rollback ≤ 10 min | ✅ PASS |
| V14.6 | No direct bypass of config admin API | All config changes require dual-approval + audit | No write path outside `ConfigSingletonStore::update()`; audit mandatory | ✅ PASS |

---

## 6. SSDF (NIST SP 800-218)

| # | Item | SSDF Practice | Implementation | Status |
|---|---|---|---|---|
| SSDF.1 | Cryptographic integrity of admin signing key | PS.1 (Protect All Forms of Code) | Admin signing key in `AdminSigningKey` zeroized struct; not in source | ✅ PASS |
| SSDF.2 | Property-based testing for security invariants | PW.7 (Review + Test for Vulnerabilities) | 21+ property tests × 10k iter; INV-ADMIN-DUAL-APPROVAL + INV-KEY-OVERLAP | ✅ PASS |
| SSDF.3 | Adversarial test suite | PW.7 | 37 adversarial scenarios across S-13 WIs; 100% mitigation rate | ✅ PASS |
| SSDF.4 | Security walkthrough for HIGH_RISK surface | RV.1 (Identify and Confirm Vulnerabilities) | 2h security walkthrough; P0=0; P1=0; P2=1 waived | ✅ PASS |
| SSDF.5 | Runbook dry-runs validate operational readiness | RV.3 (Analyze Root Causes) | RB-FM-205 + RB-FM-201 + RB-FM-206 dry-runs; all PASS | ✅ PASS |
| SSDF.6 | PRR ship gate documents residual risk | DS.4 (Archive and Protect Release Artifacts) | PRR-S13.md 11 sign-offs canonical; waivers with expiry; ADR-0034 | ✅ PASS |

---

## 7. NIST SP 800-53 — Access Control + Audit

| # | Control | Requirement | Implementation | Status |
|---|---|---|---|---|
| AC-2(1) | Automated system account management | Admin accounts managed via D1 role store; access review | `AdminRoleStore` CRUD; quarterly access review cadence documented | ✅ PASS |
| AC-2(7) | Collusion-rotation (privileged account management) | Privileged users rotated via collusion-rotation oracle | Rolling 3-op window; NIST AC-2(7); `CollusionStore`; `prop_collusion_a_b_3cycle_rejected` | ✅ PASS |
| AC-3 | Least privilege | Admin role minimally scoped; no blanket admin permissions | `is_admin()` check per op type; `AdminOpType` enum limits surface | ✅ PASS |
| AC-6(1) | Least privilege — separation of duties | Caller ≠ approver; 2-admin requirement for destructive ops | `CallerEqualsApprover` error; dual-approval mandatory | ✅ PASS |
| AU-2 | Event logging | All admin ops generate audit events | `AdminOpAuditSink::emit()`; CloudEvent format; D1 storage | ✅ PASS |
| AU-3 | Content of audit records | Audit events contain: actor_id, op_type, tenant_id, ts_ms, dual_approver, MFA_ts | `AdminOpAuditEvent` fields verified per WI-S13-002 spec | ✅ PASS |
| AU-9 | Protection of audit information | Audit chain append-only; hash-linked; daily integrity verify | INV-AUDIT-APPEND-ONLY; INV-OBS-AUDIT-CHAIN-INTEGRITY | ✅ PASS |
| CM-3 | Configuration change control | Config changes require dual-approval + CAS + audit + propagation | `ConfigSingletonStore` + `DualApprovalGate` composition | ✅ PASS |

---

## 8. NIST SP 800-57 Part 1 Rev 5 §5.3 — Key Rotation Policy

| # | Item | Requirement | Implementation | Status |
|---|---|---|---|---|
| KM-1 | Cryptoperiod limits enforced | Each key type has maximum cryptoperiod | TDK ≤ 90d; PAT signing ≤ 30d; audit chain key ≤ 30d; admin signing ≤ 30d; BYOK ≤ 1y | ✅ PASS |
| KM-2 | Overlap window during rotation | Old key remains valid during overlap to prevent decryption gaps | TDK 7d / PAT 24h / audit 24h / admin-signing 24h / BYOK 7d per INV-KEY-OVERLAP | ✅ PASS |
| KM-3 | No invalid-state transition during rotation | Writes never pass through state where neither old nor new key is valid | `INV-KEY-NO-SKIP`; `prop_key_no_skip` 10k green | ✅ PASS |
| KM-4 | Key retirement after overlap | Old key transitions to `Retired` state; decrypt rejects | `KeyState::Retired` FSM; `prop_hard_upper_30d` | ✅ PASS |
| KM-5 | Emergency rotation procedure available | Fast-path forced rotation on key compromise | `RotationWorker::force_complete()`; `RB-KEY-COMPROMISE` runbook | ✅ PASS |

---

**Checklist result: 51/51 items PASS (100%). S-13 Admin Plane OWASP ASVS V4+V5+V6+V7+V14 + SSDF + NIST SP 800-53 + NIST SP 800-57 compliance verified as of 2026-05-14.**
