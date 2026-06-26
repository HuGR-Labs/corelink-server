---
type: "ADR"
title: "ADR-0036 — D1 schema migration governance"
description: "Why every CoreLink D1 schema change is heavyweight: inline CHECKs, partial UNIQUE indexes, the SQLite 12-step rebuild, and an ADR per breaking change."
source_files:
  - "specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "d1", "schema", "migration", "sqlite", "governance"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0036 — D1 schema migration governance

CoreLink's control-plane state lives in Cloudflare D1, which is SQLite under the hood, and SQLite
deliberately lacks the in-place constraint surgery (`ALTER TABLE ADD/DROP/MODIFY CONSTRAINT`) that
Postgres engineers reach for reflexively. This ADR is the cross-sprint discipline that turns that
limitation into a predictable migration contract instead of a class of surprise CI failures — it is
the governance record every later table-rebuild ADR (e.g. [ADR-0062](/adr/adr-0062-tier-check-widen-rebuild.md))
cites as its parent pattern, and it pins the materialized-`tenant_prefix` idiom that the
[D1 CONFIG_DB](/storage/d1-config-db.md) read paths depend on.

# Context

D1 / SQLite does **not** support `ALTER TABLE ADD CONSTRAINT` or `ALTER TABLE MODIFY COLUMN`; a Lote
10.4bis P0 fix surfaced this as a class issue spanning the S-04/S-05/S-06 sprints, so CHECK
constraints must be declared inline at `CREATE TABLE` time and any later change goes through the
SQLite 12-step rebuild recipe.

# Decision

The ADR adopts five rules across all CoreLink D1 schemas: CHECK constraints are ALWAYS inline in
`CREATE TABLE` (never `ALTER TABLE ADD CONSTRAINT`); state-scoped uniqueness uses partial UNIQUE
indexes (`WHERE status = 'running'`) rather than plain UNIQUE; widening a CHECK's value set requires
a new ADR plus a migration using the 12-step recipe (`CREATE …_new` → `INSERT … SELECT` → `DROP old`
→ `RENAME`); future-attribution columns like `created_by_pat_id` are NULLable rather than carrying
`'__legacy__'` sentinels; and the per-row materialized `tenant_prefix` BLOB(16) is the canonical
pattern so cron-tier read paths never need TDK access. A bare `DROP TABLE` is blocked in prod by the
sprint-contract anti-scope.

# Consequences

Schema evolution is explicit and CI never hits a surprise `ALTER` failure, and cross-sprint
consistency (inline CHECK, partial UNIQUE, materialized prefix) is guaranteed; the cost is that
schema changes are heavyweight — the 12-step recipe discourages casual migrations, and `DROP TABLE`
in prod is barred, forcing a staged dual-write cutover discipline.

# Citations

1. `specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md:25-27` — Context: D1/SQLite does NOT support `ALTER TABLE ADD/MODIFY CONSTRAINT`; CHECKs must be inline + later changes use the 12-step recipe.
2. `specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md:33-77` — Decision: the five rules (inline CHECK, partial UNIQUE, schema-bump-needs-ADR + 12-step recipe, NULLable attribution columns, materialized `tenant_prefix`).
3. `specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md:69-69` — the prod anti-scope: `DROP TABLE` in prod is blocked.
4. `specs/03_architecture/adrs/ADR-0036-d1-schema-migration-governance.md:81-91` — Consequences: documented limits + explicit evolution path vs heavyweight changes and the prod `DROP` bar.
