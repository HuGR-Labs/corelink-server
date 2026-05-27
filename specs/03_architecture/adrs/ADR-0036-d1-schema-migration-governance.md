---
id: "ADR-0036"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
title: "D1 Schema Migration Governance (CHECK constraints + 12-step recipe + ADR per breaking change)"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner)", "Architect (TBD)"]
context_links:
  - "specs/04_sprints/_sealed/S04/work_items/WI-S04-002-d1-ac-meta-r2-bucket.md"
  - "specs/04_sprints/_sealed/S05/work_items/WI-S05-006-sweeper-rb-fm-060-prr-ship-gate.md"
  - "specs/04_sprints/_sealed/S06/work_items/WI-S06-001-worker-gc-binary-scheduler-degrade-mode.md"
tags: ["adr", "d1", "schema", "migration", "sqlite", "governance"]
---

# ADR-0036 — D1 Schema Migration Governance

## Context

Cloudflare D1 is built on SQLite. Cloudflare D1 / SQLite **does NOT support** `ALTER TABLE ADD CONSTRAINT` or `ALTER TABLE MODIFY COLUMN`. Lote 10.4bis P0 fix introduced this constraint as a class issue across S-04/S-05/S-06 sprints. Migrations must use CHECK constraints inline in `CREATE TABLE` from the start; subsequent constraint changes require the SQLite 12-step recipe (CREATE new + INSERT SELECT + DROP old + RENAME).

## Decision

Adopt the following migration discipline across all CoreLink D1 schemas:

### Rule 1 — CHECK constraints ALWAYS inline in CREATE TABLE

```sql
CREATE TABLE ac_meta (
  ...
  sig_alg TEXT NOT NULL DEFAULT 'hkdf-sha256',
  CHECK (sig_alg IN ('hkdf-sha256'))    -- inline, NEVER ALTER ADD CONSTRAINT
);
```

NEVER use:
```sql
ALTER TABLE ac_meta ADD CONSTRAINT chk_sig_alg CHECK (...);  -- D1 does NOT support
```

### Rule 2 — Partial UNIQUE indexes for state-scoped uniqueness

```sql
CREATE UNIQUE INDEX uq_gc_run_running
  ON gc_run (tenant_id, region)
  WHERE status = 'running';   -- Lote 10.5bis lesson: state-scoped via partial UNIQUE
```

NOT plain UNIQUE constraints that would block legitimate concurrent records in non-running states.

### Rule 3 — Schema bump = new ADR (when CHECK contents change)

Adding `sig_alg = 'hkdf-blake3'` to the IN list requires:
1. New ADR (e.g., ADR-0036b) documenting the addition + rationale + Crypto SME signoff.
2. New migration `00X_ac_meta_v2.sql` using SQLite 12-step recipe:
   - `CREATE TABLE ac_meta_new (..., CHECK (sig_alg IN ('hkdf-sha256', 'hkdf-blake3')));`
   - `INSERT INTO ac_meta_new SELECT * FROM ac_meta;`
   - `DROP TABLE ac_meta;`  -- ⚠️ **In production this is BLOCKED by anti-scope**
   - `ALTER TABLE ac_meta_new RENAME TO ac_meta;`
3. Production deploy: dual-write window (write to both old + new) → cutover → drop old after 7d retention.

**Anti-scope (sprint contract §10)**: `❌ DROP TABLE in prod` — SQLite 12-step migration requires careful staging deployment with feature flags + dual-write window; NOT a casual `DROP`.

### Rule 4 — `created_by_pat_id` and similar future-attribution columns are NULLable

Use `TEXT NULL` (or `BLOB NULL`); `'__legacy__'` sentinels are anti-pattern. NULL means "no PAT was used" or "pre-PAT-tracking row"; subsequent rows populate the column.

### Rule 5 — `tenant_prefix` materialized BLOB(16) — CANONICAL pattern

Per-row materialized `tenant_prefix` (HMAC-derived at INSERT time; persisted) — eliminates cron-tier need for TDK access during read paths. Lote 10.4bis lesson canonical across S-04/S-05/S-06.

## Consequences

### Positive
- D1 limitations documented; no surprise ALTER failures in CI.
- Schema evolution path explicit (12-step recipe + ADR per change).
- Cross-sprint consistency (CHECK inline; partial UNIQUE; tenant_prefix materialized).

### Negative
- Schema changes are heavyweight (12-step recipe); discourages frequent migration.
- `DROP TABLE` blocked in prod (anti-scope); requires staging-cutover discipline.

### Neutral
- Annex A (forward) enumerates all D1 schemas + their CHECK / partial UNIQUE constraints + the migration history.

## References

- SQLite documentation: ALTER TABLE limitations.
- WI-S04-002, WI-S05-006, WI-S06-001 (canonical schemas).
- Sprint contracts §10 (anti-scope DROP TABLE).

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.4-tris P1-R5-016 ADR-creation) | ADR file created; 5 rules; cross-ref to WI-S04-002 + WI-S05-006 + WI-S06-001 schemas. |
