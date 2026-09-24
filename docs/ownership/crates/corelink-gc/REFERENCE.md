---
schema: corelink-ownership/1.1
document: reference
package: corelink-gc
manifest: crates/corelink-gc/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: H
state: draft
evidence_set: w007-gc-static-20260920
---

# corelink-gc — referência de ownership

Evidência SOURCE estática somente. Ela descreve superfícies Rust e fakes declarados; não prova
Cron, D1, Durable Object, provider, exclusão física ou execução de produção.

[Identidade](#r01) · [Fronteira](#r02) · [Topologia](#r03) · [Schedule](#r04) · [Run](#r05) · [Invariantes](#r06) · [Evidência](#r07) · [Pronto](#r08).

<a id="r01"></a>
## R01 — Identidade

| Campo | Evidência SOURCE |
|---|---|
| Package | `crates/corelink-gc/Cargo.toml` |
| Raiz pública | `crates/corelink-gc/src/lib.rs` |
| Papel | plano tipado de GC, scheduler e fases com seams/fakes em memória |
| Runtime observado | nenhum |

<a id="r02"></a>
## R02 — Fronteira de autoridade

Esta unidade possui tipos, traits, erros, transições e `InMemory*`. `MIGRATION_0006_GC_RUN` e
`MIGRATION_0007_GC_CANDIDATES` são artefatos embutidos na fonte; não demonstram que migrações foram
aplicadas. Menções a Cloudflare, Cron, D1, R2 e `DeleteObject` descrevem contratos ou futuros
bindings. Não inferir chamada de provider, scheduler real ou deleção física.

<a id="r03"></a>
## R03 — Topologia de módulos

| Módulo | Responsabilidade SOURCE |
|---|---|
| `run`, `region` | identidade, row, status/fase, store e regiões |
| `schedule`, `scheduler` | config, jitter e admissão por tick em fake |
| `degrade`, `worker` | probe, abort gate e orquestração de lifecycle |
| `mark`, `sweep` | candidates, âncora e decisão de sweep em memória |
| `physical_delete`, `reconcile` | traits, decisões e fakes de fase |
| `audit`, `metrics`, `admin`, `error` | observabilidade, stub administrativo e taxonomia |

<a id="r04"></a>
## R04 — Contratos de schedule e degrade

`ScheduleConfig` aceita cron não vazio, jitter limitado e região; `jitter_ms_for_region` é helper
determinístico. `GcScheduler::cron_tick` recebe instante e lista de tenants, consulta
`DegradeProbe`, limita admissões e delega a `GcWorker`. `DegradeKind::{Off,GcPause,GcReadOnly}`
e `DegradeProbe` são contratos de fonte. A fonte trata `GcPause` como pausa de admissão e o worker
como abort em fronteira; não comprova relógio, cron ou config singleton real.

<a id="r05"></a>
## R05 — Contratos de run, checkpoint e fases

`GcRunStore` separa `insert_pending`, `acquire_running`, `transition_phase`, `checkpoint`,
`finalize` e `lookup` por `run_id` e tenant. `GcPhase` permite a cadeia Idle → Mark → Sweep →
PhysicalDelete → Reconcile → Completed, mais falha antes do terminal. `GcStatus` modela Pending,
Running e terminais. `mark`, `sweep`, `physical_delete` e `reconcile` expõem traits e fakes;
isso não transforma os respectivos nomes em operação de armazenamento ou provider.

<a id="r06"></a>
## R06 — Invariantes SOURCE falsificáveis

**INV-GC-001 — exclusão mútua de running.** Para mesmo `(tenant_id, region)`, o fake retorna
`AlreadyRunning` ao segundo `acquire_running` enquanto há row Running. Falsificação: obter dois
acquires bem-sucedidos concorrentes no mesmo fake. Não prova UNIQUE em D1.

**INV-GC-002 — fases não retrocedem.** `GcPhase::can_transition_to` aceita apenas a cadeia
seguinte ou `Failed`; `transition_phase` rejeita pulo/reversão. Falsificação: `Sweep → Mark` ou
`Idle → Sweep` ser aceito. Fonte: `src/run.rs`.

**INV-GC-003 — reentrada em Mark falha antes do guard de âncora.** Na entrada válida em Mark,
`mark_started_at_ms` é capturado. Uma reentrada normal em Mark é rejeitada primeiro como
`InvalidPhaseTransition`, pois `GcPhase::Mark` não transiciona para Mark; portanto não atribuir
`MarkStartedAtImmutable` a esse reset. Falsificação: `Mark → Mark` ser aceito ou a âncora de uma
linha existente mudar. Fonte: `src/run.rs:578-583`, `src/mark.rs`.

**INV-GC-004 — checkpoint conserva escopo.** APIs de row recebem `run_id` e `tenant_id`; lookup
de tenant diferente não retorna a row. Falsificação: tenant B ler ou checkpointar row de tenant A.
Isto é semântica do fake, não isolamento de banco.

**INV-GC-005 — `GcPause` não continua pelo worker.** Em fronteira, `requires_abort()` leva a
finalização `Aborted` com razão `degrade_mode_gc_pause`. Falsificação: fake conclui sucesso após
probe `GcPause`. Não mede propagação de config real.

<a id="r07"></a>
## R07 — Evidência e desconhecidos

Lidos estaticamente: manifesto, `src/lib.rs`, `run.rs`, `schedule.rs`, `scheduler.rs`,
`degrade.rs`, `worker.rs`, `mark.rs`, `sweep.rs`, `physical_delete.rs` e `reconcile.rs`.
SOURCE inclui implementações in-memory e comentários. Não foram executados Cargo, testes, rede ou
migrações. Desconhecidos: bindings, schema aplicado, consumers externos, Cron/DO, D1/R2/provider,
dados reais, deleção, auditoria entregue e comportamento operacional.

<a id="r08"></a>
## R08 — Sucesso, completude e qualidade

**Success criteria:** cada limite tem path SOURCE e cada invariante tem falsificador. **Completeness
criteria:** cobre schedule, checkpoint, degrade e topologia, com relações e procedimentos ligados.
**Quality standards:** usar linguagem de contrato/fake e separar SOURCE de runtime. **Definition of
Done:** links e checagens documentais passam; os desconhecidos continuam explícitos. A referência
OKF canônica não foi revalidada, copiada ou redefinida.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01)
