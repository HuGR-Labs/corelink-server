---
schema: corelink-ownership/1.1
document: reference
package: corelink-bazel-bridge
manifest: crates/corelink-bazel-bridge/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: bazel-bridge-static-20260920
---

# corelink-bazel-bridge — referência de ownership

Esta referência descreve o package a partir do manifesto e da fonte estática fixada. Não demonstra cliente Bazel, tráfego, montagem ativa, storage, deploy ou runtime.

[Identidade](#r01) · [Fronteiras](#r02) · [Mapa](#r03) · [Contratos](#r04) · [Invariantes](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Lacunas](#r08).

<a id="r01"></a>
## R01 — Identidade e função

| Campo | Evidência estática |
|---|---|
| Package / manifesto | `corelink-bazel-bridge` / `crates/corelink-bazel-bridge/Cargo.toml` |
| Função | Camada de tradução REAPI v2 REST/cache para traits CAS e AC CoreLink |
| Fonte lida | `src/{adapter,digest,error,find_missing,lib,uri}.rs` e `tests/integration.rs` |
| Dependências CoreLink | `corelink-handler-cas`, `corelink-handler-ac`, `corelink-hash` |
| Transporte declarado | REST; o manifesto não declara `tonic` ou gRPC |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Fronteira | Responsabilidade observada | Limite |
|---|---|---|
| Bridge | Parse REAPI, validação, delegação e taxonomia de erro | Não monta HTTP nem extrai request |
| CAS/AC handlers | Leitura/escrita por traits injetados | Implementação dos handlers é externa ao package |
| `corelink-hash` | Fornece teto compartilhado de entrada | SHA-256 REAPI é contrato separado |
| `corelink-container` | Único consumidor direto conhecido; possui extração e mounts | Grafo completo não foi derivado |

<a id="r03"></a>
## R03 — Mapa da implementação

| Arquivo | Papel observado |
|---|---|
| `lib.rs` | Exporta módulos, versão REAPI, cap de batch e teto de blob |
| `uri.rs` | Classifica formato de path; usa método somente para distinguir AC read/write |
| `digest.rs` | Valida hash/tamanho/forma JSON/URI e expõe verificação SHA-256 |
| `adapter.rs` | Encapsula quatro handlers e traduz operações REAPI |
| `find_missing.rs` | Define request/response/trait e delega existência CAS |
| `error.rs` | Define `BazelBridgeError` e status HTTP sugerido |

<a id="r04"></a>
## R04 — Contratos públicos observados

| Contrato | Entrada e resultado | Compatibilidade / limite |
|---|---|---|
| `Digest` | Hash lowercase de 64 hex e `size_bytes`; parse/new falham se inválidos | Alterar gramática muda URIs e JSON aceitos |
| `RoapiOperation` / `parse_reapi_path` | Classifica path; método só seleciona AC read/write | Não impõe verbos CAS/find-missing; o container monta rotas Axum |
| `BazelAdapter` | CAS/AC via quatro trait objects | Não implementa handler nem transporte |
| `FindMissingHandler` | Lote de digests para resposta REAPI | Cap declarado é `4096` |
| `BazelBridgeError` | Erro tipado com `http_status` sugerido | Mapper ativo é do container; não é resposta HTTP |

<a id="r05"></a>
## R05 — Estado e invariantes

| Invariante | Evidência estática | Não prova |
|---|---|---|
| `BazelAdapter::cas_put` compara somente comprimento antes de delegar | Fluxo de `adapter.rs` | Verificação SHA-256 no adapter |
| Container chama `digest::verify_sha256` antes de `cas_put` | `routes/bazel_v2/part-00-01.rs` | Que qualquer caller do adapter use o mesmo boundary |
| Hash REAPI exige lowercase hex de 64 caracteres | `digest.rs` | Aceitação por cliente externo |
| Find-missing preserva semântica de lote sobre CAS read | `find_missing.rs` e integração local | Latência, carga ou delivery |
| O teto de blob deriva de `corelink_hash::CACHE_ENTRY_MAX_BYTES` | `lib.rs` | Que esse teto seja a verificação SHA-256 ou a política de rota |

<a id="r06"></a>
## R06 — Configuração e dependências

| Item | Leitura estática | Ação de mudança |
|---|---|---|
| `REAPI_VERSION` | Constante `2.3.0` | Revisar formas URI/JSON e consumidor |
| `FIND_MISSING_BLOB_CAP` | Constante `4096` | Revisar batch, resposta e limites de caller |
| `MAX_BLOB_SIZE_BYTES` | Cast da constante de `corelink-hash`, distinto de SHA-256 REAPI | Coordenar limite com corelink-hash e container |
| Traits CAS/AC | Injeção por `Arc<dyn ...>` | Congelar assinatura com owners dos handlers |

<a id="r07"></a>
## R07 — Falhas e recuperação semântica

| Condição | Sinal no bridge | Recuperação de ownership |
|---|---|---|
| Digest/path inválido | `InvalidDigest` ou `Internal` de classificação | Corrigir grammar; verb/path ativo pertence ao container |
| Tamanho diverge | `SizeMismatch`; `http_status` sugerido é 400 | Coordenar com mapper do container, que mapeia-o para 422 |
| Hash diverge | `DigestMismatch` no boundary do container | Reter bytes/digest; não atribuir a checagem ao adapter |
| Tenant diverge | `CrossTenantDenied` | Escalonar autenticação/composição ao container |
| Handler falha | `AuditFailed` ou `Internal` | Investigar handler e revisar mapper ativo no container |

<a id="r08"></a>
## R08 — Evidência e lacunas explícitas

Evidência disponível: manifesto, seis módulos de fonte e teste de integração no commit fixado. O consumidor direto conhecido é `corelink-container`, cuja fonte Bazel contém a montagem/extração.

Desconhecidos explícitos: uso por cliente Bazel real; tráfego; runtime; deploy; R2; D1; edge headers; requests de edge; configuração produtiva; e grafo reverso completo. A ausência deles nesta leitura não prova inexistência.
