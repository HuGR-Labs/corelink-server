---
type: "ADR"
title: "ADR-0064 — tenant.tier CHECK widening via additive table rebuild (migration 0064)"
description: "Mirrors ADR-0062 for tenant.tier: a SQLite 12-step rebuild adding 'max' (not 'pilot') so tenant.tier, tier_selections.tier, and stripe_checkout_sessions.tier are consistent — destructive in mechanism, additive in effect."
source_files:
  - "specs/03_architecture/adrs/ADR-0064-tenant-tier-max-check-rebuild.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "d1", "migration", "tenant", "tier", "additive", "sqlite", "s19"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0064 — tenant.tier CHECK widening via additive table rebuild (migration 0064)

ADR-0062 widened the tier CHECKs on `tier_selections` and `stripe_checkout_sessions`, but it left
`tenant.tier` (added with its own inline CHECK in migration 0057) without `'max'` — a verified
landmine that would create silent quota-enforcement gaps for max-tier customers. This ADR is the
migration-mechanism record that closes that gap by mirroring the
[ADR-0062](/adr/adr-0062-tier-check-widen-rebuild.md) rebuild pattern onto the `tenant` table in the
[D1 CONFIG_DB](/storage/d1-config-db.md), under the same
[ADR-0036](/adr/adr-0036-d1-schema-migration-governance.md) governance.

# Context

PR #218 surfaced a verified landmine: `tenant.tier`'s CHECK (migration 0057) lacks `max`, and 0062 did
not widen it; its §4-Q2 asked whether to do a 0062-style rebuild. The tech-lead ratified adding `'max'`
(a canonical paid tier; omitting it creates silent quota gaps) but NOT `'pilot'` (a create-time
operational concept handled via `pilot_signups`, not a billing tier). SQLite/D1 cannot relax the
existing inline CHECK in place, so a 12-step `tenant` table rebuild is required, and the
`check_migrations_additive.py` gate demands an ADR-annotated `DROP`/`RENAME` — hence this document.

# Decision

Migration 0064 widens `tenant.tier`'s CHECK to accept `'max'` (retaining `'team'` and `'org'` for
back-compat) via the 12-step rebuild: `CREATE TABLE tenant_new` with all 19 columns reproduced
verbatim → explicit 19-column `INSERT … SELECT` 1:1 copy → `DROP TABLE tenant` → `RENAME` →
re-create all 9 indexes, with `PRAGMA foreign_keys`/`defer_foreign_keys` around the swap and the
`DROP`/`RENAME` tokens annotated `additive-allowed: ADR-0064`. The change is **destructive in
mechanism but purely additive in effect**; a note records that the migration widens the persisted
CHECK only — application-layer create-time validation may still reject `'max'` for specific flows.

# Consequences

The migration is one-shot (guarded by sequential numbering) and applied to prod only via the
owner-gated `apply-d1-migrations-prod.sh`; `tier_selections` and `stripe_checkout_sessions` are
untouched (already accept `'max'` per 0062), so afterward all three tier columns are consistent on the
full 6-tier taxonomy. A post-apply rollback is one-way (any `tier='max'` row would block the narrower
rollback copy), so the recommended path is a forward fix.

# Citations

1. `specs/03_architecture/adrs/ADR-0064-tenant-tier-max-check-rebuild.md:36-60` — Context: PR #218 §4-Q2 landmine + the ratified decision (`'max'` added, `'pilot'` not).
2. `specs/03_architecture/adrs/ADR-0064-tenant-tier-max-check-rebuild.md:62-81` — the technical necessity for a rebuild (SQLite cannot relax an inline CHECK) + the additive gate.
3. `specs/03_architecture/adrs/ADR-0064-tenant-tier-max-check-rebuild.md:91-109` — Decision: 0064 widens `tenant.tier` via the 12-step rebuild; destructive mechanism, additive effect.
4. `specs/03_architecture/adrs/ADR-0064-tenant-tier-max-check-rebuild.md:135-146` — Consequences: one-shot + owner-gated prod apply; all three tier columns consistent afterward.
5. `specs/03_architecture/adrs/ADR-0064-tenant-tier-max-check-rebuild.md:148-155` — Rollback note: a post-apply rollback is one-way; forward fix recommended.
