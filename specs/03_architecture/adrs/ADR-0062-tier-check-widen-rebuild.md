---
id: "ADR-0062"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-09"
updated: "2026-06-09"
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

# ADR-0062 — tier-CHECK widening via additive table rebuild (migration 0062)

## Status

**ACCEPTED** (2026-06-09). Companion to **ADR-S19-001** (the 5→6 tier taxonomy
*product* decision); this ADR records the D1 migration **mechanism** and is the
record cited by the `additive-allowed: ADR-0062` gate annotations in
`migrations/d1/0062_expand_tier_selections_6tier.sql`.

## Context

ADR-S19-001 amends the tier taxonomy to 6 tiers and requires the persisted tier
domain to accept `'solo'` + `'max'`. That domain is the **inline**
`CHECK (tier IN (...))` on `tier_selections.tier` and
`stripe_checkout_sessions.tier`, declared at `CREATE TABLE` time in migration
0039.

SQLite (hence Cloudflare D1) has **no** `ALTER TABLE … ALTER COLUMN` /
`DROP CONSTRAINT` / `ADD CONSTRAINT` to relax an existing inline CHECK in place.
The only schema-correct way to widen it is the SQLite-documented **12-step table
rebuild**. (The 0057 `tenant.tier` precedent widened the taxonomy by ADDing a
brand-new column — an idiom that only works for a not-yet-existing column, so it
cannot relax 0039's pre-existing CHECKs.) The other in-place workaround in the
repo — `BEFORE INSERT/UPDATE … RAISE(ABORT)` triggers (0023) — can only ADD
restrictions, never RELAX a CHECK, so it does not apply.

The `check_migrations_additive.py` gate (INV-AUTH-MIGRATION-ADDITIVE, HIGH)
rejects raw `DROP TABLE` / `RENAME` tokens unless annotated
`-- additive-allowed: ADR-NNNN <reason>` against a recorded ADR.

## Decision

Migration `0062_expand_tier_selections_6tier.sql` widens **both** CHECKs to
accept `'solo'` + `'max'` (retaining `'team'` for back-compat) via the 12-step
rebuild: `CREATE …_new` (widened CHECK) → `INSERT … SELECT` explicit-column
**1:1 copy** → `DROP` old → `RENAME` new → re-create **every** index (incl. the
UNIQUE partial `idx_tenant_active_subscription` that enforces
INV-ONBOARD-DPA-FIRST). `PRAGMA foreign_keys` is toggled OFF/ON around the swap
per the SQLite guidance.

The `DROP`/`RENAME` tokens carry `-- additive-allowed: ADR-0062 …`. The change
is **destructive in mechanism** (rebuild) but **purely additive in effect**: the
accepted-value set only grows, and **zero rows are dropped or mutated**.

## Zero-data-loss proof

1. The copy is `INSERT INTO …_new (<all columns>) SELECT <all columns> FROM <old>`
   with no `WHERE` and no transform → every row and every column value preserved.
2. No accepted tier value is removed (`'team'` is retained) → no pre-existing row
   can violate the widened CHECK, so the copy cannot fail on a constraint.
3. All indexes — including the at-most-one-`active`-subscription UNIQUE partial —
   are re-created verbatim after the swap, so the persisted invariants hold across
   the rebuild.

## Consequences

- **One-shot** (a rebuild is not self-idempotent); guarded by the migration
  runner's sequential numbering (`wrangler d1 migrations apply` records 0062
  exactly once — the same guarantee 0057 relies on).
- Applied to prod **only** via `scripts/apply-d1-migrations-prod.sh`
  (`EXPECTED_FILE_COUNT` bumped 60→61), an owner-gated step.
- `tier_selection_locks` is untouched (its CHECKs do not reference `tier`).

## References

- `migrations/d1/0062_expand_tier_selections_6tier.sql`;
  `migrations/d1/0039_tier_selection.sql` (original inline CHECKs);
  `migrations/d1/0057_tenant_tier.sql` (ADD-COLUMN widening precedent).
- ADR-S19-001 (tier taxonomy 5→6, product); ADR-0036 (D1 migration governance);
  `scripts/check_migrations_additive.py` (the gate).
