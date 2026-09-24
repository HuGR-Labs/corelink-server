---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-gc
manifest: crates/corelink-gc/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: H
state: draft
evidence_set: w007-gc-static-20260920
---

# corelink-gc — manual de manutenção

Todos os procedimentos são análise SOURCE estática. “Evidência” significa paths e predicados na
fonte, jamais observação de runtime, Cron, D1, provider ou exclusão física.

[Preparar](#m01) · [Schedule](#m02) · [Run](#m03) · [Degrade](#m04) · [Fases](#m05) · [Encerrar](#m06).

<a id="m01"></a>
## M01 — Preparar uma alteração

**Modo:** estático, sem execução. 1. Confirme baseline, manifesto e módulo-alvo. 2. Leia R01–R07
e B01–B06. 3. Escreva predicado falsificável e classifique efeitos como fake, contrato ou
desconhecido. **Evidência:** `Cargo.toml`, paths em R07 e relações B01–B06. **Pare:** se exigir
Cargo/teste, rede, source compartilhada, manifesto, migração, produção ou OKF canônica.

<a id="m02"></a>
## M02 — Alterar schedule ou scheduler

**Modo:** estático. 1. Trace `ScheduleConfig`, helper de jitter e `cron_tick`. 2. Preserve região,
limites e admission no contrato. 3. Atualize B02. **Evidência:** `src/schedule.rs` e
`src/scheduler.rs`; compare o predicado de R04. **Pare:** se a mudança requer Cron, alarm, DO ou
medida de horário real. **Recuperação:** restaure somente bytes locais; não alegue cancelar tick.

<a id="m03"></a>
## M03 — Alterar run, checkpoint ou fase

**Modo:** estático. 1. Trace todas as chamadas de `GcRunStore`. 2. Preserve tenant/run, transição
monotônica e âncora de mark. 3. Atualize B01 e B04. **Evidência:** `src/run.rs`, `src/worker.rs` e
INV-GC-001–004. **Pare:** se depender de schema/migração aplicada ou lock externo. **Recuperação:**
reverta bytes locais e mantenha a compatibilidade até decisão coordenada.

<a id="m04"></a>
## M04 — Alterar degrade mode

**Modo:** estático. 1. Compare enum, probe e gates no scheduler/worker. 2. Preserve `GcPause` como
abort no fluxo observado e distinga `GcReadOnly`. 3. Atualize B03. **Evidência:** `src/degrade.rs`,
`src/scheduler.rs`, `src/worker.rs` e INV-GC-005. **Pare:** se envolver config real, credencial ou
incidente. **Recuperação:** não afirme ter mudado estado operacional.

<a id="m05"></a>
## M05 — Alterar mark, sweep, physical-delete ou reconcile

**Modo:** estático. 1. Trace `RunId`, candidate, checkpoint e condição de cada fase. 2. Preserve
âncora e estados de candidate. 3. Declare interface/fake separadamente de provider. **Evidência:**
`src/mark.rs`, `src/sweep.rs`, `src/physical_delete.rs`, `src/reconcile.rs` e B04. **Pare:** se
pedirem chamada de R2/D1, purge, deleção física ou confirmação de retenção. **Recuperação:**
restaure somente bytes locais e a relação de contrato; não tente compensar provider, purge ou dado.

<a id="m06"></a>
## M06 — Validar, escalar e fechar

**Modo:** documental estático. Rode os quatro comandos exatos e depois `git diff --check
6ed297f5b HEAD`.

```sh
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind skill --profile H --root . .claude/skills/own-corelink-gc/SKILL.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind reference --profile H --root . docs/ownership/crates/corelink-gc/REFERENCE.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind blast_radius --profile H --root . docs/ownership/crates/corelink-gc/BLAST_RADIUS.md
python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind maintenance --profile H --root . docs/ownership/crates/corelink-gc/MAINTENANCE.md
```

Registre SHA, paths, predicado, resultado literal e desconhecidos; escale Cron/DO ao owner de
scheduler, schema ao owner D1 e provider/deleção ao owner operacional. **Evidência:** saída dos
comandos e R07/B06. Estas são checagens estruturais somente, não prova semântica/runtime/revisão.
**Success criteria:** todos os links e seções passam. **Completeness criteria:** M01–M06 cobrem o
caminho. **Quality/DoD:** sem claim runtime; conclusão não é certificação independente.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
