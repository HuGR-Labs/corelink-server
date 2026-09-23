---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-replication-coordinator
manifest: crates/corelink-replication-coordinator/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: replication-coordinator-static-20260920
---

# corelink-replication-coordinator — blast radius

Cada relação abaixo é atômica e deriva de manifesto ou fonte estática. Ela não afirma um coordenador ativo, estado replicado, integração de Durable Object, tráfego, deploy ou produção.

[Superfície](#b01) · [Heartbeat](#b02) · [Lag](#b03) · [Papel](#b04) · [Auditoria](#b05) · [Lacunas](#b06).

<a id="b01"></a>
## B01 — Superfície própria

`src/lib.rs` expõe módulos e tipos do package. Impacto: alterar um tipo, trait ou constante reexportado pode alterar callers que o compilam. Evidência: `src/lib.rs`. Pare antes de declarar que todos os callers foram enumerados.

<a id="b02"></a>
## B02 — Heartbeat para decisão

`InMemoryReplicationCoordinator` mantém `Arc<dyn HeartbeatRegistry>` e lê `latest(region)` para avaliar frescor e lag. Impacto: mudar o registry, timestamp ou limiar muda resultado estático de escrita, avaliação e status. Evidência: `coordinator.rs`, `heartbeat.rs`. Pare: não há observação de heartbeat recebido.

<a id="b03"></a>
## B03 — Lag para elegibilidade

`LagBundle::within_slo` alimenta `route_write`, `evaluate`, `promote` e `replication_status`; R2, D1 e KV são hard, Neon fica fora desse predicado. Impacto: alterar ceiling ou predicado muda elegibilidade codificada. Evidência: `lag.rs`, `coordinator.rs`. Pare: números não são uma medição operacional.

<a id="b04"></a>
## B04 — Papel para guarda local

`register`, `promote` e `failback` alteram ou consultam `HashMap<&'static str, RegionRoleState>` sob `Mutex`; `primary_in_guard` encontra o papel atual. Impacto: mudar papel ou guarda altera a recusa local contra segundo primário. Evidência: `coordinator.rs`, `state.rs`. Pare: mutex local não demonstra lock distribuído ou split-brain evitado fora da instância.

<a id="b05"></a>
## B05 — Auditoria para mutação codificada

`promote` chama `CoordinatorAuditSink::emit` para demissão e promoção antes das inserções de estado; `failback` emite evento bloqueado ou committed antes de sua mutação correspondente. Impacto: mudar tipo, ordem ou erro altera o `Result` e o mapa local desse caminho. Evidência: `audit.rs`, `coordinator.rs`. Pare: sink trait não demonstra entrega, persistência ou auditoria externa.

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

Conhecido: dependências diretas declaradas para `corelink-replica-worker` e `corelink-failover-router`, além das relações internas B01–B05. Desconhecido: consumidores, grafo reverso completo, coordenador ativo, estado replicado, DO/lock externo, métricas, transporte, auditoria entregue, tráfego, deploy e produção. A lacuna é evidência de não observação, não de inexistência.
