---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-bazel-bridge
manifest: crates/corelink-bazel-bridge/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: bazel-bridge-static-20260920
---

# corelink-bazel-bridge — blast radius

Relações abaixo são atômicas e derivadas de manifesto e fonte estática. Elas identificam dependência, fluxo ou impacto, não execução em produção.

[Escopo](#b01) · [Consumidor](#b02) · [CAS](#b03) · [AC](#b04) · [Hash](#b05) · [Lacunas](#b06).

<a id="b01"></a>
## B01 — Superfície própria

`uri`, `digest`, `adapter`, `find_missing` e `error` convergem em `lib.rs`; mudar um contrato exportado pode afetar callers do package. Evidência: módulos públicos de `src/lib.rs`. Pare antes de concluir que todos os callers foram enumerados.

<a id="b02"></a>
## B02 — Dependência do consumidor direto conhecido

`corelink-container` declara dependência em `corelink-bazel-bridge`; sua rota Bazel constrói `BazelAdapter` e `InMemoryFindMissing`. Impacto: mudanças de assinatura, URI ou erro requerem coordenação com a montagem HTTP. Evidência: manifest do container e `src/routes/bazel_v2/part-00.rs`. Pare: extração, autenticação e mounts pertencem ao container.

<a id="b03"></a>
## B03 — Fluxo CAS

As rotas Axum do `corelink-container` impõem path e verbo CAS, fazem parse de digest e chamam o adapter. `uri.rs` classifica formato, mas não impõe método para CAS. `BazelAdapter::cas_put` confere tamanho; o container chama `digest::verify_sha256` antes de delegar. Impacto: mudar qualquer uma dessas fronteiras requer coordenação container/bridge/handler. Evidência: `part-00.rs`, `part-00-01.rs`, `uri.rs` e `adapter.rs`. Pare se o handler precisar mudar.

<a id="b04"></a>
## B04 — Fluxo AC

As rotas Axum do `corelink-container` impõem path e verbos AC antes do adapter. `uri.rs` usa método para distinguir suas duas variantes AC, mas não é a autoridade de rota montada. O adapter delega a `AcLookupHandler` e `AcUpdateHandler`. Impacto: mudança de método, digest ou erro exige coordenação com container e handlers. Evidência: `part-00.rs`, `uri.rs` e `adapter.rs`. Pare se a regra de cache AC não estiver no bridge.

<a id="b05"></a>
## B05 — Dependência hash e limite

`MAX_BLOB_SIZE_BYTES` deriva de `corelink_hash::CACHE_ENTRY_MAX_BYTES`; esse teto é distinto do contrato SHA-256 REAPI. A verificação SHA-256 ocorre pela função do bridge chamada no boundary do container, antes de `cas_put`; o adapter só verifica comprimento.

Impacto: mudar teto, digest ou resposta requer coordenar corelink-hash e container sem fundir os contratos. **Identidade compartilhada:** `repo:1232040291:boundary:hash-bazel-bridge-size-limit-001`; hash-side record: [corelink-hash REL-024](../corelink-hash/BLAST_RADIUS.md#rel-024). Essa identidade cobre somente a derivação e uso do limite; SHA-256 continua contrato separado. Evidência: manifesto, `lib.rs`, `digest.rs`, `adapter.rs` e `part-00-01.rs`. Pare se a migração de dados for alegada sem prova.

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

Conhecido: um consumidor direto de manifesto, `corelink-container`, com montagem/extração Bazel observada em fonte. Desconhecido: outros consumidores, cliente Bazel real, R2, D1, edge headers, requests, runtime, deploy e tráfego. Não há observação de tais elementos neste recorte; isso não é prova de ausência.
