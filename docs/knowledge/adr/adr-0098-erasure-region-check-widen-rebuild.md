---
type: "ADR"
title: "ADR-0098 — erasure region-CHECK widening via additive table rebuild (migration 0098)"
description: "Records the D1 mechanism for admitting the apac attestation region: a SQLite 12-step rebuild that widens the erasure_attestations/erasure_public_keys region CHECKs to all 6 macro-regions — destructive in mechanism, purely additive in effect."
source_files:
  - "specs/03_architecture/adrs/ADR-0098-erasure-region-check-widen-rebuild.md"
  - "migrations/d1/0098_widen_erasure_region_check_apac.sql"
source_blobs:
  - "specs/03_architecture/adrs/ADR-0098-erasure-region-check-widen-rebuild.md@793d720840540f3480105ec5051c1f9475cb71f0"
  - "migrations/d1/0098_widen_erasure_region_check_apac.sql@6205411ab0b0cdbe1395c1691973e9fa16707359"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["adr", "d1", "migration", "erasure-attestation", "additive", "sqlite", "s14", "apac"]
timestamp: "2026-08-18T00:00:00Z"
---
# ADR-0098 — erasure region-CHECK widening via additive table rebuild (migration 0098)

This is a fully anchored concept. The source ADR and the landed migration are
both cited below; the migration is the executable schema change and the ADR is
its additive-governance record.

The GDPR erasure attestation subsystem gained an `apac` region, but the persisted region domain lives
in an inline `CHECK (region IN (...))` that SQLite cannot relax in place — so admitting `apac` requires
the full table-rebuild ceremony. This ADR is the migration-mechanism record (the direct analogue of
[ADR-0062](/adr/adr-0062-tier-check-widen-rebuild.md)) that the `additive-allowed: ADR-0098` gate
annotations cite, applying the [ADR-0036](/adr/adr-0036-d1-schema-migration-governance.md) governance
to the [D1 CONFIG_DB](/storage/d1-config-db.md) erasure tables.

# Context

`crates/corelink-erasure-attestation/src/region.rs` added an `Apac` variant so the subsystem signs and
serves apac attestations, but the persisted domain is an inline `CHECK` on `erasure_attestations.region`
and `erasure_public_keys.region` declared at `CREATE TABLE` time in migration 0032 accepting only the
4-region baseline; SQLite/D1 has no `ALTER … ALTER COLUMN` / `DROP`/`ADD CONSTRAINT` to relax an inline
CHECK, so the only schema-correct path is the SQLite 12-step rebuild, and the
`check_migrations_additive.py` gate rejects raw `DROP`/`RENAME` tokens unless annotated against a
recorded ADR.

# Decision

Migration 0098 widens **both** CHECKs to the full 6-macro-region set
`('wnam','enam','weur','sam','apac','afr')` via the 12-step rebuild — `CREATE …_new` (widened CHECK) →
`INSERT … SELECT` explicit 1:1 column copy → `DROP` old → `RENAME` new → re-create every 0032 index —
with `PRAGMA foreign_keys` toggled around the swap. The `erasure_attestations` rebuild replicates the
0032 columns plus the two nullable columns 0079 added (`signature_ed25519`, `canonical_payload_jcs`).
The `DROP`/`RENAME` tokens carry `-- additive-allowed: ADR-0098`. The change is **destructive in
mechanism (rebuild) but purely additive in effect**: the accepted-value set is a strict superset and
zero rows are dropped or mutated.

# Consequences

The migration is one-shot (a rebuild is not self-idempotent), guarded by the runner's sequential
numbering so it records exactly once, and applied to prod only via the owner-gated
`apply-d1-migrations-prod.sh`; no dependent view references either table, so no 12-step view
drop/recreate step is needed (unlike 0062).

# Citations

- `specs/03_architecture/adrs/ADR-0098-erasure-region-check-widen-rebuild.md:24-38` — context and SQLite 12-step rebuild decision.
- `specs/03_architecture/adrs/ADR-0098-erasure-region-check-widen-rebuild.md:56-85` — widened CHECK values, additive copy semantics, and rollout consequences.
- `migrations/d1/0098_widen_erasure_region_check_apac.sql:1-36` — executable migration contract and ADR annotation requirement.
- `migrations/d1/0098_widen_erasure_region_check_apac.sql:72-128` — explicit 1:1 copies, widened CHECKs, and indexed table swaps.
