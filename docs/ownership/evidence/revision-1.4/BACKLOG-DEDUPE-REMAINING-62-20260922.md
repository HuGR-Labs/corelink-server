# Remaining backlog/alias reconciliation — 2026-09-22

After the 23 direct GitHub issue decisions and 20 contextual backlog
decisions, **62 packages remain unresolved**. The reproducible pass searched
the current 262-issue snapshot and `BACKLOG.md@3445b217` using package name,
manifest path, manifest directory and canonical skill slug. It found no direct
or expanded exact token for the packages below.

No-hit is not treated as `distinct`: aliases, historical names and semantic
intent still need a package-by-package decision. The owner is the campaign
deduplication reviewer; required next evidence is an alias/backlog/issue-intent
readback per package.

```text
chaos-campaign
corelink-ac-fuzz
corelink-adapters-cloud
corelink-adapters-vault
corelink-analytics
corelink-audit-chain-fuzz
corelink-bazel-bridge
corelink-billing-aggregator
corelink-billing-emit
corelink-billing-reconcile
corelink-billing-stripe-traits
corelink-byok-fuzz
corelink-cf-bindings
corelink-chaos-scheduler
corelink-cli-fuzz
corelink-client-verify-fuzz
corelink-config-do
corelink-crypto
corelink-dpa-acceptance
corelink-dsr
corelink-dsr-statuspage-scheduler
corelink-dt-cli
corelink-dt-reconcile
corelink-dt-webhook
corelink-erasure-attestation
corelink-eviction
corelink-go
corelink-handler-ac
corelink-handler-admin
corelink-handler-cas-erase
corelink-hash-fuzz
corelink-meta
corelink-meta-fuzz
corelink-py
corelink-r2-multipart
corelink-rate-headers
corelink-reapi
corelink-reapi-fuzz
corelink-replica-worker
corelink-replication
corelink-replication-coordinator
corelink-rotation-adapters
corelink-statuspage-real
corelink-telemetry
corelink-tenant-path
corelink-tenant-path-fuzz
corelink-terraform-drift-consumer
corelink-tier-selection
corelink-tracing
corelink-transparency-log
corelink-wasm
corelink-worker-fuzz
e2e-billing-flow
e2e-byok-revoke
e2e-chaos
e2e-dsr
e2e-failover-router
e2e-replication-failover
e2e-resilience
e2e-signup-flow
migrate-single-to-multi-region
sbom-publish
```

This closes the accounting of the unresolved surface, not the backlog gate.
