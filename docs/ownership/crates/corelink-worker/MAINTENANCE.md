---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-worker
manifest: crates/corelink-worker/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: H
state: draft
evidence_set: w011-worker-static-20260920
---

# corelink-worker — manutenção

Este manual aceita somente evidência estática e documental. Não autoriza Cargo,
build, teste, rede, request, worker, storage, provider, deploy ou produção.
OKF é uma rota canônica externa, sem cópia ou revalidação neste artefato.

[Modo](#m01) · [Baseline](#m02) · [Composição](#m03) · [Consumers](#m04) · [Parada](#m05) · [Fecho](#m06).

<a id="m01"></a>
## M01 — Modo de evidência

| Modo | Predicado | Ação permitida | Pare / recovery |
|---|---|---|---|
| SOURCE | Pergunta trata de manifesto, `lib.rs`, módulo, gate ou reexport | Ler R01–R06 e registrar predicado refutável | Se pedir efeito, marcar R08/B06 e escalar |
| Censo | Relação aparece em manifest/import Rust | Registrar somente path e impacto de compatibilidade | Não chamar import de execução |
| Documental | Só quatro paths autorizados mudam | Validar schema, links, IDs e diff | Se outro path mudar, retirar do change ou escalar |
| Operacional | Pergunta pede request/worker/storage/deploy | Não executar nem inferir | Encaminhar por rota OKF ao owner correspondente |

<a id="m02"></a>
## M02 — Baseline e escopo

Os quatro paths atribuídos a este package são: `.claude/skills/own-corelink-worker/SKILL.md`,
`docs/ownership/crates/corelink-worker/REFERENCE.md`, `BLAST_RADIUS.md` e
`MAINTENANCE.md`. Os comandos abaixo limitam a verificação a estes paths; um diff
da campanha inteira pode conter alterações de outros packages/evidências, cuja
reconciliação cabe à integração. O manifesto
`crates/corelink-worker/Cargo.toml` é entrada SOURCE somente leitura.

| Checagem | Comando documental | Resultado / recovery |
|---|---|---|
| Linhagem | `git merge-base --is-ancestor 16d9f0303d849a1ab3df14688bd2c7cdbfee8140 HEAD` | Exit 0 vincula o SHA; não prova runtime |
| Fonte imutável | `git diff --quiet 16d9f0303d849a1ab3df14688bd2c7cdbfee8140 HEAD -- crates/corelink-worker` | Exit 0; se falhar, não concluir esta referência |
| Escopo do package | `git status --short -- .claude/skills/own-corelink-worker/SKILL.md docs/ownership/crates/corelink-worker/REFERENCE.md docs/ownership/crates/corelink-worker/BLAST_RADIUS.md docs/ownership/crates/corelink-worker/MAINTENANCE.md` | Conferir os quatro paths atribuídos; a reconciliação do diff completo da campanha é da integração |

**Procedimentos:** [PROC-001 — revisão SOURCE](#proc-001) · [PROC-002 — handoff documental](#proc-002).

<a id="m03"></a>
## M03 — Mudança de composição

| Mudança | Predicado a preservar | Ação SOURCE | Pare quando |
|---|---|---|---|
| `default` ou `tower-middleware` | API-003 permanece verdadeiro ou recebe revisão explícita | Atualizar R03–R06 e B03/B05 | Precisar resolver feature ou medir build |
| `pub mod` ou reexport raiz | API-001/API-004 descrevem o root atual | Traçar path e consumers B02–B05 | Consumer externo ou contrato persistido surgir |
| Novo submódulo | Mapa R03 enumera a composição raiz correta | Classificar gate e exposição | Nome implicar request/storage/worker e houver alegação de efeito |
| Dependência opcional | Lista `dep:` e gate permanecem coerentes | Atualizar R06 e impactos | Alteração requerer Cargo.lock ou configuração compartilhada |

<a id="m04"></a>
## M04 — Mudança para consumers

| Consumer estático | Antes de mudar | Limite |
|---|---|---|
| `corelink-adapter-host` | Conferir path de `KvBackend`/`InMemoryKv` em B02 | Não inferir package instalado ou cache real |
| auth/cas/replication | Conferir reexports e encaminhamento de feature em B03 | Não inferir seleção da feature |
| cf-bindings | Conferir traits/erros citados em B04 | Não inferir wasm, binding ou provider |
| reapi | Conferir `host-server`, reexport e paths em B05 | Não inferir servidor/request/backend |

<a id="proc-001"></a>
### PROC-001 — Revisar contrato e relações SOURCE

**Objetivo/gatilho:** mudança em manifesto, módulo, API pública ou consumer listado em B02–B05. **Pré-condições/inputs:** revisão read-only do commit pinado e paths de fonte citados; sem credencial ou estado externo.

**Modo/ambiente/permissões:** `READ_ONLY`, checkout local; sem Cargo, testes, rede ou provider.

1. Compare `Cargo.toml`, `src/lib.rs` e os arquivos da superfície alterada com R03–R06 e API-009–014.
2. Pesquise o consumer exato e atualize sua relação atômica em B02–B06, separando declaração, import/call e efeito desconhecido.
3. Registre path, commit e predicado refutável; se não puder verificar a ponta peer, marque-a desconhecida e encaminhe ao owner.

**Predicado esperado:** cada afirmação alterada aponta a uma fonte e relação; reexport/import não é descrito como execução. **Falha/parada:** fonte mudou desde o pin, consumer ausente, ou alegação exige resolução/build/runtime. **Recovery:** reter desconhecido e reduzir a afirmação ao que o pin demonstra.

**Evidência:** arquivos lidos, commit e IDs afetados. **Estado:** revisão SOURCE local executada para esta atualização documental; nenhuma execução de pacote ou runtime foi feita.
[Índice de procedimentos](#m02)

<a id="proc-002"></a>
### PROC-002 — Validar e encaminhar os quatro artefatos

**Objetivo/gatilho:** alteração de qualquer byte nos documentos de ownership deste package. **Pré-condições/inputs:** root do repo, Python 3, checker no checkout e os quatro paths da skill/reference/blast/maintenance.

**Modo/ambiente/permissões:** `LOCAL_ISOLATED`; Markdown local apenas. **Comandos:** executar o checker H listado em M06 para cada tipo; então `git diff --check -- <quatro paths>` e conferir `git status --short -- <quatro paths>`.

**Predicado esperado:** quatro `IMPLEMENTED_CHECKS_PASS`, diff sem whitespace error, alterações restritas ao conjunto autorizado neste handoff. **Falha/parada:** checker falha, arquivo fora do escopo do handoff, pin/fonte diverge ou review independente falta. **Recovery:** corrigir só documento atribuído, repetir os checks e solicitar cold review das bytes finais.

**Evidência:** saída JSON dos quatro checks, resultado do diff e hashes dos quatro arquivos. **Estado:** checks documentais locais serão executados após esta edição; cold review permanece pendente; sem teste Cargo ou execução operacional.
[Índice de procedimentos](#m02)

<a id="m05"></a>
## M05 — Parada e recuperação

Pare diante de Cargo/build/teste, benchmark, rede, request HTTP/gRPC, worker/cron,
R2/KV/D1/DO/Neon, credencial, tenant real, provider, auditoria, observabilidade,
deploy, tráfego ou produção. Registre qual predicado SOURCE não responde, não
deduza a resposta pelo nome do módulo e encaminhe ao owner do consumer,
composition root ou provider via rota OKF externa.

<a id="m06"></a>
## M06 — Fecho, checker e entrega

| Checagem | Comando permitido | Resultado esperado |
|---|---|---|
| Skill | `python3 docs/ownership/tools/check_docs.py --kind skill --profile H --root . .claude/skills/own-corelink-worker/SKILL.md` | `IMPLEMENTED_CHECKS_PASS`; forma, não aprovação |
| Referência | `python3 docs/ownership/tools/check_docs.py --kind reference --profile H --root . docs/ownership/crates/corelink-worker/REFERENCE.md` | `IMPLEMENTED_CHECKS_PASS`; não runtime |
| Impacto/manutenção | Rodar o mesmo checker com `blast_radius` e `maintenance` nos paths correspondentes | `IMPLEMENTED_CHECKS_PASS`; não cold review |
| Diff do handoff | `git diff --check -- .claude/skills/own-corelink-worker/SKILL.md docs/ownership/crates/corelink-worker/REFERENCE.md docs/ownership/crates/corelink-worker/BLAST_RADIUS.md docs/ownership/crates/corelink-worker/MAINTENANCE.md` | Exit 0; valida whitespace nos quatro paths mesmo com alterações concorrentes em outros packages |

Entregue SHA, quatro paths, comandos/resultados e lacunas R08/B06. Não chame
checker estrutural de validação operacional, certificação OKF ou prova de produção.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
