---
id: "ADR-0103"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-09-22"
updated: "2026-09-22"
owner: "Gustavo Schneiter"
final_approver: "pending"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "d1", "migration", "gc", "b-071", "additive"]
references:
  - "migrations/d1/0142_gc_accounting_legal_hold.sql"
  - "migrations/d1/0143_gc_accounting_region_upgrade.sql"
  - "scripts/check_migrations_additive.py"
  - "crates/corelink-ops/tests/migrations_prop_migration_additivity.rs"
---

# ADR-0103 — Name-scoped B-071 accounting trigger replacement

## Decision

**ACTIVE, migration-scoped; no production apply is authorized.** Fresh
installs use corrected `0142`; deployments with historical `0118` trigger
bodies require `0143`, because SQLite `CREATE TRIGGER IF NOT EXISTS` does not
replace an existing body. The verifier accepts only these exact annotated
statements, once each and only in `migrations/d1/0143_gc_accounting_region_upgrade.sql`:

- `DROP TRIGGER IF EXISTS trg_gc_purge_accounting_required;`
- `DROP TRIGGER IF EXISTS trg_gc_purge_finalize_accounting;`

Every other trigger, table, index, column, or destructive statement in `0143`
fails the gate. The migration preserves legal hold, tenant/digest fencing,
atomic accounting, concurrency, idempotent replay, and saturated byte
arithmetic. Hosted checks cover fresh `0142` → `0143` and historical `0118` →
`0143` replayed twice. This one-way exception is unusable for any later
migration; later changes require a new decision and forward migration.
