---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-replica-worker
manifest: crates/corelink-replica-worker/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: replica-worker-static-20260920
---

# corelink-replica-worker — manutenção

Este manual admite apenas evidência SOURCE/documental. Não autoriza Cargo,
build, teste, rede, worker/cron, R2, D1, provider, Prometheus, alerta, deploy
ou produção. OKF canônico segue referência externa, sem cópia, redefinição ou
revalidação.

[Modo](#m01) · [Baseline](#m02) · [Dados/residência](#m03) · [Réplica/audit](#m04) · [SLI](#m05) · [Fecho](#m06).

<a id="m01"></a>
## M01 — Modo de evidência

Os cinco paths do recorte são: (1) `crates/corelink-replica-worker/Cargo.toml`,
somente leitura como evidência; (2) `.claude/skills/own-corelink-replica-worker/SKILL.md`;
(3) `docs/ownership/crates/corelink-replica-worker/REFERENCE.md`; (4)
`docs/ownership/crates/corelink-replica-worker/BLAST_RADIUS.md`; e (5)
`docs/ownership/crates/corelink-replica-worker/MAINTENANCE.md`. Somente os quatro
últimos são mutáveis neste trabalho.

| Modo | Predicado | Ação permitida | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| SOURCE | A pergunta trata de tipo, trait, constante, fake ou ordem de código | Ler R03–R07 e formular predicado falsificável | Manifesto/fonte; não runtime | Se precisar execução, registrar R08/B06 e escalar |
| Documental | Só os quatro documentos mutáveis autorizados mudam | Conferir schema, IDs, links, paths e diff | Estrutural; não aprovação | Se código/Cargo aparecer, parar e restaurar escopo |
| Runtime | A pergunta pede worker, provider, SLI medido ou produção | Não executar nem inferir | Não observado neste pacote | Encaminhar ao owner operacional com lacuna explícita |

<a id="m02"></a>
## M02 — Baseline e escopo

Paths autorizados, exatamente: (1) `crates/corelink-replica-worker/Cargo.toml`
(leitura SOURCE), (2) `.claude/skills/own-corelink-replica-worker/SKILL.md`, (3)
`docs/ownership/crates/corelink-replica-worker/REFERENCE.md`, (4)
`docs/ownership/crates/corelink-replica-worker/BLAST_RADIUS.md` e (5)
`docs/ownership/crates/corelink-replica-worker/MAINTENANCE.md`; somente (2)–(5)
podem constar no diff.

| Modo | Predicado | Ação permitida | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Baseline SOURCE | Antes de documentar contrato | Rodar `git diff --quiet 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6 HEAD -- crates/corelink-replica-worker/Cargo.toml crates/corelink-replica-worker` | Exit `0` confirma que o recorte SOURCE não divergiu do baseline; não avalia runtime | Se diferente, parar e atualizar a prova antes de concluir |
| Escopo documental | Antes de commit | Inspecionar `git diff --name-only 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6 HEAD` | Devem aparecer somente os cinco paths autorizados | Se outro path aparecer, remover do change ou escalar |
| Linhagem | Antes da saída | Rodar `git merge-base --is-ancestor 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6 HEAD && git rev-parse HEAD` | Ancestralidade e SHA documental, não fonte/runtime adicional | Se falhar, não chamar HEAD de baseline |

<a id="m03"></a>
## M03 — Dados, agregação e residência

| Modo | Predicado | Ação permitida | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Contrato de agregação | Muda `AggregationEntry`, `HotBlob`, janela, top-1% ou status | Traçar `OfflineAggregator`, A2 e B02 | `aggregator.rs`, `hot_blob.rs` | Se exigir audit log/D1/query real, escalar ao owner do provider |
| Residência estática | Muda `Region`, `TenantTier` ou `ResidencyGraph` | Preservar pares fechados e atualizar B03–B05 | A1 em `region.rs` | Se exigir topologia/política real, congelar mudança para decisão externa |
| Compatibilidade | Tipo público/serializado muda | Censar consumers estáticos e reexports | B01, B03–B05 | Se schema persistido ou consumer externo entrar, não presumir compatibilidade |

<a id="m04"></a>
## M04 — Réplica, hash e audit

| Modo | Predicado | Ação permitida | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Contrato de réplica | Muda trait, status, retry ou store fake | Traçar `ReplicationWorker`, A3 e R07 | `replication.rs`, `error.rs` | Não alegar R2 copy, backoff temporal ou worker executado |
| Ordem de audit | Muda sink, record, evento ou propagação de erro | Preservar predicado específico de A2/A3 e distinguir audit inicial de audit de conclusão | `audit.rs`, `aggregator.rs`, `replication.rs` | Se precisar delivery/retention, parar e escalar ao backend de audit |
| Falha | Nova recuperação for proposta | Declarar o comportamento local exato e atualizar R07 | `ReplicaError`, fakes | Não tratar retorno local como alerta, ticket ou recuperação operacional |

<a id="m05"></a>
## M05 — SLI, cobertura e cardinalidade

| Modo | Predicado | Ação permitida | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| SLI de lag/outcome | Muda nome, domínio, bucket ou trait | Traçar A4, R06 e consumers B04–B05 | `metrics.rs` e fixtures | Se exigir scrape/dashboard/alerta, registrar runtime desconhecido |
| Cobertura | Muda ratio, target ou deadline | Preservar semântica de janela vazia e limite de fixture | `coverage.rs` | Não chamar ratio calculado de cobertura medida |
| Guardas de cardinalidade | Muda as constantes ou sua documentação | Preservar A5 como os dois valores constantes, sem inferir schema | `cardinality.rs` | Não alegar labels, séries reais ou compliance de todo o workspace |

<a id="m06"></a>
## M06 — Fecho, checker e recovery

O fecho confere os mesmos cinco paths, sem expansão: (1)
`crates/corelink-replica-worker/Cargo.toml` como entrada SOURCE read-only; (2)
`.claude/skills/own-corelink-replica-worker/SKILL.md`; (3)
`docs/ownership/crates/corelink-replica-worker/REFERENCE.md`; (4)
`docs/ownership/crates/corelink-replica-worker/BLAST_RADIUS.md`; e (5)
`docs/ownership/crates/corelink-replica-worker/MAINTENANCE.md`. Apenas (2)–(5)
podem ser commitados.

| Modo | Predicado | Ação permitida | Estado da evidência | Pare / recovery |
|---|---|---|---|---|
| Checker estrutural | Os artefatos estão prontos e `docs/ownership/tools/check_docs.py` está disponível | Rodar `python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-replica-worker/SKILL.md`; `python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-replica-worker/REFERENCE.md`; `python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-replica-worker/BLAST_RADIUS.md`; `python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-replica-worker/MAINTENANCE.md` | Cada resultado esperado é `IMPLEMENTED_CHECKS_PASS`; isso é forma documental, não runtime, aprovação ou cold review | Se indisponível/falhar, registrar literal e corrigir somente os quatro documentos mutáveis |
| Diff | Após os quatro checkers | Rodar `git diff --check 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6 HEAD` | Exit `0` sem whitespace; não valida conteúdo operacional | Corrigir somente whitespace/documento autorizado |
| Saída | Check estrutural/diff concluídos | Entregar SHA, paths, comandos/resultados e desconhecidos | R08/B06 delimitam a evidência | Não declarar worker, replicação, provider ou produção verificados |

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
