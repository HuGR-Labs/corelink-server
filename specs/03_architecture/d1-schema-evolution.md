---
id: "d1-schema-evolution"
type: "architecture"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-14"
updated: "2026-05-27"
owner: "SRE Lead"
final_approver: "Engineering Director"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "d1", "migrations", "schema", "r2-13"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-d1-migrations` was absorbed into `corelink-ops` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md. Canonical consumer path is now `corelink_ops::*`.

# D1 schema evolution registry

> **Status:** ACTIVE. Per-migration source of truth for the
> Cloudflare D1 (SQLite) schema. Updated as part of every PR that
> lands a new file under `migrations/d1/`.

## 1. Overview

CoreLink uses Cloudflare D1 as the durable metadata store for CAS
(`blob_meta`), Action Cache (`ac_meta`), audit outbox, quota state,
billing idempotency, and the wide tail of S-08…S-20 governance /
incident-response tables.

All migrations are **additive-only** per INV-AUTH-MIGRATION-ADDITIVE
(HIGH). DROP / RENAME / ALTER-TYPE statements are rejected at PR time
by `scripts/check_migrations_additive.py`.

This document is the per-migration registry — one row per file under
`migrations/d1/`. The numbering has two natural gaps (`0004`, `0005`)
inherited from S-01 / S-04 planning churn; downstream consumers
should not rely on dense numbering.

## 2. Apply runtime

- Canonical apply: `scripts/d1-migration-runner.sh --env {dev,staging,prod}`
  (R2-13).
- Apply runbook: `specs/_runbooks/RB-D1-MIGRATION-APPLY.md`.
- PR validation: `.github/workflows/d1-migration-validate.yml`.
- Schema drift detector: `scripts/d1-migration-verify.py`.
- In-memory replay harness: `crates/corelink-d1-migrations` (Rust).

## 3. Per-migration registry

> Scope column indicates the primary domain (S-NN sprint). Owner is
> the WI author for the migration; long-term ownership of the table
> follows the crate that reads/writes it.

| # | File | WI | Date | Scope | Tables / objects | Depends on |
|---|------|----|------|-------|------------------|------------|
| 1 | `0001_blob_meta.sql` | WI-S01-004 | 2026-02 | S-01 CAS metadata | `blob_meta`, `audit_outbox` | — |
| 2 | `0002_ac_meta.sql` | WI-S04-002 | 2026-03 | S-04 Action Cache | `ac_meta` | `0001` (audit_outbox FK) |
| 3 | `0003_multipart_chunks_manifest.sql` | WI-S05-004 | 2026-03 | S-05 Multipart upload | `chunks`, `manifest_chunks`, `multipart_sessions` | `0001` |
| 4 | `0006_gc_run.sql` | WI-S06-001 | 2026-03 | S-06 GC controller | `gc_run` | `0001` |
| 5 | `0007_gc_candidates.sql` | WI-S06-002 | 2026-03 | S-06 GC enumerator | `gc_candidates` | `0001`, `0006` |
| 6 | `0008_tenant_storage_state.sql` | WI-S07-002 | 2026-03 | S-07 Quota state | `tenant_storage_state` | — |
| 7 | `0009_quota_reservations.sql` | WI-S07-003 | 2026-03 | S-07 Quota reservations | `quota_reservations` | `0008` |
| 8 | `0010_ratelimit_buckets.sql` | WI-S08-001 | 2026-03 | S-08 Rate limit | `ratelimit_buckets` | — |
| 9 | `0011_edge_blocklist.sql` | WI-S08-002 | 2026-03 | S-08 Edge blocklist | `edge_blocklist` | — |
| 10 | `0012_quota_cas_attempts.sql` | WI-S08-003 | 2026-03 | S-08 Abuse signals | `quota_cas_attempts` | `0008` |
| 11 | `0013_abuse_scores.sql` | WI-S08-004 | 2026-03 | S-08 Abuse scoring | `abuse_score_history` | `0012` |
| 12 | `0014_global_circuit_state.sql` | WI-S08-005 | 2026-03 | S-08 Circuit breaker | `global_circuit_state`, `global_circuit_trips_history` | — |
| 13 | `0015_analytics_cardinality_budgets.sql` | WI-S09-001 | 2026-03 | S-09 Analytics budgets | `analytics_cardinality_budgets`, `analytics_cardinality_observed` | — |
| 14 | `0016_log_schema.sql` | WI-S09-002 | 2026-03 | S-09 Log schema versions | `log_schema_versions`, `log_redaction_patterns` | — |
| 15 | `0017_usage_event_idem.sql` | WI-S10-001 | 2026-04 | S-10 Billing usage events | `usage_event_staging` | — |
| 16 | `0018_stripe_idem_keys.sql` | WI-S10-003 | 2026-04 | S-10 Stripe idempotency | `stripe_idempotency_keys`, `stripe_event_log` | — |
| 17 | `0019_billing_reconciliation_drift.sql` | WI-S10-004 | 2026-04 | S-10 Billing reconciliation | `billing_reconciliation_drift`, `stripe_submission_state` | `0017`, `0018` |
| 18 | `0020_quota_fsm_state.sql` | WI-S10-005 | 2026-04 | S-10 Quota FSM | `quota_fsm_state` | `0008`, `0009` |
| 19 | `0021_billing_replay_audit.sql` | WI-S10-006 | 2026-04 | S-10 Billing replay | `billing_replay_audit` | `0017`, `0019` |
| 20 | `0022_dsr_erasure_log.sql` | WI-S11-002 | 2026-04 | S-11 DSR erasure log | `dsr_erasure_log` | — |
| 21 | `0023_residency_check_constraints.sql` | WI-S11-007 | 2026-04 | S-11 Residency triggers | `tenant`, `blob_meta`, `ac_meta`, `audit_outbox`, `billing_events_staging` (referenced; created elsewhere), `region_migration_request` | `0001`, `0002` |
| 22 | `0024_config_change_log.sql` | WI-S13-001 | 2026-04 | S-13 Config audit | `config_change_log` | — |
| 23 | `0025_terraform_drift_findings.sql` | WI-S13-004 | 2026-04 | S-13 Terraform drift | `terraform_drift_findings` | — |
| 24 | `0026_rollout_state_and_budget.sql` | WI-S13-005 | 2026-04 | S-13 Rollout controller | `rollout_state`, `rollout_budget_consumption` | — |
| 25 | `0027_region_provisioning.sql` | WI-S14-001 | 2026-04 | S-14 Region provisioning | `region_provisioning_audit_log`, `region_migration_progress` | — |
| 26 | `0028_tenant_primary_region.sql` | WI-S14-002 | 2026-04 | S-14 Tenant region binding | `tenant` (adds `primary_region`) | `0023` |
| 27 | `0029_hot_blobs.sql` | WI-S14-003 | 2026-04 | S-14 Hot blob index | `hot_blobs` | `0001` |
| 28 | `0030_byok_envelope.sql` | WI-S14-004 | 2026-04 | S-14 BYOK envelope | `byok_envelope` | — |
| 29 | `0031_byok_tenant_status.sql` | WI-S14-006 | 2026-04 | S-14 BYOK kill switch | `tenants` (ADD COLUMN) | — (table naming hazard: references `tenants` plural; see R2-13 follow-up) |
| 30 | `0032_erasure_attestation.sql` | WI-S14-007 | 2026-04 | S-14 Erasure attestation | `erasure_attestations`, `erasure_public_keys` | `0022` |
| 31 | `0033_chaos_runs.sql` | WI-S17-001 | 2026-04 | S-17 Chaos scheduler | `chaos_runs`, `chaos_results` | — |
| 32 | `0034_dr_drill_runs.sql` | WI-S17-002 | 2026-04 | S-17 DR drill scheduler | `dr_drill_runs` | — |
| 33 | `0035_runbook_drills.sql` | WI-S17-003 | 2026-04 | S-17 Runbook drills | `runbook_drills` | — |
| 34 | `0036_oncall_pages.sql` | WI-S17-005 | 2026-04 | S-17 Oncall + pages | `oncall_rotations`, `oncall_shifts`, `oncall_pages` | — (pre-existing syntax hazard: see R2-13 follow-up) |
| 35 | `0037_signup_orchestration.sql` | WI-S19-001 | 2026-04 | S-19 Signup orchestration | `signup_orchestration`, `signup_attempts`, `tenant` (re-declared), `dpa_acceptance_pending`, `pat`, `usage_counter` | `0023` (existing tenant) |
| 36 | `0038_dpa_acceptances.sql` | WI-S19-002 | 2026-04 | S-19 DPA acceptance | `dpa_acceptances` | `0037` |
| 37 | `0039_tier_selection.sql` | WI-S19-004 | 2026-04 | S-19 Tier selection | `tier_selections`, `tier_selection_locks`, `stripe_checkout_sessions` | `0018` |
| 38 | `0040_enterprise_inquiries.sql` | WI-S19-005 | 2026-04 | S-19 Enterprise inquiries | `enterprise_inquiries`, `enterprise_inquiry_outbox` | — |
| 39 | `0041_dpa_versioning.sql` | WI-S19-003 | 2026-04 | S-19 DPA versioning | `dpa_versions`, `tenants` (ADD COLUMN) | `0038` (same plural-naming hazard as 0031) |
| 40 | `0042_lighthouse_customers.sql` | WI-S20-004 | 2026-04 | S-20 Lighthouse tracker | `lighthouse_customers`, `lighthouse_sla_samples` | — |
| 41 | `0043_synthetic_page_drills.sql` | WI-S20-006 | 2026-04 | S-20 Synthetic page drill | `synthetic_page_drills` | `0036` (oncall context) |

Total: **41 migrations** (0001..0043 with intentional gaps at 0004,
0005). Aggregate object count from `d1-migration-verify.py
--schema-only`: 64 tables, 129 indexes, 9 triggers.

## 4. Open issues (R2-13 surfaced)

The R2-13 integration test (`crates/corelink-d1-migrations`) replays
every migration against in-memory SQLite. Three pre-existing hazards
were surfaced and pinned in
`tests/d1_migration_integration.rs::PRE_EXISTING_FAILURES` so they
do not block new work but remain diff-visible:

1. **`0027_region_provisioning.sql`** — `region_migration_progress`
   places `PRIMARY KEY (tenant_id, migration_run_id)` BEFORE
   `source_region`. SQLite (and likely D1, if re-applied from scratch)
   requires all column defs before table-level constraints.
   Follow-up: re-issue PK at end of column list in a 0027b
   additive correction.

2. **`0036_oncall_pages.sql`** — `oncall_shifts` interleaves
   CONSTRAINT clauses between column defs. Same root cause as 0027.
   Follow-up: re-issue in 0036b.

3. **`0037_signup_orchestration.sql`** — `CREATE INDEX` on
   `tenant.email_hash` + `tenant.tenant_state`, but those columns
   were never added to `tenant` (the table was first declared in
   0023 with only 4 columns; `CREATE TABLE IF NOT EXISTS` in 0037
   is a no-op). Follow-up: split into `ALTER TABLE … ADD COLUMN` per
   INV-AUTH-MIGRATION-ADDITIVE.

4. **Plural-vs-singular table naming.** Migrations `0031` and `0041`
   target `tenants` (plural) while the canonical table is `tenant`
   (singular, declared in `0023`). Follow-up: standardise on
   singular; add a per-migration linter to `check_migrations_additive.py`.

## 5. Numbering policy

- File names are zero-padded 4-digit prefix + `_` + snake_case scope
  + `.sql`. Example: `0044_new_feature.sql`.
- New migrations append `0044`, `0045`, … — never reuse a number.
- Two gaps exist (0004, 0005). Both were reserved in early planning
  and never used; do not refill them.

## 6. Cross-references

- INV: `specs/03_architecture/invariant_registry.md`
  INV-AUTH-MIGRATION-ADDITIVE.
- ADR: `specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md`.
- Runbook: `specs/_runbooks/RB-D1-MIGRATION-APPLY.md`.
- Runner: `scripts/d1-migration-runner.sh`.
- Verifier: `scripts/d1-migration-verify.py`.
- Replay harness: `crates/corelink-d1-migrations/`.
