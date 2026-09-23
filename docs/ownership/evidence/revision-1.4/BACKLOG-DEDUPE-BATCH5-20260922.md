# Backlog/deduplication batch 5 — read-only Luna reconciliation

Source readback: `origin/main` at
`2185ae9ad26b555319bf62b635f6ca1faa31c9fb`; `BACKLOG.md` was read from that
remote object because the campaign branch differs. No issue or repository
write was performed.

| Package | Decision |
|---|---|
| `corelink-reapi` | EXPAND #2067/#2047 for authenticated gRPC ingress |
| `corelink-reapi-fuzz` | DISTINCT |
| `corelink-replica-worker` | DISTINCT |
| `corelink-replication` | DISTINCT |
| `corelink-replication-coordinator` | DISTINCT |
| `corelink-rotation-adapters` | EXPAND B-054/#1647 and child #1794 only for audit-chain custody/rotation overlap |
| `corelink-statuspage-real` | DISTINCT |
| `corelink-telemetry` | EXPAND B-129/#1671 for Server-Timing overlap |
| `corelink-tenant-path` | REUSE B-171/B-188/B-192 for covered tenant-prefix invariants |
| `corelink-tenant-path-fuzz` | DISTINCT |
| `corelink-terraform-drift-consumer` | EXPAND #1721/#1722 for workflow-state/evidence overlap |
| `corelink-tier-selection` | REUSE B-076/B-079/B-218/B-275 |

Result: 6 `DISTINCT`, 4 `EXPAND`, 2 `REUSE`, 0 new `UNRESOLVED`. Batch 4
immediately before this one left 36 undecided rows (seven distinct and five
unresolved within its 12-row input); this batch leaves 24 rows needing
semantic review.
`EXPAND` and `REUSE` remain ledger decisions to confirm, not publication
authorization.
