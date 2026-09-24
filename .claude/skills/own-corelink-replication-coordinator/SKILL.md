---
name: own-corelink-replication-coordinator
description: >-
  Route source-backed ownership changes for CoreLink replication-coordinator
  contracts, in-memory state transitions, heartbeat and lag decisions.
metadata:
  schema: "corelink-ownership/1.3"
  package: "corelink-replication-coordinator"
  manifest: "crates/corelink-replication-coordinator/Cargo.toml"
  source-commit: "6be030999de1f0e0fe62d3a9abb04ec2a4fefde6"
  evidence-set: "replication-coordinator-static-20260920"
---

# Ownership — corelink-replication-coordinator

Fonte estática somente. Estes contratos descrevem código e fixtures em memória; não certificam um coordenador ativo, estado replicado, lock externo, tráfego, deploy ou produção. O OKF verificado é referência externa, sem cópia, redefinição ou revalidação.

[Acionamento](#s01) · [Fronteira](#s02) · [Leitura](#s03) · [Invariantes](#s04) · [Fluxo](#s05) · [Parada](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Condição | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Mudam papel, promoção, failback ou escrita | Assumir o contrato estático e abrir R04–R06 | `coordinator.rs`, `state.rs` | A mudança requer coordenador ativo ou estado replicado |
| Mudam heartbeat ou lag | Traçar R03, R05 e B02 | `heartbeat.rs`, `lag.rs` | A origem observada precisar de rede |

<a id="s02"></a>
## S02 — Fronteira de autoria

| Condição | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Alterar tipo, trait, constante ou fixture local | Alterar contrato desta crate | `src/lib.rs` e módulos reexportados | A interface de dependência não estiver congelada |
| Alterar `Region` | Coordenar com `corelink-replica-worker` | Reexport e dependência direta | A topologia for necessária |
| Alterar roteamento HTTP, DO ou dashboard | Escalonar ao owner de composição | Ausentes do manifesto/fonte executável | Só houver comentário ou OKF |

<a id="s03"></a>
## S03 — Roteamento de leitura

| Dúvida | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Contrato e axiomas | Ler R01–R06 | [Referência](../../../docs/ownership/crates/corelink-replication-coordinator/REFERENCE.md#r01) | For necessária prova externa |
| Relação afetada | Ler B01–B05 | [Blast radius](../../../docs/ownership/crates/corelink-replication-coordinator/BLAST_RADIUS.md#b01) | O grafo reverso completo for necessário |
| Procedimento permitido | Ler M01–M06 | [Manutenção](../../../docs/ownership/crates/corelink-replication-coordinator/MAINTENANCE.md#m01) | A ação pedir Cargo, teste, rede ou deploy |

<a id="s04"></a>
## S04 — Invariantes de decisão

| Condição | Decisão obrigatória | Evidência | Pare quando |
|---|---|---|---|
| Mudar elegibilidade | Preservar heartbeat fresco e três SLOs hard | `lag.rs`, `coordinator.rs` | For pedido dado de lag real |
| Mudar promoção/failback | Preservar emit antes da mutação no caminho codificado | `coordinator.rs`, `audit.rs` | For exigida entrega de auditoria |
| Mudar papel primário | Preservar guarda de primário único no mapa local | `coordinator.rs` | For exigido lock distribuído |
| Mudar cooldown | Preservar `86_400` segundos e recusa precoce | `state.rs`, `coordinator.rs` | For preciso provar tempo de produção |

<a id="s05"></a>
## S05 — Fluxo de trabalho

| Fase | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Antes | Confirmar baseline, manifesto e recorte | M02 | HEAD ou escopo divergirem |
| Durante | Mapear contrato, relação e procedimento | R01–R07, B01–B05 | Depender de owner externo |
| Depois | Rodar somente checks documentais | M06 | Resultado exigir execução |

<a id="s06"></a>
## S06 — Condições de parada

| Condição | Decisão | Evidência | Pare quando |
|---|---|---|---|
| Pedido cita DO, lock distribuído, tráfego, dashboard ou produção | Declarar não observado | R08, B06 | Não inferir operação da fonte |
| Pedido afirma estado replicado ou coordenador ao vivo | Escalar para owner operacional | Limite do recorte | Não inventar confirmação |
| Pedido altera OKF | Encaminhar à autoridade do OKF | Escopo deste skill | Não duplicar política |

<a id="s07"></a>
## S07 — Evidência e saída

| Item | Critério |
|---|---|
| Sucesso | Os quatro artefatos existem e apontam o manifesto correto. |
| Completude | S01–S07, R01–R08, B01–B06 e M01–M06 são navegáveis. |
| Qualidade | Relações são atômicas; axiomas e desconhecidos permanecem falsificáveis. |
| Pronto | Os quatro checks estruturais e o diff-base de M06 passam; isso não prova runtime nem revisão independente. |
