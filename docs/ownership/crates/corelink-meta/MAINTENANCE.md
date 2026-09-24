---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-meta
manifest: crates/corelink-meta/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: meta-static-graph-20260920
---

# corelink-meta — manutenção

Modos limitados à análise SOURCE e checks documentais. Eles não autorizam
banco, worker, migração aplicada, deploy, rede ou declaração de RUNTIME.

[Baseline](#m01) · [Schema](#m02) · [Refcount](#m03) · [Tombstone](#m04) ·
[Outbox](#m05) · [Registro](#m06).

<a id="m01"></a>
## M01 — Confirmar baseline

Modo: `STATIC_LOCAL`. Predicado: SHA e manifesto coincidem com esta edição.
Ação: comparar baseline, branch e `crates/corelink-meta/Cargo.toml`. Parada:
fonte ou recorte divergente. Recuperação: não editar; reconciliar a referência.
Evidência: SHA, branch e caminhos lidos.

<a id="m02"></a>
## M02 — Alterar schema ou SQL

Modo: `STATIC_LOCAL`. Predicado: migração, constante, template, chave ou check
muda. Ação: mapear bytes, `include_str!`, binds, A01--A05 e registrar owner/
schema de `gc_purge_intent`, `tenant.primary_region` e `audit_outbox.region`.
Parada: dados, executor, ordem, arquivo ou owner de migração desconhecidos.
Recuperação: separar SOURCE da operação de banco. Evidência: diff, símbolos e
rota de schema/migração.

<a id="m03"></a>
## M03 — Alterar refcount

Modo: `STATIC_LOCAL`. Predicado: put, incremento, decremento, outcome ou erro
muda. Ação: verificar zero, underflow, `last_accessed_at`, `deleted_at IS NULL`,
guard `gc_purge_intent` e registrar as três rotas externas/owners. Parada:
atomicidade/concurrency ou schema GC requer backend/owner concreto. Recuperação:
preservar ambos guards ou escalar. Evidência: tipos, template e rota de schema.

<a id="m04"></a>
## M04 — Alterar tombstone

Modo: `STATIC_LOCAL`. Predicado: `deleted_at`, soft delete ou leitura muda.
Ação: comparar idempotência, não ressurreição, `deleted_at IS NULL`, guard
`gc_purge_intent` e registrar as três rotas externas/owners. Parada: grace,
retenção, deleção física ou schema GC solicitados. Recuperação: encaminhar ao
owner GC/schema. Evidência: `cas_query.rs`, trait e rota de schema/migração.

<a id="m05"></a>
## M05 — Alterar audit outbox

Modo: `STATIC_LOCAL`. Predicado: evento, request ID, payload, erro ou commit
muda. Ação: verificar pareamento, dedupe, `tenant.primary_region` →
`audit_outbox.region` e registrar as três rotas externas/owners. Parada:
afirmação de drain, chain, retry operacional, entrega ou schema externo.
Recuperação: exigir adapter, entrypoint e evidência RUNTIME. Evidência: trait,
SQL e rota de schema/migração.

<a id="m06"></a>
## M06 — Registrar e escalar

Modo: `STATIC_LOCAL`. Predicado: análise ou alteração terminou. Ação: registrar
SHA, arquivos, relações, comandos documentais, resultados e desconhecidos.
Parada: checker e `git diff --check` não são aprovação nem execução. Recuperação:
abrir decisão com fonte, consumer e owner necessários. Evidência: diff e saída
dos checks.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
