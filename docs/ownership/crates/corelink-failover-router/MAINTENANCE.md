---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-failover-router
manifest: crates/corelink-failover-router/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: failover-router-static-20260920
---

# corelink-failover-router — manutenção

Procedimentos são somente source/static-only. Eles não autorizam Cargo, build, teste, rede, deploy, produção, execução de probe, Tower, Cloudflare ou roteamento ao vivo. OKF canônico permanece referência externa sem cópia, redefinição ou revalidação.

[Modo](#m01) · [Baseline](#m02) · [Saúde](#m03) · [Rota/auditoria](#m04) · [Failback/dependência](#m05) · [Fecho](#m06).

<a id="m01"></a>
## M01 — Modo e limite

| Modo de execução | Predicado | Ação | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Estático | Não se pede prova operacional | Ler manifesto e módulos de R03 | Fonte fixada | Se pedir runtime, registrar R08 e escalar. |
| Contrato | API, tipo, constante ou trait muda | Traçar R04–R07 e B01–B05 | Fonte e diff, não execução | Congelar interface quando outro owner decidir. |

<a id="m02"></a>
## M02 — Baseline e escopo

| Modo de execução | Predicado | Ação | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Baseline de fonte | Antes de alterar contrato descrito | Confirmar `git diff --quiet 6ed297f5b2b64cf97447985111a2ecbbaa9536bb 59dc5336a3978b529fecdd3f0c2f2ee712dfb4fb -- Cargo.toml crates/corelink-failover-router` | Predicado esperado: exit `0`; a fonte do recorte na âncora documental é igual ao baseline fixado `6ed297f5b`. | Se o exit não for `0`, parar: reconstituir evidência antes de alterar a referência. |
| Linhagem documental | Antes de editar os artefatos | Confirmar `git merge-base --is-ancestor 59dc5336a3978b529fecdd3f0c2f2ee712dfb4fb HEAD && git rev-parse HEAD` | Predicado esperado: o primeiro comando sai `0` e o segundo imprime o SHA do tip corrente dos artefatos; `59dc5336` é âncora ancestral, não tip transitório. | Se a ancestralidade falhar, comparar o diff documental e não tratar HEAD como fonte. |
| Documental | Somente artefatos de ownership autorizados | Conferir paths alterados e diff | Lista de paths e diff | Se surgir código/Cargo, escalar sem ampliar escopo. |

<a id="m03"></a>
## M03 — Alteração de saúde ou probe

| Modo de execução | Predicado | Ação | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Contrato estático | Trigger, limiar ou `evaluate` muda | Preservar três triggers e comparadores documentados em R05/R06 | `health.rs` | Se precisar janela sustentada real, parar: não há máquina de estado nela. |
| Trait estático | `HealthProbe` ou fixture muda | Revisar todos os três métodos de `FailoverRouter` e B02 | `probe.rs`, `router.rs` | Se exigir execução/cadence real, registrar desconhecido. |

<a id="m04"></a>
## M04 — Alteração de rota ou auditoria

| Modo de execução | Predicado | Ação | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Decisão estática | `route_read`, `ReadMode` ou `WriteMode` muda | Preservar caminho saudável e auditoria antes da decisão degradada | R05, B03 e `router.rs` | Se depender de HTTP/503/middleware, passar ao owner de composição. |
| Falha estática | Sink, record ou `FailoverError` muda | Revisar propagação de `Audit` e `NoReplicaAvailable` | `audit.rs`, `error.rs`, `router.rs` | Se delivery externo for necessário, não alegar sucesso operacional. |

<a id="m05"></a>
## M05 — Alteração de failback ou residência

| Modo de execução | Predicado | Ação | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Gate estático | Outbox, counter ou gate muda | Preservar zero como caminho `Ok` e consulta com erro/sujeira como recusa | R05, B05 e `failback.rs` | Se precisar consultar outbox real, escalar ao implementador do trait. |
| Dependência | `Region`, `ResidencyGraph` ou auditoria reexportada muda | Coordenar com `corelink-replica-worker` antes da alteração | Manifesto, B04 e `lib.rs` | Se topologia não for fonte do owner, congelar mudança. |

<a id="m06"></a>
## M06 — Stop, recovery e evidência final

| Modo de execução | Predicado | Ação | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Checker estrutural externo | Artefatos prontos; ferramenta disponível em `/tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py` | Rodar `python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-failover-router/SKILL.md`; `python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-failover-router/REFERENCE.md`; `python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-failover-router/BLAST_RADIUS.md`; `python3 /tmp/corelink-ownership-import.VFOYl7/corelink-ownership-v1.3/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-failover-router/MAINTENANCE.md` | Predicado esperado: os quatro retornam `IMPLEMENTED_CHECKS_PASS`; isto é somente checagem estrutural, não aprovação, prova de runtime ou cold review. | Se indisponível/falhar, registrar o estado e corrigir somente os quatro artefatos autorizados. |
| Diff documental | Checker estrutural passou | Rodar `git diff --check 6ed297f5b2b64cf97447985111a2ecbbaa9536bb HEAD` | Predicado esperado: exit `0` sem whitespace diagnosticado. | Se falhar, corrigir somente whitespace/documento autorizado. |
| Lacuna | Tower, Cloudflare, live routing, probe executado, tráfego ou produção solicitados | Declarar desconhecido e indicar owner/ação seguinte | R08 e B06 | Parar: fonte estática não certifica operação. |
