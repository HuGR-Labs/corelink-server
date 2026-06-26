---
type: "ADR"
title: "ADR-0062 — tier-CHECK widening via additive table rebuild (migration 0062)"
description: "Records the D1 mechanism for the 5→6 tier amendment: a SQLite 12-step rebuild that widens the tier_selections/stripe_checkout_sessions CHECKs to accept 'solo'+'max' — destructive in mechanism, purely additive in effect."
source_files:
  - "specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "d1", "migration", "tier-selection", "additive", "sqlite", "s19"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0062 — tier-CHECK widening via additive table rebuild (migration 0062)

The 5→6 tier product amendment needs the persisted tier domain to accept new values, but those values
live in an inline `CHECK (tier IN (...))` that SQLite cannot relax in place — so widening it requires
the full table-rebuild ceremony. This ADR is the migration-mechanism record (companion to the
ADR-S19-001 product decision) that the `additive-allowed: ADR-0062` gate annotations cite, applying
the [ADR-0036](/adr/adr-0036-d1-schema-migration-governance.md) governance to the
[D1 CONFIG_DB](/storage/d1-config-db.md) tier tables.

# Context

ADR-S19-001 amends the taxonomy to 6 tiers and requires the persisted domain to accept `'solo'` +
`'max'`, but that domain is an inline `CHECK` on `tier_selections.tier` and
`stripe_checkout_sessions.tier` declared at `CREATE TABLE` time in migration 0039; SQLite/D1 has no
`ALTER … ALTER COLUMN` / `DROP`/`ADD CONSTRAINT` to relax an inline CHECK, so the only schema-correct
path is the SQLite 12-step rebuild, and the `check_migrations_additive.py` gate rejects raw
`DROP`/`RENAME` tokens unless annotated against a recorded ADR.

# Decision

Migration 0062 widens **both** CHECKs to accept `'solo'` + `'max'` (retaining `'team'` for
back-compat) via the 12-step rebuild — `CREATE …_new` (widened CHECK) → `INSERT … SELECT` explicit
1:1 column copy → `DROP` old → `RENAME` new → re-create every index (including the UNIQUE partial
enforcing INV-ONBOARD-DPA-FIRST), with `PRAGMA foreign_keys` toggled around the swap. The `DROP`/
`RENAME` tokens carry `-- additive-allowed: ADR-0062`. The change is **destructive in mechanism
(rebuild) but purely additive in effect**: the accepted-value set only grows and zero rows are dropped
or mutated, proven by the no-`WHERE`/no-transform copy and the retention of every prior tier value.

# Consequences

The migration is one-shot (a rebuild is not self-idempotent), guarded by the runner's sequential
numbering so it records exactly once, and applied to prod only via the owner-gated
`apply-d1-migrations-prod.sh`; `tier_selection_locks` is untouched because its CHECKs do not reference
`tier`.

# Citations

1. `specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md:34-52` — Context: ADR-S19-001 needs `'solo'`+`'max'`, SQLite cannot relax an inline CHECK, so the 12-step rebuild + the additive gate apply.
2. `specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md:55-66` — Decision: 0062 widens both CHECKs via the 12-step rebuild; destructive in mechanism, additive in effect.
3. `specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md:68-75` — the zero-data-loss proof (no-`WHERE` 1:1 copy, no tier value removed, indexes re-created).
4. `specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md:79-84` — Consequences: one-shot + sequential-numbering guard + owner-gated prod apply.
