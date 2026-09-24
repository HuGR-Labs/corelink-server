---
schema: corelink-ownership/1.1
document: reference
package: corelink-wasm
manifest: crates/corelink-wasm/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-wasm-structural-normalization-20260921
---

# corelink-wasm — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) · [Contratos](#r04) · [Estado](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003)

`corelink-wasm` é um wrapper Rust de um único arquivo sobre `corelink-client-verify`. O manifesto declara `crate-type = ["cdylib"]` e `publish = false`; a metadata `wasm-pack` e `package.json` são descrições estáticas. Nada nesta referência afirma que um módulo WASM, arquivos `pkg/`, pacote npm ou API JavaScript tenha sido produzido, publicado ou executado.

| Campo | Valor verificado em fonte |
|---|---|
| Package / manifesto | `corelink-wasm` / `crates/corelink-wasm/Cargo.toml` |
| Implementação | somente `crates/corelink-wasm/src/lib.rs` |
| Artifact declarado | `cdylib`, nome `corelink_wasm` |
| Publicação Rust | `publish = false` |
| Dependência de integridade | path `../corelink-client-verify`, sem features explícitas |
| Runtime / build / pacote distribuído | não observado |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Este crate possui | Coordenação/limite |
|---|---|---|
| ponte | struct e impl anotados `wasm_bindgen` | macro determina bindings gerados, não inspecionados |
| integridade | chamada a `Digest`, `VerifyConfig` e `ClientVerifier` | algoritmo e política canônica pertencem a client-verify |
| pacote | descrição `@corelink/client` e lista de arquivos em `package.json` | não prova arquivo, npm ou consumidor |
| CAS | helpers locais `get_inner`, `put_inner`, `stat_inner` | não há cliente HTTP nem backend nesta fonte |

O diretório `sdks/js` também declara `@corelink/client`; esta inspeção não estabelece identidade, seleção, publicação ou compatibilidade entre ele e `crates/corelink-wasm/package.json`.

<a id="r03"></a>
## R03 — Mapa da implementação

| Arquivo/parte | Papel | Evidência SOURCE |
|---|---|---|
| `src/lib.rs` | crate root, `#![deny(unsafe_code)]`, structs e bridge | único arquivo Rust do crate |
| `ClientConfig` | desserializa `pat`, `tenantId` e `clientVerify` camelCase | `Deserialize`, `default_true` |
| `CoreLinkClient` | conserva PAT/tenant, verifier e flag | `#[wasm_bindgen]` struct/impl |
| `get_inner` | parseia digest e verifica um corpo vazio stub quando flag está ativa | comentário e chamada `verifier.verify` |
| `put_inner` / `stat_inner` | calcula digest / valida e retorna metadata stub | `Digest::compute` / `Digest::from_hex` |

<a id="r04"></a>
## R04 — Contratos públicos

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003).

<a id="api-001"></a>
### API-001 — Construção e flag de verificação
**Símbolos fonte:** `ClientConfig` (Rust público), `CoreLinkClient`, `CoreLinkClient::new`, getter `_clientVerifyEnabled`.
**Entrada/saída declaradas:** `new(JsValue) -> Result<CoreLinkClient, JsValue>` desserializa `{ pat, tenantId, clientVerify? }`; o getter retorna `bool`. `ClientConfig` usa `serde(rename_all = "camelCase")` e `client_verify` tem default `true`.
**Efeito/erro:** com opt-out a fonte chama `VerifyConfig::disabled()` e emite warning `COR_CAS_VERIFY_DISABLED`; configuração inválida é convertida em `JsValue`.
**Limite:** `ClientConfig` é público em Rust, mas não anotado `wasm_bindgen`; os exports realmente gerados não foram inspecionados.
**Prova:** INV-001; `src/lib.rs`.
[Índice](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Operações anotadas de conteúdo
**Símbolos fonte:** `CoreLinkClient::get`, `put`, `stat`, todos em `impl` `#[wasm_bindgen]`.
**Assinaturas declaradas:** `get(String) -> Result<js_sys::Uint8Array, JsValue>`; `put(js_sys::Uint8Array) -> Result<String, JsValue>`; `stat(String) -> Result<JsValue, JsValue>`.
**Efeito/erro:** `put` calcula o digest localmente; `get` usa o helper de corpo vazio; `stat` serializa um resultado stub. Digest inválido ou mismatch torna-se `JsValue`.
**Limite:** estas assinaturas e anotações são evidência de fonte, não confirmação de nomes JS, Promise, marshaling produzido, transporte CAS ou execução no browser/Node.
**Prova:** INV-002; `src/lib.rs`.
[Índice](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Metadata de status
**Símbolo fonte:** `StatResult { digest, size_bytes, exists }` é struct Rust pública serializada com `serde(rename_all = "camelCase")`.
**Saída declarada:** `stat_inner` produz `size_bytes = 0` e `exists = false`; `stat` a converte em `JsValue`.
**Limite:** `StatResult` não recebe anotação `wasm_bindgen`; portanto não é afirmado como tipo/export JS gerado.
**Prova:** `src/lib.rs`.
[Índice](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado | Owner | Vida observada na fonte |
|---|---|---|
| PAT e tenant | `CoreLinkClient` | instância; não há getter público no arquivo |
| configuração/verifier | `CoreLinkClient` e client-verify | instância |
| corpo/metadata de `get` e `stat` | helpers locais | stub temporário por chamada |

<a id="inv-001"></a>
### INV-001 — Verificação é default-on
**Predicado:** ausência de `clientVerify` escolhe `true`; esse caminho constrói `VerifyConfig::new()`.
**Imposição:** `#[serde(default = "default_true")]` e condicional no construtor. **Violação:** alterar default ou desviar a construção do verifier. **Estado:** SOURCE; não afirma comportamento de JS gerado.
[Índice](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Helpers não simulam transporte CAS
**Predicado:** `get_inner` usa `Vec::new()` e `stat_inner` retorna `0`/`false`; `put_inner` só calcula o digest.
**Imposição:** implementações locais atuais. **Violação:** alegar download, upload ou existência persistida a partir desta fonte. **Estado:** SOURCE.
[Índice](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Fonte própria não contém operação unsafe
**Predicado:** `src/lib.rs` fixa `#![deny(unsafe_code)]`; inspeção textual do único arquivo não encontrou bloco/operação `unsafe`.
**Imposição:** atributo de crate e fonte inspecionada. **Limite:** não prova segurança de dependências, macro expandida, binário, binding ou runtime. **Estado:** SOURCE.
[Índice](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

O manifesto não declara feature própria além de `default = []`. Declara `wasm-bindgen`, `js-sys`, `wasm-bindgen-futures`, `serde`, `serde-wasm-bindgen`, `tracing` e `corelink-client-verify`; `wasm-bindgen-test` e proptest entram como dev-dependencies. Para dev no `cfg(target_arch = "wasm32")`, fixa `getrandom` 0.3 com feature `wasm_js`. A metadata de release do wasm-pack contém opções de `wasm-opt`; isso é configuração declarada, não build. `package.json` declara `type: module`, arquivos esperados em `pkg/` e `typescript` devDependency, sem estabelecer compatibilidade, instalação ou publicação.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal de fonte | Significado limitado | Ação de manutenção |
|---|---|---|
| `invalid config:` | falha de desserialização antes de construir cliente | conferir forma Rust/serde, não uma exceção JS observada |
| `COR_CAS_DIGEST_MISMATCH` | helper mapeia mismatch do verifier | preservar código; escalar política ao client-verify |
| `COR_CAS_VERIFY_DISABLED` | opt-out constrói configuração desabilitada e registra warning | confirmar intenção explícita; não alegar entrega de telemetria |

<a id="r08"></a>
## R08 — Verificação e evidências

Foram inspecionados manifesto, `package.json`, único `lib.rs`, manifesto/fonte de `corelink-client-verify` necessários para a borda e referências estáticas do nome do pacote. A busca textual dos itens Rust públicos identificou os símbolos de R04; a busca de `unsafe` no arquivo achou somente a negação no crate root, sem operação unsafe. Não foram executados Cargo, wasm-pack, wasm-opt, testes, Node, browser, npm ou rede. A geração macro, grafo inverso resolvido, artefato `pkg`, compatibilidade JS/TS e publicação continuam desconhecidos.

[Ownership guide](../../../../.claude/skills/own-corelink-wasm/SKILL.md#s01) · [Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).
