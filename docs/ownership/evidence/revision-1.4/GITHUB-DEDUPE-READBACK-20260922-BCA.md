# GitHub dedup readback — `bcaaceb` era (2026-09-22)

The latest authenticated read-only snapshot returned **267 issues** across open
and closed states. It contained zero ownership titles and zero
`corelink-ownership:v1:manifest=` markers. The canonical owner resolved by the
connector was `HuGR-dev/corelink-server`; no issue or repository write was
performed.

The second 12-package batch was rechecked against this snapshot. Its result
remains 7 `DISTINCT` and 5 `UNRESOLVED` (`corelink-ac-fuzz`,
`corelink-adapters-cloud`, `corelink-analytics`, `corelink-billing-emit`, and
`corelink-billing-stripe-traits`). The five no-hit rows remain unresolved; no
absence of an exact title was promoted to `DISTINCT`.

This readback supersedes the batch's 265-issue count only for snapshot
freshness. It does not close the remaining 55-row semantic dedup gate.
