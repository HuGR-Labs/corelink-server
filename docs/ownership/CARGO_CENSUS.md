---
schema: corelink-ownership/1.1
document: cargo_census
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
state: provisional
evidence_set: cargo-census-20260920
---

# Censo de manifests Cargo

Este é o censo de identidade do campaign, não uma aprovação, ledger de
publicação ou prova de reachability. Foi obtido na baseline indicada com:

```sh
git ls-files -- '*Cargo.toml'
cargo metadata --locked --offline --format-version 1 --no-deps
```

O primeiro retornou 107 manifests rastreados; o segundo retornou 95 membros
de workspace. Consultas Cargo aqui só resolveram metadata offline, sem build
ou teste.

| Classificação | Quantidade | Decisão |
|---|---:|---|
| Raiz virtual | 1 | não é package; sem issue |
| Workspace first-party | 95 | elegível provisoriamente; uma issue por package após gates |
| Fuzz independente first-party | 10 | elegível provisoriamente; uma issue por package, separada do pai |
| Arquivo histórico | 1 | não elegível nesta campanha; ver lacuna de contrato |

## Raiz e arquivo

| Manifesto | Classificação | Razão | Decisão |
|---|---|---|---|
| `Cargo.toml` | workspace virtual | contém membros/excludes; sem `[package]` | sem issue |
| `_archive/wi-s11-002-partial/Cargo.toml` | arquivo histórico | caminho `_archive`, fora de `workspace_members` | sem issue; a categoria `archive` precisa entrar no schema congelado |

O manifest arquivado declara `corelink-erasure`, mas não se torna elegível por
ter um `[package]`: não é membro de workspace e está sob `_archive`. A regra
de exclusão está documentada aqui e ainda precisa ser representada pelo
contrato compartilhado antes da certificação global.

## Fuzz independente

| Package | Manifesto | Classificação | Decisão |
|---|---|---|---|
| `corelink-ac-fuzz` | `crates/corelink-ac/fuzz/Cargo.toml` | workspace interno independente | issue própria provisória |
| `corelink-audit-chain-fuzz` | `crates/corelink-audit-chain/fuzz/Cargo.toml` | workspace interno independente | issue própria provisória |
| `corelink-byok-fuzz` | `crates/corelink-byok/fuzz/Cargo.toml` | workspace interno independente | issue própria provisória |
| `corelink-client-verify-fuzz` | `crates/corelink-client-verify/fuzz/Cargo.toml` | workspace interno independente | issue própria provisória |
| `corelink-hash-fuzz` | `crates/corelink-hash/fuzz/Cargo.toml` | workspace interno independente | issue própria provisória |
| `corelink-meta-fuzz` | `crates/corelink-meta/fuzz/Cargo.toml` | workspace interno independente | issue própria provisória |
| `corelink-reapi-fuzz` | `crates/corelink-reapi/fuzz/Cargo.toml` | workspace interno independente | issue própria provisória |
| `corelink-worker-fuzz` | `crates/corelink-worker/fuzz/Cargo.toml` | workspace interno independente | issue própria provisória |
| `corelink-tenant-path-fuzz` | `crates/tenant-path/fuzz/Cargo.toml` | workspace interno independente | issue própria provisória |
| `corelink-cli-fuzz` | `tools/cli/fuzz/Cargo.toml` | workspace interno independente; fora do exclude raiz | issue própria provisória |

## Membros do workspace

Cada linha abaixo é um package first-party distinto, identificado por metadata
Cargo — nunca pelo basename do diretório.

| Package | Manifesto |
|---|---|
| `chaos-campaign` | `tests/chaos/Cargo.toml` |
| `corelink-ac` | `crates/corelink-ac/Cargo.toml` |
| `corelink-adapter-host` | `crates/corelink-adapter-host/Cargo.toml` |
| `corelink-adapters-cloud` | `crates/corelink-adapters-cloud/Cargo.toml` |
| `corelink-adapters-vault` | `crates/corelink-adapters-vault/Cargo.toml` |
| `corelink-analytics` | `crates/corelink-analytics/Cargo.toml` |
| `corelink-audit-chain` | `crates/corelink-audit-chain/Cargo.toml` |
| `corelink-audit` | `crates/corelink-audit/Cargo.toml` |
| `corelink-auth` | `crates/corelink-auth/Cargo.toml` |
| `corelink-bazel-bridge` | `crates/corelink-bazel-bridge/Cargo.toml` |
| `corelink-billing-aggregator` | `crates/corelink-billing-aggregator/Cargo.toml` |
| `corelink-billing-emit` | `crates/corelink-billing-emit/Cargo.toml` |
| `corelink-billing-reconcile` | `crates/corelink-billing-reconcile/Cargo.toml` |
| `corelink-billing-stripe-materializer` | `crates/corelink-billing-stripe-materializer/Cargo.toml` |
| `corelink-billing-stripe-traits` | `crates/corelink-billing-stripe-traits/Cargo.toml` |
| `corelink-billing-stripe` | `crates/corelink-billing-stripe/Cargo.toml` |
| `corelink-billing` | `crates/corelink-billing/Cargo.toml` |
| `corelink-byok` | `crates/corelink-byok/Cargo.toml` |
| `corelink-cas` | `crates/corelink-cas/Cargo.toml` |
| `corelink-cf-bindings` | `crates/corelink-cf-bindings/Cargo.toml` |
| `corelink-chaos-scheduler` | `crates/corelink-chaos-scheduler/Cargo.toml` |
| `corelink-clerk-cf` | `crates/corelink-clerk-cf/Cargo.toml` |
| `corelink-clerk` | `crates/corelink-clerk/Cargo.toml` |
| `corelink-cli` | `tools/cli/Cargo.toml` |
| `corelink-client-verify` | `crates/corelink-client-verify/Cargo.toml` |
| `corelink-config-do` | `crates/corelink-config-do/Cargo.toml` |
| `corelink-core` | `crates/corelink-core/Cargo.toml` |
| `corelink-crypto` | `crates/corelink-crypto/Cargo.toml` |
| `corelink-dpa-acceptance` | `crates/corelink-dpa-acceptance/Cargo.toml` |
| `corelink-dsr-statuspage-scheduler` | `crates/corelink-dsr-statuspage-scheduler/Cargo.toml` |
| `corelink-dsr` | `crates/corelink-dsr/Cargo.toml` |
| `corelink-dt-cli` | `tools/dt-cli/Cargo.toml` |
| `corelink-dt-reconcile` | `tools/dt-reconcile/Cargo.toml` |
| `corelink-dt-webhook` | `crates/corelink-dt-webhook/Cargo.toml` |
| `corelink-dual-approval` | `crates/corelink-dual-approval/Cargo.toml` |
| `corelink-enterprise-inquiry` | `crates/corelink-enterprise-inquiry/Cargo.toml` |
| `corelink-erasure-attestation` | `crates/corelink-erasure-attestation/Cargo.toml` |
| `corelink-eviction` | `crates/corelink-eviction/Cargo.toml` |
| `corelink-failover-router` | `crates/corelink-failover-router/Cargo.toml` |
| `corelink-gc` | `crates/corelink-gc/Cargo.toml` |
| `corelink-go` | `tools/sdks/go/Cargo.toml` |
| `corelink-handler-ac` | `crates/corelink-handler-ac/Cargo.toml` |
| `corelink-handler-admin` | `crates/corelink-handler-admin/Cargo.toml` |
| `corelink-handler-cas-erase` | `crates/corelink-handler-cas-erase/Cargo.toml` |
| `corelink-handler-cas` | `crates/corelink-handler-cas/Cargo.toml` |
| `corelink-handler-customer` | `crates/corelink-handler-customer/Cargo.toml` |
| `corelink-hash` | `crates/corelink-hash/Cargo.toml` |
| `corelink-meta` | `crates/corelink-meta/Cargo.toml` |
| `corelink-openapi` | `tools/openapi/Cargo.toml` |
| `corelink-ops` | `crates/corelink-ops/Cargo.toml` |
| `corelink-pat` | `crates/corelink-pat/Cargo.toml` |
| `corelink-privacy-erasure-worker` | `crates/corelink-privacy-erasure-worker/Cargo.toml` |
| `corelink-privacy-pseudonymize` | `crates/corelink-privacy-pseudonymize/Cargo.toml` |
| `corelink-privacy` | `crates/corelink-privacy/Cargo.toml` |
| `corelink-py` | `tools/sdks/python/Cargo.toml` |
| `corelink-r2-multipart` | `crates/corelink-r2-multipart/Cargo.toml` |
| `corelink-rate-headers` | `crates/corelink-rate-headers/Cargo.toml` |
| `corelink-ratelimit` | `crates/corelink-ratelimit/Cargo.toml` |
| `corelink-reapi` | `crates/corelink-reapi/Cargo.toml` |
| `corelink-region` | `crates/corelink-region/Cargo.toml` |
| `corelink-replica-worker` | `crates/corelink-replica-worker/Cargo.toml` |
| `corelink-replication-coordinator` | `crates/corelink-replication-coordinator/Cargo.toml` |
| `corelink-replication` | `crates/corelink-replication/Cargo.toml` |
| `corelink-rotation-adapters` | `crates/corelink-rotation-adapters/Cargo.toml` |
| `corelink-runbook-tracker` | `crates/corelink-runbook-tracker/Cargo.toml` |
| `corelink-runner-aggregate` | `crates/corelink-runner-aggregate/Cargo.toml` |
| `corelink-runner-overage` | `crates/corelink-runner-overage/Cargo.toml` |
| `corelink-server` | `crates/corelink-container/Cargo.toml` |
| `corelink-signup` | `crates/corelink-signup/Cargo.toml` |
| `corelink-slack-real` | `crates/corelink-slack-real/Cargo.toml` |
| `corelink-slo` | `crates/corelink-slo/Cargo.toml` |
| `corelink-statuspage-real` | `crates/corelink-statuspage-real/Cargo.toml` |
| `corelink-stripe-real` | `crates/corelink-stripe-real/Cargo.toml` |
| `corelink-telemetry` | `crates/corelink-telemetry/Cargo.toml` |
| `corelink-tenant-path` | `crates/tenant-path/Cargo.toml` |
| `corelink-terraform-drift-consumer` | `crates/corelink-terraform-drift-consumer/Cargo.toml` |
| `corelink-tier-selection` | `crates/corelink-tier-selection/Cargo.toml` |
| `corelink-tracing` | `crates/corelink-tracing/Cargo.toml` |
| `corelink-transparency-log` | `crates/corelink-transparency-log/Cargo.toml` |
| `corelink-turbo-bridge` | `crates/corelink-turbo-bridge/Cargo.toml` |
| `corelink-wasm` | `crates/corelink-wasm/Cargo.toml` |
| `corelink-worker` | `crates/corelink-worker/Cargo.toml` |
| `e2e-billing-flow` | `tests/e2e-billing-flow/Cargo.toml` |
| `e2e-byok-revoke` | `tests/e2e-byok-revoke/Cargo.toml` |
| `e2e-chaos` | `tests/e2e-chaos/Cargo.toml` |
| `e2e-dsr` | `tests/e2e-dsr/Cargo.toml` |
| `e2e-failover-router` | `tests/e2e-failover-router/Cargo.toml` |
| `e2e-pilot-onboarding` | `tests/e2e-pilot-onboarding/Cargo.toml` |
| `e2e-replication-failover` | `tests/e2e-replication-failover/Cargo.toml` |
| `e2e-resilience` | `tests/e2e-resilience/Cargo.toml` |
| `e2e-signup-flow` | `tests/e2e-signup-flow/Cargo.toml` |
| `e2e-tenant-isolation` | `tests/e2e-tenant-isolation/Cargo.toml` |
| `e2e-user-journeys` | `tests/e2e-user-journeys/Cargo.toml` |
| `migrate-single-to-multi-region` | `apps/migrate-single-to-multi-region/Cargo.toml` |
| `sbom-publish` | `tools/sbom-publish/Cargo.toml` |

## Limites e próximo gate

O resultado provisório é 105 packages first-party elegíveis para emissão: 95
workspace + 10 fuzz. Essa contagem não libera publicação. Antes disso, o
standard congelado precisa modelar a classificação `archive`, o census deve
entrar em registry/anti-drift, e cada package precisa passar preflight,
deduplicação e gates de artefatos/review.
