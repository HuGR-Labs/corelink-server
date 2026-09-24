---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-hash
manifest: crates/corelink-hash/Cargo.toml
source_commit: cacc44fc43ee2f481e933419ba88b9a19ac6c8e8
profile: H
state: draft
evidence_set: hash-pilot-source-20260919
---

# corelink-hash — blast radius

Mapa candidato de fronteiras verificadas no pin `cacc44fc43ee2f481e933419ba88b9a19ac6c8e8`. Não é prova de resolução Cargo, reachability ou runtime.

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) · [Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06).

<a id="b01"></a>
## B01 — Escopo e riscos prioritários

Os efeitos mais importantes são mudanças de identidade do conteúdo, formato persistido,
contrato do writer e admissão de payload. A library não controla deployment nem tenant.
**Evidência histórica:** builds/testes foram executados em baseline anterior, no host
nativo e wasm; não foram executados no pin `cacc44`. Ativação em artefato e produção
não foram demonstradas. O gate de performance histórico falhou.
**Limite:** inversas Cargo, resolução de features e peers continuam em B06; não usar este mapa como prova de alcance runtime.

<a id="b02"></a>
## B02 — Inventário e método

| População | Evidência | Resultado | Limite |
|---|---|---|---|
| Dependências próprias | Manifesto completo [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) | 5 normais, 4 dev, zero build explícitas | Versões/features transitivas não resolvidas |
| Implementação local | [S02](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/lib.rs)–[S06](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/error.rs) | Cinco arquivos lidos | Não prova deployment |
| Testes | Árvore de tests + [S07](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/blob_store_contract.rs)–[S09](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/prop_hash.rs) | Evidência histórica: 28 testes debug + doctest e CT release passaram; perf release falhou; nada executado em `cacc44` | Sem runtime ou benchmark aprovado |
| Consumidores | Busca literal no pin e manifests | Treze consumers Cargo diretos; callers materiais adicionais estão em REL-037..043 | Ocorrência textual não é chamada/ativação |
| Cargo graph | `cargo tree --locked --offline --workspace --invert corelink-hash --target x86_64-apple-darwin --edges normal,build` | `RESOLVED` histórico na baseline anterior; nenhuma resolução atual executada | Não chamar resolved de runtime |
| Runtime | Não inspecionado | Nenhuma observação | Não inferir ausência ou ativação |

**Identidades locais:** `hash` e os consumers usam seus manifests reais em `crates/`;
`testes`/`test`, example e benches são targets de hash, não packages separados.
`workflow` é `.github/workflows/corelink-hash.yml`, não dependência Cargo.
`corelink-core` expõe outro `Digest` e fica explicitamente excluído.
`corelink-crypto::blake3::*` e `corelink-client-verify::{Digest, ParseError, DIGEST_LEN}`
são aliases/reexports; o header C define `DIGEST_BYTES=32` e `DIGEST_HEX_LEN=64`, sem provar ABI/runtime.

**Âncoras do peer `corelink-hash-fuzz` (source pin cacc44fc43ee2f481e933419ba88b9a19ac6c8e8):**
[F01 manifesto/targets](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/fuzz/Cargo.toml),
[F03 digest_parse](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/fuzz/fuzz_targets/digest_parse.rs),
[F04 verify_body](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/fuzz/fuzz_targets/verify_body.rs),
[F09 PR/nightly workflow](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml),
[F10 matrix workflow](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/nightly.yml),
[F14 snapshot de steps/skips](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/reports/perf/b152-actions-2026-09-06.json).

**Âncoras dos consumidores fuzz independentes (source pin cacc44fc43ee2f481e933419ba88b9a19ac6c8e8):**
[M01 manifesto](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-meta/fuzz/Cargo.toml),
[M02 commit_put](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-meta/fuzz/fuzz_targets/commit_put_roundtrip.rs),
[M03 audit](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-meta/fuzz/fuzz_targets/audit_idempotency.rs),
[W01 manifesto](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-worker/fuzz/Cargo.toml),
[W02 r2_path](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs),
[W03 roundtrip](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-worker/fuzz/fuzz_targets/r2_put_get_roundtrip.rs).

Termos de descoberta usados: `corelink-hash`, `corelink_hash`, `BlobStoreWrite`, `CACHE_ENTRY_MAX_BYTES`.
Foram lidos os consumidores diretos resolvidos para as fronteiras abaixo; ainda faltam aliases
de crypto/client-verify, imports renomeados, schemas e código gerado fora desses recortes.
As listas locais continham comentários e fixtures; ocorrência textual não foi promovida automaticamente a chamada.

O ledger [relations-candidate.json](../../evidence/revision-1.3/relations-candidate.json) serializa as 43 relações desta edição. Os fingerprints REL-015/016/026/027 coincidem com `corelink-server` REL-047/048/049/050; REL-033 coincide com `corelink-worker-fuzz` REL-002. REL-037..042 agora têm contraparte atômica no `corelink-server` REL-082..087, com as identidades compartilhadas registradas em ambos os lados; essa reconciliação cobre fatos de fonte, não owner nominal, resolução, runtime ou deploy.

Isso reconcilia apenas identidade e superfície descrita nos documentos peer citados abaixo. Owners nominais, ativação/runtime, deploy e população global continuam sem prova; `peer_review` permanece `not_reconciled` e a integração final continua bloqueada.

<a id="bazel-calls"></a>
**REL-016 — membros da fronteira de limite no router Bazel:**

| Path de rota | Método de escrita | Construção compartilhada |
|---|---|---|
| `/bazel/v2/{instance}/blobs/ac/{hash}/{size}` | PUT | `DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)` |
| `/bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}` | PUT | Mesma construção |
| `/bazel/cache/cas/{hash}` | PUT | Mesma construção |
| `/bazel/cache/ac/{hash}` | PUT | Mesma construção |

O recorte S18 enumera as quatro camadas. A tabela não certifica os handlers nem o mount em produção.
[Voltar à relação](#rel-016) · [Índice](#b03).

<a id="b03"></a>
## B03 — Relações verificadas nesta edição

| ID | Fronteira | Dependência | Owner/peer |
|---|---|---|---|
| [REL-001](#rel-001) | Algoritmo externo BLAKE3 | hash→blake3 | hash |
| [REL-002](#rel-002) | Primitiva de comparação | hash→subtle | hash |
| [REL-003](#rel-003) | Ownership dos bytes | hash→bytes | hash |
| [REL-004](#rel-004) | Codificação hexadecimal | hash→hex | hash |
| [REL-005](#rel-005) | Derivação de erros | hash→thiserror | hash |
| [REL-006](#rel-006) | Propriedades de integridade | testes→proptest | hash |
| [REL-007](#rel-007) | Benchmark framework | benches→criterion | hash |
| [REL-008](#rel-008) | Runtime dos testes async | testes→tokio | hash |
| [REL-009](#rel-009) | Erro do fake de storage | testes→thiserror | hash |
| [REL-010](#rel-010) | Reexport de toda a superfície | crypto→hash | hash; peer crypto pendente |
| [REL-011](#rel-011) | Reexports do verificador cliente | client-verify→hash | hash; peer client-verify pendente |
| [REL-012](#rel-012) | Writer de escopo por tenant | worker→hash | worker; peer pendente |
| [REL-013](#rel-013) | Código de erro no cliente | client-verify→hash | hash; peer client-verify pendente |
| [REL-014](#rel-014) | Digest em chave persistente R2 | worker→hash | worker; peer pendente |
| [REL-015](#rel-015) | Bytes do digest em envelope persistido | server→hash | fingerprint peer REL-047; owner/runtime pendentes |
| [REL-016](#rel-016) | Limite de corpo nas quatro rotas Bazel | server→hash | fingerprint peer REL-048; owner/runtime pendentes |
| [REL-017](#rel-017) | Fake demonstrador da fronteira | test→hash | hash |
| [REL-018](#rel-018) | Regressões do contrato público | test→hash | hash |
| [REL-019](#rel-019) | Gate PR de Rust e wasm | workflow→hash | integração do repo |
| [REL-020](#rel-020) | Serialização de identidade AC | ac→hash | ac; peer pendente |
| [REL-021](#rel-021) | Chave persistente de metadados | meta→hash | meta; peer pendente |
| [REL-022](#rel-022) | Fingerprint da migração D1 | meta→hash | meta; peer pendente |
| [REL-023](#rel-023) | Escrita REAPI fail-closed | reapi→hash | reapi; peer pendente |
| [REL-024](#rel-024) | Limite REST do Bazel bridge | bazel-bridge→hash | bazel-bridge; peer pendente |
| [REL-025](#rel-025) | R2 do harness de signup | e2e-signup-flow→hash | harness; peer pendente |
| [REL-026](#rel-026) | Limite de leitura CAS do servidor | server→hash | fingerprint peer REL-049; owner/runtime pendentes |
| [REL-027](#rel-027) | Limite DELETE CAS do servidor | server→hash | fingerprint peer REL-050; owner/runtime pendentes |
| [REL-028](#rel-028) | Dependência declarada do CAS | cas→hash | cas; peer pendente |
| [REL-029](#rel-029) | Harness standalone de fuzz | hash-fuzz→hash | hash (contrato); peer hash-fuzz pendente |
| [REL-030](#rel-030) | Oráculo do parser de digest | hash-fuzz→hash | hash; peer hash-fuzz pendente |
| [REL-031](#rel-031) | Oráculo do corpo verificado | hash-fuzz→hash | hash; peer hash-fuzz pendente |
| [REL-032](#rel-032) | Meta fuzz usa Digest | meta-fuzz→hash | hash; peer meta-fuzz pendente |
| [REL-033](#rel-033) | Worker fuzz usa digest verificado | worker-fuzz→hash | fingerprint peer REL-002; owner/runtime pendentes |
| [REL-034](#rel-034) | Gate PR fuzz-smoke | workflow→hash-fuzz | CI; peer hash-fuzz pendente |
| [REL-035](#rel-035) | Gate PR wasm-build | workflow→hash | CI; peer hash-fuzz pendente |
| [REL-036](#rel-036) | Cache condicionado do fuzz-smoke | workflow→cache | CI; peer hash-fuzz pendente |
| [REL-037](#rel-037) | Hash de conteúdo da cache de adapter | `corelink-server`→hash | fatos SOURCE pareados em server REL-082; owner/runtime UNKNOWN |
| [REL-038](#rel-038) | Hash de bytes TCS encapsulados BYOK | `corelink-server`→hash | fatos SOURCE pareados em server REL-083; owner/runtime UNKNOWN |
| [REL-039](#rel-039) | Fingerprint da política BYOK | `corelink-server`→hash | fatos SOURCE pareados em server REL-084; owner/runtime UNKNOWN |
| [REL-040](#rel-040) | Fingerprint da requisição BYOK | `corelink-server`→hash | fatos SOURCE pareados em server REL-085; owner/runtime UNKNOWN |
| [REL-041](#rel-041) | Fingerprint canônico de evento de billing | `corelink-server`→hash | fatos SOURCE pareados em server REL-086; owner/runtime UNKNOWN |
| [REL-042](#rel-042) | Verificação BLAKE3 de conteúdo CAS multipart | `corelink-server`→hash | fatos SOURCE pareados em server REL-087; owner/runtime UNKNOWN |
| [REL-043](#rel-043) | Alias público para FFI | client-verify→hash | client-verify; peer facts not reconciled |

Chave completa: `repo:1232040291:boundary:` + sufixo indicado. Peers pendentes não são marcados frescos.

<a id="rel-001"></a>
### REL-001 — Algoritmo externo BLAKE3
**Identidade:** `repo:1232040291:boundary:hash-blake3-001`.  
**Dependência / fluxo / impacto:** hash→blake3; corpo→hash; provedor→hash.  
**Superfície:** Digest::compute → blake3::hash.  
**Ativação:** compilação normal.  
**Contrato:** saída de 32 bytes.  
**Estado / efeitos:** cálculo síncrono; sem IO.  
**Falha / propagação:** digest diferente altera identidade de conteúdo.  
**Contenção:** runtime/versão resolvida não observados.  
**Validação:** canonical_vectors; não executado.  
**Coordenação / fontes:** hash; [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs). [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Primitiva de comparação
**Identidade:** `repo:1232040291:boundary:hash-subtle-001`.  
**Dependência / fluxo / impacto:** hash→subtle; digests→bool; provedor→verificadores.  
**Superfície:** Digest::verify_constant_time → ct_eq.  
**Ativação:** compilação normal.  
**Contrato:** comparar arrays sem contrato de parsing temporal.  
**Estado / efeitos:** sem escrita.  
**Falha / propagação:** substituir primitiva muda garantia da comparação.  
**Contenção:** não garante tempo constante da requisição.  
**Validação:** constant_time_variance release; não executado.  
**Coordenação / fontes:** hash; [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs). [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Ownership dos bytes
**Identidade:** `repo:1232040291:boundary:hash-bytes-001`.  
**Dependência / fluxo / impacto:** hash→bytes; caller→envelope→caller; provedor→envelope.  
**Superfície:** VerifiedBody::{new,body,into_parts,Clone}.  
**Ativação:** compilação normal.  
**Contrato:** Bytes preservado com Digest verificado.  
**Estado / efeitos:** memória; sem storage.  
**Falha / propagação:** quebrar ownership afeta callers e cópias.  
**Contenção:** persistência pertence ao adapter.  
**Validação:** into_parts_roundtrip; não executado.  
**Coordenação / fontes:** hash; [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) [S04](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/verified_body.rs). [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Codificação hexadecimal
**Identidade:** `repo:1232040291:boundary:hash-hex-001`.  
**Dependência / fluxo / impacto:** hash→hex; array→String; provedor→chaves.  
**Superfície:** Digest::to_hex → hex::encode.  
**Ativação:** compilação normal.  
**Contrato:** 64 caracteres lowercase.  
**Estado / efeitos:** String nova.  
**Falha / propagação:** case/largura diferentes alteram chaves.  
**Contenção:** parser próprio usa decode_nibble, não hex::decode.  
**Validação:** prop_hex_roundtrip; não executado.  
**Coordenação / fontes:** hash; [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) [S03](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/digest.rs). [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Derivação de erros
**Identidade:** `repo:1232040291:boundary:hash-thiserror-001`.  
**Dependência / fluxo / impacto:** hash→thiserror; não aplicável; provedor→API.  
**Superfície:** derive(Error) em ParseError/HashMismatch.  
**Ativação:** compilação normal.  
**Contrato:** Error e Display; code estável separado.  
**Estado / efeitos:** sem IO.  
**Falha / propagação:** mudanças afetam mensagens/tipos de consumidores.  
**Contenção:** transport mapping fora da crate.  
**Validação:** hash_mismatch_code_is_canonical; não executado.  
**Coordenação / fontes:** hash; [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) [S06](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/error.rs). [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Propriedades de integridade
**Identidade:** `repo:1232040291:boundary:hash-test-proptest-001`.  
**Dependência / fluxo / impacto:** hash(testes)→proptest; amostras→assertivas; framework→testes.  
**Superfície:** proptest! em prop_hash.rs.  
**Ativação:** dev-dependency; PROPTEST_CASES.  
**Contrato:** gerar casos; falhar nos predicados.  
**Estado / efeitos:** teste local; pode gerar arquivos de regressão.  
**Falha / propagação:** menos casos reduz evidência, não valida produção.  
**Contenção:** não prova resistência matemática a colisões.  
**Validação:** prop_hash_determinism/prop_verified_body_only_on_match; não executados.  
**Coordenação / fontes:** hash; [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) [S09](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/prop_hash.rs). [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Benchmark framework
**Identidade:** `repo:1232040291:boundary:hash-bench-criterion-001`.  
**Dependência / fluxo / impacto:** hash(benches)→criterion; não certificado; framework→benches.  
**Superfície:** targets blake3_bench e blake3.  
**Ativação:** dev-dependency; bench.  
**Contrato:** harness=false no manifesto.  
**Estado / efeitos:** execução/implementação dos benches não inspecionadas.  
**Falha / propagação:** troca de framework pode impedir medição.  
**Contenção:** não extrapolar benchmark nativo para p99 wasm.  
**Validação:** ler benches e executar em ambiente registrado.  
**Coordenação / fontes:** hash; [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml). [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Runtime dos testes async
**Identidade:** `repo:1232040291:boundary:hash-test-tokio-001`.  
**Dependência / fluxo / impacto:** hash(testes)→tokio; fixture→future; framework→testes.  
**Superfície:** dois tokio::test em blob_store_contract.  
**Ativação:** dev-dependency.  
**Contrato:** executar futuro do fake.  
**Estado / efeitos:** HashMap/Mutex local.  
**Falha / propagação:** incompatibilidade afeta teste, não prova serviço.  
**Contenção:** não é integração R2.  
**Validação:** blob_store_contract; não executado.  
**Coordenação / fontes:** hash; [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) [S07](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/blob_store_contract.rs). [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Erro do fake de storage
**Identidade:** `repo:1232040291:boundary:hash-test-thiserror-001`.  
**Dependência / fluxo / impacto:** hash(testes)→thiserror; não aplicável; framework→fake.  
**Superfície:** MemoryStoreError::Poisoned.  
**Ativação:** dev-dependency.  
**Contrato:** erro implementa contrato associado.  
**Estado / efeitos:** fake local.  
**Falha / propagação:** mudança de trait/error rompe fixture.  
**Contenção:** separado do erro do adapter real.  
**Validação:** blob_store_contract; não executado.  
**Coordenação / fontes:** hash; [S01](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/Cargo.toml) [S07](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/blob_store_contract.rs). [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Reexport de toda a superfície
**Identidade:** `repo:1232040291:boundary:hash-crypto-export-001`.  
**Dependência / fluxo / impacto:** crypto→hash; não aplicável; hash→crypto→callers.  
**Superfície:** corelink_crypto::blake3::*.  
**Ativação:** pub use sem cfg local.  
**Contrato:** mesmos tipos/exportações do hash.  
**Estado / efeitos:** sem implementação duplicada.  
**Falha / propagação:** remover item quebra também caminho canônico.  
**Contenção:** callers transitivos ainda não enumerados.  
**Validação:** compilar ambos os caminhos; não executado.  
**Coordenação / fontes:** hash; peer crypto pendente; [S13](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-crypto/src/blake3.rs). [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Reexports do verificador cliente
**Identidade:** `repo:1232040291:boundary:hash-client-types-001`.  
**Dependência / fluxo / impacto:** client-verify→hash; não aplicável; hash→client-verify.  
**Superfície:** digest::{Digest,ParseError,DIGEST_LEN}.  
**Ativação:** pub use sem cfg local.  
**Contrato:** identidade dos tipos preservada.  
**Estado / efeitos:** sem backend.  
**Falha / propagação:** alterar largura/tipo propaga ao verificador.  
**Contenção:** FFI/wrappers finais não traçados nesta edição.  
**Validação:** matriz cliente/FFI; seleção pendente.  
**Coordenação / fontes:** hash; peer client-verify pendente; [S14](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-client-verify/src/digest.rs). [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Writer de escopo por tenant
**Identidade:** `repo:1232040291:boundary:hash-worker-write-001`.  
**Dependência / fluxo / impacto:** worker→hash; envelope→writer; hash↔writer.  
**Superfície:** ScopedR2Writer::put_verified → writer.put.  
**Ativação:** adapter construído por for_tenant.  
**Contrato:** Fresh e Duplicate mapeiam para Ok(()).  
**Estado / efeitos:** efeito pertence a backend; tenant no self.  
**Falha / propagação:** Result unitário perde distinção fresh/duplicate.  
**Contenção:** não inferir novidade, accounting ou rollback de Ok/Err.  
**Validação:** fresh/duplicate/falha em teste do writer; não executado.  
**Coordenação / fontes:** worker; peer pendente; [S05](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/store.rs) [S15](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-worker/src/storage/r2.rs#L200-L285). [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Código de erro no cliente
**Identidade:** `repo:1232040291:boundary:hash-client-error-001`.  
**Dependência / fluxo / impacto:** client-verify→hash; não aplicável; hash→SDK.  
**Superfície:** digest::COR_CAS_DIGEST_MISMATCH.  
**Ativação:** constante pública.  
**Contrato:** valor copiado da constante canônica.  
**Estado / efeitos:** sem transporte aqui.  
**Falha / propagação:** renomear código afeta consumidores que o reconhecem.  
**Contenção:** não é prova do wire de cada linguagem.  
**Validação:** testar error mapping real; não executado.  
**Coordenação / fontes:** hash; peer client-verify pendente; [S06](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/error.rs) [S14](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-client-verify/src/digest.rs). [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Digest em chave persistente R2
**Identidade:** `repo:1232040291:boundary:hash-worker-key-001`.  
**Dependência / fluxo / impacto:** worker→hash; Digest→chave; hash→namespace.  
**Superfície:** canonical_key: to_hex, offsets 0..2/2..4.  
**Ativação:** writer/reader deste módulo.  
**Contrato:** 64 hex lowercase; componente blake3.  
**Estado / efeitos:** chave inclui região/prefixo fornecidos pelo caller.  
**Falha / propagação:** algoritmo/case/largura alteram endereçamento.  
**Contenção:** não migrar chaves por simples revert.  
**Validação:** canonical_keys e leitura anterior; não executados.  
**Coordenação / fontes:** worker; peer pendente; [S16](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-worker/src/storage/key.rs#L1-L75). [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Bytes do digest em envelope persistido
**Identidade:** `repo:1232040291:boundary:hash-container-envelope-001`.  
**Dependência / fluxo / impacto:** server→hash; digest→envelope.
**Superfície / ativação:** R2KvStore wrap/unwrap.
**Contrato:** `CLTBINT1` + 32 bytes + payload; hash define bytes; server/adapter define framing e estados Legacy/Verified/Corrupt.
**Efeito:** Legacy passa; Corrupt falha.
**Falha:** mudar digest quebra leitura antiga; mudar framing altera parse.
**Contenção:** `as_bytes` é pub/`doc(hidden)`, descrito como teste-only; uso observado não prova estabilidade formal.
**Validação:** Legacy/Verified/Corrupt + N/N-1; não executado.

**Coordenação / fontes:** hash: bytes; server/adapter: framing. Fingerprint e superfície coincidem com `corelink-server` BLAST REL-047 (source pin `47f4db7`); owner nominal, aprovador, runtime e deploy continuam desconhecidos. [S17](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-container/src/storage/r2_kv.rs#L155-L240) · [peer REL-047](../corelink-server/BLAST_RADIUS.md#rel-047). [Index](#b03)

<a id="rel-016"></a>
### REL-016 — Limite de corpo nas quatro rotas Bazel
**Identidade:** `repo:1232040291:boundary:hash-server-bazel-limit-001`.  
**Dependência / fluxo / impacto:** server→hash; corpo→extrator; hash→router.  
**Superfície:** router: DefaultBodyLimit::max; [call sites enumerados](#bazel-calls).  
**Ativação:** AC/CAS v2 e aliases /bazel/cache/{ac,cas}.  
**Contrato:** CACHE_ENTRY_MAX_BYTES usado na extração.  
**Estado / efeitos:** admissão de payload; memória no consumer.  
**Falha / propagação:** mudar constante muda admissão; não muda todos os limites.  
**Contenção:** handler/edge/transporte restantes não certificados.  
**Validação:** abaixo/no/acima em cada rota; não executado.  

**Coordenação / fontes:** fingerprint e superfície coincidem com `corelink-server` BLAST REL-048 (source pin `47f4db7`); owner nominal, montagem/runtime, edge e tráfego não foram verificados. [S18](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-container/src/routes/bazel_v2/part-00.rs#L290-L357) [S02](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/src/lib.rs) · [peer REL-048](../corelink-server/BLAST_RADIUS.md#rel-048). [Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — Fake demonstrador da fronteira
**Identidade:** `repo:1232040291:boundary:hash-test-memory-store-001`.  
**Dependência / fluxo / impacto:** test→hash; VerifiedBody→HashMap; hash→fixture.  
**Superfície:** MemoryBlobStore::put_verified.  
**Ativação:** tokio::test.  
**Contrato:** chave digest.to_hex; body.clone.  
**Estado / efeitos:** HashMap protegido por Mutex.  
**Falha / propagação:** contrato quebrado falha no teste.  
**Contenção:** só fake; não prova storage do produto.  
**Validação:** dois testes em blob_store_contract; não executados.  
**Coordenação / fontes:** hash; [S07](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/blob_store_contract.rs). [Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — Regressões do contrato público
**Identidade:** `repo:1232040291:boundary:hash-test-regressions-001`.  
**Dependência / fluxo / impacto:** test→hash; entradas→assertivas; hash→testes.  
**Superfície:** prop_hash + mutation_kills; APIs da referência.  
**Ativação:** targets de integração.  
**Contrato:** parser, vetores, par, erros e formatters.  
**Estado / efeitos:** estado local de teste.  
**Falha / propagação:** falso positivo se houver testes ignorados ou seleção vazia.  
**Contenção:** nomes e flags devem aparecer no log real.  
**Validação:** PROC-002/004; não executados.  
**Coordenação / fontes:** hash; [S08](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/mutation_kills.rs) [S09](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-hash/tests/prop_hash.rs). [Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — Gate PR de Rust e wasm
**Identidade:** `repo:1232040291:boundary:hash-ci-pr-001`.  
**Dependência / fluxo / impacto:** workflow→hash; comandos→resultados; hash↔pipeline.  
**Superfície:** pr-gate/wasm-build; início do fuzz-smoke.  
**Ativação:** pull_request paths no trecho on lido.  
**Contrato:** clippy/debug/release/wasm; fuzz não certificado neste recorte.  
**Estado / efeitos:** consome runners; não executado agora.  
**Falha / propagação:** comentário de nightly não cria schedule.  
**Contenção:** restante do workflow e runs não certificados.  
**Validação:** conferir on e executar gates pertinentes.  
**Coordenação / fontes:** integração do repo; [S12](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L1-L155). [Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — Serialização de identidade AC
**Identidade compartilhada:** `repo:1232040291:boundary:hash-ac-digest-serde-001`; peer [corelink-ac B06](../corelink-ac/BLAST_RADIUS.md#b06).

**Dependência / fluxo / impacto:** ac→hash; Digest↔JSON; hash→AC→clientes.  
**Superfície:** `digest_serde::{serialize,deserialize}` e `ActionDigest`.  
**Ativação:** serialização/leitura de resultado de ação.  
**Contrato:** 64 hex lowercase; leitura chama `Digest::from_hex`.  
**Estado / efeitos:** tipos de ação e output carregam `Digest`; sem IO nesta função.  
**Falha / propagação:** mudar case, largura ou parser quebra wire/serde AC.  
**Contenção:** `size_bytes` pertence ao tipo AC/REAPI, não ao hash.  
**Validação:** round-trip e entradas inválidas AC; pendente.  
**Coordenação / fontes:** AC; peer pendente; [S20](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-ac/src/ac_core/types.rs#L15-L55). [Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — Chave persistente de metadados
**Identidade compartilhada:** `repo:1232040291:boundary:hash-meta-blob-key-001`; peer [corelink-meta B01](../corelink-meta/BLAST_RADIUS.md#b01).

**Dependência / fluxo / impacto:** meta→hash; `(tenant,Digest)`→texto D1; hash→metadados.  
**Superfície:** `BlobMetaKey::{new,digest_canonical_text}`.  
**Ativação:** leitura ou escrita de `blob_meta`.  
**Contrato:** chave recebe `Digest`; texto é 64 hex lowercase.  
**Estado / efeitos:** endereço composto de linha D1; SQL é do package meta.  
**Falha / propagação:** mudar `to_hex` muda chave e compatibilidade de linhas.  
**Contenção:** hash não valida tenant, schema nem operação D1.  
**Validação:** schema_canonical e leitura N/N-1; não executados nesta revisão.  
**Coordenação / fontes:** meta; peer pendente; [S21](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-meta/src/types.rs#L53-L94). [Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — Fingerprint da migração D1
**Identidade compartilhada:** `repo:1232040291:boundary:hash-meta-migration-fingerprint-001`; peer [corelink-meta B02](../corelink-meta/BLAST_RADIUS.md#b02).

**Dependência / fluxo / impacto:** meta→hash; SQL embutido→fingerprint; hash→gate de migração.  
**Superfície:** `migration_sql_blake3_hex`.  
**Ativação:** teste/lint de migração canônica.  
**Contrato:** BLAKE3 do `include_str!` da migração, em hex.  
**Estado / efeitos:** somente string computada; execução D1 é externa.  
**Falha / propagação:** alteração do algoritmo ou bytes sinaliza drift de migração.  
**Contenção:** fingerprint não aplica a migração nem observa banco implantado.  
**Validação:** schema e gate meta; pendentes.  
**Coordenação / fontes:** meta; peer pendente; [S22](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-meta/src/schema.rs#L35-L52). [Relation index](#b03)

<a id="rel-023"></a>
### REL-023 — Escrita REAPI fail-closed
**Identidade compartilhada:** `repo:1232040291:boundary:hash-reapi-verified-write-001`; peer [corelink-reapi REL-001](../corelink-reapi/BLAST_RADIUS.md#rel-001).

**Dependência / fluxo / impacto:** reapi→hash; body+claim→VerifiedBody→R2/D1; hash→REAPI.  
**Superfície:** `CasWriteOrchestrator::commit_put`.  
**Ativação:** `CommitPutPlan` na escrita por blob.  
**Contrato:** mismatch retorna antes de tocar R2 ou D1.  
**Estado / efeitos:** verificação precede I/O; rollback posterior pertence ao orquestrador.  
**Falha / propagação:** mudança na verificação pode permitir ou bloquear escrita remota.  
**Contenção:** não prova montagem gRPC, R2, D1 ou rollback em produção.  
**Validação:** prop_cas e handler; pendentes.  
**Coordenação / fontes:** REAPI; peer pendente; [S23](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-reapi/src/orchestrator.rs#L245-L285). [Relation index](#b03)

<a id="rel-024"></a>
### REL-024 — Limite REST do Bazel bridge
**Identidade compartilhada:** `repo:1232040291:boundary:hash-bazel-bridge-size-limit-001`; peer [corelink-bazel-bridge B05](../corelink-bazel-bridge/BLAST_RADIUS.md#b05).

**Dependência / fluxo / impacto:** bazel-bridge→hash; tamanho REAPI→admissão REST; hash→bridge.  
**Superfície:** `MAX_BLOB_SIZE_BYTES` e `validate_size`.  
**Ativação:** parse de digest de upload REST.  
**Contrato:** máximo é `CACHE_ENTRY_MAX_BYTES`; excedente é `InvalidDigest`.  
**Estado / efeitos:** a bridge usa digest SHA-256 próprio; não usa `corelink_hash::Digest`.  
**Falha / propagação:** mudar a constante altera admissão REST e recomendação gRPC.  
**Contenção:** não valida o corpo nem certifica a rota montada.  
**Validação:** max/max+1 em integration.rs; pendente.  
**Coordenação / fontes:** bazel-bridge; peer pendente; [S24](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-bazel-bridge/src/lib.rs#L68-L80) [S25](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-bazel-bridge/src/digest.rs#L120-L162). [Relation index](#b03)

<a id="rel-025"></a>
### REL-025 — R2 do harness de signup
**Identidade:** `repo:1232040291:boundary:hash-signup-r2-integrity-001`.  
**Dependência / fluxo / impacto:** e2e-signup-flow→hash; Digest→chave+bytes→harness.  
**Superfície:** `SignupR2::{put,get}` e `compose_key`.  
**Ativação:** cenário de signup que grava ou lê blob do fake.  
**Contrato:** chave `tenant:digest.to_hex`; GET recomputa e compara em tempo constante.  
**Estado / efeitos:** `HashMap` isolado e contadores do harness.  
**Falha / propagação:** corrupção retorna `DigestMismatch`; alterar hex muda a chave.  
**Contenção:** é harness local, não R2/produção.  
**Validação:** happy_path_free e caso de corrupção; não executados nesta revisão.  
**Coordenação / fontes:** e2e-signup-flow; peer pendente; [S26](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/tests/e2e-signup-flow/src/r2.rs#L90-L170). [Relation index](#b03)

<a id="rel-026"></a>
### REL-026 — Limite de leitura CAS do servidor
**Identidade:** `repo:1232040291:boundary:hash-server-cas-read-limit-001`.  
**Dependência / fluxo / impacto:** server→hash; constante→orçamento de leitura CAS; hash→servidor.  
**Superfície:** `CAS_READ_MAX_OBJECT_BYTES`.  
**Ativação:** caminho CAS que consulta o máximo.  
**Contrato:** alias `u64` de `CACHE_ENTRY_MAX_BYTES`, separado da verificação de digest.  
**Estado / efeitos:** política/limite do consumer; não há I/O nesta constante.  
**Falha / propagação:** mudança altera seleção/admissão de leitura CAS.  
**Contenção:** não prova que todos os caminhos de leitura aplicam o máximo.  
**Validação:** testes e montagem CAS; pendentes.  

**Coordenação / fontes:** fingerprint e superfície coincidem com `corelink-server` BLAST REL-049 (source pin `47f4db7`); owner nominal, seleção do caminho e runtime continuam UNKNOWN. [S27](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-container/src/routes/cas/foundation_core.rs#L181-L185) · [peer REL-049](../corelink-server/BLAST_RADIUS.md#rel-049). [Relation index](#b03)

<a id="rel-027"></a>
### REL-027 — Limite DELETE CAS do servidor
**Identidade:** `repo:1232040291:boundary:hash-server-cas-delete-limit-001`.  
**Dependência / fluxo / impacto:** server→hash; corpo DELETE→extrator; hash→router CAS.  
**Superfície:** `DefaultBodyLimit::max(CACHE_ENTRY_MAX_BYTES)` no setup single CAS.  
**Ativação:** rota DELETE do setup.  
**Contrato:** limite de corpo é a constante canônica; não verifica integridade do objeto.  
**Estado / efeitos:** admissão antes do handler.  
**Falha / propagação:** mudar a constante muda aceitação/memória dessa rota.  
**Contenção:** não é uma das quatro rotas Bazel de REL-016.  
**Validação:** abaixo/no/acima e montagem; pendentes.  

**Coordenação / fontes:** fingerprint e superfície coincidem com `corelink-server` BLAST REL-050 (source pin `47f4db7`); owner nominal, montagem/runtime e request permanecem UNKNOWN. [S28](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-container/src/routes/cas/single_setup.rs#L158-L161) · [peer REL-050](../corelink-server/BLAST_RADIUS.md#rel-050). [Relation index](#b03)

<a id="rel-028"></a>
### REL-028 — Dependência declarada do CAS
**Identidade compartilhada:** `repo:1232040291:boundary:hash-cas-manifest-001`; peer [corelink-cas B06](../corelink-cas/BLAST_RADIUS.md#b06).
**Dependência / fluxo / impacto:** cas→hash; manifesto direto.  
**Superfície:** `corelink-hash` no manifesto CAS.  
**Ativação:** seleção CAS no target normal.  
**Contrato:** graph inclui hash; busca não encontrou import no `src` CAS.  
**Estado / efeitos:** não há uso semântico provado.  
**Falha / propagação:** remover pode afetar build.  
**Contenção:** declaração não prova uso nem reachability.  
**Validação:** metadata/tree e import indireto/gerado; pendente.  
**Coordenação / fontes:** CAS; peer pendente; [S29](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-cas/Cargo.toml#L29-L33). [Relation index](#b03)

<a id="rel-029"></a>
### REL-029 — Harness standalone de fuzz (relação recíproca)
- **Identidade:** `repo:1232040291:boundary:hash-fuzz-harness-001`; fingerprint histórico, recalcular em `cacc44`.
- **Endpoints:** `corelink-hash/fuzz/Cargo.toml` → `corelink-hash/Cargo.toml`; contrato do provider, implementação do harness.
- **Contrato/ativação:** dependência path, oráculos `Digest`/`VerifiedBody` (F01) e dois bins; PR/nightly são declarações (F09/F10), não execuções.
- **Falha:** mudança de API/engine pode quebrar build, oráculo ou job; nenhum alcance produtivo demonstrado.
- **Estado/validação:** Cargo, target, fuzz, cobertura e runtime `UNKNOWN`; F14 registra preparação falha e steps skipped. Validar metadata, bins e workflows.
- **Revisão:** peer reconciliation e cold review `REQUIRED`; reviewer/operador `UNKNOWN`; anchors F01/F03/F04/F09/F10/F14. [Relation index](#b03)

<a id="rel-030"></a>
### REL-030 — Oráculo local do parser de digest
- **Identidade:** `repo:1232040291:boundary:hash-fuzz-digest-parse-001`.
- **Endpoints/owner:** consumer `repo:1232040291:crates/corelink-hash/fuzz/fuzz_targets/digest_parse.rs`; provider `repo:1232040291:crates/corelink-hash/src/digest.rs`; contrato `corelink-hash`, implementação/oráculo `corelink-hash-fuzz`.
- **Superfície/ativação:** bytes não UTF-8 retornam; UTF-8 chama `Digest::from_hex`; bin `digest_parse` selecionado pelo engine (F03), sem execução observada.
- **Contrato/falha:** input UTF-8 não deve causar panic; `Result` é descartado pelo alvo. Panic pode derrubar processo/job; cobertura e reachability `UNKNOWN`.
- **Validação/revisão:** validar target e casos UTF-8/invalid; reviewer nominal `UNKNOWN`; peer reconciliation e cold review `REQUIRED`. [Relation index](#b03)

<a id="rel-031"></a>
### REL-031 — Oráculo local do corpo verificado
- **Identidade:** `repo:1232040291:boundary:hash-fuzz-verify-body-001`.
- **Endpoints/owner:** consumer `repo:1232040291:crates/corelink-hash/fuzz/fuzz_targets/verify_body.rs`; provider `repo:1232040291:crates/corelink-hash/src/{digest,verified_body}.rs`; contrato `corelink-hash`, implementação/oráculo `corelink-hash-fuzz`.
- **Superfície/ativação:** prefixo de 32 bytes é claim e sufixo é `Bytes`; usa `Digest::{from_hex,compute,verify_constant_time}` e `VerifiedBody::{new,body}`; bin `verify_body` selecionado pelo engine (F04), sem execução observada.
- **Contrato/falha:** match exige `Ok` e comprimento preservado; mismatch exige `Err`; assertion/expect pode gerar crash input; cobertura e runtime `UNKNOWN`.
- **Validação/revisão:** validar inputs `<32`, match e mismatch; reviewer nominal `UNKNOWN`; peer reconciliation e cold review `REQUIRED`. [Relation index](#b03)

<a id="rel-032"></a>
### REL-032 — Meta fuzz usa Digest
- **Identidade compartilhada:** `repo:1232040291:boundary:meta-fuzz-hash-001`; peer [corelink-meta-fuzz REL-002](../corelink-meta-fuzz/BLAST_RADIUS.md#rel-002).
- **Endpoints/owner:** consumer `repo:1232040291:crates/corelink-meta/fuzz/Cargo.toml`; provider `repo:1232040291:crates/corelink-hash/Cargo.toml`; `corelink-meta-fuzz→corelink-hash`; contrato `corelink-hash`, harness `corelink-meta-fuzz`.
- **Superfície/ativação:** ambos os bins transformam bytes em hex lowercase e chamam `Digest::from_hex` para `BlobMetaKey` (M02/M03); seleção externa, sem execução observada.
- **Falha/estado:** mudança de tipo/parser pode quebrar o harness/oráculo; valor fica em memória e os targets não chamam D1/R2/worker.
- **Validação/revisão:** Cargo resolve/target e targets `UNKNOWN`; validar M01–M03 e peer; reviewer nominal `UNKNOWN`, cold review `REQUIRED`. [Relation index](#b03)

<a id="rel-033"></a>
### REL-033 — Worker fuzz usa digest verificado
- **Identidade / chave global:** `repo:1232040291:boundary:worker-fuzz-hash-001`; fingerprint e uso de `Digest::compute`/`VerifiedBody::new` coincidem com `corelink-worker-fuzz` BLAST REL-002 (source pin `1177dad`). A identidade está reconciliada; owner nominal, resolução Cargo, execução fuzz e runtime permanecem UNKNOWN.
- **Endpoints/owner:** consumer `repo:1232040291:crates/corelink-worker/fuzz/Cargo.toml`; provider `repo:1232040291:crates/corelink-hash/Cargo.toml`; `corelink-worker-fuzz→corelink-hash`; contrato `corelink-hash`, harness `corelink-worker-fuzz`.
- **Superfície/ativação:** ambos os bins usam `Digest::compute` e `VerifiedBody::new` (W02/W03); seleção externa, sem execução observada.
- **Falha/estado:** mudança de API pode quebrar setup/oráculo; estado é `InMemoryR2` novo por input, sem binding remoto ou reachability produtiva demonstrada.
- **Validação/revisão:** Cargo resolve/target e fuzz `UNKNOWN`; validar W01–W03; reviewer nominal `UNKNOWN`, cold review `REQUIRED`. [peer REL-002](../corelink-worker-fuzz/BLAST_RADIUS.md#rel-002) [Relation index](#b03)

<a id="rel-034"></a>
### REL-034 — Gate PR fuzz-smoke
- **Identidade/fingerprint:** `repo:1232040291:boundary:hash-ci-fuzz-pr-001`; chave compartilhada com peer.
- **Endpoints/owner:** workflow `.github/workflows/corelink-hash.yml` → job `fuzz-smoke`; owner CI `UNKNOWN`; path/job/status → PR.
- **Superfície/ativação:** [paths](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L8-L14); PR elegível; [job/targets](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L144-L192); smoke declarado de 60 s por alvo.
- **Contrato/estado/efeito:** `digest_parse`/`verify_body` → status do job/PR; runner self-hosted Mac declarado; run/resultados `UNKNOWN`; YAML não prova execução.
- **Falha/propagação:** falha ou fila pode bloquear PR; não propaga para runtime, cobertura ou reachability produtiva. **Boundary/contensão:** configuração CI; cache separado em REL-036.
- **Coordenação/evidência:** owner CI `UNKNOWN`; peer `corelink-hash-fuzz` REL-011; [S12 job](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L144-L192); revisar paths/condition/targets e cold review `REQUIRED`. [Relation index](#b03)

<a id="rel-035"></a>
### REL-035 — Gate PR wasm-build
- **Identidade/fingerprint:** `repo:1232040291:boundary:hash-ci-wasm-001`; chave compartilhada com peer.
- **Endpoints/owner:** workflow `.github/workflows/corelink-hash.yml` → job `wasm-build`; owner CI `UNKNOWN`; source/check/status → PR.
- **Superfície/ativação:** [job/target](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L116-L142); PR elegível, distinto de fuzz-smoke.
- **Contrato/estado/efeito:** `cargo check --package corelink-hash --target wasm32-unknown-unknown` verifica compilação wasm; runner/output `UNKNOWN`; não compila nem executa fuzz.
- **Falha/propagação:** falha pode bloquear PR; efeito limitado ao status/check de compilação. **Boundary/contensão:** não prova runtime Cloudflare ou reachability produtiva; cache separado em REL-036.
- **Coordenação/evidência:** owner CI `UNKNOWN`; peer `corelink-hash-fuzz` REL-012; [S12 wasm](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L116-L142); revisar path/target/ausência de fuzz e cold review `REQUIRED`. [Relation index](#b03)

<a id="rel-036"></a>
### REL-036 — Cache condicionado do fuzz-smoke
- **Identidade/fingerprint:** `repo:1232040291:boundary:hash-fuzz-cache-001`; chave compartilhada com peer.
- **Endpoints/owner:** workflow → `Swatinem/rust-cache`; workspace `crates/corelink-hash/fuzz` → `target`; owner CI `UNKNOWN`.
- **Superfície/ativação:** [cache/condition/workspace](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L183-L186); no runner self-hosted, condição `github-hosted` pula cache.
- **Contrato/estado/efeito:** cache acelera build; hit/miss pode alterar warmness, duração e custo; cache remoto/runner `UNKNOWN`.
- **Falha/propagação:** falha de cache pode afetar tempo/job conforme ação; não altera contrato nem prova build, fuzz, cobertura ou runtime. **Boundary/contensão:** aceleração CI, distinta de gates REL-034/035.
- **Coordenação/evidência:** owner CI `UNKNOWN`; peer `corelink-hash-fuzz` REL-013; [S12 cache](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/.github/workflows/corelink-hash.yml#L183-L186); revisar `if`/workspace/runner, sem consulta live, cold review `REQUIRED`. [Relation index](#b03)

<a id="rel-037"></a>
### REL-037 — Hash de conteúdo da cache de adapter
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-adapter-cache-001`; peer `corelink-server` REL-082.

**Endpoints/owner:** `canonical_hash_hex(bytes)` → `Digest::compute(bytes).to_hex()`; server usa o hex na chave de conteúdo da MoatCache production adapter, alinhada ao digest BLAKE3 de CAS.

**Superfície/ativação:** a função retorna `Digest::compute(bytes).to_hex()` para a chave de conteúdo usada pela `MoatCache::production` e alinhada à chave do CAS BLAKE3.
**Efeito/falha:** mudança de algoritmo/case/largura altera a identidade e pode causar cache miss ou desacordo com CAS; não é framing de objeto persistido.
**Contenção/validação:** conteúdo e hit/miss dependem do caller/store; comparar chave canônica e CAS em fixtures. SOURCE em `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`, `adapter_cache.rs:153-158`. Runtime/cache remota UNKNOWN. [peer REL-082](../corelink-server/BLAST_RADIUS.md#rel-082) · [Relation index](#b03)

<a id="rel-038"></a>
### REL-038 — Hash de bytes TCS encapsulados BYOK
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-byok-tcs-001`; peer `corelink-server` REL-083.

**Endpoints/owner:** `ActivationHashes::new` → `Digest::compute(&activation.tcs_wrapped).to_hex()`; hash possui digest; server associa a coluna `wrapped_tcs_blake3` à intent BYOK.
**Superfície/ativação:** cada construção de `ActivationHashes` produz fingerprint hex dos bytes encapsulados TCS, persistido pela preparação da intent.
**Efeito/falha:** algoritmo ou codificação alterados mudam o fingerprint persistido e comparações subsequentes; não decifra nem valida a chave.
**Contenção/validação:** bytes e D1 pertencem ao server; comparar registro persistido e reconstrução em fixture N/N-1. SOURCE em `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`, `byok_control_transition.rs:1841-1851,875-878`. Runtime BYOK/D1 UNKNOWN. [peer REL-083](../corelink-server/BLAST_RADIUS.md#rel-083) · [Relation index](#b03)

<a id="rel-039"></a>
### REL-039 — Fingerprint da política de ativação BYOK
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-byok-policy-001`; peer `corelink-server` REL-084.

**Endpoints/owner:** `ActivationHashes::new` → `framed_blake3("corelink.byok.activation-policy.v1", mode, crypto_mode, provider, key_id, region)` → `Digest::compute(canonical).to_hex()`; hash possui digest, server define domínio/campos/framing.
**Superfície/ativação:** computado na preparação/transição BYOK e gravado como `policy_blake3` da intent.
**Efeito/falha:** alterar domínio, ordem, bytes ou framing muda fingerprint da política; contrato compõe campos codificados com prefixo de comprimento u64 big-endian.
**Contenção/validação:** mudanças de semântica/framing coordenam server + hash; comparar golden bytes/fingerprint e leitura N/N-1. SOURCE em `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`, `byok_control_transition.rs:1841-1850,1868-1880,875-878`. Runtime/D1 UNKNOWN. [peer REL-084](../corelink-server/BLAST_RADIUS.md#rel-084) · [Relation index](#b03)

<a id="rel-040"></a>
### REL-040 — Fingerprint da requisição BYOK
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-byok-request-001`; peer `corelink-server` REL-085.

**Endpoints/owner:** `ActivationHashes::new` codifica domínio e campos tenant/policy/wrapped-TCS, depois calcula BLAKE3 hex; `request_blake3` participa da lookup/idempotência da intent.

**Superfície/ativação:** lookup da intent usa o fingerprint; INSERT da intent grava a coluna correspondente.
**Efeito/falha:** alterar campo, ordem, codificação ou domain separator altera igualdade/idempotência do request e pode mudar lookup existente.
**Contenção/validação:** tenant bytes, texto policy/TCS e persistência são do server; testar igualdade de mesma requisição e mudança unitária de cada campo contra fixture persistida. SOURCE em `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`, `byok_control_transition.rs:74-78,113-116,1841-1864,875-878`. Runtime/D1 UNKNOWN. [peer REL-085](../corelink-server/BLAST_RADIUS.md#rel-085) · [Relation index](#b03)

<a id="rel-041"></a>
### REL-041 — Fingerprint canônico de evento de billing
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-billing-payload-001`; peer `corelink-server` REL-086.

**Endpoints/owner:** `payload_hash` envia oito campos canônicos unidos por US ao `Digest`; server define serialização, hash crate fornece BLAKE3/hex.

**Superfície/ativação:** campos `tenant_id,event_kind,qty,billing_period,region,source,time_ms,idem_key` são formatados em UTF-8 separados por `\x1f`; resultado hex alimenta `event_payload_hash`.
**Efeito/falha:** fingerprint é entrada da detecção de conflito no staging por `(tenant_id, request_id)`; mudar hash ou serialização muda deduplicação/conflito, não o valor monetário em si.
**Contenção/validação:** separador é não ambíguo para os domínios canônicos descritos no source, sem prova adicional de validação de todos os campos aqui; testar determinismo/mudança de campo/conflito. SOURCE em `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`, `routes/billing_ingest.rs:255-278`. Runtime/D1 UNKNOWN. [peer REL-086](../corelink-server/BLAST_RADIUS.md#rel-086) · [Relation index](#b03)

<a id="rel-042"></a>
### REL-042 — Verificação BLAKE3 de conteúdo CAS multipart
**Identidade compartilhada:** `repo:1232040291:boundary:hash-container-cas-verify-001`; peer `corelink-server` REL-087.

**Endpoints/owner:** `verify_content_hash(DigestAlgo::Blake3, claimed_hash, bytes)` → `Digest::{compute,from_hex,verify_constant_time}`; hash define operações digest/parser; server seleciona algoritmo e decide admission/error.
**Superfície/ativação:** helper seleciona BLAKE3 para CAS nativo; caminho SHA-256 separado não usa `corelink-hash`. Claim inválido ou mismatch retorna `Err(actual_hex)` antes de servir/persistir bytes conforme o caller.
**Efeito/falha:** parser/hash alterados mudam admissão de conteúdo CAS; `Ok(())` indica correspondência, não escrita nova.
**Contenção/validação:** helper privado não prova request R2; validar match/mismatch/malformed claim por algoritmo. SOURCE em `b9b3ee8cba6ad6f43f73fe785fb99acb55192019`, `storage/r2_s3_parts/cas_helpers.rs:188-202`. Runtime/R2 UNKNOWN. [peer REL-087](../corelink-server/BLAST_RADIUS.md#rel-087) · [Relation index](#b03)

<a id="rel-043"></a>
### REL-043 — Alias público para FFI
**Identidade:** `repo:1232040291:boundary:hash-client-ffi-001`. **Endpoints:** `corelink-client-verify` → `corelink-hash`; contrato owner: hash.
**Superfície/ativação:** reexporta `Digest`, `ParseError`, `DIGEST_LEN` e código; header C define `DIGEST_BYTES` e `DIGEST_HEX_LEN` para a ABI.
**Efeito/falha:** quebra de tipo/largura/nome pode quebrar consumidores FFI; reexport não prova ABI compatível nem chamada.
**Contenção/validação:** manter alias e revisar header/geração no peer. [fonte](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-client-verify/src/digest.rs#L1-L20) [header](https://github.com/HuGR-Labs/corelink-server/blob/cacc44fc43ee2f481e933419ba88b9a19ac6c8e8/crates/corelink-client-verify/include/corelink_client_verify.h#L20-L31). [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagação transitiva sem prova fictícia

| Destino | Caminho | Condição | Efeito | Evidência faltante |
|---|---|---|---|---|
| Imports canônicos | hash → crypto → callers | Reexport compilado | Mudança de API aparece no alias | Enumerar callers/targets |
| SDKs | hash → client-verify → wrapper | Features e alvo do wrapper | Tipos, tamanho e código de erro | Trace FFI e teste por linguagem |
| Objetos R2 | hash → key/writer → backend | Backend construído pelo caller | Novo namespace ou formato | Artefato + leitura N/N-1 |
| Envelope KV | hash → R2KvStore → caller | Código de wrap/unwrap utilizado | Decode/accounting divergentes | Teste de integração e ativação |
| Adapters internos | REL-037..042 → cache/BYOK/billing/CAS | Caller selecionado | Identidade ou decisão de integridade muda | Fixtures N/N-1 e testes do consumer |
| Bazel | constante → camadas de router | Router montado | Payload admitido/recusado | Mounts, edge e handlers |
| FFI | REL-043 → header/client verifier | cbindgen/feature selecionados | Alias ou largura incompatível | Header, ABI e teste do peer |
| Fuzz independentes | REL-029..033 → targets standalone | Bin/engine selecionado | Mudança de API pode quebrar build/oráculo/job | Metadata, target e run record; sem runtime inferido |
| Gates do package | REL-034/035/036 | PR path e condição do runner | Status, compilação wasm ou warmness mudam | YAML, target e run/cache record; desconhecidos |

Estes são caminhos causais identificados, não todos os nós do grafo.
Não existe nesta edição prova de não alcance por target; tabela não substitui Cargo nem inspeção semântica.

<a id="b05"></a>
## B05 — Mudança, impacto e validação

| Mudança | Contratos / REL | Efeito | Validação exigida | Recuperação |
|---|---|---|---|---|
| Algoritmo/largura/case | API-001/003; 014/015 | Chaves e conteúdo anterior incompatíveis | Vetores + leitura antes/depois | PROC-006; não reinterpretar bytes |
| Alterar/remover as_bytes ou largura | API-006; REL-015 | O consumer persiste/compara bytes e pode deixar de ler envelopes anteriores; suporte de estabilidade oficial não está estabelecido | Compilar consumers e validar envelopes Legacy/Verified/Corrupt + compatibilidade N/N-1 | Coordenar contrato do tipo (hash) e framing (server/adapter); aprovador formal não identificado |
| Alterar parser | API-002; 011/018 | Inputs aceitos, offsets e clients | Uppercase, posições, length, erro | Versão compatível ou transição |
| Alterar writer result | API-008; 012/017 | Fresh/duplicate/falha deixam de equivaler | Estado final e efeito do adapter | Sem retry cego de IO ambíguo |
| Aumentar limite | API-009; 016/024/026/027 | Admissão e memória | Rotas Bazel, bridge e CAS | Rollback de admissão, não de dados |
| Alterar debug/erro | API-005/007; 013/018 | Logs e error mapping | Redaction + códigos por consumer | Preservar compatibilidade |
| Alterar gates/cache | REL-034/035/036 | Trigger, target wasm, runner ou warmness muda | Revisar paths, condição, target e cache; run unknown | Reverter configuração própria; não tratar cache como prova |
| Alterar identidade de bytes | REL-037..041 | Cache, BYOK ou billing deriva fingerprints diferentes | Fixtures e leitura N/N-1 no consumer | Não apagar estado; coordenar migração |
| Alterar parser/claim CAS | REL-042 | Aceitação de objeto muda | Match/mismatch/parse por algoritmo | Parar antes de aceitar dados ambíguos |
| Alterar reexport/largura | REL-010/011/042 | Rust, header ou SDK quebra | Reexport, header e peer FFI | Não declarar ABI compatível sem artefato |

<a id="b06"></a>
## B06 — Cobertura e pendências de fechamento

O censo detalhado dos pares consultados está em
[`HASH-PEER-CENSUS-20260923.md`](../../evidence/revision-1.4/HASH-PEER-CENSUS-20260923.md);
ele preserva as fronteiras sem par como `UNKNOWN` e não promove ausência de
chave a ausência de uso.

| População | Descobertos | Documentados | Excluídos | Desconhecidos |
|---|---:|---:|---:|---|
| Declarações externas normais | 5 | 5 | 0 | Versões resolvidas |
| Declarações externas dev | 4 | 4 | 0 | Features resolvidas |
| Relações específicas desta edição | 43 | 43 | 0 | População global não certificada |
| Ledger estruturado de relações | 43 | 43 | 0 | Owner nominal, target, runtime e peer cold review ainda têm gates separados |
| Consumidores Cargo diretos | 13 | 13 | 0 | Alias/target/uso indireto pendentes |
| Demais recursos/contratos externos | Não certificado | Nenhum runtime observado | Nenhuma exclusão definitiva | Cargo, SQL, geração, scripts, demais repos |

**Candidatos por manifesto descobertos no Mac:** client-verify, cas, meta, worker, server,
reapi, ac, bazel-bridge, crypto, e2e-signup-flow; fuzzers hash, meta e worker.
A lista é semente: confirmar todos os manifests no commit fixado e procurar aliases/path dependencies.

**Candidatos semânticos ainda abertos:** imports renomeados, código gerado além do header FFI,
scripts, demais repos, resolução de features e a razão da declaração não usada do CAS. Não foram excluídos por falta de leitura.

**Conciliação documental dos peers nesta revisão:** identidades compartilhadas e
fatos SOURCE agora estão espelhados nos documentos peer citados. Isso não conclui
cold review, não identifica owner nominal, target selecionado, deployment nem runtime.

| Relação hash | Evidência peer localizada | Resultado documental |
|---|---|---|
| REL-020 | `corelink-ac` B06 and S20 `digest_serde`/`ActionDigest` | Shared source identity aligned; wire consumers/runtime unknown |
| REL-021/022 | `corelink-meta` B01/B02 and S21/S22 | Two atomic source identities aligned; D1 application/runtime unknown |
| REL-023 | `corelink-reapi` REL-001 and `commit_put` → `VerifiedBody::new` | Shared source seam aligned; request/storage reachability unknown |
| REL-024 | `corelink-bazel-bridge` B05 size-limit derivation | Shared constant relation aligned; SHA-256 remains distinct; route/runtime unknown |
| REL-028 | `corelink-cas` B06 manifest identity | Manifest edge only; semantic use/target/runtime unknown |
| REL-032 | `corelink-meta-fuzz` REL-002 and both `Digest` parser consumers | Shared source identity aligned; invocation/runtime unknown |
| REL-037..042 | `corelink-server` REL-082..087 at source-identical pins `b9b3ee8`/`91630ba` | Six source identities aligned; nominal owner, selection, deployment/runtime unknown |
| REL-043 | `corelink-client-verify` B02 shared Rust-alias identity | Alias source aligned; ABI use/compatibility unknown |

O ledger mantém `peer_review=not_reconciled` até validação independente separada;
isso não contradiz o alinhamento documental de identities/facts SOURCE. Para
REL-037..042, os source files são idênticos entre o readback `b9b3ee8` e o pin
server `91630ba`; os seis peer records agora separam cada fronteira.

REL-041 foi recapturada em `b9b3ee8c`: confirma digest no fingerprint canônico de evento de billing, não hash de imagem de mídia. Owners/runtime seguem desconhecidos; `peer_review` permanece `not_reconciled` até validação documental independente do peer.

REL-028 é dependência declarada, não consumidor semântico comprovado.

**Fechamento:** validação independente de peer, consumer checks e cold review continuam pendentes. Não há reivindicação de censo global. A igualdade 43=43 vale somente para as relações descobertas nesta edição; alcance Cargo, FFI gerado e runtime continuam `UNKNOWN`.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01).
