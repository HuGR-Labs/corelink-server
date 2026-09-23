---
name: own-corelink-hash-fuzz
description: >-
  Assuma ownership de corelink-hash-fuzz ao alterar seus dois harnesses,
  targets, dependências ou chamadas a Digest e VerifiedBody. Oriente a revisão
  estática e validação local isolada; não use para implementar hash, operar
  storage, executar fuzz em produção ou declarar cobertura/runtime.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-hash-fuzz"
  manifest: "crates/corelink-hash/fuzz/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "hash-fuzz-source-20260921"
---

# Ownership — corelink-hash-fuzz

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use como owner principal quando |
|---|---|
| Mudar digest_parse ou seu filtro de UTF-8 | Mudar parser, Digest ou compatibilidade: coordenar corelink-hash |
| Mudar verify_body, sua entrada ou assertions | Mudar algoritmos, VerifiedBody ou storage consumidor |
| Alterar bins, dependências ou wiring cargo-fuzz/CI | Operar R2, banco, provider, deploy ou dados de cliente |

<a id="s02"></a>
## S02 — Território e autoridade

Implementação própria: os dois arquivos em fuzz_targets/ e o manifesto independente. O package corelink-hash-fuzz é excluído do workspace raiz e tem [workspace] próprio. Dono verificado da implementação do harness: package Cargo corelink-hash-fuzz (manifesto canônico). Dono verificado do contrato Digest/VerifiedBody: package corelink-hash.

| Estado independente | Valor | Limite da evidência |
|---|---|---|
| implemented | sim | fonte dos dois targets e manifesto; prova estática |
| wired | declarado | manifesto/workflows/scripts declaram seleção; não houve execução |
| runtime_verified | UNKNOWN | S14 mostra preparação falha/skips; nenhum target foi observado |

| Camada de ownership | Dono/estado verificado | Limite |
|---|---|---|
| Implementação | corelink-hash-fuzz (package) | targets e manifesto |
| Contrato público | corelink-hash (package) | Digest/VerifiedBody |
| Composition root | N/A | este harness não compõe produto |
| Operador de runtime | UNKNOWN | runner/target não observado |
| Autoridade de revisão | UNKNOWN | cold reviewer independente exigido |

O manifesto declara unsafe_code=forbid; não copiar isso como prova sobre dependências ou provider. Alvos indicam intenção, não execução. Rota de escalonamento/aprovação: issue/PR do repositório → owner do package afetado → cold reviewer independente. Owner nominal da rota: REQUIRED; identidade atual UNKNOWN; aprovação permanece bloqueada até atribuição verificada. Código ou CI alcançável não prova execução/runtime.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Abrir |
|---|---|
| Quais alvos e oráculos o código contém? | [R03/R04](../../../docs/ownership/crates/corelink-hash-fuzz/REFERENCE.md#r03) |
| Qual dependência compartilhada muda? | [REL-001](../../../docs/ownership/crates/corelink-hash-fuzz/BLAST_RADIUS.md#rel-001) |
| O que workflow e script realmente declaram? | [B02/B03](../../../docs/ownership/crates/corelink-hash-fuzz/BLAST_RADIUS.md#b02) |
| Qual contexto arquitetural é canônico? | [okf-context](../okf-context/SKILL.md): `python3 scripts/okf_context.py --tag cas --full`; depois `--file crates/corelink-hash/src/digest.rs --full` e `--file crates/corelink-hash/src/verified_body.rs --full`; [OKF CAS/AC](../../../docs/knowledge/crates/cas-ac-core.md). O manifesto fuzz pode retornar no-concepts direto. |
| Código existe mas está alcançável? | [built-not-wired](../built-not-wired/SKILL.md): separar built, wired e runtime_verified |
| Como recensear ou validar isoladamente? | [M02](../../../docs/ownership/crates/corelink-hash-fuzz/MAINTENANCE.md#m02) |
| Como manter corpus/reprodução? | [M05](../../../docs/ownership/crates/corelink-hash-fuzz/MAINTENANCE.md#m05) |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição | Ação e evidência | Parar quando |
|---|---|---|
| Alterar Digest::from_hex ou bytes de entrada | REL-001/002; conferir filtro UTF-8 e mudança no hash | Fonte/contrato do hash não está conciliado |
| Alterar verify_body ou VerifiedBody::new | REL-001/003/004; conferir limiar de 32 bytes e ramos | Chamar assertion de prova de tempo constante |
| Alterar dependência, target ou test=false | REL-005 e R06; conferir lockfile, seleção e workflow | Tratar declaração como target executado |
| Encontrar crash/corpus local | Preservar input original e hash; PROC-003 | Apagar, minimizar ou sobrescrever a única reprodução |
| Solicitar prova de production/runtime | Encaminhar ao owner/operator verificado | Harness, build, CI YAML ou fonte forem a única evidência |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme package pelo nome Cargo, os quatro paths e a baseline de fonte.
2. Leia somente as fichas de target, contrato hash e relações atingidas.
3. Faça o recenso estático PROC-001; se depender de comportamento, selecione PROC-002.
4. Registre input/oráculo, corpus/artifact envolvidos e recuperação antes da mudança.
5. Separe resultado local observado de configuração de workflow.
6. Atualize artefatos atingidos e entregue hashes para cold review independente.

<a id="s06"></a>
## S06 — Condições de parada

Pare com package/path divergente, fonte stale, contrato hash contraditório, crash sem reprodução preservada, corpus de origem desconhecida, dependência ausente que exigiria rede/instalação, ou pedido para usar credenciais, provider, produção, banco ou storage. Ausência de invocação na busca não prova ausência global. Não execute fuzz durante autoria documental.

Pare também quando owner de implementação/contrato, operador de runtime ou reviewer/autoridade de revisão estiver UNKNOWN para aprovação ou publicação; atribuição verificada é pré-condição.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue objetivo, baseline, targets/APIs/RELs/PROCs afetados, comandos realmente executados, estado real, inputs/crash preservados, limites e pendências. Não afirme cobertura, execução CI ou runtime a partir de [[bin]], comentários, assertions ou workflow YAML. Autorrevisão não vale como cold review; o estado destes artefatos continua draft.

[Voltar ao início](#s01)
