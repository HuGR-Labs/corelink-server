# Backlog/deduplication batch 2 — read-only Luna reconciliation

Snapshot: 265 GitHub issues, open/closed; no ownership title or
`corelink-ownership:v1:manifest=` marker. No issue or file was written.

| Package | Decision | Evidence summary |
|---|---|---|
| `chaos-campaign` | DISTINCT | #1652 is Worker synthetic cron; the manifest is a Rust failure-scenario harness. |
| `corelink-ac-fuzz` | UNRESOLVED | No exact match; broad #1634/#1640/#2067 results do not bind to `hkdf_expand`. |
| `corelink-adapters-cloud` | UNRESOLVED | No exact package/manifest/alias match; #1634 is insufficient. |
| `corelink-adapters-vault` | DISTINCT | #1653 covers `byok-vault-real` activation/KMS evidence, not façade ownership deliverables. |
| `corelink-analytics` | UNRESOLVED | #2075/#1850 are broad hits without a proven Rust-package link. |
| `corelink-audit-chain-fuzz` | DISTINCT | #1647/#1668 concern audit-chain policy/hash and throughput, not the fuzz harness. |
| `corelink-bazel-bridge` | DISTINCT | #2067 requests gRPC REAPI ingress; the package is the existing REST bridge. |
| `corelink-billing-aggregator` | DISTINCT | #1630/#1631 concern runner billing aggregation, not this usage-counter package. |
| `corelink-billing-emit` | UNRESOLVED | #1635 is a broad hit without confirmed linkage to this emitter. |
| `corelink-billing-reconcile` | DISTINCT | #1649 concerns Stripe webhook IDs; this package reconciles usage/ledger events. |
| `corelink-billing-stripe-traits` | UNRESOLVED | No exact name, manifest, alias, or focused match. |
| `corelink-byok-fuzz` | DISTINCT | #1653 is operational KMS activation; this package fuzzes envelope/DEK behavior. |

Result: 7 `DISTINCT`, 5 `UNRESOLVED`, no `REUSE`/`EXPAND`. The five unresolved
rows remain open and are not silently treated as distinct.
