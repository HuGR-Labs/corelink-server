---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-replication-coordinator
manifest: crates/corelink-replication-coordinator/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: replication-coordinator-static-20260920
---

# corelink-replication-coordinator — manutenção

Procedimentos são somente de fonte e documentação. Não autorizam Cargo, build, testes, rede, deploy, coordenador ativo, estado replicado, DO, lock distribuído ou produção. OKF permanece referência externa sem cópia ou revalidação.

[Modo](#m01) · [Baseline](#m02) · [Heartbeat/lag](#m03) · [Transição](#m04) · [Dependência](#m05) · [Fecho](#m06).

<a id="m01"></a>
## M01 — Modo e limite

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Estático | Não se pede prova operacional | Ler manifesto e R01–R07 | Fonte fixada | Registrar R08 e escalar se pedir runtime |
| Contrato | Tipo, trait, papel ou constante muda | Traçar relações B01–B05 | Fonte e diff, não execução | Congelar interface quando outro owner decidir |

<a id="m02"></a>
## M02 — Baseline e escopo

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Baseline | Antes de descrever contrato | Rodar `git diff --quiet 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6 -- crates/corelink-replication-coordinator/Cargo.toml crates/corelink-replication-coordinator` | Exit `0` confirma fonte igual ao baseline | Se não-zero, parar e reconstituir evidência |
| Escopo | Só quatro artefatos autorizados | Inspecionar `git diff --name-only 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6` | Lista deve conter somente estes paths | Se surgir código ou Cargo, escalar |

<a id="m03"></a>
## M03 — Heartbeat e lag

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Contrato de frescor | `Heartbeat` ou seu threshold muda | Preservar timestamp futuro stale e limiar estrito documentados em R03 | `heartbeat.rs` | Se exigir fonte ou cadence real, declarar desconhecido |
| Contrato de lag | Bundle ou ceilings mudam | Revisar R03, R05 e B03; separar hard de Neon soft | `lag.rs` | Não alegar lag medido |

<a id="m04"></a>
## M04 — Transição e auditoria

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Papel local | `register`, `promote` ou `failback` muda | Revisar A1–A5, inclusive A2 sem `Primary` atual e A5 condicionado ao emit bloqueado | `coordinator.rs`, `state.rs` | Se pedir exclusão distribuída, escalar |
| Ordem codificada | Sink, record ou erro muda | Preservar emit antes das inserções de promoção e distinguir `Audit` de `CooldownNotElapsed` no caminho bloqueado | `audit.rs`, `coordinator.rs` | Não alegar delivery externo |

<a id="m05"></a>
## M05 — Dependência e fronteira

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Interface regional | `Region` ou reexport muda | Coordenar com owner de `corelink-replica-worker` | Manifesto, `lib.rs`, B06 | Se topologia não vier do owner, congelar mudança |
| Dependência declarada | `corelink-failover-router` muda | Verificar contrato antes de consumir símbolo novo | Manifesto | Não inferir que dependência é integração executada |

<a id="m06"></a>
## M06 — Check estrutural, diff e lacunas

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Checker externo | Artefatos prontos | Rodar os quatro comandos abaixo | Cada um deve imprimir `IMPLEMENTED_CHECKS_PASS`; isso é estrutural, não runtime ou revisão | Se falhar, corrigir somente os quatro artefatos |
| Diff-base | Checkers passaram | Rodar `git diff --check 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6 HEAD` | Exit `0` sem whitespace | Corrigir somente documento autorizado |
| Lacuna | Pedem DO, estado replicado ou produção | Declarar R08/B06 e escalar | Fonte não é prova operacional | Parar |

```sh
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-replication-coordinator/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-replication-coordinator/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-replication-coordinator/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-replication-coordinator/MAINTENANCE.md
```
