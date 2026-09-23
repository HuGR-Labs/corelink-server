---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-hash-fuzz
manifest: crates/corelink-hash/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: hash-fuzz-source-20260921
---

# corelink-hash-fuzz — blast radius

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) ·
[Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Escopo e leitura rápida

Esta unidade contém dois harnesses independentes em seleção, mas em um package Cargo excluído do workspace: digest_parse e verify_body. A relação hash compartilhada é REL-001. Manifestos e workflow declaram como invocar alvos; não comprovam build, fuzzing, cobertura ou runtime. Não há database, bucket, tenant, segredo, provider ou efeito produtivo identificado no código dos harnesses.

**Pins:** source 1177dad2ca2a9f21c29b5a118aa7944b77147798; baseline de integração 8cdc02828132b9b6f03a3b57117b8140325f6762. O diff dos quatro paths source do package entre esses trees não listou diferenças. Snapshot local arquivado registra um job de PR que falhou na preparação nightly e pulou os dois steps fuzz; nenhum target foi observado rodando. Cold review e peer reconciliation continuam pendentes.

| Estado independente | Valor | Evidência / limite |
|---|---|---|
| implemented | sim | dois targets e manifesto no source pin; prova estática |
| wired | declarado | workflows/scripts/manifesto declaram seleção; não executados |
| runtime_verified | UNKNOWN | S14 registra falha pré-target/skips; nenhum runtime observado |

| Camada de ownership | Dono/estado verificado | Limite |
|---|---|---|
| Implementação | corelink-hash-fuzz (package) | targets/manifesto |
| Contrato público | corelink-hash (package) | Digest/VerifiedBody |
| Composition root | N/A | harness não compõe produto |
| Operador de runtime | UNKNOWN | runner/target não observado |
| Autoridade de revisão | UNKNOWN | cold reviewer independente exigido |

**Rotas:** [okf-context](../../../../.claude/skills/okf-context/SKILL.md) pela tag `cas` e arquivos `crates/corelink-hash/src/{lib.rs,digest.rs,verified_body.rs}`; o manifesto fuzz pode retornar no-concepts direto. [built-not-wired](../../../../.claude/skills/built-not-wired/SKILL.md) separa existência, wiring e runtime. Escalonamento/aprovação: issue/PR → owner do package afetado → cold reviewer; owner nominal REQUIRED, identidade UNKNOWN.
**Peer REL-001:** o BLAST lead-owned de `corelink-hash` deve espelhar a identidade `repo:1232040291:boundary:hash-fuzz-harness-001`, fingerprint `rel-001:v1:sha256:8bf1f82daa5ae77adca4b3f2e0c1d2e27799686b0a23192ea6ce65ff94dda8cc` e os mesmos producer/consumer, dependência, dados, impacto, ativação e contrato; peer PENDING.

<a id="b02"></a>
## B02 — Censo e método

| População | Fonte e busca estática | Resultado desta revisão | Limite |
|---|---|---|---|
| Identidade/workspace | S01/S05; manifests Cargo rastreados | Um manifest standalone [workspace]; root o exclui | cargo metadata não executado |
| Targets/deps declarados | S01/S02 | Dois bins; deps normais corelink-hash, bytes, libfuzzer-sys; nenhum feature declarado | Sem resolução/target selecionado |
| Inversas Cargo | Busca literal por nome/path em manifests rastreados | Nenhum outro manifest consumidor observado; package é excluído do workspace | Alias de path, geração e alcance não excluídos por resolver |
| Chamadas do harness | S03/S04 + hash S06/S07/S08 | Parser UTF-8 e verificação de corpo em memória | Fonte não prova execução |
| Fora de Cargo | Busca literal por package/path/targets/fuzz nos paths rastreados | corelink-hash.yml, nightly.yml, scripts/fuzz-all.sh, .gitignore; S14 registra job de PR com os dois steps fuzz skipped | YAML/script não provam run; S14 prova somente estado dos steps arquivado |
| Corpus/artifacts | Árvore versionada + S12 + workflows | Nenhum arquivo corpus rastreado; corpus, artifacts, coverage ignorados; nightly upload declara artifacts por 14 dias | Estado remoto/local não inspecionado |

**Método:** comparação de paths package entre source pin e integration baseline; inspeção literal de manifestos, dois targets, sources provider, workflows e script; git grep por corelink-hash-fuzz, crates/corelink-hash/fuzz, digest_parse, verify_body e cargo fuzz. Matches em outros fuzz packages, nomes de erro digest_parse, TLA, relatórios de LOC, pentest e specs seladas são registros/documentação, não consumidores do package nem evidência de run atual. Nenhuma busca semântica fora do tree/repositório foi feita.

**Identidades:** consumidor e target owner repo:1232040291:crates/corelink-hash/fuzz/Cargo.toml; provider repo:1232040291:crates/corelink-hash/Cargo.toml; workflow é arquivo .github/workflows/..., script é scripts/fuzz-all.sh. Nenhum UID de runner/provedor externo foi atribuído.

<a id="b03"></a>
## B03 — Índice de relações diretas

| ID | Tipo / direção de dependência | Superfície | Ativação |
|---|---|---|---|
| [REL-001](#rel-001) | dependency, fuzz→hash | Declaração path e API provider | Bin selecionado |
| [REL-002](#rel-002) | test, target→hash | digest_parse | Target invocado |
| [REL-003](#rel-003) | test, target→hash | verify_body | Target invocado |
| [REL-004](#rel-004) | dependency, fuzz→bytes | Buffer do corpo | verify_body |
| [REL-005](#rel-005) | dependency, fuzz→libfuzzer-sys | Macro/engine harness | Bin de fuzz |
| [REL-006](#rel-006) | build-deploy, workflow→fuzz | PR smoke de 60 s | PR com paths correspondentes |
| [REL-007](#rel-007) | build-deploy, workflow→fuzz | Job nightly no workflow hash | Condição schedule sem trigger ativo |
| [REL-008](#rel-008) | build-deploy, nightly→fuzz | Matrix de duas execuções de 3600 s | schedule ou workflow_dispatch |
| [REL-009](#rel-009) | data, fuzz→artefato CI | Crash artifact em falha | Matrix nightly falha |
| [REL-010](#rel-010) | runtime-call, script→fuzz | Lista de targets em fuzz-all.sh | Invocação manual do script |
| [REL-011](#rel-011) | build-deploy, parent-workflow→fuzz | PR fuzz-smoke path gate | PR com path elegível |
| [REL-012](#rel-012) | build-deploy, parent-workflow→fuzz | wasm-build path gate | PR com path elegível |
| [REL-013](#rel-013) | data, workflow→cache | Cache declarado do fuzz-smoke | Runner github-hosted |
| [REL-014](#rel-014) | data, nightly→cache | Cache fuzz-nightly em corelink-hash.yml | Run e cache desconhecidos |
| [REL-015](#rel-015) | data, nightly→cache | Cache fuzz-matrix em nightly.yml | Run e cache desconhecidos |

<a id="rel-001"></a>
### REL-001 — Dependência compartilhada corelink-hash
- **Identidade / fingerprint:** `repo:1232040291:boundary:hash-fuzz-harness-001`; `rel-001:v1:sha256:8bf1f82daa5ae77adca4b3f2e0c1d2e27799686b0a23192ea6ce65ff94dda8cc`.
- **Endpoints / owner:** producer `repo:1232040291:crates/corelink-hash/Cargo.toml`; consumer `repo:1232040291:crates/corelink-hash/fuzz/Cargo.toml`; dependência fuzz→hash; contrato `corelink-hash`; implementação `corelink-hash-fuzz`.
- **Contrato / ativação:** path e `Digest`/`VerifiedBody` (S01/S03/S04); dois bins selecionados externamente; PR/nightly declarados (S09/S10), não execuções; alvos em REL-002/003.
- **Dados / impacto / falha:** bytes/resultados em memória; API/engine pode alterar build/oráculo, crash ou job; sem reachability produtiva demonstrada.
- **Validação / estado:** Cargo target, fuzz, cobertura e runtime UNKNOWN; validar metadata, bins, package e workflows; S14 registra preparação falha e steps skipped.
- **Peer / anchors:** peer hash BLAST REL-029 deve manter endpoints/fingerprint; S01 manifest, S03/S04 targets, S09/S10 workflows, S14 snapshot em [R08](REFERENCE.md#r08); revisão REQUIRED. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Oráculo do parser de digest
- **Identidade / peer:** `repo:1232040291:boundary:hash-fuzz-digest-parse-001`; parent corelink-hash BLAST REL-030 usa a mesma identidade/fingerprint.
- **Endpoints / owner:** consumer `repo:1232040291:crates/corelink-hash/fuzz/fuzz_targets/digest_parse.rs`; provider `repo:1232040291:crates/corelink-hash/src/digest.rs`; contrato hash; implementação/oráculo fuzz.
- **Superfície / ativação:** bytes não UTF-8 retornam; UTF-8 chama `Digest::from_hex`; bin selecionado pelo engine (S03); sem execução observada.
- **Contrato / falha:** UTF-8 não deve causar panic; `Result` é descartado; panic pode derrubar job; cobertura/reachability UNKNOWN.
- **Estado / efeito:** entrada e resultado ficam no processo; panic altera status do job.
- **Boundary:** sem produção, storage ou prova de cobertura.
- **Coordenação/validação:** peer REL-030; S03/R08; validar UTF-8/invalid; reviewer/cold REQUIRED. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Oráculo do corpo verificado
- **Identidade / peer:** `repo:1232040291:boundary:hash-fuzz-verify-body-001`; parent corelink-hash BLAST REL-031 usa a mesma identidade/fingerprint.
- **Endpoints / owner:** consumer `repo:1232040291:crates/corelink-hash/fuzz/fuzz_targets/verify_body.rs`; provider `repo:1232040291:crates/corelink-hash/src/{digest,verified_body}.rs`; contrato hash; implementação/oráculo fuzz.
- **Superfície / ativação:** prefixo 32 bytes é claim, sufixo `Bytes`; usa Digest/VerifiedBody; bin `verify_body` selecionado pelo engine (S04); sem execução.
- **Contrato / falha:** match exige Ok/comprimento; mismatch Err; assertion pode gerar crash; cobertura/runtime UNKNOWN.
- **Estado / efeito:** claim/body/Bytes ficam no processo; assertion altera status do job.
- **Boundary:** sem streaming, storage ou prova de runtime.
- **Coordenação/validação:** peer REL-031; S04/R08; validar `<32`, match/mismatch; reviewer/cold REQUIRED. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Buffer bytes
- **Identidade:** repo:1232040291:boundary:hash-fuzz-bytes-001.
- **Dependência / fluxo / impacto:** fuzz→bytes; bidirecional; bytes→fuzz.
- **Superfície:** dependência direta; Bytes::copy_from_slice(body_bytes) em verify_body.
- **Ativação:** target verify_body; declaração normal no manifesto.
- **Contrato:** cópia dos bytes do corpo para argumento de VerifiedBody::new.
- **Estado / efeitos:** alocação em memória proporcional ao corpo de entrada.
- **Falha / propagação:** input grande pode consumir memória; limite efetivo não foi observado.
- **Contenção:** não há limite explícito no harness revisado.
- **Validação:** nenhuma seleção/resolução/teste executado.
- **Coordenação / fontes:** boundary externa; S01/S04 em [R08](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Engine libfuzzer-sys
- **Identidade:** repo:1232040291:boundary:hash-fuzz-libfuzzer-001.
- **Dependência / fluxo / impacto:** fuzz→libfuzzer-sys; provider→consumer; engine→fuzz.
- **Superfície:** dependência direta e fuzz_target! em ambos os bins.
- **Ativação:** somente runner cargo-fuzz/engine compatível.
- **Contrato:** engine entrega &[u8] ao closure; comportamento real depende da build/invocação.
- **Estado / efeitos:** processo local; corpus/artifacts não rastreados no tree.
- **Falha / propagação:** engine/build ausentes impedem execução; metadata não prova suporte.
- **Contenção:** versão declarada 0.4; lock consta em S02, sem resolver nesta revisão.
- **Validação:** nenhum bin foi buildado ou executado.
- **Coordenação / fontes:** boundary externa; S01–S04 em [R08](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Smoke de PR em corelink-hash.yml
- **Identidade:** repo:1232040291:boundary:hash-fuzz-pr-smoke-001.
- **Dependência / fluxo / impacto:** workflow→fuzz; não aplicável; fuzz→workflow.
- **Superfície:** job fuzz-smoke; dois alvos, max_total_time=60; cargo-fuzz declarado.
- **Ativação:** pull_request e paths de hash, manifests/locks ou workflow.
- **Contrato:** comando com falha pode falhar o job; 60 s não é cobertura completa.
- **Estado / efeitos:** runner self-hosted mac; cache de build declarado.
- **Falha / propagação:** falha do target/build afeta job PR; run desconhecido.
- **Contenção:** YAML não atesta runner/job executado.
- **Validação:** S14 registra falha antes dos fuzz steps, ambos skipped; peer workflow [corelink-hash REL-019](../corelink-hash/BLAST_RADIUS.md#rel-019); distinta da REL-001.
- **Coordenação / fontes:** integração do repo; S09 em [R08](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Job nightly condicionado sem trigger ativo
- **Identidade:** repo:1232040291:boundary:hash-fuzz-hash-nightly-config-001.
- **Dependência / fluxo / impacto:** workflow→fuzz; não aplicável; fuzz→workflow.
- **Superfície:** job fuzz-nightly no workflow hash; dois alvos, 3600 s, if: schedule.
- **Ativação:** schedule comentado; evento on ativo declara somente pull_request.
- **Contrato:** job declarado, sem caminho pelo trigger atual.
- **Estado / efeitos:** estado CI não observado.
- **Falha / propagação:** reativar schedule cria execução/custo por alvo.
- **Contenção:** não tratar este job como nightly ativo.
- **Validação:** trigger/job conferidos estaticamente; workflow não executado.
- **Coordenação / fontes:** integração do repo; S09 em [R08](REFERENCE.md#r08). [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Matrix scheduled/manual em nightly.yml
- **Identidade:** repo:1232040291:boundary:hash-fuzz-nightly-matrix-001.
- **Dependência / fluxo / impacto:** workflow→fuzz; não aplicável; fuzz→workflow.
- **Superfície:** matrix inclui digest_parse e verify_body; 3600 s por target.
- **Ativação:** cron 23 4 * * * e workflow_dispatch; runner mac declarado.
- **Contrato:** resultado pode determinar job; YAML não prova run/pass/fail.
- **Estado / efeitos:** runner temporário; upload em falha é REL-009.
- **Falha / propagação:** crash/build/timeout pode falhar uma leg.
- **Contenção:** GitHub/runner não inspecionados; execução desconhecida.
- **Validação:** fonte S10 em [R08](REFERENCE.md#r08).
- **Coordenação / fontes:** integração do repo; não é consumer runtime. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Crash artifacts do matrix nightly
- **Identidade:** repo:1232040291:boundary:hash-fuzz-crash-artifact-001.
- **Dependência / fluxo / impacto:** workflow→upload action; artifact→CI store; disponibilidade→triage.
- **Superfície:** upload condicional no failure; nome tem crate/target; retenção declarada de 14 dias.
- **Ativação:** falha da matrix; targets hash pela REL-008.
- **Contrato:** disponibiliza artifacts existentes; if-no-files-found: ignore.
- **Estado / efeitos:** crash/timeout/leak input; sem corpus persistente declarado.
- **Falha / propagação:** ausência de artifact não prova ausência de run/crash.
- **Contenção:** serviço externo/retention real não observados.
- **Validação:** fonte S10 em [R08](REFERENCE.md#r08).
- **Coordenação / fontes:** integração do repo; nenhum dado de cliente identificado no input sintético. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Lista manual scripts/fuzz-all.sh
- **Identidade:** repo:1232040291:boundary:hash-fuzz-all-script-001.
- **Dependência / fluxo / impacto:** script→fuzz; não aplicável; fuzz→script.
- **Superfície:** lista contém digest_parse e verify_body; default declarado de 300 s.
- **Ativação:** invocação manual com toolchain nightly/cargo-fuzz.
- **Contrato:** script documenta fail-closed após comando com falha.
- **Estado / efeitos:** pode criar corpus/artifact ignorado pelo Git.
- **Falha / propagação:** exit não-zero compõe FAILED e termina com erro.
- **Contenção:** script lido, não executado; não é automação provada.
- **Validação:** S11/S12 em [R08](REFERENCE.md#r08).
- **Coordenação / fontes:** owner do script desconhecido; não é teste unitário. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Gate PR path-triggered do package hash
- **Identidade / fingerprint:** `repo:1232040291:boundary:hash-ci-fuzz-pr-001`; peer corelink-hash REL-034 deve usar a mesma chave.
- **Endpoints:** workflow `corelink-hash.yml`→job `fuzz-smoke`; owner CI UNKNOWN.
- **Dependência/data/impacto:** path→job; inputs→result; status→PR.
- **Superfície:** S09 paths `crates/corelink-hash/**`, manifests, workflow; job fuzz-smoke.
- **Ativação:** PR elegível e `pull_request`; target digest_parse/verify_body selecionado pelo job.
- **Contrato:** smoke 60 s por alvo; YAML não prova compilação/execução.
- **Estado:** runner self-hosted Mac declarado, run UNKNOWN; **efeito:** falha pode bloquear PR.
- **Boundary:** não é runtime produtivo nem prova de cobertura; cache é REL-013.
- **Coordenação:** peer REL-034 parent; S09 em [R08](REFERENCE.md#r08); revisão REQUIRED.
- **Validação:** conferir paths/condition/targets; run não executado. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — wasm-build path-triggered do package hash
- **Identidade / fingerprint:** `repo:1232040291:boundary:hash-ci-wasm-001`; peer corelink-hash REL-035 deve usar a mesma chave.
- **Endpoints:** workflow `corelink-hash.yml`→job `wasm-build`; owner CI UNKNOWN.
- **Dependência/data/impacto:** path→job; source→check; status→PR.
- **Superfície:** S09 `wasm-build`, target `wasm32-unknown-unknown`, `cargo check --package corelink-hash`; paths parent.
- **Ativação:** PR elegível; job declarado; sem fuzz-smoke/execução.
- **Contrato:** compilação wasm de `corelink-hash`; **não** compila nem executa fuzz targets.
- **Estado:** runner/output UNKNOWN; **efeito:** falha pode bloquear PR.
- **Boundary:** compatibilidade de compilação, não runtime Cloudflare; cache separado em REL-013.
- **Coordenação:** peer REL-035; S09/R08; revisão REQUIRED.
- **Validação:** conferir path/job/target e ausência de fuzz; não executado. [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Cache declarado do fuzz-smoke
- **Identidade / fingerprint:** `repo:1232040291:boundary:hash-fuzz-cache-001`.
- **Endpoints:** workflow→`Swatinem/rust-cache`; workspace `crates/corelink-hash/fuzz -> target`; owner CI UNKNOWN.
- **Dependência/data/impacto:** cache→build; objetos→runner; hit/miss→tempo/status.
- **Superfície:** S09 cache condicionado a `runner.environment == github-hosted`; fuzz-smoke declara runner self-hosted Mac.
- **Ativação:** condição runner verdadeira; self-hosted pula cache.
- **Contrato:** cache é aceleração, não prova de build, fuzz, cobertura ou runtime.
- **Estado:** cache remoto/runner UNKNOWN; **efeito:** altera warmness e custo, não contrato do alvo.
- **Boundary:** não mistura wasm `. -> target`; parent wasm REL-012.
- **Coordenação:** owner UNKNOWN; S09/R08; revisão REQUIRED.
- **Validação:** conferir `if`/workspace/runner; sem consulta live. [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Cache fuzz-nightly em corelink-hash.yml
- **Identidade/fingerprint:** `repo:1232040291:boundary:hash-fuzz-nightly-cache-001`; peer parent REL-037, mesma chave.
- **Endpoints:** S09 `.github/workflows/corelink-hash.yml:233-236`→`Swatinem/rust-cache`; workspace `crates/corelink-hash/fuzz -> target`; owner CI UNKNOWN.
- **Dependência/dados/impacto:** fuzz-nightly→cache→fuzz; objetos→runner; hit/miss→duração/status.
- **Superfície/ativação:** job `fuzz-nightly`; condição `runner.environment == github-hosted`; self-hosted pode pular; nenhum run observado.
- **Contrato:** aceleração; não prova build, fuzz, cobertura ou runtime.
- **Estado/efeito:** cache remoto e runner UNKNOWN; só warmness/custo mudam.
- **Boundary:** distinto de REL-013 (PR) e REL-015 (matrix); sem efeito produtivo.
- **Coordenação/validação:** parent REL-037; S09/R08; conferir linhas, condição, workspace e runner; revisão REQUIRED; sem live. [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Cache fuzz-matrix em nightly.yml
- **Identidade/fingerprint:** `repo:1232040291:boundary:hash-fuzz-matrix-cache-001`; peer parent REL-038, mesma chave.
- **Endpoints:** source matrix `crate: corelink-hash` em S10 `.github/workflows/nightly.yml:371-372`; cache S10 `.github/workflows/nightly.yml:418-421`→`Swatinem/rust-cache`; workspace `crates/corelink-hash/fuzz -> target`; owner CI UNKNOWN.
- **Dependência/dados/impacto:** matrix `corelink-hash`→cache→fuzz; objetos→runner; hit/miss→duração/status.
- **Superfície/ativação:** job `fuzz-matrix`, targets `digest_parse`/`verify_body`; condição `runner.environment == github-hosted`; self-hosted pode pular; run não observado.
- **Contrato:** aceleração; não prova build, fuzz, cobertura ou runtime.
- **Estado/efeito:** cache remoto e runner UNKNOWN; só warmness/custo mudam.
- **Boundary:** distinto de REL-013 (PR) e REL-014 (fuzz-nightly); sem efeito produtivo.
- **Coordenação/validação:** parent REL-038; S10/R08; conferir linhas, condição, workspace e matrix; revisão REQUIRED; sem live. [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho por REL | Condição | Efeito causal | Limite |
|---|---|---|---|---|
| Resultado de PR | REL-001/002/003/004/005 → REL-006 | PR e paths elegíveis | Dependência, bytes ou engine podem mudar build/crash do job | S14 registra tentativa antiga sem executar targets; pin atual desconhecido |
| Gate extended | REL-001/002/003/004/005 → REL-008 | Schedule/manual e leg selecionada | Dependência, bytes ou engine podem mudar resultado da leg | Runner, duração e output não observados |
| Revisão de crash | REL-008 → REL-009 | Failure e artifact existente | Reprodutor pode ser retido no CI | Conteúdo/retention efetiva desconhecidos |
| Fuzz-all | REL-001/004/005/002/003 → REL-010 | Script invocado | Dependência, bytes, engine ou oráculo mudam a lista/status final | Execução manual não observada |
| Parent PR fuzz-smoke | REL-001/002/003/004/005 → REL-011 → REL-006 | Path do package hash em PR | Gate parent pode selecionar fuzz-smoke e alterar resultado | S09 é YAML; run/runner UNKNOWN |
| Parent wasm gate | REL-001/002/003 → REL-012 | Path do package hash em PR | Falha wasm pode bloquear PR; não compila fuzz | S09 só declara check |
| Cache condicionado | REL-011 → REL-013 → REL-006 | Runner github-hosted e cache hit | Warmness/custo podem alterar duração, não contrato | Condição/cache remoto UNKNOWN |
| Cache fuzz-nightly | REL-007 → REL-014 | Job fuzz-nightly selecionado e condição de runner | Warmness/custo podem alterar duração, não contrato | S09; run/cache UNKNOWN |
| Cache fuzz-matrix | REL-008 → REL-015 → REL-009 | Matrix `corelink-hash` selecionada para `digest_parse`/`verify_body` e condição de runner | Warmness/custo podem alterar duração, não contrato | S10; run/cache UNKNOWN |

Não há caminho demonstrado dos harnesses para chamada HTTP, banco, bucket ou provider. O fato de corelink-hash ter consumers de produto não propaga automaticamente runtime do fuzz package a esses consumers.

<a id="b05"></a>
## B05 — Mudança, impacto e validação

| Mudança | Relações / superfície | Efeito esperado | Validação apropriada | Recuperação |
|---|---|---|---|---|
| Mudar input/oráculo do parser | REL-001/002, INV-001 | Inputs aceitos ou panic signal mudam | Inspecionar filtro e erro; smoke isolado autorizado | Preservar corpus/crash antigo; reavaliar assertion |
| Mudar split/body/oráculo | REL-001/003/004, INV-002 | Predicados/uso de memória mudam | Casos <32, match, mismatch; fuzz bounded local | Preservar input e comparar versão anterior |
| Mudar dependência/target | REL-001/004/005 | Build e harness selecionado mudam | Censo manifesto/lock, bin exato, incompatibilidade | Restaurar só edição própria e lock compatível |
| Alterar PR/nightly wiring | REL-006/007/008/009/011/012/013/014/015 | Trigger, duração, runner, wasm, cache ou retention muda | Revisar paths/event/job/artifact/target/cache e limites | Reverter config própria; verificar runs separado |
| Mudar script target list | REL-010 | Seleção manual/status global mudam | Revisar pares e duração/default | Reverter só edição do script |

Não usar build verde para certificar runtime, cobertura ou compatibilidade produtiva. Mudanças em APIs hash precisam do owner de corelink-hash; impacto persistido pertence ao writer/consumer real.

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos com motivo | Desconhecidos |
|---|---:|---:|---:|---|
| Identidade standalone | 1 | 1 | 0 | Resolve Cargo |
| Bin targets | 2 | 2 | 0 | Build e execução |
| Deps normais declaradas | 3 | 3 | 0 | Grafo resolvido/features |
| Relações específicas desta edição | 15 | 15 | 0 | População global não certificada |
| Consumers Cargo inversos literais | 0 observados | 0 | 0 | Alias, path indireto, geração e resolução |
| Interfaces fora de Cargo operacionais | 5 | 5 | Specs/reports históricos e targets homônimos sem vínculo | Runs/retention/runner |
| Corpus no tree source | 0 rastreados | 0 | 0 | Corpus local/remoto e seeds não rastreados |

As cinco interfaces fora de Cargo documentadas são: exclusão no root manifest; PR smoke e job condicionado no workflow hash; nightly matrix e artifact failure upload; scripts/fuzz-all.sh; regras de ignore para artefatos locais. S14 é evidência histórica limitada: um job hash de PR falhou ao preparar toolchain antes dos comandos, e digest_parse/verify_body ficaram skipped; logs crus não foram preservados na snapshot. O índice de REL agrupa workflow/artifact por superfície causal, não transforma workflow em package Cargo.

**Exclusões:** ocorrências em packages fuzz distintos, DigestParseError, testes com nome semelhante, TLA, inventário LOC e histórico/spec selada não consomem este manifest por si só. As buscas exatas encontraram essas classes; não são prova de ausência de outros vínculos.

**Pendências:** Cargo metadata/resolution/inverse; target/features efetivos; adequação de census independente; aliases e geração; status real de workflows; conteúdo/retention de corpus e crash artifacts; comportamento do toolchain/engine; owner nominal e peer review da REL-001. Nenhuma ausência global, cobertura ou execução é certificada.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
