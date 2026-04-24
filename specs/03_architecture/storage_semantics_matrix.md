---
id: "STORAGE-SEMANTICS-MATRIX"
type: "storage_semantics_matrix"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "storage", "cloudflare", "semantics"]
---

# Storage Semantics Matrix — Cloudflare Stack do CoreLink

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** *(a definir)*
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) de **semântica operacional** de cada backend da stack Cloudflare usada pelo CoreLink. Endereça audit v1 Gap 1: "falta matriz de semântica por backend cobrindo consistência, atomicidade, latência, visibilidade de writes, persistência local, placement e efeito de cache pra R2, KV, D1, Durable Objects e Containers."
>
> Este documento é referência normativa: ADRs/WIs/PRRs **DEVEM** citar qual backend sustenta cada invariant relevante e `inherits_from: ["STORAGE-SEMANTICS-MATRIX"]` no YAML quando impacta operação de storage.
>
> Toda nova CAP que toca storage **DEVE** passar por este documento pra classificar corretamente suas garantias.

---

## Sumário

1. [Backends cobertos](#1-backends-cobertos)
2. [Matriz mestre por backend](#2-matriz-mestre-por-backend)
3. [Operações CoreLink × backend](#3-operações-corelink--backend)
4. [Modelos de consistência explicados](#4-modelos-de-consistência-explicados)
5. [Armadilhas críticas (foot guns)](#5-armadilhas-críticas-foot-guns)
6. [Invariants dependentes de storage semantics](#6-invariants-dependentes-de-storage-semantics)
7. [Recovery e rebuild semântica](#7-recovery-e-rebuild-semântica)
8. [Referências oficiais](#8-referências-oficiais)
9. [Change Log](#9-change-log)

---

## 1. Backends cobertos

Stack de storage do CoreLink (decidida em Lote 2 / v0.4.0, ver ADRs futuros):

| Backend | Papel no CoreLink | Cloudflare doc |
|---|---|---|
| **R2** | CAS blobs (content-addressable objects); AC result blobs | [R2 docs](https://developers.cloudflare.com/r2/) |
| **KV** | Hot metadata cache (digest→metadata), rate-limit counters | [KV docs](https://developers.cloudflare.com/kv/) |
| **D1** (opcional) ou **Neon** | Metadata relacional (tenants, AC index, CAS blob index, usage events) | [D1 docs](https://developers.cloudflare.com/d1/) |
| **Durable Objects** | Coordenação por tenant (rate limit, event bus, container orchestration) | [DO docs](https://developers.cloudflare.com/durable-objects/) |
| **Containers** | gRPC REAPI server (Rust) — data plane | [Containers docs](https://developers.cloudflare.com/containers/) |
| **Workers** | Control plane (dashboard API, auth callbacks, billing webhooks, package mirror HTTP) | [Workers docs](https://developers.cloudflare.com/workers/) |

---

## 2. Matriz mestre por backend

> **Leitura:** cada linha descreve uma propriedade operacional; cada coluna é um backend. Valores **"forte"**, **"eventual"**, **"N/A"**, **"configurável"** têm semântica rígida definida em §4.

| Propriedade | **R2** | **KV** | **D1 / Neon** | **Durable Objects** | **Containers** | **Workers** |
|---|---|---|---|---|---|---|
| Consistência de leitura | Fortemente consistente | Eventual (≤ 60s globalmente típico) | Sequencial via Sessions API (D1) / Serializable (Neon) | Strong dentro do DO (single-threaded); forte entre leituras do mesmo DO | Strong dentro do container; efêmero | Strong em KV bindings; eventual em cache de Worker |
| Atomicidade | Per-object put/get; **sem** multi-object transações | Per-key put; **sem** multi-key atômico | Transactional (ACID dentro da conexão); distribuído depende | Actor-model: operações dentro do DO são serializadas | Runtime-local | N/A (stateless) |
| Visibilidade pós-write | Imediata na região; **cache HTTP pode relaxar** até 60s se CDN cache ativo | 60s+ globalmente; **não use pra operações atômicas** | Primary immediato; réplicas assíncronas | Imediata no próprio DO; outros DOs via message passing | Imediata local; efêmero | N/A |
| Persistência | Persistente durável (11 9s) | Persistente (3 réplicas) | Persistente | Persistente (via DO storage API) | **Efêmero** — disco perdido após sleep/restart/relocação | Stateless |
| Placement | Multi-region (configurável) | Global edge, replicado | D1: single primary + read replicas; Neon: single region | Stickiness por ID — DO vive numa região por ID | **Pode relocar** após sleep; sem garantia de mesma região após restart | Edge global |
| Invalidation model | Tag-based + purge by URL; direto-modifiy object = overwrite | TTL configurável; sem pub/sub invalidation built-in | Via migrations / DML | Manual via state update | N/A | Cache TTL configurable |
| Latência típica P99 | 50-200ms (GET) | 5-30ms (read hit edge) | D1: 10-200ms (primary vs replica); Neon: 20-50ms dentro de region | 10-50ms (DO lookup + op) | Runtime-local (< 1ms) | 5-20ms (edge) |
| Throughput | Altíssimo (escala auto) | Altíssimo (edge) | D1: limitado por primary; Neon: pool de conexões | Limitado por DO único (single-thread) | Limitado por container size | Altíssimo |
| IAM/ACL propagation | Eventual (seconds-minutes) | Eventual | Imediata (DML) | N/A | N/A | Imediata em Worker config |
| Tamanho máx por item | 5 TB (PUT multipart); 5 GB simple PUT | 25 MiB | D1 row: driver default; Neon row: practical unbounded | 128 KiB per value | Container-memory bound | Request body: 100 MiB (paid) |
| Zero-egress Cloudflare↔CoreLink | ✅ (R2 zero egress para Workers/Containers Cloudflare) | ✅ | D1: ✅; Neon: **egress billing aplicável** | ✅ | ✅ entre CF | ✅ |

---

## 3. Operações CoreLink × backend

Cada operação REAPI / package mirror tem backend específico. Semântica aplicada:

### 3.1 CAS: `FindMissingBlobs`

| Passo | Backend | Semântica |
|---|---|---|
| Lookup metadata | KV (hot) → D1 (fallback) | KV pode retornar stale até 60s; D1 fonte da verdade |
| Check existência blob | R2 `HEAD` | Strong consistency |
| Emit metric | Workers Analytics + Grafana | Eventual (batch aggregation) |

**Invariant preservado:** INV-CASIdempotency.
**Armadilha:** KV eventual pode retornar "missing" pra blob que já foi `PUT`. Fallback a R2 HEAD compensa. **DEVE** sempre fazer fallback.

### 3.2 CAS: `BatchUpdateBlobs` (upload)

| Passo | Backend | Semântica |
|---|---|---|
| Validate digests | Container memory | Strong, transient |
| PUT object | R2 | Strong write; visible imediatamente via GET |
| Update metadata index | D1 INSERT | Transactional |
| Cache-through em KV | KV PUT | Eventual; não bloqueante |
| Emit audit event | Analytics + D1 `usage_events` | Transactional pra D1; eventual pra aggregates |

**Armadilha:** se Container reinicia entre `R2 PUT` e `D1 INSERT`, blob existe em R2 mas **não** no index — inconsistência. **DEVE** implementar reconciliação periódica (garbage collector varre R2 vs D1).

### 3.3 CAS: `BatchReadBlobs` (download)

| Passo | Backend | Semântica |
|---|---|---|
| Lookup KV | KV | Eventual — se miss, fallback D1 |
| Fetch object | R2 GET (pode ter Range) | Strong |
| Update `last_accessed` | D1 UPDATE | Transactional; batch para performance |

**Armadilha:** `last_accessed` atualizado em toda leitura cria hot-spot em D1. **DEVE** batch via buffer em DO e flush periódico.

### 3.4 Action Cache: `GetActionResult`

| Passo | Backend | Semântica |
|---|---|---|
| Lookup action_digest | KV → D1 fallback | Eventual+Strong |
| Fetch result proto | R2 | Strong |
| Verify blob exists | R2 HEAD | Strong |

### 3.5 Action Cache: `UpdateActionResult`

| Passo | Backend | Semântica |
|---|---|---|
| Store result blob | R2 PUT | Strong |
| Insert AC entry | D1 INSERT | Transactional |
| Cache em KV | KV PUT | Eventual |

### 3.6 Rate Limiting

| Passo | Backend | Semântica |
|---|---|---|
| Lookup per-tenant counter | Durable Object | Strong dentro do DO |
| Increment + check threshold | DO atomic op | Serializável |
| Propagate violation | DO broadcast → Workers | Imediato no DO, eventual outros |

**Invariant:** cada tenant tem **um** DO; rate limit é exato por tenant.

### 3.7 Event Bus (real-time cache notifications, fase 2)

| Passo | Backend | Semântica |
|---|---|---|
| Subscribe | WebSocket → Durable Object | Strong dentro do DO |
| Publish event | DO broadcast | Fan-out imediato a conexões ativas |

### 3.8 Audit Log

| Passo | Backend | Semântica |
|---|---|---|
| Emit event (hot) | D1 INSERT `usage_events` / `audit_log` | Transactional, **imutável** (schema constraint, no UPDATE allowed) |
| Fanout (warm) | Logpush → R2 `audit-<region>/` | Append-only, near-realtime |
| Archive (cold) | R2 Object Lock Governance Mode | Hash-chained; **retention ≥ 7 anos (SOC 2)** — ver CTRL-AUDIT-005 |
| Purge de hot tier | Cron DO rotaciona D1 após 90 dias | Dados continuam disponíveis no archive |

**Invariant:** INV-AUDIT-APPEND-ONLY (canonical; `invariant_registry.md`). D1 schema **DEVE** rejeitar UPDATE/DELETE via CHECK constraint ou permissions; R2 **DEVE** ter Object Lock habilitado. Corrigido S-03/S-18 do audit Lote 3+4.

### 3.9 Tenant Isolation

Todo lookup/update **DEVE** scope-by `tenant_id`, com prefix **HMAC-derivado** (CTRL-AUTH-004) — nunca `tenant_id` em plaintext:

- R2 key canônico: `<HMAC(tenant_key, tenant_id)[:16]>/<digest_fn>/<hex[0:2]>/<hex[2:4]>/<hex>` (ver `data_model.md §5.1`)
- KV key: `meta/<HMAC(tenant_key, tenant_id)[:16]>/<digest>`
- D1 WHERE: `tenant_id = :tid` (coluna indexada; assertion dupla no Worker antes de atingir D1)
- DO ID: derivado de HMAC(tenant_key, tenant_id), não de `tenant_id` plaintext
- Container: não persiste per-tenant (stateless)

**Invariant:** INV-TenantIsolation (CRITICAL). Verificado via property test **E** TLA+ **obrigatório** (CTRL-FORMAL-001 em `security_model.md §6.9`; invariantes CRITICAL não têm escape de TLA+). Spec em `specs/tla/tenant_isolation.tla` (a criar). Corrigido S-02/F-03 do audit Lote 3+4.

---

## 4. Modelos de consistência explicados

### 4.1 Fortemente consistente (strong consistency)

Read-your-writes garantido, imediato. Após write bem-sucedido, próxima leitura vê o valor. Aplicável em: R2 (GET após PUT), DO (dentro do mesmo actor), D1 primary writes.

### 4.2 Sequencial via Sessions API (D1)

D1 com read replicas é eventual por padrão. **Sessions API** oferece "sequential consistency" — cliente vê writes na ordem, garantido só dentro da sessão:

```javascript
const session = env.DB.withSession(sessionToken);
await session.exec("INSERT ...");
await session.exec("SELECT ..."); // vê o INSERT acima
```

**Regra:** toda transação CoreLink que escreve + lê na mesma operação **DEVE** usar Sessions API ([ref](https://developers.cloudflare.com/d1/best-practices/read-replication/)).

### 4.3 Eventual consistency (KV)

Writes propagam em 60s+ globalmente. Leituras podem retornar valor antigo.

**Regra:** KV **NÃO É** fonte da verdade. É sempre cache. Fallback a D1 obrigatório em miss ou suspeita de stale.

### 4.4 Actor-model consistency (Durable Objects)

Cada DO é um ator com estado próprio, single-threaded. Operações dentro do DO são serializadas naturalmente. Mas DOs diferentes são independentes.

**Regra:** coordenação entre DOs requer message passing (fetch); não compartilham estado.

---

## 5. Armadilhas críticas (foot guns)

Listadas em ordem de severidade. **TODA ADR/WI tocando storage DEVE considerar estas.**

### 5.1 KV eventual → decisões erradas

**Sintoma:** lookup KV retorna "not found"; WI conclui "blob não existe" e duplica.

**Mitigação:** SEMPRE fallback a R2 HEAD ou D1 quando KV miss em operações mutativas.

### 5.2 R2 IAM propagation delay

**Sintoma:** nova chave de tenant criada; GET imediato falha com 403.

**Mitigação:** retry com backoff em operações que dependem de IAM recém-propagado.

### 5.3 Container efêmero = cache local perdido

**Sintoma:** container reinicia; LRU cache local zerado; latência aumenta.

**Mitigação:** LRU local é opcional (fallback a KV→R2 sempre funciona); warming automático.

### 5.4 Durable Object migration (relocation)

**Sintoma:** DO movido de região A pra B; conexões WebSocket perdem.

**Mitigação:** clients implementam reconnect com backoff.

### 5.5 D1 primary write latency em região remota

**Sintoma:** user em EU → D1 primary em US → write latency 100ms+.

**Mitigação:** D1 com primary próximo do padrão de escrita (atualmente US primary é default); ou migrar pra Neon com primary configurável.

### 5.6 Multi-object atomicity absent

**Sintoma:** `PUT R2` + `INSERT D1` não são atômicos; crash entre eles deixa órfão.

**Mitigação:** reconciliation periódica (GC): varre R2 vs D1, elimina R2 objects sem D1 index match; orphans com idade > 24h.

### 5.7 Cache HTTP público de R2

**Sintoma:** object público via R2 domain ganha cache CDN; update não visible imediato externamente.

**Mitigação:** CoreLink blobs são **internos** (accessed via Worker → R2 binding); cache CDN não aplica. Mas se expuser blob público direto, considerar `Cache-Control: no-cache`.

---

## 6. Invariants dependentes de storage semantics

Ligação cruzada com `invariants.md` (Nível 2, pendente — Lote futuro):

| Invariant (futuro) | Depende de | Storage responsável |
|---|---|---|
| INV-TenantIsolation | scope by `tenant_id` em todos acessos | R2 keys + KV keys + D1 WHERE + DO IDs |
| INV-CASIdempotency | content-addressable (digest = key) | R2 deduplica no nível de chave |
| INV-AuditLogImmutability | schema constraint + permissions | D1 audit_log table |
| INV-QuotaEnforcement | DO atomic per-tenant counter | Durable Object |
| INV-DataResidency | placement by tenant config | R2 multi-region + D1 primary + DO stickiness |

---

## 7. Recovery e rebuild semântica

### 7.1 Partial failures entre backends

| Cenário | Recovery |
|---|---|
| R2 PUT sucesso + D1 INSERT falha | Reconciliation GC detecta órfão em R2 após TTL (24h) |
| D1 INSERT sucesso + R2 PUT falha (network) | Request retryable pelo cliente; GC detecta D1 entries sem R2 blob |
| Container crash mid-batch | Batch marcado como `FAILED` em D1; client faz retry da batch inteira (idempotente) |
| DO migration mid-write | DO storage é persistente; write completa após re-hydration |

### 7.2 Disaster recovery

- **R2:** Cross-region replication configurável. RPO ≈ minutes.
- **D1:** Point-in-time recovery (14 dias). Neon similar com branching.
- **DO:** Storage persistente; surviving migration é automático.
- **KV:** Rebuildable from D1 (cache only).
- **Container:** Stateless; rebuild from image.

### 7.3 Rebuild do cache KV

Scenario: KV bucket corrompido/truncado.

Procedure:
1. Desligar reads via feature flag.
2. Drop KV entries.
3. Warm-up: iterate D1 `cas_blobs` + `action_cache`, populate KV em batch.
4. Re-enable reads.

Duração esperada: 1-4h para 10M entries.

---

## 8. Referências oficiais

- [R2 consistency model](https://developers.cloudflare.com/r2/reference/consistency/)
- [KV how it works](https://developers.cloudflare.com/kv/concepts/how-kv-works/)
- [D1 read replication](https://developers.cloudflare.com/d1/best-practices/read-replication/)
- [Containers architecture](https://developers.cloudflare.com/containers/platform-details/architecture/)
- [Durable Objects SQLite storage](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)
- [Workers limits](https://developers.cloudflare.com/workers/platform/limits/)

---

## 9. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | Versão inicial (Lote 4). Cobre R2, KV, D1/Neon, DO, Containers, Workers. Matriz mestre + operações CoreLink + armadilhas + invariants mapping. |

---

**Fim de STORAGE-SEMANTICS-MATRIX.**
