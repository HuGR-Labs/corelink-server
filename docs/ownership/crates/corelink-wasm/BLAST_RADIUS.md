---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-wasm
manifest: crates/corelink-wasm/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-wasm-structural-normalization-20260921
---

# corelink-wasm — blast radius

[Escopo](#b01) · [Método](#b02) · [Diretas](#b03) · [Propagação](#b04) · [Mudança](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Escopo

Record index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006)

Este mapa cobre relações declaradas pelo manifesto e chamadas/referências estáticas da fonte. Ele não demonstra que a crate foi selecionada numa resolução Cargo, compilada para wasm, transformada por wasm-bindgen, distribuída como `@corelink/client` ou chamada por um ambiente JavaScript.

<a id="b02"></a>
## B02 — Método e populações

| Camada | Método/evidência | Limite |
|---|---|---|
| Identidade | `Cargo.toml`, raiz do workspace e `package.json` | declaração não é publicação/seleção |
| Fonte direta | imports e chamadas em `src/lib.rs` | não prova execução nem macro expandida |
| Referência inversa | busca textual em manifests, fonte e docs | não é grafo Cargo/JS completo |
| Artefato | lista estática `files`/metadata wasm-pack | não prova existência de `pkg` |

<a id="b03"></a>
## B03 — Relações diretas

<a id="rel-001"></a>
### REL-001 — Cliente-verificador canônico
**Tipo/direção:** dependência de código; `corelink-wasm` → `corelink-client-verify`.
**Superfície/ativação:** `ClientVerifier`, `Digest`, `VerifyConfig`, `VerifyError` importados em `lib.rs`.
**Efeito/falha:** mudança de digest/config/erro pode mudar cálculo, default-on ou texto de erro deste wrapper.
**Validação/coordenação:** owner client-verify; evidência SOURCE no path dependency e import; não habilitar nem inferir features `ffi`/`stream`. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Ponte wasm-bindgen e serialização
**Tipo/direção:** dependência de bridge; este crate → `wasm-bindgen`, `js-sys`, `serde`, `serde-wasm-bindgen`.
**Superfície/ativação:** struct/impl anotados, `JsValue`, `Uint8Array`, serialização de status.
**Efeito/falha:** mudança de assinatura, annotation ou serialização pode alterar bindings produzidos ou falhar na geração.
**Validação/coordenação:** inspeção de fonte e owner da ponte; sem concluir nome/export JS ou compatibilidade. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Configuração do consumidor
**Tipo/direção:** dado de entrada; caller desconhecido → `ClientConfig`/`CoreLinkClient::new`.
**Superfície/ativação:** campos camelCase `pat`, `tenantId`, `clientVerify`; falta de flag seleciona verificação.
**Efeito/falha:** forma inválida retorna `invalid config:`; opt-out passa pela rota de warning/configuração desabilitada.
**Validação/coordenação:** tratar consumidores efetivos como desconhecidos até evidência de grafo ou artefato. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Metadata e pretenso pacote
**Tipo/direção:** build/package; manifesto e `package.json` → eventual toolchain/registry.
**Superfície/ativação:** `cdylib`, metadata wasm-pack, `type: module` e lista `pkg/*`; Cargo declara `publish = false`.
**Efeito/falha:** mudança pode afetar uma futura produção/distribuição, mas não certifica que ela exista.
**Validação/coordenação:** release/package owner; não usar npm, wasm-pack, browser ou rede nesta campanha. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Colisão de nome JS estática
**Tipo/direção:** identidade potencial; `crates/corelink-wasm/package.json` ↔ `sdks/js/package.json`.
**Superfície/ativação:** ambos declaram `name: "@corelink/client"`; o segundo possui fonte TypeScript própria.
**Efeito/falha:** uma mudança de nome, metadata ou docs pode produzir ambiguidade de pacote/owner.
**Validação/coordenação:** owner JS/release deve definir a relação; nenhuma equivalência, seleção, compatibilidade ou publicação é inferida aqui. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Referências documentais candidatas
**Tipo/direção:** referência textual; documentos/exemplos → string `@corelink/client`/`CoreLinkClient`.
**Superfície/ativação:** há referências em `apps/docs` e em exemplos TS deste crate.
**Efeito/falha:** elas podem ficar incompatíveis com uma superfície publicada, mas não provam que apontem para este wrapper em vez do SDK JS homônimo.
**Validação/coordenação:** revisar símbolo contra o artefato selecionado quando existir; docs não são prova de runtime. [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva

Uma alteração no wrapper pode propagar para bindings futuros via REL-002 e, se houver pacote produzido, para consumidores ainda não enumerados. Uma alteração em client-verify pode atingir `get`/`put` através de REL-001. A busca de manifests encontrou apenas a própria crate para `corelink-wasm`; isso é ausência de referência textual em manifests, não prova de zero consumidores no grafo resolvido ou fora do Rust workspace. A colisão REL-005 impede atribuir automaticamente as muitas referências de `@corelink/client` ao cdylib.

<a id="b05"></a>
## B05 — Decisões de mudança

| Mudança | Raio provável | Decisão antes de editar |
|---|---|---|
| assinatura/annotation de `CoreLinkClient` | bindings gerados e consumidores futuros | congelar contrato produzido ou manter como desconhecido |
| default `clientVerify`/mapeamento de erro | integridade e callers de configuração | coordenar com client-verify e revisar INV-001 |
| crate type/deps/metadata | build/empacotamento futuro | separar check de build de publicação/compatibilidade |
| nome/arquivos de pacote | colisão com `sdks/js`, docs e release | resolver owner e artefato canônico antes de publicar |

<a id="b06"></a>
## B06 — Cobertura e lacunas

Cobertos como SOURCE: manifesto, metadata, uma fonte Rust, dependência path, anotações/métodos e colisão de nome de package. Desconhecidos: resolução inversa, cfg/feature selecionados, macro expandida, arquivo WASM/JS/d.ts, relação com `sdks/js`, instalação npm, browser/Node, transporte CAS, uso por clientes e publicação. Nenhum desses limites é reduzido por uma lista `files`, comentário de CI ou referência em documentação.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-wasm/SKILL.md#s01).
