---
id: "ADR-0098"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-18"
updated: "2026-08-18"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "d1", "migration", "erasure-attestation", "additive", "sqlite", "s14", "apac"]
references:
  - "migrations/d1/0098_widen_erasure_region_check_apac.sql"
  - "migrations/d1/0032_erasure_attestation.sql"
  - "migrations/d1/0079_erasure_attestation_signed_columns.sql"
  - "crates/corelink-erasure-attestation/src/region.rs"
  - "specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md"
  - "scripts/check_migrations_additive.py"
---

# ADR-0098 — erasure region-CHECK widening via additive table rebuild (migration 0098)

## Status

**ACCEPTED** (2026-08-18). Records the D1 migration **mechanism** for admitting
the `apac` attestation region into the GDPR erasure ATTESTATION subsystem, and
is the record cited by the `additive-allowed: ADR-0098` gate annotations in
`migrations/d1/0098_widen_erasure_region_check_apac.sql`. Direct analogue of
**ADR-0062** (tier-CHECK widening).

## Context

`crates/corelink-erasure-attestation/src/region.rs` gained an `Apac` variant
(`Region::parse("apac") → Some(Region::Apac)`), so the subsystem now signs and
serves attestations for the Asia-Pacific macro-region. The persisted region
domain is the **inline** `CHECK (region IN ('wnam','enam','weur','sam'))` on
both `erasure_attestations.region` and `erasure_public_keys.region`, declared at
`CREATE TABLE` time in migration 0032. That CHECK would reject any `apac` row,
so the code change alone cannot persist an apac attestation or public key.

SQLite (hence Cloudflare D1) has **no** `ALTER TABLE … ALTER COLUMN` /
`DROP CONSTRAINT` / `ADD CONSTRAINT` to relax an existing inline CHECK in place.
The only schema-correct way to widen it is the SQLite-documented **12-step table
rebuild** — the same mechanism ADR-0062 used for `tier_selections` /
`stripe_checkout_sessions`.

The `check_migrations_additive.py` gate (INV-AUTH-MIGRATION-ADDITIVE, HIGH)
rejects raw `DROP TABLE` / `RENAME` tokens unless annotated
`-- additive-allowed: ADR-NNNN <reason>` against a recorded ADR.

## Decision

Migration `0098_widen_erasure_region_check_apac.sql` widens **both** CHECKs to
`region IN ('wnam','enam','weur','sam','apac','afr')` — the full 6-macro-region
set (both `apac` AND `afr`, so a later afr signing key needs no further schema
change) via the 12-step rebuild: `CREATE …_new` (widened CHECK) → `INSERT …
SELECT` explicit-column **1:1 copy** → `DROP` old → `RENAME` new → re-create
**every** 0032 index. For `erasure_attestations` the `_new` table replicates the
0032 columns **plus** the two nullable columns 0079 added (`signature_ed25519`,
`canonical_payload_jcs`), so signed rows survive the copy byte-for-byte.
`PRAGMA foreign_keys` is toggled OFF/ON around the swap per the SQLite guidance.

The `DROP`/`RENAME` tokens carry `-- additive-allowed: ADR-0098 …`. The change
is **destructive in mechanism** (rebuild) but **purely additive in effect**: the
accepted-value set only grows, and **zero rows are dropped or mutated**.

## Zero-data-loss proof

1. Each copy is `INSERT INTO …_new (<all columns>) SELECT <all columns> FROM
   <old>` with no `WHERE` and no transform → every row and column value
   preserved (incl. the nullable 0079 signature columns).
2. No accepted region value is removed (the new set is a strict superset of the
   old) → no pre-existing row can violate the widened CHECK, so the copy cannot
   fail on a constraint.
3. All 0032 indexes are re-created verbatim after each swap, so the persisted
   lookup paths (tenant_id / region / signed_at_ms; region+state) hold across
   the rebuild.

## Consequences

- **One-shot** (a rebuild is not self-idempotent); guarded by the migration
  runner's sequential numbering (`wrangler d1 migrations apply` records 0098
  exactly once).
- Applied to prod only via `scripts/apply-d1-migrations-prod.sh` (dynamic
  `EXPECTED_FILE_COUNT`; `EXPECTED_TABLE_COUNT` floor is unchanged — the rebuild
  preserves both tables), an owner-gated step.
- No dependent view references either table (grep-verified), so no 12-step
  "drop and recreate views" step is needed (unlike 0062).

## References

- `migrations/d1/0098_widen_erasure_region_check_apac.sql`;
  `migrations/d1/0032_erasure_attestation.sql` (original inline CHECKs +
  indexes); `migrations/d1/0079_erasure_attestation_signed_columns.sql` (the two
  additive nullable columns).
- `crates/corelink-erasure-attestation/src/region.rs` (the `Apac` variant).
- ADR-0062 (tier-CHECK widening precedent); ADR-0036 (D1 migration governance);
  `scripts/check_migrations_additive.py` (the gate).
