---
schema: corelink-ownership/1.1
document: reference
package: corelink-audit-chain-fuzz
manifest: crates/corelink-audit-chain/fuzz/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: "S"
state: draft
evidence_set: corelink-audit-chain-fuzz-structural-normalization-20260921
---

# corelink-audit-chain-fuzz — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) ·
[Contratos](#r04) · [Estado e invariantes](#r05) · [Configuração](#r06) ·
[Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

Record index: [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003)

Este package é um workspace Cargo independente que declara dois binários de cargo-fuzz: `merkle_append` e `jcs_canonicalize`. Os arquivos inspecionados são harnesses estáticos; não demonstram execução, wiring de produção, persistência remota nem conformidade regulatória. A integração parte de `d80f245e0be3c61c250249de8292e42a6cd8ef5d`; a fonte de conteúdo está pinada em `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`, conforme front matter deste conjunto.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | `corelink-audit-chain-fuzz` / `crates/corelink-audit-chain/fuzz/Cargo.toml` |
| Library / binaries / outros targets | Sem library; bins `merkle_append`, `jcs_canonicalize`; `test`, `doc`, `bench` falsos nos dois |
| Papel | Harness de fuzz isolado; não production root |
| Licença / publish efetivos | `UNLICENSED` / `false` |
| Implementação / wiring / runtime observado | Código declarado: yes; ligação de runner: configurada em dois locais; execução/runtime: unknown, nenhum resultado inspecionado |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Papel | Owner verificável | Limite observado |
|---|---|---|
| Implementação do harness | Este package e seus três arquivos | Nenhuma pessoa/equipe atribuída nos arquivos examinados |
| Contrato Rust consumido | `corelink-audit-chain`; `corelink-analytics` para `Region` | Crates provedoras, não este harness |
| Contrato de canonicalização e parsing | `serde_jcs`; `serde_json` | Dependências externas declaradas |
| Composição da execução | `scripts/fuzz-all.sh`; `.github/workflows/fuzz-nightly.yml` | Configuração estática; operator humano unknown |
| Revisão / aprovação | Não identificado | Requer revisor independente designado |

**Não faz:** exportar uma API de library, provar semântica de produção, testar ingestão real, operar agendamento, ou provar banco, object storage, SIEM ou runtime. **Absorções e aliases:** nenhum package absorvido; basename `fuzz/` não é identidade; o nome Cargo é `corelink-audit-chain-fuzz`.

OKF é referência canônica de roteamento: [audit-chain](../../../knowledge/compliance/audit-chain.md) cobre integridade/canonicalização, e [audit-analytics](../../../knowledge/crates/audit-analytics.md) cobre o plano de analytics. Esses documentos não são duplicados nem tratados como prova de execução; contratos dos crates devem ser reconciliados nos respectivos owners.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo / entradas | Papel e dado sob ownership | Natureza | Evidência |
|---|---|---|---|
| `fuzz/Cargo.toml` | Workspace próprio, deps, lint e bins | Manifest | [F01](#f01) |
| `fuzz_targets/merkle_append.rs` | Bytes viram sementes limitadas, sequência sintética de eventos e asserts sobre dois heads | Harness owned | [F02](#f02) |
| `fuzz_targets/jcs_canonicalize.rs` | Bytes JSON válidos passam por JCS; compara bytes canônicos | Harness owned | [F03](#f03) |

**Inventário:** 3 caminhos rastreados no subtree do package no pin: 1 manifesto e 2 alvos. Nenhum source adicional foi enumerado; Cargo/targets resolvidos não foram executados.

<a id="r04"></a>
## R04 — Contratos públicos

**API de library:** nenhuma declarada por este package. Os dois binários não são API Rust pública; os alvos expõem somente convenções de entrada e asserções locais descritas abaixo. Contratos dos símbolos importados pertencem aos crates provedores e devem ser revistos lá.

| Alvo / símbolos exatos / âncoras | Comportamento afirmado pelo código | Limite do predicado |
|---|---|---|
| `merkle_append`; `kinds() -> [AuditEventKind; 8]` (`merkle_append.rs:32-43`); `mk_event(...) -> AuditEvent` (`:45-64`); `HashChainBuilder::new`, `ChainHash::genesis`, `append(&AuditEvent)` (`:94-105`) | Com pelo menos dois pares, constrói duas sequências e compara `Option<ChainHash>`; troca os dois primeiros se distintos e ambos `Some` | `append` com erro vira `None`; dois `None` igualam. Não prova append sempre válido ou ausência universal de panic |
| `jcs_canonicalize`; `serde_json::from_slice::<serde_json::Value>` (`jcs_canonicalize.rs:24-28,39-40`); `serde_jcs::to_vec(&Value)` (`:33-42,51-52`) | JSON aceito passa por RFC 8785/JCS, reparse, segunda serialização e comparação determinística | JSON inválido/primeiro `serde_jcs::Err` sai cedo; `expect`/assert sinaliza falha do alvo, não contrato universal |

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Recurso | Owner / vida útil | Persistência observada nos targets |
|---|---|---|
| Bytes, eventos sintéticos, `Value`, canonical bytes e heads | Buffer/stack/heap de uma iteração do harness | Nenhuma escrita a DB ou storage aparece nos dois arquivos |
| Corpus, crash artifacts e cache | Fora do fluxo de memória dos alvos; configurados no invocador | Conteúdo presente/retido não foi observado; ver [M05](MAINTENANCE.md#m05) |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003).

<a id="inv-001"></a>
### INV-001 — Repetição do mesmo sequence não altera o `Option` do head

**Regra:** duas construções com mesma `tenant`, `time_base` e lista gerada devem comparar iguais. **Imposição:** assert em `merkle_append.rs:108-113`. **Violação / prova:** fuzz input que produz heads distintos; não distingue dois erros (`None`). Fonte [F02](#f02); fuzz não executado nesta revisão.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Trocar os dois primeiros pares distintos muda o head se ambos são `Some`

**Regra:** comparação de ordem só roda quando os primeiros `(kind, seed)` diferem e os dois builds retornam `Some`. **Imposição:** guarda/assert em `merkle_append.rs:115-129`. **Limite:** eventos posteriores não participam da troca. Fonte [F02](#f02); fuzz não executado.

[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — JCS produz bytes fixos nas duas recomputações alcançadas

**Regra:** para `Value` aceito por `serde_jcs::to_vec`, a reserialização RFC 8785 após parse e a segunda chamada no valor original igualam `canon1`. **Imposição:** `serde_json::from_slice`/`serde_jcs::to_vec` e asserts em `jcs_canonicalize.rs:24-56`. **Limite:** bytes inválidos e erro inicial são saídas antecipadas; `-max_len` é controle operacional, não prova de cobertura. Fonte [F03](#f03); fuzz não executado.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Nome / fonte | Default efetivo | Target / condição | Efeito / falha |
|---|---|---|---|
| `[workspace]`, Cargo manifest | Workspace separado | Manifesto do fuzz package | Não herda automaticamente a raiz do workspace |
| `cargo-fuzz = true` | Marcador presente | `package.metadata` | Declara integração cargo-fuzz; não prova versão instalada |
| Features do package | Nenhuma declarada | Dois bins | Combinações de features não verificadas por execução |
| Lints | `unsafe_code=forbid`; `unwrap_used`, `panic`, `indexing_slicing`, `todo`, `unimplemented` deny | Bin package | Lints declarados não substituem compilação |
| Limite operacional | Sem cap no código de entrada | Procedimentos exigem `-max_len=4096` por target | Cap do runner não altera o predicado JCS nem prova exaustividade |

**Matriz declarada:** `merkle_append` → `fuzz_targets/merkle_append.rs`; `jcs_canonicalize` → `fuzz_targets/jcs_canonicalize.rs`. Cada bin desativa test/doc/bench. Dependências: `libfuzzer-sys`, `corelink-audit-chain` (path `..`), `corelink-analytics` (path `../../corelink-analytics`), `serde_json`, `serde_jcs`, `uuid` com features `v4`, `v7`. Nenhuma resolução foi executada.

<a id="r07"></a>
## R07 — Erros e observabilidade

| Sinal | Origem | Limite observado |
|---|---|---|
| Assert/`expect` panic | Harness encontra bytes canônicos ou igualdade divergente | Fuzzer/runner pode sinalizar falha; execução não observada |
| `append`/primeiro JCS `Err` | Tratamento de `Option` ou retorno antecipado | Pode reduzir propriedades cobertas; não é evidência de sucesso de produção |
| Logs, métricas e tracing | Nenhum emissor presente nos dois alvos | Não inferir observabilidade do servidor |

**Limites:** fixtures são sintetizadas no alvo; strings de comentário sobre auditoria/SOC não são prova de certificação ou de comportamento externo.

<a id="r08"></a>
## R08 — Verificação e evidências

| Fonte | Evidência imutável | Revisão / resultado |
|---|---|---|
| [F01](#f01) `fuzz/Cargo.toml:1-44` | Blob `f89476fb6d1ed518398f041435f06e7cab05124f` | `SOURCE`; leitura estática |
| [F02](#f02) `fuzz_targets/merkle_append.rs:1-131` | Blob `4362a2015da74947d8a7bc5c71204ec96758f51f` | `SOURCE`; leitura estática |
| [F03](#f03) `fuzz_targets/jcs_canonicalize.rs:1-57` | Blob `df89e3e760caad406392593f2a9db4ce50b8c439` | `SOURCE`; leitura estática |
| [F04](#f04) `.github/workflows/fuzz-nightly.yml:18-48,84-93,108-183` | Blob `61b86f724660f3ba318c6a1c03b7b986a9934ee9` | `SOURCE`; config apenas |
| [F05](#f05) `scripts/fuzz-all.sh:8-79` | Blob `e4dd689534c76c84554f54019f588fd440c11e59` | `SOURCE`; invocador apenas |

<a id="f01"></a>
### F01 — Declaração do package

Fonte `crates/corelink-audit-chain/fuzz/Cargo.toml`, linhas 1–44, no commit pinado. Confirma nome, workspace, dependências, lint e dois bins declarados; não confirma resolução nem build.

<a id="f02"></a>
### F02 — Harness de append

Fonte `crates/corelink-audit-chain/fuzz/fuzz_targets/merkle_append.rs`, linhas 1–131, no commit pinado. Evidência apenas do input mapping, chamadas importadas e asserções visíveis.

<a id="f03"></a>
### F03 — Harness de JCS

Fonte `crates/corelink-audit-chain/fuzz/fuzz_targets/jcs_canonicalize.rs`, linhas 1–57, no commit pinado. Evidência apenas dos guards, chamadas e asserts visíveis.

<a id="f04"></a>
### F04 — Workflow configurado

Fonte `.github/workflows/fuzz-nightly.yml`, linhas citadas acima, no commit pinado. Confirma texto de workflow/config; não confirma dispatch, runner ou resultado.

<a id="f05"></a>
### F05 — Script invocador

Fonte `scripts/fuzz-all.sh`, linhas 8–79, no commit pinado. Confirma a seleção e o comando declarados; não confirma execução.

Voltar ao índice de [evidências](#r08).

**Desconhecidos:** comandos Cargo/fuzz, targets resolvidos, resultado de CI, corpus/artifacts atuais, wiring de produção e owner humano. Os conceitos OKF canônicos estão roteados acima; sua aplicação ao runtime não foi observada. Baseline do checkout não substitui o source pin. **Continuar:** [Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).

[Voltar ao início](#r01)
