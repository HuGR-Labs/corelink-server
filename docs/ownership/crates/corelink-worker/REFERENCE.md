---
schema: corelink-ownership/1.1
document: reference
package: corelink-worker
manifest: crates/corelink-worker/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: H
state: draft
evidence_set: w011-worker-static-20260920
---

# corelink-worker — referência de ownership

Referência SOURCE estática do package no baseline indicado: manifesto, root e
declarações de módulos. Ela descreve composição Rust, não worker executado,
requisição, storage, provider, deploy, tráfego ou produção. OKF permanece uma
rota externa canônica; não é copiado, redefinido nem revalidado aqui.

[Identidade](#r01) · [Fronteiras](#r02) · [Mapa](#r03) · [Contratos](#r04) · [Axiomas](#r05) · [Targets](#r06) · [Falhas](#r07) · [Lacunas](#r08).

<a id="r01"></a>
## R01 — Identidade e evidência

| Campo | Evidência estática |
|---|---|
| Package / manifesto | `corelink-worker` / `crates/corelink-worker/Cargo.toml` |
| Baseline | `16d9f0303d849a1ab3df14688bd2c7cdbfee8140` |
| Escopo lido | Manifesto, `src/lib.rs` e 96 arquivos Rust: 77 sob `src/` e 19 sob `tests/`, somente para mapa de composição |
| Papel observável | Package que declara módulos de cache, storage, região/tenant e, por feature, auth, middleware e REAPI |
| Runtime | Não observado: nenhum resultado de Cargo/teste, request, worker, storage, provider ou deploy foi usado |

<a id="r02"></a>
## R02 — Fronteiras SOURCE

| Superfície nomeada | O que SOURCE permite afirmar | Limite falsificável |
|---|---|---|
| `storage`, `cache` | São módulos públicos declarados sem gate no root | O nome/path não prova I/O, R2, KV ou persistência |
| `Region`, `TenantCtx` | São reexports públicos de módulos privados `region` e `tenant` | Reexport não enumera callers nem prova uso |
| `auth`, `middleware`, `reapi` | São módulos públicos sob `cfg(feature = "tower-middleware")` | Gate não prova que a feature foi selecionada ou montada |
| Fakes e targets | Fonte/manifesto os declara | Declaração não prova compile, execução, cobertura ou paridade |

<a id="r03"></a>
## R03 — Mapa de composição

| Nó | Composição observada | Exposição |
|---|---|---|
| `lib.rs` | Raiz; `forbid(unsafe_code)`; seleciona módulos por feature | Reexporta `Region`, `TenantCtx` |
| `cache` | Declara `kv`, `miss_reason`, `negative` | Público sem gate; reexports locais |
| `storage` | Declara `blob_store`, `error`, `key`, `metrics`, `r2` | Público sem gate; reexporta `BlobStore` |
| `region`, `tenant` | Módulos privados da raiz | Tipos chegam pela raiz, não por módulo público |
| `auth` | Declara `revocation` e seus submódulos | Público somente com `tower-middleware` |
| `middleware` | Declara `auth`, `auth_ctx`, `auth_error`, `timing_padding` | Público somente com `tower-middleware` |
| `reapi` | Declara árvores `ac` e `cas` | Público somente com `tower-middleware` |

<a id="r04"></a>
## R04 — Contratos de composição públicos

**Índice:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) · [API-008](#api-008) · [API-009](#api-009) · [API-010](#api-010) · [API-011](#api-011) · [API-012](#api-012) · [API-013](#api-013) · [API-014](#api-014).

<a id="api-001"></a>
### API-001 — Raiz incondicional
**Predicado:** sem seleção de feature, `lib.rs` declara `pub mod cache`, `pub mod storage`, `pub use region::Region` e `pub use tenant::TenantCtx`.
**Refutação:** remover/condicionar qualquer uma dessas quatro declarações no root torna o predicado falso.
**Limite:** isto não demonstra consumer, construção de valor ou efeito.
[Índice](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Subgrafo opcional único
**Predicado:** `auth`, `middleware` e `reapi` têm exatamente o mesmo guard textual `cfg(feature = "tower-middleware")` no root.
**Refutação:** guard ausente, nome diferente ou gate distinto em uma das três declarações.
**Limite:** não implica que qualquer feature foi resolvida ou que uma pilha recebe requests.
[Índice](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Perfil default vazio
**Predicado:** `[features]` declara `default = []`; `tower-middleware` enumera dependências opcionais via `dep:`.
**Refutação:** uma entrada em `default` ou remoção/renomeação de item na lista declarada.
**Limite:** resolução, target wasm e comportamento de build são desconhecidos.
[Índice](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Encapsulamento de região/tenant
**Predicado:** `region` e `tenant` são `mod` privados no root, enquanto `Region` e `TenantCtx` são reexportados no nível da crate.
**Refutação:** tornar o módulo público ou retirar/alterar o reexport da raiz.
**Limite:** o path não garante estabilidade semver nem compatibilidade de dados.
[Índice](#r04)
[↩](#r01)

<a id="api-005"></a>
### API-005 — R2 writer/reader and blob-store contract
**Symbols:** `R2Backend`, `R2Writer`, `R2Reader`, `BlobStore`, `R2BlobStore`. **Inputs:** `TenantCtx`, digest/body or digest read. **Outputs/errors:** write outcome or bytes / typed storage error; duplicate writes can return successful result without a new object. **Effects:** injected backend; `InMemoryR2` is a process-local fake. **Compatibility:** writer and reader region pairing must remain aligned. **Evidence:** `storage/r2.rs`, `storage/blob_store.rs`.
[Index](#r04)

<a id="api-006"></a>
### API-006 — Canonical object key and size policy
**Symbols:** storage key constructors; `corelink_hash::BlobStoreWrite`; `CACHE_ENTRY_MAX_BYTES`. **Inputs:** region, tenant context, digest, verified body. **Predicate:** key shape is derived locally and the hash crate's 64 MiB constant is a separate policy value; this source does not conflate either with the worker writer's 5 MiB limit. **Effects/errors:** malformed or oversized writes can return typed storage errors. **Compatibility:** coordinate both crate owners. **Evidence:** `storage/key.rs`, `storage/r2.rs`, `corelink-hash/src/store.rs`, `corelink-hash/src/lib.rs`.
[Index](#r04)

<a id="api-007"></a>
### API-007 — KV backend seam
**Symbols:** `KvBackend::{get,put_with_ttl,delete}`. **Inputs:** key/value bytes and TTL seconds. **Outputs/errors:** `get` returns `Option<Vec<u8>>` or `KvError`; write/delete return `()` or `KvError`. **Effects:** interface has no compare-and-swap; in-memory fake does not prove Workers KV binding. **Compatibility:** TTL units and lack of atomic CAS are material. **Evidence:** `cache/kv.rs`.
[Index](#r04)

<a id="api-008"></a>
### API-008 — Optional middleware and REAPI modules
**Symbols:** `auth`, `middleware`, and `reapi` module trees; `AuthLayer`, REAPI AC/CAS handlers. **Input:** source-defined request context and protocol types when `tower-middleware` is selected. **Output/errors:** package-specific middleware/handler results. **Effects:** declarations do not establish a mounted route or selected feature. **Compatibility:** feature gate and HTTP/protocol shape must be reviewed together. **Evidence:** `src/lib.rs`, `src/middleware.rs`, `src/reapi.rs`, manifest.
[Index](#r04)

<a id="api-009"></a>
### API-009 — R2 `R2Writer` and `R2Reader`
**Symbols:** `R2Writer::{new,with_metrics,region,put,for_tenant}` and `R2Reader::{new,with_metrics,region,get}`. **Inputs/units:** pinned `Region`, injected `Arc<B>`, `&TenantCtx`, `&VerifiedBody` or `&Digest`; body limit is `SINGLE_BLOB_LIMIT_BYTES = 5 * 1024 * 1024` bytes. **Outputs/errors:** `PutOutcome::{Fresh,Duplicate}`, `Bytes`, or `R2Error` (including region mismatch, too-large and backend failures); missing reads return `NotFound`. **Effects:** async backend calls and observer callbacks; fake backend is in-memory only. **Compatibility:** preserve region pairing, tenant-derived key path and duplicate distinction. **Evidence:** `storage/r2.rs`.
[Index](#r04)

<a id="api-010"></a>
### API-010 — Canonical storage key
**Symbols:** `storage::key::canonical_key` and `storage::key::r2_path`. **Inputs:** `Region`, derived `TenantPrefix`, and `Digest`; no plaintext tenant identifier. **Output/postcondition:** deterministic key with region/tenant-prefix/digest components; exact key grammar is defined in `storage/key.rs`. **Errors/effects:** pure source-level construction; no I/O. **Compatibility:** key grammar changes affect stored-object addressability and require storage-owner coordination; this document does not prove persisted objects. **Evidence:** `storage/key.rs`, caller `storage/r2.rs`.
[Index](#r04)

<a id="api-011"></a>
### API-011 — `BlobStoreWrite` scoped adapter
**Symbols:** `R2Writer::for_tenant` → `ScopedR2Writer` implementing `corelink_hash::BlobStoreWrite::put_verified`. **Input/precondition:** a `TenantCtx` is captured and each write requires `&VerifiedBody`. **Output/errors:** `Result<(), R2Error>`; both fresh and duplicate `R2Writer::put` outcomes map to `Ok(())`. **Effects:** delegates to the writer/backend; `Ok(())` does not mean a new object was created. **Compatibility:** retain digest verification type seam and duplicate semantics. **Evidence:** `storage/r2.rs`, `corelink-hash/src/store.rs`.
[Index](#r04)

<a id="api-012"></a>
### API-012 — `KvBackend`
**Symbols:** `KvBackend::{get,put_with_ttl,delete}`, `KvError`, and `InMemoryKv`. **Inputs/units:** UTF-8 key, raw byte value, absolute `ttl_secs: u64`. **Outputs/errors:** optional bytes for `get`, unit for write/delete, or `KvError`; missing/expired reads are `Ok(None)`. **Effects:** backend-defined storage; interface has last-write-wins and no atomic compare-and-swap. `InMemoryKv` is a host fake, not a Workers KV binding. **Compatibility:** TTL seconds and no-CAS semantics are contract. **Evidence:** `cache/kv.rs`.
[Index](#r04)

<a id="api-013"></a>
### API-013 — Authentication middleware
**Symbols:** gated `auth`/`middleware` modules, `AuthLayer::new(AuthState)`, `Layer<S>::layer`, `AuthService<S>`. **Input:** inner Tower service plus configured `AuthState`; request type follows the inner service. **Output/errors:** wrapped service and source-defined authentication errors. **Effects:** the layer delegates requests through auth state when a caller composes it. **Activation:** modules are gated by `tower-middleware`; root declaration is not route mounting. **Compatibility:** feature forwarding and auth context/error changes require consumer review. **Evidence:** `src/lib.rs`, `src/middleware/auth.rs`, manifest.
[Index](#r04)

<a id="api-014"></a>
### API-014 — REAPI handler trees
**Symbols:** gated `reapi::{ac,cas}` and public handler/trait/type modules. **Inputs:** source-defined `TenantCtx`, `AuthCtx`, protocol digests and injected trait implementations; exact per-handler signatures remain in their modules. **Outputs/errors:** handler-specific results and errors. **Effects:** source-level pure-logic/trait seams; no mounted gRPC/HTTP routes, D1, R2, audit delivery, or selected feature is established. **Activation:** `tower-middleware` gate. **Compatibility:** preserve tenant/region/scope checks and coordinate `corelink-reapi` façade changes. **Evidence:** `src/reapi.rs`, `src/reapi/ac.rs`, `src/reapi/cas.rs`, `src/lib.rs`.
[Index](#r04)

<a id="r05"></a>
## R05 — Axiomas SOURCE atômicos

| Axioma | Predicado falsificável | Evidência | Não prova |
|---|---|---|---|
| A1 — raiz segura | `lib.rs` contém `#![forbid(unsafe_code)]` | `src/lib.rs` | Ausência de unsafe em dependências, binário ou produção |
| A2 — três gates | Só `auth`, `middleware`, `reapi` têm o guard da API-002 no root | `src/lib.rs` | Seleção da feature por um consumidor |
| A3 — dois reexports raiz | `Region` e `TenantCtx` vêm de módulos privados no root | `src/lib.rs` | Uso por todos os consumers |
| A4 — módulos base | `cache` e `storage` são declarados `pub` sem `cfg` imediatamente precedente | `src/lib.rs` | Acesso a cache/storage real |
| A5 — manifest separado | Dependências Tower/HTTP/Tokio listadas são opcionais e entram na feature `tower-middleware` | `Cargo.toml` | Grafo resolvido, build ou runtime |

<a id="r06"></a>
## R06 — Targets e dependências declarados

| Declaração | Estado SOURCE | Limite |
|---|---|---|
| Dependências base | `corelink-hash`, `corelink-tenant-path`, `corelink-ac`, `bytes`, `thiserror`, `uuid`, `zeroize`, `parking_lot` | Manifesto não confirma versões resolvidas nem uso efetivo |
| Feature opcional | `tower`, `tower-layer`, `tower-service`, `http`, `tokio`, `futures`, auth/crypto/serde e tracing são listados em `tower-middleware` | Não prova middleware ativo |
| Target wasm | Há dependências condicionais para `target_arch = "wasm32"` | Não prova compilação nem binding de Worker |
| Testes/bench | O manifesto declara testes, vários `required-features = ["tower-middleware"]`, e bench `side_channel` | Não foram executados; não há resultado ou medida |

<a id="r07"></a>
## R07 — Falhas, observabilidade e recovery documental

`R2Error` distinguishes region mismatch, oversized blob, not-found, and
backend failure; `R2Writer::put` maps backend outcome/errors to `PutOutcome` or
typed error and calls the injected `MetricsObserver` on the source paths.
`PutResultLabel`, `GetResultLabel`, `PutSizeBucket`, and the duration callbacks
define local observer inputs. The default `NoopMetrics` emits nothing; neither
an exporter, alert, Workers Analytics binding, nor observation is established.
See [API-009](#api-009), [API-011](#api-011),
[REL-003 / local observer boundary](BLAST_RADIUS.md#b03).

| Modo | Predicado | Ação autorizada | Pare / recovery |
|---|---|---|---|
| Composição | Path, módulo, gate, reexport ou `dep:` muda | Atualizar R03–R06 e B01–B05 | Se exigir build/efeito, registrar lacuna e escalar |
| Consumer estático | Manifesto/uso Rust aponta para este package | Registrar relação atômica em B02–B05 | Não transformar import em execução |
| Documental | Só quatro paths autorizados mudam | Rodar checker H e diff whitespace | Se fonte/Cargo/shared mudar, restaurar o escopo ou escalar |
| Operacional | Pergunta pede request, storage, worker ou deploy | Não executar nem concluir | Rota OKF externa ao owner operacional |

**Relações críticas:** API-005/009/011 → [INV-006](#inv-006), [REL-001](BLAST_RADIUS.md#rel-001), [REL-003](BLAST_RADIUS.md#rel-003); API-006/010 → [INV-006](#inv-006), [REL-002](BLAST_RADIUS.md#rel-002); API-007/012 → [INV-007](#inv-007), [REL-004](BLAST_RADIUS.md#rel-004); API-008/013 → [INV-002](#r05), [REL-005](BLAST_RADIUS.md#rel-005); API-014 → [INV-002](#r05), [REL-008](BLAST_RADIUS.md#rel-008). API-001/004 map to [REL-009](BLAST_RADIUS.md#rel-009); API-003 to [REL-004](BLAST_RADIUS.md#rel-004). These links identify source boundaries; they do not assert feature selection or runtime.

<a id="inv-006"></a>
### INV-006 — Digest, tenant key, and storage result remain separate
**Predicate:** verified-body validation remains owned by `corelink-hash`; worker key construction uses tenant prefix and digest; R2 size rejection, backend outcome, and observer calls remain distinct source steps. **Enforcement:** `storage/{key,r2,blob_store}.rs`. **Violation:** merging or bypassing one stage changes the write boundary. **Verification:** source path inspection and API-005/006/009–011; test execution unknown. **State:** SOURCE. See [REL-001](BLAST_RADIUS.md#rel-001), [REL-002](BLAST_RADIUS.md#rel-002), [REL-003](BLAST_RADIUS.md#rel-003). [Index](#r04)

<a id="inv-007"></a>
### INV-007 — KV has no compare-and-swap contract
**Predicate:** `KvBackend` exposes get, put-with-TTL, and delete, but no atomic compare-and-swap method. **Enforcement:** `cache/kv.rs`. **Violation:** adding/removing a method or changing TTL units changes the public contract. **Verification:** trait and fake source; binding/runtime unknown. **State:** SOURCE. See [REL-004](BLAST_RADIUS.md#rel-004). [Index](#r04)

<a id="r08"></a>
## R08 — Desconhecidos e rota OKF

Desconhecidos: censo completo de callers internos/externos, resolução de features,
compilação host/wasm, execução de targets, requests, cache/storage, R2/KV/D1/DO,
auth, auditoria, timing, provider, credenciais, configuração, observabilidade,
deploy, tráfego e produção. A ausência no mapa não prova ausência no sistema.
Para qualquer uma dessas perguntas, a rota é o owner do composition root ou
provider, usando o processo OKF canônico externo; esta referência não o replica.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01)
