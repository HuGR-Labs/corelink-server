# Backlog/deduplication batch 6 — read-only Luna reconciliation

Source readback: `origin/main` at
`d987926e18f3816dc95ba8a951cdd2248c32c825`; authenticated snapshot: 267
issues, with no ownership title or manifest marker. No write occurred.

| Package | Decision |
|---|---|
| `corelink-tracing` | UNRESOLVED |
| `corelink-transparency-log` | DISTINCT |
| `corelink-wasm` | UNRESOLVED |
| `corelink-worker-fuzz` | UNRESOLVED |
| `e2e-billing-flow` | DISTINCT |
| `e2e-byok-revoke` | DISTINCT |
| `e2e-chaos` | DISTINCT |
| `e2e-dsr` | DISTINCT |
| `e2e-failover-router` | DISTINCT |
| `e2e-replication-failover` | UNRESOLVED |
| `e2e-resilience` | DISTINCT |
| `e2e-signup-flow` | DISTINCT |

Result: 8 `DISTINCT`, 4 `UNRESOLVED`, 0 `REUSE`/`EXPAND`. The four no-hit
rows remain unresolved and are not silently treated as distinct.
