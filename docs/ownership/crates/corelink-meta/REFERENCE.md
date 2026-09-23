---
schema: corelink-ownership/1.1
document: reference
package: corelink-meta
manifest: crates/corelink-meta/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: meta-static-graph-20260920
---

# corelink-meta — referência de ownership

Referência SOURCE da baseline fixada. Ela descreve contratos estáticos e fake
host-side; não certifica banco, worker, dreno, entrega, GC ou outro RUNTIME.

[Identidade](#r01) · [Fronteiras](#r02) · [Mapa](#r03) · [Esquema](#r04) ·
[Refcount](#r05) · [Outbox](#r06) · [Axiomas](#r07) · [Lacunas](#r08).

<a id="r01"></a>
## R01 — Identidade

| Campo | Fonte estática |
|---|---|
| Package | `corelink-meta`; `crates/corelink-meta/Cargo.toml` |
| Papel | Contrato CAS de `blob_meta`, operações e audit outbox |
| Dependência CoreLink | `corelink-hash` para `Digest` e hash da migração |
| Regra de leitura | SOURCE não é prova de RUNTIME; o fake não é um binding D1 |

<a id="r02"></a>
## R02 — Fronteiras

| Área | Nesta crate | Fora desta crate |
|---|---|---|
| Schema e SQL | Migração embutida, templates e tipos | Aplicação da migração no banco |
| Metadados | `MetaStore`, requests, outcomes e fake | Adapter D1 e handler |
| Auditoria | Envelope, dedupe e contrato de staging | Dreno, chain e entrega |
| Tombstone | Marcação e impedimento de mutação | GC físico, prazo e retenção |
| Arestas estrangeiras | Referência em template SQL | Schema/owner de `gc_purge_intent`, `tenant` e `audit_outbox.region` |

<a id="r03"></a>
## R03 — Mapa da implementação

| Arquivo | Responsabilidade observada |
|---|---|
| `schema.rs` | Exporta caminho, bytes embutidos e hash da migração |
| `cas_query.rs` | Templates parameterizados para metadata e outbox |
| `types.rs`, `error.rs` | Chave, row, eventos, outcomes e erros tipados |
| `store.rs` | Trait `MetaStore`, requests e `OutboxRow` |
| `fake.rs` | Implementação em memória para propriedades declaradas |

<a id="r04"></a>
## R04 — Esquema e chave

`BlobMetaKey` reúne `TenantId` e `Digest`; a documentação da fonte associa a
chave a `(tenant_id,digest)`. `MIGRATION_SQL` vem de
`migrations/d1/0001_blob_meta.sql` por `include_str!`; `MIGRATION_FILE_PATH`
expõe esse caminho. O schema/testes locais declaram `size_bytes > 0`,
`refcount >= 0`, `blob_meta`, `audit_outbox` e unicidade de
`(request_id,event_type)`. Isto não demonstra que qualquer banco recebeu a
migração.

`cas_query.rs` também referencia os schemas estrangeiros `gc_purge_intent`,
`tenant.primary_region` e `audit_outbox.region`. Comentários da fonte citam
migrações 0023, 0028 e 0107; seus arquivos, estado aplicado e owners são
externos a esta crate e devem ser roteados ao owner de schema/migração. Isto não
demonstra backend D1 nem migração aplicada.

<a id="r05"></a>
## R05 — Refcount e tombstone

`commit_put` declara inserção idempotente e primeiro refcount igual a um.
`commit_decrement` distingue valor positivo, zero, ausência, tombstone e
underflow. Os templates de incremento/decremento exigem `deleted_at IS NULL`;
`commit_soft_delete` declara escrita idempotente de `deleted_at`. `get` pode
retornar linha tombstoned, deixando o filtro alive ao caller. A fonte não prova
serialização de transações nem coleta física.

Além de `deleted_at IS NULL`, incremento e decremento declaram um predicado
estrangeiro contra `gc_purge_intent` para estados de purge. O schema e owner GC
desse predicado não são locais; a fonte não prova coordenação com GC.

<a id="r06"></a>
## R06 — Audit outbox

Todo request mutante do trait contém `AuditEvent`. A fonte declara que a
mutação e a linha `audit_outbox` compartilham batch atômico; a chave
`(request_id,event_type)` deduplica retry idêntico e payload diferente produz
`AuditIdempotencyConflict`. `OutboxRow.emitted_at_ms` pode estar ausente para
item pendente. Nenhum símbolo nesta crate prova que uma fila foi drenada, que
um evento foi emitido ou que uma chain recebeu o payload.

O INSERT busca `tenant.primary_region` para preencher `audit_outbox.region`.
São dependências de schema estrangeiro, não ownership local; SOURCE não prova
que região tenha sido validada por backend, dreno ou entrega.

<a id="r07"></a>
## R07 — Cinco axiomas falsificáveis

| Axioma | Falsificador estático mínimo |
|---|---|
| A01: key é tenant+digest | Remover um componente da chave/SQL PK |
| A02: tamanho é positivo | Remover `CHECK` ou aceitar zero no contrato |
| A03: refcount não é negativo | Remover `CHECK` ou caminho de underflow |
| A04: tombstone bloqueia refcount | Remover `deleted_at IS NULL` dos updates |
| A05: retry audit deduplica | Remover unicidade ou verificação de payload |

<a id="r08"></a>
## R08 — Lacunas e encaminhamento

Desconhecidos: owner nominal; adapter D1 concreto; aplicação e versão efetiva
da migração; atomicidade/race no backend; consumidores completos; worker de
dreno, GC físico, grace period, chain e entrega. Para alegar RUNTIME, anexar
entrypoint, adapter, ambiente e evidência observável; import, comentário,
template SQL ou fake não bastam.

As três rotas de schema exigem, antes de mudança de template, registro de
arquivo/migração e owner: `gc_purge_intent` (GC), `tenant.primary_region`
(tenant) e `audit_outbox.region` (audit schema). Nesta baseline, os owners
nominais e os arquivos autoritativos permanecem desconhecidos.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#r01)
