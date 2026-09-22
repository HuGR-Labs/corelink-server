---
type: "ADR"
title: "ADR-0143 — B-071 name-scoped GC accounting trigger replacement"
tags: ["adr", "d1", "gc", "migration", "b-071", "additive"]
---
# ADR-0143 — B-071 name-scoped GC accounting trigger replacement

## Context

Migration 0142 creates the B-071 legal-hold and accounting controls.  Its two
accounting trigger bodies must select the immutable `gc_purge_intent.gc_region`:
`blob_meta.region` is macro residency, while `tenant_storage_state.region` is
the five-region GC partition.  SQLite's `CREATE TRIGGER IF NOT EXISTS` cannot
replace a body installed by an earlier deployment, so the historical body needs
one forward replacement migration.

## Decision

Migration `0143_gc_accounting_region_upgrade.sql` may execute exactly these
ordered, adjacent pairs. Each `DROP` carries the exact
`additive-trigger-replacement: ADR-0143` annotation required by
`scripts/check_migrations_additive.py`, and its immediately following
`CREATE` must recreate that same trigger:

```sql
DROP TRIGGER IF EXISTS trg_gc_purge_accounting_required;
DROP TRIGGER IF EXISTS trg_gc_purge_finalize_accounting;
```

The checker consumes each pair once and constrains the exception by migration
path, statement order, trigger name, and annotation. It rejects every other
trigger drop, renamed or reordered pairs, and a duplicate or unpaired `CREATE`
of either approved trigger, as well as all table, index, column, and other
destructive DDL. The replacement preserves
the legal-hold trigger and uses the same fenced intent predicate, saturated
`bytes_used`, lifetime counter, audit/candidate transition, and idempotent
finalization semantics.

## Verification and rollout

The credentialless hosted D1 and B-071 gates replay both corrected fresh
`0142 → 0143` and historical `0118 → 0143` paths.  The Python additive gate
and Rust idempotent-create property both reject a renamed trigger, a table
drop, a generic annotation, a path change, duplicate unpaired allowed-name
`CREATE`, and any other bare `CREATE`. This
ADR authorizes no production apply; the normal D1 owner-gated migration process
remains required.
