# Publication preflight consolidated readback — 2026-09-22

Read-only evaluation after the deduplication registry was linked to the
publication ledger. No GitHub API write or retry occurred.

- ledger items: **105**;
- item state: **105 `BLOCKED`**;
- frozen contract: **0/105**;
- body fingerprints: **105/105**;
- issue publication: **0**;
- deduplication registry: **105/105 decisions recorded** — 92 `DISTINCT`,
  4 `REUSE`, 9 `EXPAND`, 0 `UNRESOLVED`;
- per-item publication gates: still **PENDING** for all six gate types.

The registry closes the semantic census and gives safe resume evidence, but the
publication tool intentionally continues to block because the shared standard
is not frozen, contracts are not immutable, independent cold-review gates are
not all approved, and the `REUSE`/`EXPAND` rows still require operator ledger
confirmation. No item is eligible for serial creation or reuse/reconciliation.
