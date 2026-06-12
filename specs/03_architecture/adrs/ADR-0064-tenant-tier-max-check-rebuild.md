---
id: "ADR-0064"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-10"
updated: "2026-06-10"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "d1", "migration", "tenant", "tier", "additive", "sqlite", "s19"]
references:
  - "migrations/d1/0064_tenant_tier_max.sql"
  - "migrations/d1/0057_tenant_tier.sql"
  - "migrations/d1/0062_expand_tier_selections_6tier.sql"
  - "specs/03_architecture/adrs/ADR-0062-tier-check-widen-rebuild.md"
  - "specs/03_architecture/adrs/ADR-S19-001-tier-taxonomy-amendment-5-to-6.md"
  - "scripts/check_migrations_additive.py"
---

# ADR-0064 — `tenant.tier` CHECK widening via additive table rebuild (migration 0064)

## Status

**ACCEPTED** (2026-06-10). Companion to **ADR-S19-001** (the 5→6 tier taxonomy
product decision) and **ADR-0062** (the `tier_selections`/`stripe_checkout_sessions`
rebuild pattern); this ADR records the D1 migration **mechanism** for `tenant.tier`
and is the record cited by the `additive-allowed: ADR-0064` gate annotations in
`migrations/d1/0064_tenant_tier_max.sql`.

## Context

### §4-Q2 ratification record (PR #218)

PR #218 ("docs(design): admin pilot-tenant endpoint + billing→ratelimit mapping")
surfaced the following **verified landmine** in its body:

> `tenant.tier` CHECK (migration 0057) lacks `max` (and `pilot`); 0062 did NOT
> widen it → additive 0063 follow-up.

PR #218 §4 open questions include **Q2**:

> Approve the 0062-style rebuild ADR widening `tenant.tier` to add `max`
> (and `pilot`?), or keep create-time tier ⊂ {free,solo,starter,pro,enterprise} forever.

The tech-lead ratified this as **non-blocking at PR #218 merge time** (design-only
PR; no implementation shipped) with the following decision:

- **`'max'` IS added** — it is a canonical paid tier per ADR-S19-001; the 0062
  rebuild already added it to `tier_selections.tier` and
  `stripe_checkout_sessions.tier`; leaving it out of `tenant.tier` would create
  silent quota-enforcement gaps for max-tier customers.
- **`'pilot'` is NOT added** — pilot is a create-time operational concept handled
  via `pilot_signups` (migration 0053), not a billing-tier value that should
  ever be stored in `tenant.tier`.

This migration (0064) implements that ratified decision.

### Technical necessity for a table rebuild

Migration 0057 added `tenant.tier` via:

```sql
ALTER TABLE tenant
  ADD COLUMN tier TEXT NOT NULL DEFAULT 'free'
    CHECK (tier IN ('free', 'solo', 'starter', 'team', 'pro', 'org', 'enterprise'));
```

SQLite (hence Cloudflare D1) has **no** `ALTER TABLE … ALTER COLUMN` /
`DROP CONSTRAINT` / `ADD CONSTRAINT` to relax an existing inline CHECK in place
(SQLite documentation: "Making Other Kinds Of Table Schema Changes" — the 12-step
rebuild procedure). The column and its CHECK now exist in-schema; the only
schema-correct way to widen the CHECK is to rebuild the `tenant` table.

The `check_migrations_additive.py` gate (INV-AUTH-MIGRATION-ADDITIVE, HIGH)
rejects raw `DROP TABLE` / `RENAME TO` tokens unless annotated
`-- additive-allowed: ADR-NNNN <reason>` against a recorded ADR — hence this
document.

### ADR-0062 precedent

ADR-0062 established the same rebuild pattern for `tier_selections` and
`stripe_checkout_sessions`. This ADR mirrors ADR-0062's structure and
zero-data-loss proof discipline exactly, applying it to `tenant`.

## Decision

Migration `0064_tenant_tier_max.sql` widens `tenant.tier`'s CHECK to accept
`'max'` (retaining `'team'` and `'org'` for back-compat) via the 12-step rebuild:

1. `CREATE TABLE tenant_new` with the widened CHECK and all 19 columns reproduced
   verbatim from the accumulated schema (migrations 0023 + 0031 + 0037 + 0041 +
   0056 + 0057).
2. `INSERT INTO tenant_new … SELECT … FROM tenant` — explicit 19-column list,
   no `WHERE`, no transform → every row and column value preserved 1:1.
3. `DROP TABLE tenant` — annotated `additive-allowed: ADR-0064`.
4. `ALTER TABLE tenant_new RENAME TO tenant` — annotated `additive-allowed: ADR-0064`.
5. Re-create all 9 indexes verbatim (origins recorded in the migration header).

`PRAGMA foreign_keys = OFF` / `PRAGMA defer_foreign_keys = true` are set around
the swap per the SQLite 12-step guidance (same pattern as 0062).

The `DROP`/`RENAME` tokens carry `-- additive-allowed: ADR-0064 …`.

**The change is destructive in mechanism (rebuild) but purely additive in effect**:
the accepted-value set only grows, and **zero rows are dropped or mutated**.

**Create-time enforcement note:** the migration widens the persisted CHECK only.
Application-layer code that validates tier values at create time (e.g.,
`tier_select.rs` / `quota.ts`) may still reject `'max'` for specific flows
(e.g., direct admin creation without a Stripe checkout) — that is a separate
code-level rule and is NOT changed by this migration.

## Zero-data-loss proof

1. The copy is `INSERT INTO tenant_new (<19 columns>) SELECT <19 columns> FROM tenant`
   with no `WHERE` and no transform → every row and every column value preserved.
2. No accepted tier value is removed (`'team'` and `'org'` are retained) → no
   pre-existing row can violate the widened CHECK, so the copy cannot fail on a
   constraint.
3. All 9 indexes are re-created verbatim after the swap (origins enumerated in the
   migration header), so every lookup invariant is preserved across the rebuild.
4. The 3 triggers on `tenant` created by migration 0028
   (`trg_tenant_primary_region_required`, `trg_tenant_primary_region_immutable`,
   `trg_tenant_primary_region_valid_insert`) are name-bound to the table name
   `tenant`, not the physical storage object — they survive the DROP + RENAME
   swap on the same connection and do NOT need to be dropped and recreated.
5. No dependent view references `tenant` (confirmed: `stripe_tier_drift_view`
   from 0048 references `tier_selections` + `stripe_subscriptions` only) — no
   view drop/recreate step is required (contrast ADR-0062 §safety).

## Consequences

- **One-shot** (a rebuild is not self-idempotent); guarded by the migration
  runner's sequential numbering (`wrangler d1 migrations apply` records 0064
  exactly once — the same guarantee 0062 relies on).
- Applied to prod **only** via `scripts/apply-d1-migrations-prod.sh`
  (`EXPECTED_FILE_COUNT` bumped 62→63), an owner-gated step.
- `tier_selections` and `stripe_checkout_sessions` are **not** touched (their
  CHECKs already accept `'max'` per migration 0062).
- After this migration lands, `tenant.tier`, `tier_selections.tier`, and
  `stripe_checkout_sessions.tier` are consistent — all three accept the full
  canonical 6-tier taxonomy plus legacy back-compat values.

## Rollback note

A rollback after the migration has been applied to prod requires a complementary
rebuild migration (the same 12-step pattern) that removes `'max'` from the CHECK.
However, any tenant row with `tier = 'max'` written after 0064 was applied would
fail the narrower CHECK and block the rollback copy step — the operator must
back-fill those rows to an adjacent tier before running a rollback migration.
This is the standard one-way migration risk; the recommended path is a forward fix.

## References

- `migrations/d1/0064_tenant_tier_max.sql` (the migration)
- `migrations/d1/0057_tenant_tier.sql` (original inline CHECK)
- `migrations/d1/0062_expand_tier_selections_6tier.sql` (12-step rebuild pattern)
- ADR-0062 (tier CHECK widen via rebuild — the direct precedent for this ADR)
- ADR-S19-001 (tier taxonomy 5→6, product decision)
- PR #218 §4-Q2 (ratification record)
- `scripts/check_migrations_additive.py` (the gate this ADR suppresses)
