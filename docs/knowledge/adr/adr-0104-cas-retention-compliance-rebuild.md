---
type: "ADR"
title: "ADR-0104 — Widen cas retention through a one-way SQLite rebuild"
source_files:
  - "migrations/d1/0102_cas_retention.sql"
  - "migrations/d1/0144_cas_retention_compliance_metadata.sql"
checkpoint_sha: "cec5b3a95cb69da29c0daaed740f09af2cb49230"
tags: ["adr", "d1", "sqlite", "retention", "compliance", "b-046"]
---

# ADR-0104 — Widen cas retention through a one-way SQLite rebuild

`0102_cas_retention` has an inline mode check that permits only Governance.
SQLite and D1 cannot widen that check in place. Migration `0144` creates a
replacement table, copies existing rows with their original primary key,
replaces the deployed table, and recreates the tenant-leftmost indexes.

Compliance rows require provider account, archive target, exact object version,
evidence reference, and a concrete retention date. Governance rows must have
none of that metadata. The R2 legal-hold adapter queries only Governance rows,
so Compliance records cannot enter a code-reversible drain or release path.

The numbered migration is applied once by the migration ledger. Application
replays remain idempotent through the unchanged Governance `INSERT OR IGNORE`
writer. A rollback after any Compliance row exists would risk losing immutable
state; retain the widened schema and perform a forward repair instead.
