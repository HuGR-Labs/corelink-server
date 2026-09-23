---
schema: corelink-ownership/1.1
document: reference
package: corelink-hash
manifest: crates/corelink-hash/Cargo.toml
source_commit: cacc44fc43ee2f481e933419ba88b9a19ac6c8e8
profile: H
state: draft
evidence_set: hash-pilot-source-20260919
---

# corelink-hash — referência de ownership

Library de integridade de conteúdo. Esta edição está ancorada no `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8`; não certifica runtime nem resolução Cargo.

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) ·
[Contratos](#r04) · [Estado e invariantes](#r05) · [Configuração](#r06) ·
[Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

| Campo | Valor / alcance |
|---|---|
| Package / manifesto | `corelink-hash` / `crates/corelink-hash/Cargo.toml` |
| Papel | Library leaf quanto às dependências próprias declaradas |
| Conteúdo | Digest BLAKE3-256, VerifiedBody, contrato de escrita e erros |
| Targets inspecionados | library `src/lib.rs`; example `blake3_vectors`; tests `blob_store_contract`, `mutation_kills`, `prop_hash`; benches `blake3_bench`, `blake3`; fuzz é package independente |
| Licença / publicação declaradas | `MIT OR Apache-2.0`, `publish=true`; isso não prova publicação em registry |
| Implementação / integração / runtime | Implementação lida; integração traçada em consumidores selecionados; runtime não observado |
| Papéis de ownership | Implementação: mantenedores de `corelink-hash` (nome não verificado); contrato público: esta unidade; composition root: `corelink-server`/adapters; operador/runtime: desconhecido; aprovação: mantenedores/CODEOWNERS não verificados |
| Fuzzer | Package independente `corelink-hash-fuzz`; não confundir com target do package principal |

Fontes: [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) e [S02](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/lib.rs). O manifesto atual declara `repository = https://github.com/HuGR-dev/corelink-server`; links de código usam o repositório GitHub que contém o commit. Não reproduzir a descrição da crate como se fosse prova de todas as escritas do produto.

O censo literal no pin encontra treze manifests consumidores: `corelink-ac`, `corelink-bazel-bridge`, `corelink-cas`, `corelink-client-verify`, o package `corelink-server` (manifesto `crates/corelink-container/Cargo.toml`), `corelink-crypto`, `corelink-meta`, `corelink-reapi`, `corelink-worker`, `corelink-hash-fuzz`, `corelink-meta-fuzz`, `corelink-worker-fuzz` e `e2e-signup-flow`. Isso prova declaração ou path dependency, não reachability.

Rotas navegáveis para os consumers:

| Consumer | Referência |
|---|---|
| AC | [corelink-ac](../corelink-ac/REFERENCE.md#r01) |
| Bazel bridge | [corelink-bazel-bridge](../corelink-bazel-bridge/REFERENCE.md#r01) |
| CAS | [corelink-cas](../corelink-cas/REFERENCE.md#r01) |
| Client verify | [corelink-client-verify](../corelink-client-verify/REFERENCE.md#r01) |
| Server/container | [corelink-server](../corelink-server/REFERENCE.md#r01) |
| Crypto | [corelink-crypto](../corelink-crypto/REFERENCE.md#r01) |
| Meta | [corelink-meta](../corelink-meta/REFERENCE.md#r01) |
| REAPI | [corelink-reapi](../corelink-reapi/REFERENCE.md#r01) |
| Worker | [corelink-worker](../corelink-worker/REFERENCE.md#r01) |
| Hash fuzz | [corelink-hash-fuzz](../corelink-hash-fuzz/REFERENCE.md#r01) |
| Meta fuzz | [corelink-meta-fuzz](../corelink-meta-fuzz/REFERENCE.md#r01) |
| Worker fuzz | [corelink-worker-fuzz](../corelink-worker-fuzz/REFERENCE.md#r01) |
| Signup | [e2e-signup-flow](../e2e-signup-flow/REFERENCE.md#r01) |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Fronteira | Implementação | Contrato / coordenação |
|---|---|---|
| Digest / VerifiedBody / erros, inclusive leitura observada por `as_bytes` | `corelink-hash` | Esta package implementa e define o contrato de bytes do tipo; consumidores participam de mudanças incompatíveis |
| BLAKE3 / ct_eq | Dependências externas | Manter contrato observado; não atribuir implementação criptográfica ao CoreLink |
| Reexport | `corelink-crypto` / `corelink-client-verify` | Preservar aliases; implementação dos tipos permanece aqui |
| Envelope CLTBINT1 persistido | `corelink-server` / adapter consumidor | O container define framing, leitura Legacy/Verified/Corrupt e efeitos de persistência; `corelink-hash` fornece `Digest`/`DIGEST_LEN`/`as_bytes`. Mudanças no formato exigem coordenação; aprovador nominal não verificado |
| Escrita / tenant / persistência fora desse envelope | Adapter e composition root | Recuperação de dados pertence ao writer real |

**Não faz:** resolve tenant, autoriza usuário, abre conexão, escreve R2/D1, emite evento de auditoria ou aplica status HTTP.
**Canônico transversal:** [conceito OKF CAS/AC](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/docs/knowledge/crates/cas-ac-core.md); diferenças entre intenção e código ficam explícitas em R08.
**Rota de revisão:** issue/PR do repositório; atribuição nominal, operador/runtime e autoridade de aprovação ainda não verificáveis. Isso é bloqueio de governança, não aprovação implícita.

<a id="r03"></a>
## R03 — Mapa da implementação

| Arquivo / entradas | Responsabilidade | Natureza | Prova |
|---|---|---|---|
| `lib.rs` | Quatro módulos privados; oito nomes públicos, incluindo limite compartilhado | Própria | [S02](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/lib.rs) |
| `digest.rs` | Compute, parse, renderização, comparação e bytes | Própria | [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs) |
| `verified_body.rs` | Validar par corpo/digest e preservar envelope | Própria | [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs) |
| `error.rs` | Erros tipados e código estável | Própria | [S06](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/error.rs) |
| `store.rs` | Trait de escrita assíncrona com VerifiedBody | Contrato; sem backend | [S05](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/store.rs) |
| `examples/blake3_vectors.rs` | Vetores executáveis da função de hash | Target auxiliar | [S19](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/examples/blake3_vectors.rs) |

Os cinco módulos Rust acima foram inspecionados estaticamente; o example é target auxiliar listado no manifesto. O manifesto não declara dependência em outro package CoreLink. Essa inspeção é `SOURCE`, não execução do target.
Não foi encontrado reexport externo nesses módulos; reexports da library existem nas crates consumidoras.

<a id="r04"></a>
## R04 — Contratos públicos

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) · [API-008](#api-008) · [API-009](#api-009) · [API-010](#api-010) · [API-011](#api-011) · [API-012](#api-012) · [API-013](#api-013) · [API-014](#api-014) · [API-015](#api-015) · [API-016](#api-016).

<a id="api-001"></a>
### API-001 — Computação e representação
**Símbolos:** `Digest`, `DIGEST_LEN`, `Digest::compute`.
**Entrada:** qualquer `&[u8]`, inclusive vazio; nenhuma identidade de tenant.
**Saída:** newtype privado de 32 bytes, preenchido com `blake3::hash`.
**Efeitos:** leitura síncrona do corpo; sem IO. Não impõe limite de tamanho.
**Compatibilidade:** mudar algoritmo/largura muda endereços e envelopes persistidos.
**Prova:** INV-001; REL-001/014/015; [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs).
[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Parsing não é verificação do corpo
**Símbolos:** `Digest::from_hex`, `ParseError::InvalidLength`, `ParseError::InvalidHexByte`.
**Entrada:** string UTF-8; exige exatamente 64 bytes, todos ASCII hex, minúsculos ou maiúsculos.
**Saída:** Digest decodificado; não compara nenhum corpo.
**Erros:** tamanho observado ou primeira posição inválida em bytes; nenhuma entrada ecoada.
**Compatibilidade:** preservar aceitação de uppercase e offsets exatos; enum é não exaustivo.
**Prova:** INV-001; testes parse e mutation; [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs) [S06](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/error.rs).
[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Renderização e igualdade não sensível
**Símbolos:** `Digest::to_hex`, `Display`.
**Saída:** to_hex retorna String com 64 caracteres hex minúsculos; Display escreve a mesma representação no formatter e propaga seu resultado.
**Efeitos:** to_hex aloca String; Display o chama. Não confere relação com um corpo.
**Compatibilidade:** consumidores usam hex em chaves; não trocar case, largura ou gramática silenciosamente.
**Limite:** igualdade derivada não oferece o contrato temporal de API-004.
**Prova:** REL-014; [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs) [S08](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/mutation_kills.rs).
[Índice de contratos](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Comparação de dois digests
**Símbolo:** `Digest::verify_constant_time`.
**Entrada:** referências a dois Digests de 32 bytes.
**Saída:** bool obtido de `self.0.ct_eq(&other.0).into()`.
**Limite:** o mecanismo compara digests; não torna parsing, hashing ou resposta HTTP inteiros constantes no tempo.
**Compatibilidade:** não substituir por igualdade derivada em contratos que exigem ct_eq.
**Prova:** INV-003; REL-002; [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs). O resultado histórico de `constant_time_variance` teve delta de 0,125%, mas não foi reproduzido em `cacc44` e não mede toda a requisição.
[Índice de contratos](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — Envelope verificado
**Símbolos:** `VerifiedBody`, `VerifiedBody::new`.
**Entrada:** Bytes possuído e Digest declarado.
**Saída:** calcula hash, compara e retorna o envelope com corpo e declaração originais somente quando coincidem.
**Erros:** HashMismatch antes da construção, sem escrita externa.
**Estado:** campos privados; não oferece setters públicos. APIs 010–014 detalham acesso, consumo, cópia e Debug.
**Limite:** prova igualdade no construtor; não prova tenant, publicação nem persistência.
**Prova:** INV-002/004; [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs).
[Índice de contratos](#r04)

<a id="api-006"></a>
[↩](#r01)
### API-006 — Empréstimo de bytes e envelope consumidor
**Símbolo:** `Digest::as_bytes -> &[u8; DIGEST_LEN]`.

**Entrada/saída:** empresta os 32 bytes; não recalcula nem verifica.

**Visibilidade:** `pub` e `#[doc(hidden)]`; oculto no rustdoc, não privado. O rustdoc diz “test-only” e fora da superfície estável: intenção, não restrição de compilação.

**Uso/ownership:** container chama-o no envelope persistido `CLTBINT1` (uso em source, sem prova de build/runtime). Hash possui tipo/largura; server/adapter, framing e estados Legacy/Verified/Corrupt. Aprovador inter-package desconhecido.

**Compatibilidade:** coordenar mudanças; uso não prova garantia formal de estabilidade.

**Prova:** [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs) [S17](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-container/src/storage/r2_kv.rs#L155-L240); nenhum runtime foi observado.
[Índice de contratos](#r04)

<a id="api-007"></a>
[↩](#r01)
### API-007 — Taxonomia de mismatch
**Símbolos:** `HashMismatch`, `HashMismatch::code`, `COR_CAS_DIGEST_MISMATCH`; `ParseError`.
**Saída:** mismatch unitário; code retorna string estável; ParseError distingue comprimento e posição inválida.
**Efeitos:** este módulo não emite auditoria nem produz resposta de transporte.
**Compatibilidade:** mapear pelo código/tipo, não por parsing de Display; enum ParseError permanece não exaustivo.
**Prova:** INV-004; REL-013; [S06](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/error.rs) [S09](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/prop_hash.rs).
[Índice de contratos](#r04)

<a id="api-008"></a>
[↩](#r01)
### API-008 — Contrato de escrita
**Símbolos:** `BlobStoreWrite: Send + Sync`, associated `Error`, `put_verified`.
**Entrada:** `&self`, `&VerifiedBody` com lifetime compartilhado.
**Saída:** Future Send de `Result<(), Error>`; erro implementa Error + Send + Sync + static.
**Efeitos:** pertencem ao adapter; trait não garante idempotência, transação ou rollback por si só.
**Compatibilidade:** unitário não distingue fresh/duplicate; ScopedR2Writer descarta essa distinção intencionalmente.
**Prova:** REL-012; [S05](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/store.rs) [S15](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-worker/src/storage/r2.rs#L200-L285).
[Índice de contratos](#r04)

<a id="api-009"></a>
[↩](#r01)
### API-009 — Limite compartilhado de entrada
**Símbolo:** `CACHE_ENTRY_MAX_BYTES: usize = 64 * 1024 * 1024`.
**Contrato:** constante de 67.108.864 bytes; não é validação dentro de Digest/VerifiedBody.
**Consumidor verificado:** quatro camadas DefaultBodyLimit do router Bazel no recorte inspecionado.
**Compatibilidade:** mudanças afetam extração de corpo e orçamento de memória; outros limites de protocolo não se tornam iguais automaticamente.
**Prova:** REL-016; [S02](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/lib.rs) [S18](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-container/src/routes/bazel_v2/part-00.rs#L290-L357).
[Índice de contratos](#r04)

<a id="api-010"></a>
[↩](#r01)
### API-010 — Empréstimo do corpo
**Símbolo:** `VerifiedBody::body(&self) -> &Bytes`.
**Pré-condição:** envelope existente; nenhum novo input não verificado.
**Resultado:** referência ao campo body, vinculada ao empréstimo de self; não move nem recalcula hash.
**Efeitos/erros:** sem IO, alocação explícita, mutação ou Result neste método.
**Compatibilidade:** não expor empréstimo mutável que permita incoerência com digest.
**Evidência:** INV-002; [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs).
[Índice de contratos](#r04)

<a id="api-011"></a>
[↩](#r01)
### API-011 — Empréstimo do digest
**Símbolo:** `VerifiedBody::digest(&self) -> &Digest`.
**Pré-condição:** envelope previamente construído.
**Resultado:** referência ao digest armazenado; não recomputa nem prova autenticação.
**Efeitos/erros:** sem IO, mutação ou retorno falível.
**Compatibilidade:** manter a associação com body; consumers de chaves usam esta leitura.
**Evidência:** INV-002; REL-012/017; [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs).
[Índice de contratos](#r04)

<a id="api-012"></a>
[↩](#r01)
### API-012 — Consumo do envelope
**Símbolo:** `VerifiedBody::into_parts(self) -> (Bytes, Digest)`.
**Pré-condição:** propriedade do envelope, que é consumido.
**Resultado:** move corpo e digest para o caller, sem nova verificação.
**Efeitos/erros:** nenhuma escrita externa nem erro exposto; o tipo VerifiedBody deixa de proteger alterações posteriores no par separado.
**Compatibilidade:** preservar ordem e tipos da tupla; não prometer integridade após modificações pelo caller.
**Evidência:** into_parts_roundtrip, fonte apenas; [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs).
[Índice de contratos](#r04)

<a id="api-013"></a>
[↩](#r01)
### API-013 — Clone do envelope
**Símbolo:** `Clone` derivado de `VerifiedBody`.
**Resultado:** clona os dois campos; Digest é Copy e Bytes usa sua própria semântica de Clone.
**Limite:** não refaz hashing; a preservação do par depende da ausência de mutação pública dos campos.
**Efeitos/erros:** não faz IO; não promete uma cópia profunda dos bytes nem alocação universalmente ausente.
**Compatibilidade:** mudanças de ownership ou novos campos exigem reavaliar clonagem.
**Evidência:** INV-002; REL-003; [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs).
[Índice de contratos](#r04)

<a id="api-014"></a>
[↩](#r01)
### API-014 — Debug do envelope sem conteúdo
**Símbolo:** `Debug` de `VerifiedBody`.
**Resultado:** formatter recebe estrutura com campos digest e len; body não é enviado ao formatter.
**Limite:** digest e comprimento continuam visíveis; isso não é uma garantia de anonimização de toda telemetria.
**Efeitos/erros:** retorna resultado da formatação, sem IO explícito nesta implementação.
**Compatibilidade:** não acrescentar payload ao debug; consumidores não devem parsear a representação como protocolo.
**Evidência:** INV-004; verified_body_debug_redacts não executado; [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs).
[Índice de contratos](#r04)

<a id="api-015"></a>
[↩](#r01)
### API-015 — Debug do digest
**Símbolo:** `Debug` de `Digest`.
**Resultado:** escreve `Digest(`, hexadecimal minúsculo completo e `)`.
**Efeitos/erros:** usa to_hex e retorna resultado do formatter; não recebe corpo.
**Limite:** debug não redige o digest. Não atribuir a ele sigilo, anonimização ou autenticação.
**Compatibilidade:** preservar diagnóstico; não converter formatação de debug em formato persistido.
**Evidência:** debug_renders_wrapped_hex_not_default, fonte apenas; [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs) [S08](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/mutation_kills.rs).
[Índice de contratos](#r04)

<a id="api-016"></a>
[↩](#r01)
### API-016 — Semântica derivada de valor de Digest
**Símbolos:** derives `Clone`, `Copy`, `PartialEq`, `Eq`, `Hash`.
**Operações:** Clone/Copy duplicam o valor; Eq/PartialEq comparam representação; Hash alimenta um hasher com a representação derivada.
**Limites:** Hash não é Digest::compute; igualdade derivada não oferece o contrato temporal de API-004; nenhuma operação verifica relação com corpo.
**Efeitos:** sem storage ou autorização.
**Compatibilidade:** mudança de campos/derives pode alterar coleções e bounds de consumidores.
**Evidência:** [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs).
[Índice de contratos](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado | Ownership | Vida útil / persistência | Efeito |
|---|---|---|---|
| Digest | Valor Copy | Memória; caller pode serializar | Sem tenant/clock |
| VerifiedBody | Envelope com Bytes e Digest | Memória; conteúdo pode ser compartilhado por Bytes | Sem mutação pública |
| Storage | Fora desta crate | Depende do adapter | API-008 não define rollback |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [FLOW-001](#flow-001).

<a id="inv-001"></a>
### INV-001 — Digest textual tem representação definida
**Regra:** Digest tem 32 bytes; from_hex aceita somente 64 bytes hex; to_hex produz lowercase.
**Imposição:** from_hex/decode_nibble/to_hex; [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs).
**Prova disponível:** canonical_vectors, parse_uppercase_accepted, prop_hex_roundtrip; somente fonte.
[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Construção segura preserva coerência do par
**Regra:** toda construção pela API pública segura de VerifiedBody calcula e compara o hash antes de retornar Ok.
**Imposição:** campos privados e new; [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs).
**Prova disponível:** mismatch_rejected e prop_verified_body_only_on_match; não demonstram cobertura de todas as rotas.
[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Verificação delega à primitiva correta
**Regra:** verify_constant_time usa ct_eq sobre os dois arrays.
**Imposição:** [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs).
**Prova disponível:** fonte e teste de medianas; não existe nesta rodada medição de tempo ou prova criptográfica universal.
[Índice de estado](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Erro e debug não despejam o corpo
**Regra:** HashMismatch não carrega corpo; Debug de VerifiedBody não inclui seus bytes.
**Imposição:** erro unitário e formatter explícito; [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs) [S06](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/error.rs).
**Prova disponível:** verified_body_debug_redacts e hash_mismatch_code_is_canonical; digest/comprimento ainda aparecem.
[Índice de estado](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Limite é responsabilidade do consumidor
**Regra:** constante não constitui check no construtor.
**Imposição:** nenhum ramo de tamanho em new; enforcement deve ser encontrado no consumer.
**Prova disponível:** [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs) e quatro camadas do router em [S18](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-container/src/routes/bazel_v2/part-00.rs#L290-L357).
[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Do input ao limite de escrita
1. Caller obtém corpo e, separadamente, identidade/autorização.
2. from_hex valida apenas representação do digest informado.
3. new calcula BLAKE3 do corpo e chama verify_constant_time.
4. Mismatch retorna erro; esta crate não faz IO nem auditoria.
5. Ok entrega envelope imutável pela API segura.
6. Caller escolhe adapter/contexto e pode chamar put_verified.
7. Erro de IO ou durabilidade pertence ao adapter, não ao construtor.
**Fonte:** [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs) [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs) [S05](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/store.rs).
[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Item | Default / origem | Momento | Consequência |
|---|---|---|---|
| Features próprias | Nenhuma seção `features` no manifesto | Build | Não prescrever `all-features` |
| Targets auxiliares | example `blake3_vectors`; benches `blake3_bench`, `blake3`; tests de integração | Seleção Cargo explícita | Não confundir fuzz independente com target desta library |
| Consumidores públicos | `corelink-crypto::blake3::*`; `corelink-client-verify::{Digest, ParseError, DIGEST_LEN, COR_CAS_DIGEST_MISMATCH}` | Alias/reexport; header FFI define `DIGEST_BYTES=32` e `DIGEST_HEX_LEN=64` | Reexport não prova invocação runtime |
| Dependências externas | workspace=true; versões resolvidas não levantadas aqui | Cargo | Cargo.lock ainda precisa ser conferido na execução |
| PROPTEST_CASES | Código: parse u32 ou 10000 | Teste | Carga de testes, não configuração do serviço |
| Debug/release | debug_assertions | Build de testes | Timing e performance ignorados em debug |
| wasm32-unknown-unknown | Job wasm-build | Check | Compatibilidade de compilação, não runtime Cloudflare |

Fontes: [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) [S09](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/prop_hash.rs) [S12](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L1-L155). Conferir toolchain/host do checkout antes dos procedimentos.

<a id="r07"></a>
## R07 — Erros e observabilidade

| Sinal | Causa | Estado após falha | Procedimento |
|---|---|---|---|
| InvalidLength(n) | Quantidade de bytes diferente de 64 | Nenhum Digest novo | PROC-002 |
| InvalidHexByte(i) | Primeiro byte fora do alfabeto | Nenhum Digest novo | PROC-002 |
| HashMismatch | Corpo e declaração divergem | Nenhum VerifiedBody novo | PROC-002 |
| Error do writer | Falha específica do adapter | Não inferir rollback | PROC-006 |
| Gate temporal/perf vermelho | Código ou ambiente; causa exige diagnóstico | Nada prova corrupção de dados por si só | PROC-004 |

A library não declara métricas ou eventos próprios nos cinco módulos inspecionados. Tratar logs e status HTTP nos consumers.
Fonte: [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs) [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs) [S05](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/store.rs) [S06](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/error.rs).

<a id="r08"></a>
## R08 — Verificação e evidências

Um `cargo test -p corelink-hash --locked --offline` foi executado localmente na
branch da campanha em 2026-09-22: 29 passaram, 2 release-only foram ignorados e
0 falharam. Os arquivos Rust são byte-idênticos ao pin `cacc44`; o manifesto só
difere no URL do repositório. A evidência completa está em
[`HASH-CARGO-TEST-20260922.md`](../../evidence/revision-1.4/HASH-CARGO-TEST-20260922.md).
Isso não substitui execução no pin exato nem certifica resolução de consumers,
release timing, wasm, runtime ou alcance.

Os dois gates release-only também passaram no mesmo checkout source-equivalent:
constant-time delta `0.019%` e performance `21.792027 ms` para 10 × 5 MiB; ver
[`HASH-RELEASE-GATES-20260922.md`](../../evidence/revision-1.4/HASH-RELEASE-GATES-20260922.md).
A identidade estática atual está no
[registro de reanchor](../../evidence/revision-1.4/corelink-hash-source-identities.json).

| Contrato | Testes já existentes | Evidência nesta edição |
|---|---|---|
| API-001/003 | canonical_vectors, digest_len_constant_matches_blake3_output, mutation_kills | EXECUTED_LOCAL histórico na baseline anterior; não executado em `cacc44` |
| API-002 | parse_too_short/long, parse_non_hex_byte, uppercase, roundtrip | EXECUTED_LOCAL histórico; dois gates release ficaram fora da seleção debug |
| API-004 | constant_time_variance | EXECUTED_LOCAL histórico: delta 0,125%; não mede toda requisição nem este pin |
| API-005 | successful_write_hello_world; mismatch_rejected; empty_body_is_verifiable; into_parts_roundtrip; verified_body_debug_redacts; prop_verified_body_only_on_match | EXECUTED_LOCAL histórico no `prop_hash`; não executado em `cacc44` |
| API-008 | blob_store_contract | EXECUTED_LOCAL histórico: dois testes; fake MemoryBlobStore não é R2 real |
| API-009 | Consumer Bazel inspecionado | Boundary tests ainda precisam ser selecionados/executados |
| manifesto/reanchor | `Cargo.toml` comparado entre `cca798ff` e `fb611330` | somente URL `repository` mudou para `HuGR-dev`; fontes Rust do package não mudaram | metadata corrigido no remoto; revisão fria e consumers seguem pendentes |

**Limites abertos:** inversas Cargo e resolução de features não executadas nesta revisão; callers e FFI estão enumerados no blast radius, mas alcance/runtime continuam desconhecidos; atribuição nominal; cold review; PROC-004 histórico vermelho em performance e PROC-006 não executado.

**Questões/limites rastreados:** API-006 registra separadamente a intenção do rustdoc, a visibilidade `pub`, o call site persistente observado e a fronteira de ownership do framing. Aprovação formal inter-package permanece desconhecida. README/OKF usam linguagem de garantia global que exige trace de cada superfície; não foram editados.
Fontes dos testes: [S07](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/blob_store_contract.rs) [S08](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/mutation_kills.rs) [S09](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/prop_hash.rs); documentação anterior: [S10](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/README.md) [S11](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/docs/knowledge/crates/cas-ac-core.md).

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01).
