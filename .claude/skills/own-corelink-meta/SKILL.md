---
name: own-corelink-meta
description: >-
  Governa alterações nos contratos estáticos de metadados CAS: esquema, chave,
  refcount, tombstone e audit_outbox de corelink-meta. Não atribui D1, worker,
  GC, auditoria externa, deploy ou operação de dados.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-meta"
  manifest: "crates/corelink-meta/Cargo.toml"
  source-commit: "6be030999de1f0e0fe62d3a9abb04ec2a4fefde6"
  evidence-set: "meta-static-graph-20260920"
---

# Ownership — corelink-meta

Candidata limitada à fonte na baseline. SOURCE declara tipos, SQL e interfaces;
RUNTIME exige adapter, entrypoint e evidência operacional separados.

[Escopo](#s01) · [Triagem](#s02) · [Esquema](#s03) · [Refcount](#s04) ·
[Outbox](#s05) · [Parada](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Escopo

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Alterar `schema`, `types`, `store`, `cas_query`, `fake` ou exports | Tratar como contrato desta crate | [R03](../../../docs/ownership/crates/corelink-meta/REFERENCE.md#r03) | Se requer binding ou execução D1 |
| Alterar worker, GC ou dreno | Encaminhar ao owner da integração | [R08](../../../docs/ownership/crates/corelink-meta/REFERENCE.md#r08) | Não assumir runtime local |

<a id="s02"></a>
## S02 — Triagem do contrato

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Chave, coluna ou SQL muda | Traçar schema, tipo e três arestas estrangeiras | [B01](../../../docs/ownership/crates/corelink-meta/BLAST_RADIUS.md#b01) | Owner/schema de `gc_purge_intent`, `tenant` ou `region` não registrado |
| Método `MetaStore` muda | Mapear implementador e callers estáticos | [B06](../../../docs/ownership/crates/corelink-meta/BLAST_RADIUS.md#b06) | Grafo reverso incompleto |

<a id="s03"></a>
## S03 — Esquema e identidade

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| PK, `size_bytes` ou digest muda | Preservar forma canônica e constraints declaradas | [R04](../../../docs/ownership/crates/corelink-meta/REFERENCE.md#r04) | Dados existentes ou migração desconhecidos |
| `MIGRATION_SQL` ou template muda | Comparar bytes, caminho, vetor e rotas estrangeiras | [M02](../../../docs/ownership/crates/corelink-meta/MAINTENANCE.md#m02) | Aplicação ou owner/schema externo não demonstrados |

<a id="s04"></a>
## S04 — Refcount e tombstone

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Incremento, decremento ou `get` muda | Preservar guards de tombstone e `gc_purge_intent` | [R05](../../../docs/ownership/crates/corelink-meta/REFERENCE.md#r05) | Concorrência ou schema GC não evidenciados |
| Soft delete muda | Preservar `deleted_at` sticky e sem ressurreição | [B03](../../../docs/ownership/crates/corelink-meta/BLAST_RADIUS.md#b03) | Política GC/grace não local |

<a id="s05"></a>
## S05 — Audit outbox

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Audit request, tipo ou payload muda | Verificar dedupe e rota `tenant.primary_region` → `audit_outbox.region` | [R06](../../../docs/ownership/crates/corelink-meta/REFERENCE.md#r06) | Owner/schema externo, entrega ou chain requerido |
| Commit mutante muda | Manter par metadata+outbox no contrato | [B04](../../../docs/ownership/crates/corelink-meta/BLAST_RADIUS.md#b04) | Atomicidade real sem adapter |

<a id="s06"></a>
## S06 — Paradas obrigatórias

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Baseline, manifesto ou fonte divergem | Reconciliar SHA e recorte | [M01](../../../docs/ownership/crates/corelink-meta/MAINTENANCE.md#m01) | Não editar nesta edição |
| Solicitação afirma D1, worker ou auditoria funcionando | Exigir evidência de integração/runtime | [R08](../../../docs/ownership/crates/corelink-meta/REFERENCE.md#r08) | Não converter SOURCE em RUNTIME |

<a id="s07"></a>
## S07 — Saída mínima

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Mudança concluída | Registrar SHA, símbolos, relações, comando e resultado | [M06](../../../docs/ownership/crates/corelink-meta/MAINTENANCE.md#m06) | Checker não equivale a aprovação |
| Lacuna permanece | Declarar desconhecido e decisão necessária | [R08](../../../docs/ownership/crates/corelink-meta/REFERENCE.md#r08) | Ausência não é garantia |

[Início](#s01)
