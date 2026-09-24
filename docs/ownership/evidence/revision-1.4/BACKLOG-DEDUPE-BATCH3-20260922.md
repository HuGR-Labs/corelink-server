# Backlog/deduplication batch 3 — read-only Luna reconciliation

Source readback: `origin/main` at
`d141f11d678495b749c21ceec56fe773152f0bf5`; the batch inspected the `main`
version of `BACKLOG.md`, because the campaign branch has a different copy.
No issue or repository write was performed.

| Package | Decision |
|---|---|
| `corelink-cf-bindings` | DISTINCT |
| `corelink-chaos-scheduler` | EXPAND into B-072/#1652 only for the drill/handoff path |
| `corelink-cli-fuzz` | DISTINCT |
| `corelink-client-verify-fuzz` | DISTINCT |
| `corelink-config-do` | DISTINCT |
| `corelink-crypto` | DISTINCT |
| `corelink-dpa-acceptance` | REUSE B-248 |
| `corelink-dsr` | EXPAND B-216/#1678 or #2092 when the candidate matches those paths; otherwise split |
| `corelink-dsr-statuspage-scheduler` | DISTINCT |
| `corelink-dt-cli` | DISTINCT |
| `corelink-dt-reconcile` | DISTINCT |
| `corelink-dt-webhook` | REUSE B-249 |

Result: 8 `DISTINCT`, 2 `EXPAND`, 2 `REUSE`, 0 new `UNRESOLVED`. Applied to
the prior 55 unresolved rows, this leaves 43 rows requiring semantic review.
`EXPAND` and `REUSE` still require package-level confirmation in the
publication ledger; they are not silently treated as publication approval.
