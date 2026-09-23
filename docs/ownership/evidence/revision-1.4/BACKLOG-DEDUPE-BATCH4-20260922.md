# Backlog/deduplication batch 4 — read-only Luna reconciliation

Source readback: `origin/main` at
`d987926e18f3816dc95ba8a951cdd2248c32c825`; `BACKLOG.md` blob
`1d2dc27c523164b85c07a71aa9c4e1be3adffb04`. The authenticated snapshot had
267 issues, with no ownership title or manifest marker. No write occurred.

| Package | Decision |
|---|---|
| `corelink-erasure-attestation` | DISTINCT |
| `corelink-eviction` | DISTINCT |
| `corelink-go` | UNRESOLVED |
| `corelink-handler-ac` | DISTINCT |
| `corelink-handler-admin` | DISTINCT |
| `corelink-handler-cas-erase` | UNRESOLVED |
| `corelink-hash-fuzz` | UNRESOLVED |
| `corelink-meta` | DISTINCT |
| `corelink-meta-fuzz` | UNRESOLVED |
| `corelink-py` | UNRESOLVED |
| `corelink-r2-multipart` | DISTINCT |
| `corelink-rate-headers` | DISTINCT |

Result: 7 `DISTINCT`, 5 `UNRESOLVED`, 0 `REUSE`/`EXPAND`. The five no-hit
rows remain unresolved and are not silently treated as distinct.
