---
schema: corelink-ownership/1.1
document: reference
package: corelink-replica-worker
manifest: crates/corelink-replica-worker/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: replica-worker-static-20260920
---

# corelink-replica-worker — referência de ownership

Referência SOURCE estática fixada no commit indicado. Ela descreve interfaces,
constantes e fakes em memória; não demonstra worker/cron executado, replicação,
R2, D1, Prometheus, provider, auditoria entregue, deploy ou produção. OKF
canônico verificado permanece apenas referência externa, sem cópia ou revalidação.

[Identidade](#r01) · [Fronteiras](#r02) · [Mapa](#r03) · [Contratos](#r04) · [Axiomas](#r05) · [SLI](#r06) · [Falhas](#r07) · [Lacunas](#r08).

<a id="r01"></a>
## R01 — Identidade e função

| Campo | Evidência SOURCE |
|---|---|
| Package / manifesto | `corelink-replica-worker` / `crates/corelink-replica-worker/Cargo.toml` |
| Papel observado | Tipos de hot blob, agregação offline, contrato de réplica, residência, audit e SLI/fakes locais. |
| Fonte lida | `src/{lib,aggregator,audit,cardinality,coverage,error,hot_blob,metrics,region,replication}.rs`; testes declarados foram somente localizados. |
| Dependências declaradas | `thiserror`, `serde`; `proptest` e `corelink-slo` são dev-dependencies. |
| Runtime observado | Nenhum: o manifesto não declara runtime/provider de worker, R2, D1 ou Prometheus. |

<a id="r02"></a>
## R02 — Fronteiras: SOURCE não é runtime

| Superfície SOURCE | Esta crate declara | Limite falsificável |
|---|---|---|
| Agregação | `OfflineAggregator`, `InMemoryOfflineAggregator`, entradas/resultados e `Mutex` local | Não consulta audit log, D1 ou cron observáveis neste recorte |
| Réplica | `ReplicationWorker`, `InMemoryReplicationWorker`, `FailingReplicationWorker` e `HashMap` em memória | Não demonstra worker executado, R2 GET/PUT ou retry temporal real |
| Residência | `Region`, `TenantTier` e `ResidencyGraph` estáticos | Não aprova topologia, jurisdição ou transferência real |
| Auditoria | `ReplicaAuditSink`, record, taxonomia e sinks em memória/falha | Não demonstra delivery, retenção ou backend de audit |
| Observabilidade | traits `ReplicationLagSli`/`HotBlobCoverageSli`, métricas e fixtures | Não demonstra endpoint Prometheus, scrape, dashboard ou alerta |

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo | Responsabilidade observada |
|---|---|
| `lib.rs` | Expõe módulos, traits, constantes, tipos e fakes. |
| `aggregator.rs` | Agrupa entradas em memória por `(tenant_id, blob_hash)`, escolhe top 1% e mantém snapshot local. |
| `replication.rs` | Valida residência, registra audit, copia bytes no `HashMap` local, compara hashes e expõe trait/fixture. |
| `region.rs` | Quatro regiões/tier canônicos e mapa estático de sibling. |
| `hot_blob.rs` | Linha/tipos `HotBlob`, `AggregationEntry` e `ReplicaStatus`. |
| `audit.rs`, `error.rs` | Taxonomia de eventos, sink, fakes e `ReplicaError`. |
| `metrics.rs`, `coverage.rs`, `cardinality.rs` | Contratos SLI, observações/falhas em memória, nomes/limites e guard de label. |

<a id="r04"></a>
## R04 — Contratos públicos observados

| Contrato | Entrada e resultado | Limite falsificável |
|---|---|---|
| `OfflineAggregator::{run,hot_blobs}` | Janela em dias retorna `AggregationResult`/`ReplicaError` e snapshot de `HotBlob` | A implementação fornecida só lê/muta estado em memória |
| `ReplicationWorker::replicate_batch` | Slice de `HotBlob` retorna contagem ou `ReplicaError` | A interface não recebe binding de provider nem agenda trabalho |
| `ResidencyGraph::{sibling,is_allowed,is_acyclic}` | Regiões canônicas retornam sibling, `Result` ou `bool` | O grafo não prova política aplicada fora do processo |
| `ReplicaAuditSink::emit` | `ReplicaAuditRecord` retorna `Result<(), String>` | Sink não define transporte, persistência ou entrega |
| `ReplicationLagSli::{emit_lag,emit_batch_outcome}` | Domínio/regiões/outcome/tempo retornam `Result<(), String>` | Trait/fixture não publica métrica real |
| `HotBlobCoverageSli::emit_coverage` | Região, numerador, denominador e tempo retornam `Result<(), String>` | O ratio/fake não prova cobertura observada |

<a id="r05"></a>
## R05 — Cinco axiomas SOURCE falsificáveis

| Axioma | Predicado falsificável | Evidência SOURCE | Não prova |
|---|---|---|---|
| A1 — sibling fechado | Para cada `Region::ALL`, `sibling` retorna o par WNAM↔ENAM ou WEUR↔SAM; qualquer outro par faz `is_allowed` retornar erro | `region.rs` | Transferência/regra de residência aplicada em runtime |
| A2 — agregação inicia antes de mutar | Em `InMemoryOfflineAggregator::run`, falha de `AggregationStarted` retorna `ReplicaError::Audit` antes do bloco que atribui `s.hot_blobs` | `aggregator.rs` | Atomicidade ou escrita D1/audit externa |
| A3 — réplica rejeita antes da cópia local | Em `replicate_one`, `is_allowed` é chamado antes de `ReplicationStarted` e antes de inserir no store local | `replication.rs` | Cópia R2 ou worker real bloqueado |
| A4 — SLI é best-effort após audit de conclusão | Após `ReplicationCompleted` com sucesso, erro de `emit_lag` é tratado localmente e o caminho retorna `Ok(())`; ele não é propagado como falha de réplica | `replication.rs`, `metrics.rs` | Emissão, scrape ou entrega de métrica |
| A5 — guardas constantes | `NO_TENANT_ID_LABEL == true` e `LIVE_METRIC_LABEL_CARDINALITY == 16` | `cardinality.rs` | Schema de labels, séries reais, orçamento usado ou ausência de labels em outros packages |

<a id="r06"></a>
## R06 — SLI, fake e constantes declaradas

| Item | Leitura SOURCE | Limite |
|---|---|---|
| Lag | `corelink_replication_lag_seconds`, `corelink_replication_batch_total`, cinco domínios e buckets `[5,10,30,60,120,300]` | Strings/arrays não são histogramas ou séries emitidas |
| Cobertura | `corelink_hot_blob_replication_coverage_ratio`, target `0.90`, deadline `24*3600`; janela vazia calcula `1.0` | Não mede hot blobs ou cobertura real |
| Fakes SLI | `InMemoryReplicationLagSli`, `FailingReplicationLagSli`, `InMemoryHotBlobCoverageSli`, `FailingHotBlobCoverageSli` | Fixture não é adapter Prometheus/provider |
| Agregação | `AGGREGATION_WINDOW_DAYS = 30`, `TOP_1_PCT_THRESHOLD = 0.01` | Constantes não configuram scheduler nem consulta real |
| Lag SLO | `REPLICATION_LAG_P99_SLO_SECS = 60` | Valor declarado não demonstra p99 sustentado |

<a id="r07"></a>
## R07 — Falhas e recuperação semântica

| Condição SOURCE | Resultado local | Limite |
|---|---|---|
| Sibling proibido | `ReplicaError::ResidencyViolation` retorna da réplica | Não prova bloqueio de tráfego/provider |
| Falha de audit inicial | `ReplicaError::Audit` antes da mutação local indicada nos axiomas | Não confirma delivery ou recuperação externa |
| Hash local não coincide | Loop tenta até `MAX_REPLICATION_RETRIES = 5`, emite `ReplicationFailed` e retorna `RetryExhausted` se o emit permitir | Não demonstra backoff temporal, alerta SEV-2 ou R2 |
| Falha de SLI | Caminho de sucesso de réplica não a propaga; tentativa de audit do detalhe ignora erro | Não prova observabilidade entregue |
| Batch sem sucesso não-residência | `replicate_batch` pode retornar contagem parcial/zero e tentar observação de outcome | Não demonstra classificação de batch em backend real |

<a id="r08"></a>
## R08 — Evidência e desconhecidos explícitos

Evidência disponível: manifesto e os dez módulos SOURCE listados em R01, no
baseline `6be030999de1f0e0fe62d3a9abb04ec2a4fefde6`; os cinco arquivos de teste
foram apenas identificados, não executados. Desconhecidos: todos os consumers
fora do censo estático, worker/cron, R2, D1, audit backend, Prometheus/scrape,
SLI/SLO medido, provider, retry/backoff temporal, alertas, tráfego, configuração,
credenciais, deploy e produção. Ausência nesta referência não prova ausência no
sistema; SOURCE não é runtime.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01)
