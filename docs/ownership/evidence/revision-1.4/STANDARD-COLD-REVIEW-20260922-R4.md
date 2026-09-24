# Cold rereview do `STANDARD.md` — R4 (2026-09-22)

Veredito: **BLOCKED**. Fresh read-only review of the current candidate after
the deduplication registry was linked to the publication ledger.

- session: `/root/luna_cold_billing_current`;
- `STANDARD.md` SHA-256: `e9b9c8ae77d3f12eff6955a466aa62453051a1217243db23dd68bc7a616a74d8`;
- worktree: clean;
- no publication, runtime operation or repository write.

## Gates

- **G0 — documentary sub-blockers resolved:** the billing census defines and
  lists 49 qualifying modules; the CF correction records atomic relations; the
  five-pilot calibration is present. This closes only the documentary
  interpretation gap, not operational approval.
- **G1 — BLOCKED:** the current candidate still has no freeze or unblocked
  cold-review verdict.
- **G2 — partially resolved, overall BLOCKED:** the ledger now links 105
  dedup decisions (92 `DISTINCT`, 4 `REUSE`, 9 `EXPAND`, 0 unresolved), but all
  105 items remain `BLOCKED`, all six per-item gates are `PENDING`, contracts
  are unfrozen, and `REUSE`/`EXPAND` require confirmation.
- **G3 — BLOCKED:** no four-current-`APPROVE` set exists for each of the five
  pilots; source reconciliation, owner authority and execution evidence remain
  incomplete.

The R4 result narrows the standard blockers but does not authorize freeze or
issue publication.
