---
id: "ADR-0102"
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
tags: ["adr", "d1", "migration", "terraform-drift", "region", "additive", "sqlite", "s13"]
references:
  - "migrations/d1/0139_terraform_drift_region_contract.sql"
  - "migrations/d1/0025_terraform_drift_findings.sql"
  - "migrations/d1/0131_terraform_drift_summary_artifact.sql"
  - "scripts/check_migration_prefixes.py"
  - "scripts/check_migrations_additive.py"
  - "scripts/test_terraform_drift_region_contract.py"
  - "crates/corelink-terraform-drift-consumer/src/classifier.rs"
---

# ADR-0102 — Terraform-drift region contract via compatibility rebuild (migration 0139)

## Status

**ACTIVE, migration-scoped record.** This ADR records the D1 migration
mechanism and its data-preservation, write-contract, rollout, and rollback
constraints. It does not authorize a production apply by itself.

## Context

Migration `0025_terraform_drift_findings.sql` created an inline SQLite CHECK
for the original Terraform region names: `us-east`, `us-west`, `eu-west`,
`ap-southeast`, `sa-east`, and `global`. The deployed Terraform roots and the
drift consumer now use the canonical four-region set `wnam`, `enam`, `weur`,
and `sam`.

SQLite and Cloudflare D1 do not provide an in-place operation for changing an
inline CHECK constraint. Migration `0139` therefore uses a bounded table
rebuild. The rebuild must keep persisted historical findings readable while
making new findings and region updates obey the canonical contract.

The migration additivity gate rejects the physical `DROP TABLE` and `RENAME
TO` steps unless each line cites a migration-specific ADR. This ADR is the
record for those two line-local waivers.

## Decision

Migration `0139_terraform_drift_region_contract.sql`:

1. Creates `terraform_drift_findings_new` with the canonical four regions and
   every historical region literal retained in its CHECK. The explicit
   `INSERT ... SELECT` copies every column 1:1, including
   `plan_summary_artifact_url`, with no filter or transformation.
2. Swaps the rebuilt table into the original name. The physical `DROP TABLE`
   and `ALTER TABLE ... RENAME TO` are annotated with
   `-- additive-allowed: ADR-0102 <reason>` because the table replacement is
   data-preserving and required to change the inline CHECK.
3. Recreates the existing indexes exactly: the open-finding partial index,
   the region/time index, and the severity/time index.
4. Adds a BEFORE INSERT trigger that accepts only `wnam`, `enam`, `weur`, and
   `sam`. Historical rows are not rewritten or rejected during the copy.
5. Adds a BEFORE UPDATE OF `region` trigger. A region cannot change, and a
   legacy region cannot be updated, even to itself; a canonical row may be
   updated while leaving its region unchanged.
6. Uses the existing migration runner's numeric ordering. Migration `0139` is
   a one-shot rollout and is not a replay-safe down migration.

The migration keeps `PRAGMA foreign_keys` disabled only around the table swap
and restores it before completion. It does not change the consumer's event
shape, remove the legacy rows, or introduce a production apply path.

## Preservation and rollout invariants

- Every pre-existing finding row and column value survives the explicit copy,
  including rows whose region uses a historical literal.
- New writes have exactly the canonical four-region domain. The compatibility
  literals exist only so persisted historical findings remain readable.
- Region identity is immutable after insertion. Remediation and other
  non-region updates remain available through the existing columns.
- All three pre-existing Terraform-drift indexes are recreated after the
  swap.
- Migration prefix uniqueness and the additive migration gate remain required
  before any apply. Production application remains an owner-gated operation
  through the existing D1 migration runner.

## Rollback constraints

There is no in-place down migration for `0139`. Replaying the numbered
migration or manually reversing its table swap is not a rollback procedure.
If `0139` has not been applied, leave it pending. If it has been applied, any
future compatibility or correction must use a separately reviewed, higher
numbered forward migration that preserves the findings table and its indexes.
Rolling application code back does not remove the canonical write trigger or
make legacy region names valid for new writes.

## References

- `migrations/d1/0025_terraform_drift_findings.sql` — original table and
  historical region CHECK.
- `migrations/d1/0131_terraform_drift_summary_artifact.sql` — summary artifact
  column preserved by the explicit copy.
- `migrations/d1/0139_terraform_drift_region_contract.sql` — executable
  rebuild, triggers, indexes, and line-local additive waivers.
- `scripts/check_migration_prefixes.py` and
  `scripts/check_migrations_additive.py` — migration guards.
- `scripts/test_terraform_drift_region_contract.py` — SQLite contract harness.
