---
id: "ADR-0062"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "2.0.0"
created: "2026-06-09"
updated: "2026-09-06"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "d1", "migration", "tier-selection", "additive", "sqlite", "s19"]
references:
  - "migrations/d1/0062_expand_tier_selections_6tier.sql"
  - "migrations/d1/0039_tier_selection.sql"
  - "migrations/d1/0057_tenant_tier.sql"
  - "specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md"
  - "scripts/check_migrations_additive.py"
---

# ADR-0062 — tier-CHECK widening via additive catalog edit (migration 0062)

## Status

**ACCEPTED** (2026-06-09; mechanism amended 2026-09-06). Companion to
**ADR-S19-001** (the 5→6 tier taxonomy *product* decision); this ADR records the
D1 migration **mechanism** and its additive proof.

## Context

ADR-S19-001 amends the tier taxonomy to 6 tiers and requires the persisted tier
domain to accept `'solo'` + `'max'`. That domain is the **inline**
`CHECK (tier IN (...))` on `tier_selections.tier` and
`stripe_checkout_sessions.tier`, declared at `CREATE TABLE` time in migration
0039.

SQLite (hence Cloudflare D1) has **no** `ALTER TABLE … ALTER COLUMN` /
`DROP CONSTRAINT` / `ADD CONSTRAINT` to relax an existing inline CHECK through
ordinary DDL. A 12-step rebuild would replace tables and violate
`INV-AUTH-MIGRATION-ADDITIVE`, so it is not acceptable for this migration. The
0057 `tenant.tier` ADD-column precedent and 0023 restriction triggers likewise
cannot alter 0039's existing column.

The property gate (INV-AUTH-MIGRATION-ADDITIVE, HIGH) rejects destructive
statement prefixes, including table-swap `ALTER TABLE <name> RENAME TO`.

## Decision

Migration `0062_expand_tier_selections_6tier.sql` widens **both** CHECKs to
accept `'solo'` + `'max'` (retaining `'team'` for back-compat) by exact
`UPDATE sqlite_master SET sql = replace(...)` statements scoped to the two
0039 table names. SQLite's `writable_schema` pragma is enabled only for those
catalog edits, then disabled; the schema cookie is invalidated so the current
connection reparses the CHECKs. A TEMP `CHECK (ok = 1)` postcondition fails
closed if either edit is missing or partial. No physical table, row, index, FK,
or dependent view is replaced.

## Zero-data-loss and replay proof

1. Both table objects remain in place; only their catalog CHECK expressions are
   replaced. Existing rows and every column value are therefore untouched.
2. No accepted tier value is removed (`'team'` is retained), while `'solo'` and
   `'max'` are added to the appropriate domains.
3. Existing indexes, the at-most-one-`active` UNIQUE partial, FK, and dependent
   view are not dropped or recreated.
4. Replaying 0062 finds no old expression, performs no durable edit, and passes
   the same postcondition guard.

## Consequences

- **Replay-safe** (old-expression replacements become no-ops); also guarded by
  the migration runner's sequential numbering (`wrangler d1 migrations apply`
  records 0062 exactly once).
- Applied to prod **only** via `scripts/apply-d1-migrations-prod.sh`
  (`EXPECTED_FILE_COUNT` bumped 60→61), an owner-gated step.
- `tier_selection_locks` is untouched (its CHECKs do not reference `tier`).

## References

- `migrations/d1/0062_expand_tier_selections_6tier.sql`;
  `migrations/d1/0039_tier_selection.sql` (original inline CHECKs);
  `migrations/d1/0057_tenant_tier.sql` (ADD-COLUMN widening precedent).
- ADR-S19-001 (tier taxonomy 5→6, product); ADR-0036 (D1 migration governance);
  `scripts/check_migrations_additive.py` and
  `crates/corelink-ops/tests/migrations_prop_migration_additivity.rs` (the
  destructive-prefix gates); `scripts/verify_b256_migration_additivity.py`
  (focal SQLite and mutation proof).
