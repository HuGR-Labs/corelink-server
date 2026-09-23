---
name: own-corelink-replica-worker
description: >-
  Route source-backed changes for CoreLink hot-blob classification, replica
  worker contracts, residency, audit, and SLI fixtures without asserting a
  running worker, replication, provider operation, or production evidence.
metadata:
  schema: "corelink-ownership/1.3"
  package: "corelink-replica-worker"
  manifest: "crates/corelink-replica-worker/Cargo.toml"
  source-commit: "6be030999de1f0e0fe62d3a9abb04ec2a4fefde6"
  evidence-set: "replica-worker-static-20260920"
---

# Ownership — corelink-replica-worker

SOURCE estática somente: contratos, tipos e fakes locais não certificam um worker,
cron, R2, D1, Prometheus, auditoria entregue, replicação, provider, deploy ou
produção. OKF canônico verificado é referência externa; não é copiado,
redefinido nem revalidado aqui.

[Acionamento](#s01) · [Fronteira](#s02) · [Triagem](#s03) · [Invariantes](#s04) · [Fluxo](#s05) · [Parada](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Condição | Decisão | Evidência SOURCE | Pare quando |
|---|---|---|---|
| Muda tipo, trait, fake, constante ou reexport do package | Assumir a alteração de contrato e abrir R04–R06 | `src/lib.rs` e módulos públicos | For necessário provar que um caller executa a alteração |
| Muda classificação offline ou estado de hot blob | Traçar R04, R05 e B02 | `aggregator.rs`, `hot_blob.rs` | For necessário consultar audit log ou D1 real |
| Muda cópia, residência, auditoria ou SLI | Traçar R05, B03–B05 e M03–M05 | `replication.rs`, `region.rs`, `audit.rs`, `metrics.rs`, `coverage.rs` | For necessário operar R2, provider, métrica ou alerta |

<a id="s02"></a>
## S02 — Fronteira de autoria

| Condição | Decisão | Evidência SOURCE | Pare quando |
|---|---|---|---|
| Alterar agregador, worker ou sink em memória | Alterar somente contrato/fake desta crate | `InMemoryOfflineAggregator`, `InMemoryReplicationWorker`, `ReplicaAuditSink` | A alteração exigir binding, storage ou scheduler concreto |
| Alterar `Region` ou `ResidencyGraph` | Coordenar com consumidores diretos encontrados em B03–B05 | `region.rs`, manifesto e censo estático | A topologia nova não estiver decidida por fonte autorizada |
| Alterar métrica, label ou SLI trait | Preservar a separação entre observação/fake e backend real | `metrics.rs`, `coverage.rs`, `cardinality.rs` | A mudança pedir série observada, dashboard ou regra de alerta |

<a id="s03"></a>
## S03 — Triagem de fonte

| Dúvida | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Interface e limite | Ler R01–R04 | [Referência](../../../docs/ownership/crates/corelink-replica-worker/REFERENCE.md#r01) | Faltar uma assinatura ou caller classificado |
| Predicado de segurança | Ler os cinco axiomas em R05 | [Invariantes](../../../docs/ownership/crates/corelink-replica-worker/REFERENCE.md#r05) | A evidência for comentário, ticket ou resultado operacional |
| Impacto estático | Ler B01–B06 | [Blast radius](../../../docs/ownership/crates/corelink-replica-worker/BLAST_RADIUS.md#b01) | For necessário censo externo ou grafo reverso completo |
| Procedimento permitido | Ler M01–M06 | [Manutenção](../../../docs/ownership/crates/corelink-replica-worker/MAINTENANCE.md#m01) | A ação incluir Cargo, teste, rede, deploy ou provider |

<a id="s04"></a>
## S04 — Invariantes de decisão

| Condição | Decisão obrigatória | Evidência SOURCE | Pare quando |
|---|---|---|---|
| Mudar agregação | Preservar `AggregationStarted` antes de mutar `hot_blobs` no fake | Axioma 2, `aggregator.rs` | For preciso inferir atomicidade de D1/audit externo |
| Mudar cópia | Preservar validação de sibling antes de `ReplicationStarted` e de escrita no store em memória | Axiomas 1 e 3, `replication.rs` | For preciso alegar cópia R2 real |
| Mudar SLI | Preservar que erro do SLI não retorna erro de conclusão de réplica | Axioma 4, `replication.rs`, `metrics.rs` | For preciso alegar métrica emitida/entregue |
| Mudar guardas de cardinalidade | Preservar somente os dois valores constantes descritos | Axioma 5, `cardinality.rs` | For preciso afirmar schema de label, orçamento ou cardinalidade medida |

<a id="s05"></a>
## S05 — Fluxo de trabalho

| Fase | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Antes de editar | Confirmar baseline, manifesto e apenas cinco paths autorizados | M02 | HEAD/escopo divergir |
| Durante a edição | Formular predicado falsificável e mapear relações atômicas | R05, B01–B05 | Um efeito depender de integração externa |
| Depois da edição | Rodar só quatro checkers estruturais e `git diff --check` | M06 | A checagem pedir build, teste ou runtime |

<a id="s06"></a>
## S06 — Condições de parada

| Condição | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Pedido cita worker em execução, cron, R2, D1, Prometheus, provider, tráfego, deploy ou produção | Declarar não observado | R08 e B06 | Não converter SOURCE em runtime |
| Pedido exige topologia regional, consumer não classificado ou compatibilidade persistida | Coordenar com o owner identificado | B03–B06 | Não inventar rota, provider ou consumer |
| Pedido altera ou exige validação do OKF | Encaminhar à autoridade do OKF | Escopo deste skill | Não duplicar, redefinir ou revalidar OKF |

<a id="s07"></a>
## S07 — Evidência e saída

| Item | Critério |
|---|---|
| Sucesso | Os quatro artefatos apontam `crates/corelink-replica-worker/Cargo.toml`, distinguem SOURCE de runtime e contêm todos os IDs requeridos. |
| Completude | S01–S07, R01–R08, B01–B06 e M01–M06 são navegáveis; R05 traz cinco axiomas falsificáveis e A5 limita-se aos dois guardas constantes. |
| Qualidade | Relações atômicas, fakes/SLIs explícitos, nenhum claim de replicação, worker, provider ou operação real. |
| Pronto | Os quatro comandos estruturais de M06 retornam `IMPLEMENTED_CHECKS_PASS` e `git diff --check 6be030999...HEAD` não acusa whitespace; isto não é runtime, aprovação ou cold review. |
| Saída mínima | SHA, paths, checks literais e desconhecidos/escalação; nunca chamar checagem estrutural de prova operacional. |
