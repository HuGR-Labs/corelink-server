# Ownership rollout — fila provisória por package

Esta é a fila operacional dos 105 packages elegíveis provisoriamente do
censo. Cada nome recebe autoria por um agente distinto; a concorrência é
limitada a seis por onda para preservar worktrees, revisão fria e integração
determinística. A ordem não congela elegibilidade, perfil S/H, aprovação ou
publicação.

Os cinco pilotos (`corelink-hash`, `corelink-billing`, `corelink-server`,
`corelink-cf-bindings`, `e2e-billing-flow`) seguem trilha de reconciliação e
revisão, não uma segunda autoria cega.

| Onda | Packages de autoria individual |
|---|---|
| 001 | `corelink-ac`; `corelink-adapter-host`; `corelink-analytics`; `corelink-audit`; `corelink-audit-chain`; `corelink-bazel-bridge` |
| 002 | `corelink-auth`; `corelink-core`; `corelink-crypto`; `corelink-dpa-acceptance`; `corelink-dsr`; `corelink-pat` |
| 003 | `corelink-billing-aggregator`; `corelink-billing-emit`; `corelink-billing-reconcile`; `corelink-billing-stripe`; `corelink-billing-stripe-materializer`; `corelink-billing-stripe-traits` |
| 004 | `corelink-handler-ac`; `corelink-handler-admin`; `corelink-handler-cas`; `corelink-handler-cas-erase`; `corelink-handler-customer`; `corelink-cas` |
| 005 | `corelink-adapters-cloud`; `corelink-adapters-vault`; `corelink-clerk`; `corelink-clerk-cf`; `corelink-config-do`; `corelink-wasm` |
| 006 | `corelink-byok`; `corelink-erasure-attestation`; `corelink-privacy`; `corelink-privacy-erasure-worker`; `corelink-privacy-pseudonymize`; `corelink-dual-approval` |
| 007 | `corelink-eviction`; `corelink-failover-router`; `corelink-gc`; `corelink-r2-multipart`; `corelink-region`; `corelink-tenant-path` |
| 008 | `corelink-rate-headers`; `corelink-ratelimit`; `corelink-reapi`; `corelink-slo`; `corelink-telemetry`; `corelink-tracing` |
| 009 | `corelink-meta`; `corelink-ops`; `corelink-replica-worker`; `corelink-replication`; `corelink-replication-coordinator`; `corelink-rotation-adapters` |
| 010 | `corelink-runbook-tracker`; `corelink-runner-aggregate`; `corelink-runner-overage`; `corelink-signup`; `corelink-slack-real`; `corelink-statuspage-real` |
| 011 | `corelink-stripe-real`; `corelink-tier-selection`; `corelink-transparency-log`; `corelink-turbo-bridge`; `corelink-worker`; `corelink-client-verify` |
| 012 | `corelink-chaos-scheduler`; `corelink-dsr-statuspage-scheduler`; `corelink-enterprise-inquiry`; `corelink-terraform-drift-consumer`; `corelink-go`; `corelink-py` |
| 013 | `corelink-cli`; `corelink-dt-cli`; `corelink-dt-reconcile`; `corelink-dt-webhook`; `corelink-openapi`; `sbom-publish` |
| 014 | `chaos-campaign`; `e2e-byok-revoke`; `e2e-chaos`; `e2e-dsr`; `e2e-failover-router`; `e2e-pilot-onboarding` |
| 015 | `e2e-replication-failover`; `e2e-resilience`; `e2e-signup-flow`; `e2e-tenant-isolation`; `e2e-user-journeys`; `migrate-single-to-multi-region` |
| 016 | `corelink-ac-fuzz`; `corelink-audit-chain-fuzz`; `corelink-byok-fuzz`; `corelink-client-verify-fuzz`; `corelink-hash-fuzz`; `corelink-meta-fuzz` |
| 017 | `corelink-reapi-fuzz`; `corelink-worker-fuzz`; `corelink-tenant-path-fuzz`; `corelink-cli-fuzz` |

## Gate between waves

An authoring slot is released only after the lead verifies baseline and scope,
reproduces documentary checks, captures a separate cold review with four
artifact verdicts, and either integrates the package or sends a bounded fix.
Shared standard, registry, anti-drift index, deduplication and publication are
lead-owned sequential work after package artifacts are ready.
