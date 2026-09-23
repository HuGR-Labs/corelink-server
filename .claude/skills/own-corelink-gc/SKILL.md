---
name: own-corelink-gc
description: >-
  Governa contratos estáticos de agendamento, checkpoint, fases e degradação
  do corelink-gc; não declara Cron, D1, provider ou exclusão física executados.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-gc"
  manifest: "crates/corelink-gc/Cargo.toml"
  source-commit: "6ed297f5b2b64cf97447985111a2ecbbaa9536bb"
  evidence-set: "w007-gc-static-20260920"
---

# Ownership — corelink-gc

Escopo SOURCE estático: contratos, fakes e topologia de módulos. Não é prova de runtime,
Cron, D1, providers, exclusão física, deployment ou dados reais.

[Escopo](#s01) · [Contrato](#s02) · [Decisão](#s03) · [Fluxo](#s04) · [Limites](#s05) · [Saída](#s06) · [Qualidade](#s07).

<a id="s01"></a>
## S01 — Escopo e autoridade

| Use quando | Não use quando |
|---|---|
| Alterar tipos de run, fase, schedule, checkpoint, degrade ou fakes | Executar Cron, D1, R2, provider, deploy ou dado de tenant |
| Traçar consumidor estático e compatibilidade de trait/reexport | Declarar exclusão física, retenção, operação ou incidente resolvido |

<a id="s02"></a>
## S02 — Contrato sob ownership

`lib.rs` expõe `run`, `schedule`, `scheduler`, `degrade`, `worker`, `mark`, `sweep`,
`physical_delete` e `reconcile`. O contrato inclui `GcRunStore`, `GcScheduler`, `GcWorker`,
`DegradeProbe`, fases/status, checkpoints e implementações `InMemory*`; os comentários sobre
bindings externos são fronteira declarada, não evidência executada.

<a id="s03"></a>
## S03 — Decisão obrigatória

| Se mudar (SOURCE) | Preserve / investigue (SOURCE) | Pare quando | Evidência / citação |
|---|---|---|---|
| fase, status ou checkpoint | grafo monotônico, escopo tenant/run e âncora de mark | exigir schema aplicado ou operação D1 | `src/run.rs` (`GcPhase`, `GcRunStore`); R05 e INV-GC-001–004 |
| schedule ou scheduler | região, jitter, cap e distinção fake/contrato | exigir Cron, fila ou alarm real | `src/schedule.rs`, `src/scheduler.rs`; R04 e B01 |
| degrade | `GcPause` fail-closed e `GcReadOnly` como contrato declarado | exigir config singleton, credencial ou incidente | `src/degrade.rs`, `src/worker.rs`; R04 e INV-GC-005 |
| sweep/physical-delete | predicates, candidate state e interfaces | exigir provider ou deleção física | `src/{mark,sweep,physical_delete,reconcile}.rs`; B03–B06 |

<a id="s04"></a>
## S04 — Fluxo de decisão

1. Fixe manifesto, baseline, símbolos e predicado falsificável na [Referência](../../../docs/ownership/crates/corelink-gc/REFERENCE.md#r01).
2. Classifique cada afirmação como SOURCE, fake local, contrato alvo ou desconhecido.
3. Trace run → checkpoint → fase e consulte as relações em [B01–B06](../../../docs/ownership/crates/corelink-gc/BLAST_RADIUS.md#b01).
4. Preserve invariantes ou pare antes de tocar integração externa.
5. Registre apenas evidência estática e lacunas na saída.

<a id="s05"></a>
## S05 — Paradas obrigatórias

Pare diante de Cargo/teste, rede, Cron, Durable Object, D1, R2, provider, migração aplicada,
credencial, tenant real, exclusão física, retenção, deploy, alerta ou produção. Referência OKF
canônica é aceita como verificada: não a revalide, copie ou redefina.

<a id="s06"></a>
## S06 — Saída mínima

Entregue SHA, paths SOURCE, contrato/relação afetada, predicado, comando documental e resultado
literal. Declare explicitamente o que não foi observado em runtime e escale ao owner apropriado.

<a id="s07"></a>
## S07 — Qualidade e pronto

**Sucesso:** limites e invariantes são rastreáveis à fonte. **Completude:** S01–S07 e R01–R08,
B01–B06 e M01–M06 cobrem o pedido. **Padrão:** fonte separada de execução e sem extrapolação.
**Definition of Done:** checagens documentais passam, links resolvem e desconhecidos permanecem
visíveis; isso não certifica runtime ou revisão independente.

[Referência](../../../docs/ownership/crates/corelink-gc/REFERENCE.md#r01) · [Impactos](../../../docs/ownership/crates/corelink-gc/BLAST_RADIUS.md#b01) · [Manutenção](../../../docs/ownership/crates/corelink-gc/MAINTENANCE.md#m01)
