# Backlog/deduplication batch 7 — read-only Luna reconciliation

Source readback: `origin/main` at
`d987926e18f3816dc95ba8a951cdd2248c32c825`; authenticated snapshot: 267
issues, with no ownership title or manifest marker. No write occurred.

| Package | Decision |
|---|---|
| `migrate-single-to-multi-region` | EXPAND #1654/B-086 |
| `sbom-publish` | DISTINCT |
| `corelink-ac-fuzz` | DISTINCT |
| `corelink-adapters-cloud` | DISTINCT |
| `corelink-analytics` | DISTINCT |
| `corelink-billing-emit` | EXPAND #1635 |
| `corelink-billing-stripe-traits` | EXPAND #1649/B-065 |
| `corelink-go` | DISTINCT |
| `corelink-handler-cas-erase` | DISTINCT |
| `corelink-hash-fuzz` | DISTINCT |
| `corelink-meta-fuzz` | DISTINCT |
| `corelink-py` | DISTINCT |

Result: 9 `DISTINCT`, 3 `EXPAND`, 0 `REUSE`/`UNRESOLVED`. All rows have a
bounded semantic decision; `EXPAND` still requires ledger confirmation.
