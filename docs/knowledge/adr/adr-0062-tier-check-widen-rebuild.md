---
type: "ADR"
title: "ADR-0062 — tier-CHECK widening via additive catalog edit (migration 0062)"
description: "Records the D1 mechanism for the 5→6 tier amendment: an in-place SQLite catalog edit that widens the tier_selections/stripe_checkout_sessions CHECKs to accept 'solo'+'max' without replacing tables or rows."
source_files:
  - "specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md"
source_blobs:
  - "specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md@73c69b4149b872ee10901db21171d7fabae01439"
checkpoint_sha: "a810ff13ddee10d4af4589a51610c1fd0422cbaf"
provenance: "AUTHORED"
tags: ["adr", "d1", "migration", "tier-selection", "additive", "sqlite", "s19"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0062 — tier-CHECK widening via additive catalog edit (migration 0062)

The 5→6 tier product amendment needs the persisted tier domain to accept new values, but those values
live in an inline `CHECK (tier IN (...))` that SQLite cannot relax through ordinary DDL. A
rebuild would replace tables and violate the additive gate, so migration 0062 edits only the
SQLite catalog SQL while retaining the physical objects. This ADR is the migration-mechanism
record (companion to the ADR-S19-001 product decision), applying the
[ADR-0036](/adr/adr-0036-d1-schema-migration-governance.md) governance to the
[D1 CONFIG_DB](/storage/d1-config-db.md) tier tables.

# Context

ADR-S19-001 amends the taxonomy to 6 tiers and requires the persisted domain to accept `'solo'` +
`'max'`, but that domain is an inline `CHECK` on `tier_selections.tier` and
`stripe_checkout_sessions.tier` declared at `CREATE TABLE` time in migration 0039; SQLite/D1 has no
`ALTER … ALTER COLUMN` / `DROP`/`ADD CONSTRAINT` to relax an inline CHECK. The migration therefore
uses SQLite's `writable_schema` catalog-editor pragma with exact table/name guards and a TEMP
postcondition CHECK; no table, row, index, FK, or view is replaced.

# Decision

Migration 0062 widens **both** CHECKs to accept `'solo'` + `'max'` (retaining `'team'` for
back-compat) by exact `UPDATE sqlite_master SET sql = replace(...)` statements scoped to the two
0039 table names. The schema cookie is invalidated and a TEMP `CHECK (ok = 1)` postcondition guard
fails closed on a partial edit. The physical tables, all columns/constraints/indexes, the FK, the
dependent view, and every row remain in place; the accepted-value set only grows.

# Consequences

The migration is replay-safe: old-expression replacements affect zero rows after the first run and
the TEMP guard remains valid. It is still recorded by the runner's sequential numbering and applied
to prod only via the owner-gated `apply-d1-migrations-prod.sh`; `tier_selection_locks` is untouched
because its CHECKs do not reference `tier`.

# Citations

1. `specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md:1-12` — Context and decision for the exact in-place catalog edit.
2. `migrations/d1/0062_expand_tier_selections_6tier.sql` — executable guards and six-tier CHECK postconditions.
3. `scripts/verify_b256_migration_additivity.py` — SQLite rows/schema/view preservation, mutation, and replay proof.
