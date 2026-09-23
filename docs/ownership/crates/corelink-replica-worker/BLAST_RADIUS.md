---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-replica-worker
manifest: crates/corelink-replica-worker/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: replica-worker-static-20260920
---

# corelink-replica-worker — blast radius

Cada relação é atômica e derivada de manifesto ou SOURCE estática. Uma seta
descreve import, reexport, teste declarado ou adjacência textual encontrada; ela
não demonstra replicação, worker, provider, runtime ou produção. OKF canônico
não é redefinido nem revalidado.

[Superfície](#b01) · [Fluxo local](#b02) · [Failover](#b03) · [Coordenação](#b04) · [Fachada/testes](#b05) · [Lacunas](#b06).

<a id="b01"></a>
## B01 — Superfície própria

`src/lib.rs` publica tipos/traits de agregação, audit, cobertura, SLI, região e
réplica. Impacto: mudar qualquer símbolo reexportado pode alterar callers Rust
estáticos. Evidência: `src/lib.rs`. Pare antes de afirmar que todos os callers
foram enumerados ou que algum caller executa a API.

<a id="b02"></a>
## B02 — Agregação para réplica local

`InMemoryOfflineAggregator::hot_blobs` entrega `Vec<HotBlob>` e
`ReplicationWorker::replicate_batch` recebe `&[HotBlob]`; o exemplo e a fonte
conectam os contratos dentro desta crate. Impacto: formato de `HotBlob`, status,
janela, top-1% ou regiões afeta essa compatibilidade local. Evidência:
`aggregator.rs`, `hot_blob.rs`, `replication.rs`. Pare: não há D1/audit log,
worker ou cópia de provider observados.

<a id="b03"></a>
## B03 — Failover router para região/audit

`corelink-failover-router` declara dependência direta e reexporta `Region`,
`ResidencyGraph` e tipos de audit deste package. Impacto: mudar esses tipos ou
semântica do sibling pode quebrar o router em fonte. Evidência:
`crates/corelink-failover-router/Cargo.toml`, `src/{lib,audit,router}.rs` desse
consumer e manifesto desta crate. Pare: isso não prova failover, rota HTTP ou
auditoria em execução.

<a id="b04"></a>
## B04 — Replication coordinator e ops para constantes/tipos

`corelink-replication-coordinator` declara dependência direta, reexporta
`Region` e referencia o teto de lag; `corelink-ops` declara dependência direta.
Impacto: alteração de `Region`, constante/metric name ou superfície pública pede
coordenação com esses consumers. Evidência: respectivos `Cargo.toml`,
`corelink-replication-coordinator/src/{lib,lag}.rs`. Pare: referências em docs
ou comentários não demonstram heartbeat, operação ou SLO real.

<a id="b05"></a>
## B05 — Fachada replication e testes E2E

`corelink-replication` declara dependência e `src/replica.rs` reexporta toda a
API; `tests/e2e-replication-failover` e `tests/e2e-failover-router` declaram a
crate como dependência. Impacto: qualquer breaking change público propaga à
fachada e aos harnesses fonte. Evidência: seus manifestos e `replica.rs`. Pare:
testes não foram executados, e uma fachadura/reexport não prova cenário E2E.

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

Conhecido: a raiz do workspace, failover-router, replication-coordinator, ops,
replication e dois manifestos E2E foram encontrados pela busca estática do nome
do package/crate. Desconhecido: censo reverso completo, consumidores externos,
compatibilidade persistida, bindings de worker, R2/D1/audit/Prometheus, provider,
cron, alertas, tráfego, deploy e produção. A ausência de uma relação no censo
não prova que ela inexiste; escale ao owner do consumer/provider antes de inferir.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
