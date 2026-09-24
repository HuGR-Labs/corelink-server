---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-gc
manifest: crates/corelink-gc/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: H
state: draft
evidence_set: w007-gc-static-20260920
---

# corelink-gc — blast radius

Relações abaixo são obtidas de SOURCE estática; a direção é consumidor → pacote ou módulo →
contrato. Nenhuma relação demonstra execução de Cron, D1, provider ou exclusão física.

[Scheduler](#b01) · [Worker](#b02) · [Mark](#b03) · [Sweep](#b04) · [Physical delete](#b05) · [Reconcile](#b06).

<a id="b01"></a>
## B01 — Scheduler → schedule, run store e degrade

**Relação atômica:** `scheduler.rs` consome `ScheduleConfig`, `GcRunStore`, `DegradeProbe` e
`GcWorker`. **Ativação:** `cron_tick` recebe instante/lista e calcula admissão. **Falha:** erro de
probe/store/worker propaga pela taxonomia; `AlreadyRunning` pula o tenant no fake. **Validação:**
leia assinaturas e chamadas em `src/{schedule,scheduler,run,degrade,worker}.rs`. **Limite:** não
prova Cron, D1, Durable Object ou execução de tick.

<a id="b02"></a>
## B02 — Worker → run store, degrade, audit e metrics

**Relação atômica:** `worker.rs` consome `GcRunStore`, `DegradeProbe`, `GcAuditSink` e
`GcMetricsObserver`. **Ativação:** `execute_run` recebe run/tenant/config previamente admitidos.
**Falha:** probe, store, audit ou metric retorna `GcError`; `GcPause` finaliza `Aborted` no fake.
**Validação:** trace `transition_or_abort` e `finalize_aborted` em `src/worker.rs`. **Limite:** não
prova emissão de auditoria/métrica, config real ou abort operacional.

<a id="b03"></a>
## B03 — Mark → run store e candidate store

**Relação atômica:** `mark.rs` consome `GcRunStore`, `ReachableSetSource` e `GcCandidatesStore`.
**Ativação:** `MarkPhase::execute` recebe run/tenant/região e captura a âncora antes do scan.
**Falha:** erro de fonte/store ou estado de run inválido retorna `MarkError`. **Validação:** confira
`capture_mark_anchor`, `execute` e traits em `src/mark.rs`. **Limite:** não prova scan D1 ou
candidates persistidos.

<a id="b04"></a>
## B04 — Sweep → run store, candidates e referência AC

**Relação atômica:** `sweep.rs` consome `GcRunStore`, `GcCandidatesStore`, `BlobMetaStore` e
`AcReferenceIndex`. **Ativação:** `SweepPhase::execute` usa a âncora e candidates do mark run.
**Falha:** âncora ausente, store ou referência falha retorna `SweepError`. **Validação:** trace
`SweepPhase`, `SweepDecision` e traits em `src/sweep.rs`. **Limite:** não prova soft-delete, AC ou
armazenamento real.

<a id="b05"></a>
## B05 — Physical delete → run store, candidates e R2Delete trait

**Relação atômica:** `physical_delete.rs` consome `GcRunStore`, `GcCandidatesStore`,
`BlobMetaPurgeStore` e `R2Delete`. **Ativação:** `PhysicalDeletePhase::execute` recebe o run e
candidates elegíveis. **Falha:** backend, purge/audit/metric ou `R2Delete` é expresso por
`PhysicalDeleteError`. **Validação:** confira trait e `InMemoryPhysicalDeletePhase` em
`src/physical_delete.rs`. **Limite:** `R2Delete` é uma interface; nenhuma deleção física é provada.

<a id="b06"></a>
## B06 — Reconcile → refcount source e run store

**Relação atômica:** `reconcile.rs` consome `GcRunStore`, `BlobMetaRefcountStore` e
`RefcountSource`. **Ativação:** `ReconcilePhase::execute` recebe run/tenant/região e snapshot de
refcount. **Falha:** fonte/store, budget ou decisão inválida retorna `ReconcileError`.
**Validação:** trace `ReconcilePhase`, `ReconcileDecision` e traits em `src/reconcile.rs`.
**Limite:** não prova leitura de banco, autofix ou efeito persistido. Método e lacunas continuam
SOURCE estáticos: censo externo, schema aplicado, Cron/DO, D1/R2, providers, dados e deleção são
desconhecidos e devem escalar ao owner de integração.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
