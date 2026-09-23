---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-meta
manifest: crates/corelink-meta/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: meta-static-graph-20260920
---

# corelink-meta — blast radius

Relações atômicas derivadas de SOURCE: manifesto, exports, tipos e templates.
Setas não provam RUNTIME, banco conectado, worker ativo ou entrega de auditoria.

[Chave](#b01) · [Schema](#b02) · [Tombstone](#b03) · [Commit](#b04) ·
[Dedupe](#b05) · [Dependentes](#b06).

<a id="b01"></a>
## B01 — Chave para linha

`TenantId` + `Digest` → `BlobMetaKey` → `blob_meta` PK. Alterar a forma,
ordem ou canonicalização afeta lookup, inserção, update e leitura. A fonte
descreve texto UUID/digest e parâmetros SQL; não prova chaves nas linhas reais.
**Peer estático:** `repo:1232040291:boundary:hash-meta-blob-key-001`; `BlobMetaKey` recebe `corelink_hash::Digest` e usa texto canônico no endereço. Hash-side record: [corelink-hash REL-021](../corelink-hash/BLAST_RADIUS.md#rel-021). Linhas reais D1, resolução e runtime continuam desconhecidos.

<a id="b02"></a>
## B02 — Migração para contratos

`migrations/d1/0001_blob_meta.sql` → `include_str!` → `MIGRATION_SQL` →
schema tests. Mudar bytes, caminho, tabela, check ou índice afeta a constante,
vetor de hash e expectativa da fake. Não se infere execução de migração.
**Peer estático:** `repo:1232040291:boundary:hash-meta-migration-fingerprint-001`; `migration_sql_blake3_hex` usa `corelink_hash::Digest` sobre o SQL embutido. Hash-side record: [corelink-hash REL-022](../corelink-hash/BLAST_RADIUS.md#rel-022). A fingerprint estática não prova aplicação da migration nem estado D1.

<a id="b03"></a>
## B03 — Tombstone para bloqueio

`commit_soft_delete` → `deleted_at` → updates com `deleted_at IS NULL` e
`NOT EXISTS gc_purge_intent` nos estados `purging`, `r2_deleted` ou `retry`.
Mudar qualquer guard pode permitir mutação após tombstone ou durante purge;
mudar `get` pode alterar a decisão do caller. `gc_purge_intent` pertence ao
schema/owner GC externo; a relação SOURCE não prova GC físico ou política.

<a id="b04"></a>
## B04 — Mutação para outbox

`CommitPutRequest`/`CommitDecrementRequest`/`CommitSoftDeleteRequest` →
`MetaStore` → metadata + `AuditEvent`. Alterar request, trait, erro ou template
afeta a promessa de pareamento no contrato e a fake. Não prova batch D1,
commit, callback ou entrega externa.

<a id="b05"></a>
## B05 — Retry para dedupe

`RequestId` + `AuditEventType` → `(request_id,event_type)` UNIQUE → outbox.
Alterar a chave ou comparação de payload muda o tratamento de retries e
conflitos. Não está demonstrado que clientes preservem request IDs nem que o
dreno processe a linha uma vez.

<a id="b06"></a>
## B06 — Superfície para dependentes

Exports públicos incluem `MetaStore`, requests, outcomes, tipos, `MetaError`,
`InMemoryMetaStore`, SQL/migração e `OutboxRow`; dependentes estáticos precisam
ser localizados antes de mudança incompatível. Relações estáticas observadas:
`corelink-cas` reexporta a superfície, `corelink-reapi` a declara/usa, e o
harness independente `corelink-meta-fuzz` consome `MetaStore::{commit_put,get}`
e `InMemoryMetaStore`; consumer-side record: [corelink-meta-fuzz REL-001](../corelink-meta-fuzz/BLAST_RADIUS.md#rel-001), shared key
`repo:1232040291:boundary:meta-fuzz-meta-provider-001`.
Elas não transferem ownership nem provam execução. O grafo reverso completo,
feature-gated, generated, externo e runtime permanece desconhecido.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
