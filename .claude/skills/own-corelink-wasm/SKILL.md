---
name: own-corelink-wasm
description: >-
  Own static source and package-metadata review for the corelink-wasm cdylib
  wrapper; do not infer generated bindings, publication, browser, or npm use.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-wasm
  manifest: crates/corelink-wasm/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  profile: "S"
  evidence-set: corelink-wasm-structural-normalization-20260921
---

# Ownership — corelink-wasm

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) · [Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Alterar a ponte `wasm-bindgen`, manifesto ou metadata de pacote deste crate | Publicar npm, gerar `pkg`, executar browser/Node ou validar compatibilidade JS |
| Revisar `CoreLinkClient`, seus métodos anotados ou a fronteira com client-verify | Alterar a semântica de `corelink-client-verify` sem o owner daquele crate |
| Investigar target/dependência WASM declarados | Tratar declaração de target, CI ou `package.json` como artefato produzido ou executado |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** `crates/corelink-wasm/src/lib.rs`, a declaração `cdylib`, dependências de ponte e metadata estática do pacote.
**Contrato sob revisão:** símbolos Rust públicos e métodos marcados `#[wasm_bindgen]`, `clientVerify` default-on e códigos de erro que a fonte constrói.
**Fora do território:** geração macro de bindings, conteúdo de `pkg/`, transporte CAS real, npm, browser, Node, publicação e a implementação canônica de digest/verificação.
**Escalonamento:** owner `corelink-client-verify` para integridade, owner JS/documentação para a colisão de nome `@corelink/client`, e release para artefatos/publicação.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| Qual símbolo/método a fonte expõe? | [Referência](../../../docs/ownership/crates/corelink-wasm/REFERENCE.md#r04) e `crates/corelink-wasm/src/lib.rs` |
| Quem depende, documenta ou pode confundir o pacote? | [Blast radius](../../../docs/ownership/crates/corelink-wasm/BLAST_RADIUS.md#b03) |
| Qual validação é permitida? | [Manutenção](../../../docs/ownership/crates/corelink-wasm/MAINTENANCE.md#m04) |
| Qual configuração de crate foi declarada? | `crates/corelink-wasm/Cargo.toml` e `crates/corelink-wasm/package.json` |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição → ação | Evidência | Parar quando |
|---|---|---|
| Muda método `#[wasm_bindgen]` → inventariar assinatura, nome getter e conversão `JsValue` | `lib.rs`, API-001–003 | Exigir API JS gerada ou compatibilidade de consumidor |
| Muda `clientVerify` → preservar default `true`, opt-out explícito e rota por `ClientVerifier` | INV-001, `lib.rs` | Alterar política/verificador canônico |
| Muda crate type, dependência ou metadata → separar declaração de artefato observado | manifesto, REL-001/004 | Necessitar build, wasm-pack, npm ou runtime |
| Muda código inseguro → confirmar `#![deny(unsafe_code)]` e ausência de operação `unsafe` no arquivo | INV-003, fonte | Exceção unsafe, FFI manual ou justificativa de segurança |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Fixe SHA, manifesto, `package.json` e o único arquivo fonte.
2. Separe símbolos Rust públicos, itens anotados para wasm-bindgen e nomes somente serializados.
3. Trace `get`/`put`/`stat` até helpers e `corelink-client-verify`; não converta stub em transporte real.
4. Registre feature, target e pacote como declarações estáticas.
5. Atualize os três documentos; execute somente os quatro checks documentais e `git diff --check`.
6. Entregue lacunas para cold review independente.

<a id="s06"></a>
## S06 — Condições de parada

Pare diante de Cargo, wasm-pack, wasm-opt, Node, browser, npm, rede, publicação, segredo/PAT, endpoint CAS, artefato gerado ou operação de release. Pare também se uma decisão depender de export produzido por macro, comportamento de JS/TypeScript, compatibilidade de bundler ou consumidor externo: a fonte Rust não os prova.

<a id="s07"></a>
## S07 — Evidência e saída

Informe SHA, manifesto/fonte lidos, símbolos públicos inspecionados, relações estáticas, arquivos alterados, quatro checks estruturais e `git diff --check`. Declare que a inspeção não executou build WASM, geração de bindings, teste, JS/browser/Node/npm ou publicação. Não chame validação documental de runtime, compatibilidade ou cold review.

[Voltar ao início](#s01)
