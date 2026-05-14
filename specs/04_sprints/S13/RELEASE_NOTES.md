---
id: "RELEASE-NOTES-S13"
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
tags: ["release-notes", "s13", "admin-plane", "config-singleton", "dual-approval", "secret-rotation", "terraform-drift", "progressive-rollout"]
---

# Release Notes — S-13: Admin Plane

> **Sprint:** S-13 | **Date:** 2026-05-14 | **Status:** CONDITIONALLY_APPROVED (PRR-S13)
> **Promotion gate:** Staging-stable. GA promotion (S-20) pending GA Evidence Gate D+45.

---

## What's New

### CAP-ADMIN-001 — DO Config-Singleton

- Durable Object `ConfigSingleton` per-region with CAS atomic update (`version, payload`).
- Typed config: feature flags (`Map<feature_id, {enabled, rollout_pct, allowlist_tenants}>`), rate limits, retention policies.
- Config propagation to all edge Workers ≤ 5s p99 via DO pub-sub change events.
- Config history retention 90d in D1 `config_change_log`; rollback API `POST /v1/admin/config/rollback?to_version=X`.
- New crates: `corelink-config-do`, `corelink-config-api`.
- D1 migration: `0024_config_change_log.sql`.
- ADR: `ADR-S13-001`.

### CAP-ADMIN-002 — Dual-Approval Workflow

- `PAT-DUAL-APPROVAL-001` enforced: admin op without 2 valid signatures = hard-fail 403.
- HMAC-SHA256 signing (constant-time compare via `subtle::ConstantTimeEq`).
- Collusion-rotation oracle (NIST AC-2(7)): rolling 3-op window, 3 distinct approvers required.
- MFA freshness window 30 min (CTRL-AUTH-010).
- Nonce single-use replay prevention.
- Audit event emitted per op (approved + denied paths).
- New crate: `corelink-dual-approval` (+ `corelink-admin-api` composition layer).
- ADR: `ADR-S13-002`.

### CAP-ADMIN-003 — Secret Rotation Automation

- Zero-downtime rotation for 5 asset types: TDK (7d overlap) / PAT signing key (24h) / audit chain key (24h) / admin signing key (24h) / BYOK (7d).
- `PAT-ROLL-FORWARD-001`: reads always from active key; writes to new key during overlap.
- `INV-KEY-OVERLAP` + `INV-KEY-NO-SKIP`: property test verified 10k iter per asset class.
- BYOK customer revocation triggers emergency rotation.
- New crates: `corelink-rotation-worker`, `corelink-rotation-adapters`.

### CAP-ADMIN-004 — Terraform Drift Detection Daily

- Daily GitHub Actions cron: `terraform plan --out=plan.bin`; pushes `DriftPlanEvent` to `corelink-terraform-drift-consumer`.
- `DefaultDriftClassifier`: `exit_code=2 + diff_count ≥ 3 → Medium (SEV-3)`, `diff_count ≥ 10 → High (SEV-2)`.
- D1 `terraform_drift_findings` table (migration `0025`).
- Production-grade `RB-FM-206` runbook with decision tree (6 sections, ~335 lines).
- New crate: `corelink-terraform-drift-consumer`.

### CAP-ADMIN-005 — Progressive Rollout Orchestrator

- 4-stage rollout: 1% → 10% → 50% → 100% with FSM enforcement.
- Auto-rollback on error budget burn > 30% within any stage window (≤ 10 min).
- Budget cap: cumulative rollback cost bounded; excess ops blocked.
- Cosign signature gate: unsigned artifacts cannot advance rollout.
- D1 `rollout_state_and_budget` table (migration `0026`).
- New crate: `corelink-rollout-controller`.

### CAP-ADMIN-006 — Admin Op Audit-Rich Event

- CloudEvent emission per admin op: `actor, MFA_ts, dual_approver, op_payload, prev_state_hash`.
- `serde_jcs` canonical JSON serialization.
- Append-only chain with hash-linking; daily integrity verifier.

### CAP-ADMIN-007 — Config Rollback API

- `POST /v1/admin/config/rollback?to_version=X` with dual-approval gate.
- CAS version-aware rollback; new version number advanced.
- Recovery target ≤ 5 min p99 (SLO-ADMIN-ROLLBACK-RECOVERY).

---

## Operational Readiness

- **RB-FM-205** (Admin Mistake, annual): dry-run PASS 2026-05-14; 5/5 validations green.
- **RB-FM-201** (Config Rate-Limit Drop, monthly): dry-run PASS 2026-05-14; 5/5 validations green.
- **RB-FM-206** (Terraform Drift, monthly): dry-run PASS 2026-05-14; 6/6 validations green.
- **Security walkthrough** (2h, 2026-05-14): 6 adversarial scenarios; P0=0, P1=0, P2=1 (deferred S-14+).

---

## New Invariants (5)

| Invariant | Level | CI Gate |
|---|---|---|
| INV-ADMIN-DUAL-APPROVAL | HIGH | `cargo test` + CI |
| INV-ADMIN-MFA-FRESHNESS | HIGH | `cargo test` + CI |
| INV-KEY-OVERLAP | HIGH | `cargo test` + CI |
| INV-KEY-NO-SKIP | HIGH | `cargo test` + CI |
| INV-AUDIT-APPEND-ONLY | CRITICAL | `cargo test` + CI (inherited S-09; reinforced S-13) |

---

## New SLOs (4)

| SLO ID | Target | Measurement |
|---|---|---|
| SLO-ADMIN-CONFIG-PROPAGATION | ≤ 5s p99 | 30d sustained staging (GA Evidence Gate D+45) |
| SLO-ADMIN-DUAL-APPROVAL-LATENCY | ≤ 50ms p99 | 30d sustained staging (GA Evidence Gate D+45) |
| SLO-ADMIN-ROLLBACK-RECOVERY | Config ≤ 5 min / deploy ≤ 10 min | 30d sustained staging (GA Evidence Gate D+45) |
| SLO-ADMIN-ROTATION-OVERLAP | Per asset class canonical | 30d sustained staging (GA Evidence Gate D+45) |

---

## Property Tests (21 properties)

| WI | Properties | Iterations |
|---|---|---|
| WI-S13-001 | 3 (CAS monotonic + schema drift + rollback) | 10,000 |
| WI-S13-002 | 7 (dual-approval + collusion + MFA + nonce) | 10,000 |
| WI-S13-003 | 7 (overlap per asset + no-skip + hard-upper) | 10,000 |
| WI-S13-005 | 4 (stage FSM + auto-rollback + budget cap + concurrent) | 10,000 |
| Cross-WI | 5 (composition stress + collusion cross-op) | 1,000 |

---

## Breaking Changes

None. S-13 introduces new admin plane crates; no existing API contracts changed.

---

## Migration Notes

- D1 migrations `0024`, `0025`, `0026` must run in order before first deploy.
- `wrangler.toml` must bind `ConfigSingleton` DO class + admin D1 binding.
- `ADMIN_SIGNING_KEY` secret must be set in CF Workers environment before dual-approval gate is live.

---

## Downstream Impact

- **S-14 unblocked:** BYOK + region partitioning requires S-13 admin plane (config-singleton + dual-approval + rotation).
- **S-16 unblocked:** Admin UI depends on S-13 admin API contracts.
- **S-19 unblocked:** SCIM/SAML enterprise SSO depends on S-13 admin role store.
- **S-20 GA:** GA Evidence Gate D+45 (30d sustained SLO measurement) gates GA promotion.

---

**S-13 sprint complete. 6/6 WIs SEALED. PRR-S13 CONDITIONALLY_APPROVED. S-14/S-16/S-19 development unblocked.**
