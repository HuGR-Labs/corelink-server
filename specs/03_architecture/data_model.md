---
id: "DATA-MODEL"
type: "data_model"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-04-23"
updated: "2026-04-29"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["architecture", "data-model", "schema", "cas", "ac", "billing"]
---

# Data Model — Esquema Lógico e Físico

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-23
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) do modelo de dados do CoreLink — **entidades**, **chaves**, **tabelas**, **estados**, **invariantes de integridade**. Complementa `storage_semantics_matrix.md` (comportamento por backend) e `remote_cache_product_profile.md` (semântica do produto). Consumido por:
> - Todo WI que introduz tabela, índice ou schema de storage **DEVE** estender este documento + ADR.
> - `auth_model.md`, `privacy_model.md`, `observability_model.md` referenciam entidades aqui.
>
> Regra: mudanças em entidades core requerem ADR + migration plan + data integrity test antes do merge.

> **🚦 Phase boundary (F-12 audit Lote 3+4):**
> CoreLink GA inicial cobre **Fase 1 — Remote Cache** (CAS + AC + GC + REAPI cache-only).
> **Fase 2 — Remote Execution** (`execute-action`, executor identity, sandbox runtime) é **futuro** (roadmap pós-GA).
> Seções/CTRLs/SLOs/labels marcados com `(Fase 2)` ou `execute-action` referem-se a planejamento; em GA inicial podem ser omitidos do scope mínimo.



---

## Sumário

1. [Entidades canônicas](#1-entidades-canônicas)
2. [Keys, identifiers e naming](#2-keys-identifiers-e-naming)
3. [Layout físico por backend](#3-layout-físico-por-backend)
4. [Esquemas relacionais (D1 + Neon)](#4-esquemas-relacionais-d1--neon)
5. [Esquemas de object storage (R2)](#5-esquemas-de-object-storage-r2)
6. [Esquemas de KV e DO](#6-esquemas-de-kv-e-do)
7. [Invariantes de integridade](#7-invariantes-de-integridade)
8. [Migration strategy](#8-migration-strategy)
9. [Ciclo de vida do dado](#9-ciclo-de-vida-do-dado)
10. [Cardinalidade esperada (capacity planning)](#10-cardinalidade-esperada-capacity-planning)
11. [Referências](#11-referências)

---

## 1. Entidades canônicas

| Entidade             | Descrição                                                | Fonte de verdade      | Chave primária                        |
|----------------------|----------------------------------------------------------|------------------------|----------------------------------------|
| `Account`            | Conta master HuGR                                         | Neon                   | `account_id` (UUIDv7)                  |
| `Tenant`             | Unidade de isolamento; pertence a `Account`              | Neon                   | `tenant_id` (UUIDv7)                   |
| `User`               | Pessoa física com acesso                                  | Neon                   | `user_id` (UUIDv7)                     |
| `Membership`         | `User × Tenant` + role                                    | Neon                   | composite                               |
| `PAT`                | Personal Access Token (hashed)                            | Neon (hash) + D1 (quick verify) | `pat_id` (UUIDv7) + `token_hash` |
| `Plan`               | free / solo / team / business / enterprise (5 tiers canonical; Lote 10.7bis P0-7 fix — was 3 tiers; align com slo_catalog.md §3.1 + WI-S07-002 ttl_for_tier 5-arm match) | Neon                   | `plan_id`                               |
| `Subscription`       | `Tenant × Plan × billing_period`                          | Neon                   | `subscription_id`                       |
| `Region`             | Código da região (enum)                                    | static / config         | `region_code`                           |
| `Blob`               | Binário CAS; conteúdo indexado por hash                   | R2 `cas-<region>`      | `digest` (`algo:hex`)                  |
| `BlobMeta`           | Metadata leve do blob (size, refcount, created_at)         | D1 `blob_meta`         | `digest` + `tenant_id`                 |
| `AC-Entry`           | Action Cache entry (result digest → outputs)              | R2 `ac-<region>`       | `action_digest`                         |
| `AC-Meta`            | Metadata AC (created, last_seen, expires)                  | D1 `ac_meta`            | composite                               |
| `UsageEvent`         | Evento de uso (read/write/exec)                            | R2 append + Kafka fan-out | `event_id` (ULID)                    |
| `UsageCounter`       | Agregado cronológico por tenant                            | D1 `usage_counter`      | `tenant_id + period`                    |
| `AuditEvent`         | Audit trail entry                                           | R2 `audit-<region>`    | `event_id` (ULID)                       |
| `DSRTicket`          | Data Subject Request ticket                                 | Neon `dsr_tickets`     | `ticket_id` (UUIDv7)                    |
| `Waiver`             | Waiver de política (spec)                                   | Repo `_waivers/`       | `waiver_id`                             |
| `ConfigFlag`         | Feature flag / policy tunable                               | DO `config-singleton`  | `flag_key`                              |
| `RateBucket`         | Token bucket state                                           | DO `rate-<tenant>`     | `tenant_id`                             |

---

## 2. Keys, identifiers e naming

### 2.1 ID schemes

| Caso                                 | Scheme      | Exemplo                             |
|--------------------------------------|-------------|-------------------------------------|
| Internal entity (account, tenant…)  | UUIDv7       | `01938af0-abcd-7123-8456-..........` |
| Event (usage/audit)                  | ULID         | `01HKE3Z9ABCDEF01234567890`         |
| PAT                                  | `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` (canonical hybrid HMAC + Argon2id per auth_model.md §2.3 + S-03 cycle 9 SEAL decision (a)) | `corelink_pat_abc12345.x9k....abcDEF12345` |
| Blob digest                          | `algo:hex`   | `blake3:a1b2c3d4...`                |
| Region                               | enum code    | `wnam`, `weur`, `sam`, ...          |
| Request-id                           | ULID          | propagado em logs/traces            |

> **Por que UUIDv7:** time-ordered, evita hotspot em index B-tree, é opaque (não exposição de ordinal), matches LGPD pseudonimização.

### 2.2 Naming conventions

- **Tabelas:** `snake_case` singular (`blob_meta`, não `blob_metas`).
- **Colunas:** `snake_case`; timestamps terminam em `_at` (UTC); booleans em `is_` ou `has_`.
- **Index:** prefix `idx_<table>_<cols>`.
- **FK:** `fk_<src>_<ref>`.
- **R2 keys:** `tenant/<tenant_id>/<kind>/<subpath>`, onde `<tenant_id>` é **prefixado com HMAC** (CTRL-AUTH-004) para isolamento.

### 2.3 Proibido

- IDs sequenciais inteiros em API externa.
- Nome comercial do tenant em qualquer chave (PII/IP risk).
- Path com caracteres especiais não canonicalizados.

---

## 3. Layout físico por backend

Resumo (detalhe por entidade em §4–§6):

| Backend       | Escopo                                  | Regionalidade           |
|---------------|------------------------------------------|--------------------------|
| Neon Postgres | Control plane (accounts, billing, users) | Cluster US + EU read replicas |
| D1 (SQLite)   | Operational metadata por região          | 1 D1 por região         |
| R2            | Blobs, AC, audit                          | 1 bucket por kind × região |
| KV            | Short-lived caches (nonces, pre-signed)   | Global eventual          |
| DO            | Singletons (rate, quota, config)          | 1 DO per-tenant/per-flag |

Motivo da divisão: `storage_semantics_matrix.md §2` documenta os trade-offs.

---

## 4. Esquemas relacionais (D1 + Neon)

### 4.1 Neon (control plane) — DDL abreviada

```sql
-- accounts & users
CREATE TABLE account (
  account_id       UUID        PRIMARY KEY,
  legal_name        TEXT        NOT NULL,
  billing_email     TEXT        NOT NULL,
  status            TEXT        NOT NULL CHECK (status IN ('active','suspended','closed')),
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE tenant (
  tenant_id         UUID        PRIMARY KEY,
  account_id        UUID        NOT NULL REFERENCES account(account_id),
  display_name       TEXT        NOT NULL,
  primary_region    TEXT        NOT NULL CHECK (primary_region IN ('wnam','enam','weur','sam','apac','afr')),
  locale_default    TEXT        NOT NULL DEFAULT 'en-US' CHECK (locale_default IN ('pt-BR','en-US','es-MX')),
  plan_id           TEXT        NOT NULL REFERENCES plan(plan_id),
  status            TEXT        NOT NULL CHECK (status IN ('provisioning','active','suspended','deleting')),
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  deleted_at        TIMESTAMPTZ NULL
);
-- primary_region: 6-region canonical (privacy_model.md §7.1 L313-320). CHECK enforced PG-side.
-- locale_default: 3-locale canonical (WI-S11-004 privacy notice). pt-BR primary, en-US fallback, es-MX LATAM.

CREATE TABLE user_account (
  user_id           UUID        PRIMARY KEY,
  email             TEXT        NOT NULL UNIQUE,
  display_name      TEXT        NULL,
  webauthn_pubkey   BYTEA       NULL,
  totp_secret_enc   BYTEA       NULL,
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  deleted_at        TIMESTAMPTZ NULL
);

CREATE TABLE membership (
  tenant_id         UUID        REFERENCES tenant(tenant_id),
  user_id           UUID        REFERENCES user_account(user_id),
  role              TEXT        NOT NULL CHECK (role IN ('owner','admin','developer','viewer')),
  created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, user_id)
);

CREATE TABLE pat (
  pat_id            UUID        PRIMARY KEY,
  tenant_id         UUID        NOT NULL REFERENCES tenant(tenant_id),
  issued_to_user    UUID        NULL REFERENCES user_account(user_id),
  kind              TEXT        NOT NULL CHECK (kind IN ('user','ci','readonly','executor','service')),
  token_id          TEXT        NOT NULL UNIQUE,           -- 16-char deterministic indexed lookup key (cycle 9 SEAL decision (a) hybrid; per auth_model.md §2.3)
  token_hash        BYTEA       NOT NULL UNIQUE,           -- Argon2id PHC string serialized as BYTEA
  signing_key_id    INTEGER     NOT NULL DEFAULT 1,        -- pat_signing_key version for HMAC sig validation (multi-key rotation per key_management.md §3.2)
  scopes            TEXT[]      NOT NULL,
  expires_at        TIMESTAMPTZ NULL,
  last_used_at       TIMESTAMPTZ NULL,
  revoked_at         TIMESTAMPTZ NULL,
  created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_pat_tenant ON pat(tenant_id) WHERE revoked_at IS NULL;

-- billing
CREATE TABLE plan (
  plan_id           TEXT        PRIMARY KEY,
  name              TEXT        NOT NULL,
  monthly_base_usd_cents INT     NOT NULL,
  included_storage_gb  INT       NOT NULL,
  overage_per_gb_cents INT       NOT NULL,
  -- ...
  created_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE subscription (
  subscription_id   UUID        PRIMARY KEY,
  tenant_id         UUID        NOT NULL REFERENCES tenant(tenant_id),
  plan_id           TEXT        NOT NULL REFERENCES plan(plan_id),
  started_at        TIMESTAMPTZ NOT NULL,
  ends_at           TIMESTAMPTZ NULL,
  status            TEXT        NOT NULL CHECK (status IN ('active','past_due','canceled'))
);

-- DSR (canonical storage = Neon; NOT D1. Lote 10.11.0-bis decision.)
-- ticket_id = UUIDv7 (RFC 9562 §5.7) — embeds Unix-ms timestamp for sort-by-arrival without separate index.
-- 7-state canonical machine (Lote 10.11.0-bis): granularity needed for SLO-FRESH-DSR-ERASURE §4.12 + WI-S11-001..002.
--   received    — DSR submitted, awaiting subject verification (MFA step-up CTRL-AUTH-010)
--   verified    — Subject identity confirmed via JWT receipt + WebAuthn/TOTP step-up
--   queued      — Routed to erasure pipeline, awaiting worker slot (PAT-RETRY-IDEMPOTENT-001)
--   in_progress — Worker actively executing across 12 backends (8 effective + 4 pseudonymized)
--   completed   — All backends ack'd within 24h SLO; receipt sealed in audit chain
--   denied      — Refused with documented legal ground (legal_hold, fraud_check, admin_override)
--   failed      — System error after retry exhaustion; 24h SLO breach; on-call paged
CREATE TABLE dsr_tickets (
  ticket_id         UUID        PRIMARY KEY,                -- UUIDv7
  tenant_id         UUID        NOT NULL REFERENCES tenant(tenant_id),
  subject_user_id   UUID        NULL REFERENCES user_account(user_id),
  request_kind      TEXT        NOT NULL CHECK (request_kind IN ('access','correction','erasure','portability','objection','consent_revoke')),
  status            TEXT        NOT NULL CHECK (status IN ('received','verified','queued','in_progress','completed','denied','failed')),
  received_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
  verified_at       TIMESTAMPTZ NULL,
  completed_at      TIMESTAMPTZ NULL,
  failed_at         TIMESTAMPTZ NULL,
  denial_reason     TEXT        NULL CHECK (denial_reason IN ('legal_hold','fraud_check','admin_override','jurisdiction_mismatch','duplicate_request')),
  failure_reason    TEXT        NULL,                       -- free-text post-mortem ref
  legal_hold        BOOLEAN     NOT NULL DEFAULT false,
  receipt_jws       TEXT        NULL,                       -- JWS compact serialization (CTRL-PRIV-DSR-RECEIPT)
  payload           JSONB       NULL,
  -- terminal-state invariants (split em CHECKs separados — Lote 10.11.0-bis fix:
  -- comma-separated dentro de single CHECK é invalid SQL syntax)
  CHECK ((status <> 'denied')    OR (denial_reason  IS NOT NULL)),
  CHECK ((status <> 'failed')    OR (failure_reason IS NOT NULL)),
  CHECK ((status <> 'completed') OR (receipt_jws    IS NOT NULL))
);
CREATE INDEX idx_dsr_tickets_tenant_status ON dsr_tickets(tenant_id, status) WHERE status NOT IN ('completed','denied','failed');
CREATE INDEX idx_dsr_tickets_subject ON dsr_tickets(subject_user_id) WHERE subject_user_id IS NOT NULL;
```

### 4.2 D1 (operational, por região) — DDL abreviada

```sql
-- Blob metadata (S-01 WI-S01-004 SEALED — canonical implementation file
-- migrations/d1/0001_blob_meta.sql; column types + CHECK constraints
-- enforced server-side).
CREATE TABLE IF NOT EXISTS blob_meta (
  tenant_id         TEXT        NOT NULL,                                 -- canonical UUIDv7 text (§2.1 L91)
  digest            TEXT        NOT NULL,                                 -- canonical 'algo:hex' (§1 L71)
  size_bytes        INTEGER     NOT NULL CHECK (size_bytes > 0),
  refcount          INTEGER     NOT NULL DEFAULT 1 CHECK (refcount >= 0), -- first write yields 1 (S-01 sprint §1.4)
  compression       TEXT        NULL,                                     -- 'zstd' | NULL (S-05 multipart)
  created_at        INTEGER     NOT NULL,                                 -- unix epoch ms
  last_accessed_at  INTEGER     NOT NULL,                                 -- unix epoch ms
  deleted_at        INTEGER     NULL,                                     -- soft-delete grace (S-06)
  PRIMARY KEY (tenant_id, digest)
);

-- Partial index: alive set (AuthZ checks; S-02 read path).
CREATE INDEX IF NOT EXISTS idx_blob_meta_tenant_alive
  ON blob_meta(tenant_id, deleted_at)
  WHERE deleted_at IS NULL;
-- Partial index: GC sweep candidates (S-06).
CREATE INDEX IF NOT EXISTS idx_blob_meta_gc_candidates
  ON blob_meta(deleted_at)
  WHERE deleted_at IS NOT NULL;

-- Audit outbox (Outbox Pattern, ADR-0027). INSERTed in the same
-- `db.batch([...])` as every blob_meta mutation so the pair is atomic
-- (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). Drained to the S-09 audit chain
-- by a separate worker.
CREATE TABLE IF NOT EXISTS audit_outbox (
  id            TEXT        PRIMARY KEY,                  -- UUIDv7 text
  tenant_id     TEXT        NOT NULL,                     -- canonical UUIDv7 text
  digest        TEXT        NULL,                         -- nullable for non-blob events
  request_id    TEXT        NOT NULL,                     -- client idempotency key
  event_type    TEXT        NOT NULL,                     -- e.g. 'corelink.cas.put_completed'
  payload_json  TEXT        NOT NULL,                     -- CloudEvents 1.0 envelope
  enqueued_at   INTEGER     NOT NULL,                     -- unix epoch ms
  emitted_at    INTEGER     NULL,                         -- NULL until drained to S-09 chain
  UNIQUE (request_id, event_type)                          -- idempotent retry dedupe
);
-- Partial index: pending drain queue (S-09 worker).
CREATE INDEX IF NOT EXISTS idx_audit_outbox_pending
  ON audit_outbox(enqueued_at)
  WHERE emitted_at IS NULL;

-- Action cache
CREATE TABLE ac_meta (
  tenant_id         TEXT        NOT NULL,
  action_digest     TEXT        NOT NULL,
  result_hash       TEXT        NOT NULL,
  blob_refs         TEXT        NOT NULL,  -- JSON array of digests
  created_at        INTEGER     NOT NULL,           -- unix ms
  last_hit_at       INTEGER     NOT NULL,           -- unix ms
  expires_at        INTEGER     NULL,               -- unix ms TTL
  deleted_at        INTEGER     NULL,               -- soft-delete grace 24h (S-06 GC; Lote 10.6 cycle 1 canonical)
  PRIMARY KEY (tenant_id, action_digest)
);
CREATE INDEX idx_ac_meta_tenant_deleted ON ac_meta(tenant_id, deleted_at) WHERE deleted_at IS NULL;

-- usage counters (agregados incremental)
CREATE TABLE usage_counter (
  tenant_id         TEXT        NOT NULL,
  period            TEXT        NOT NULL,   -- '2026-04'
  metric            TEXT        NOT NULL,   -- 'cas_get_bytes', 'exec_minutes', ...
  value             INTEGER     NOT NULL DEFAULT 0,
  updated_at        INTEGER     NOT NULL,
  PRIMARY KEY (tenant_id, period, metric)
);

-- tenant quota (POLICY: config; immutable per period)
CREATE TABLE tenant_quota (
  tenant_id         TEXT        PRIMARY KEY,
  max_storage_bytes INTEGER     NOT NULL,
  max_rps           INTEGER     NOT NULL,
  max_concurrent_exec INTEGER   NOT NULL,
  updated_at        INTEGER     NOT NULL
);

-- tenant storage state (STATE: mutable telemetry; running counter; Lote 10.7bis P0-2 NEW)
-- Separates "policy" (max_storage_bytes em tenant_quota) from "state" (bytes_used here).
-- DO singleton `quota-<tenant_id>` em S-07 WI-S07-003 reservation pattern reads/writes here every 5min.
CREATE TABLE tenant_storage_state (
  tenant_id           TEXT        PRIMARY KEY,
  bytes_used          INTEGER     NOT NULL DEFAULT 0,           -- monotonic except on eviction (decrements)
  bytes_used_updated_at  INTEGER  NOT NULL,                      -- unix ms
  last_synced_at      INTEGER     NOT NULL,                      -- unix ms; DO last sync to D1
  CHECK (bytes_used >= 0)
);
```

### 4.3 Razão de D1 vs Neon

- **D1** (região-local, SQLite-based): baixo latency reads do hot path; escrita primary com consistency Sessions.
- **Neon** (global, Postgres): rich queries, transactions ACID, integração billing (Stripe), fonte de verdade para accounts.
- **Não duplicar** tenant basics entre D1 e Neon além do essencial (`tenant_id`, `primary_region`, `status`); mudanças propagam via event (usage events / admin events).

---

## 5. Esquemas de object storage (R2)

### 5.1 CAS layout

```
cas-<region>/
  <hmac(tenant_key, tenant_id)[:16]>/
    <algo>/
      <hex[0:2]>/
        <hex[2:4]>/
          <hex>        ← body (binary)
          <hex>.meta   ← sidecar (JSON) — OPCIONAL; source of truth é D1
```

Exemplo:
```
cas-sam/a3b9c8d7e2f14756/blake3/a1/b2/a1b2c3d4e5f6...
```

### 5.2 AC layout

```
ac-<region>/
  <hmac(tenant_key, tenant_id)[:16]>/
    <action_digest_hex>.json     ← envelope com result + blob refs
```

### 5.3 Audit layout

```
audit-<region>/
  date=<YYYY-MM-DD>/
    <hour>/
      part-<ulid>.ndjson.gz       ← append-only, Object Lock Governance
```

### 5.4 Políticas

- **Lifecycle rule**: multipart incomplete > 7d → abort (FM-060).
- **Object Lock**: audit bucket com Governance Mode 7y; emergency erasure requer legal hold process.
- **CORS**: apenas `origin = api.corelink.dev` e `origin = <tenant-domain>` allowlisted para presigned URL uploads.

---

## 6. Esquemas de KV e DO

### 6.1 KV — caches curtos

| Prefix                | Valor                      | TTL default | Propósito                          |
|-----------------------|----------------------------|-------------|-------------------------------------|
| `presigned:put:<hash>` | signed URL                 | 15 min      | Evitar regerar pre-sign            |
| `presigned:get:<hash>` | signed URL                 | 15 min      | Idem                                |
| `nonce:<tenant>:<id>`  | timestamp                  | 120s        | Replay protection (CTRL-AUTH-007)   |
| `pat_valid:<hash>`     | minimal claims JSON         | 60s         | Rápido verify path (defesa em profundidade vs DB miss) |
| `ac_neg:<tenant_prefix>:<digest>` | flag             | 60s         | Negative cache (tenant-scoped via Layer 4 prefix; TTL shorter than CAS — actions stale rápido pós-rebuild; per-tenant scoping previne cross-tenant negative-cache leak/poisoning; amended Lote 10.4bis per WI-S04-001 §9.3) |

**KV é global eventual** — jamais source-of-truth.

### 6.2 DO — singletons stateful

| DO class           | Instance ID          | State                              |
|--------------------|----------------------|-------------------------------------|
| `RateLimiter`      | `rate-<tenant_id>`   | `{bucket_capacity, refill_rate, last_refill, tokens}` |
| `TenantQuota`      | `quota-<tenant_id>`  | `{used_bytes_now, max_bytes, last_sample_at}` |
| `ConfigSingleton`  | `config-global`      | Policy flags, kill-switches         |
| `MultipartSession` | `multipart-<upload_id>` | Upload state (parts, etags)       |

---

## 7. Invariantes de integridade

| ID                       | Descrição                                                                          | Enforcement           |
|--------------------------|------------------------------------------------------------------------------------|-----------------------|
| INV-DATA-BLOB-HASH       | Body em R2 satisfaz `hash(body) == digest do path`                                 | CI scrub + client verify |
| INV-DATA-REFCOUNT        | `blob_meta.refcount ≥ 0` sempre; `= 0 ⇒ deletable`                                 | Check em transaction + GC |
| INV-DATA-BLOB-NO-ZOMBIE  | Não existe body em R2 sem row em `blob_meta` (exceto durante multipart in-flight) | Reconcile diário       |
| INV-DATA-AC-REFS-EXIST   | Todo `ac_meta.blob_refs[i]` tem `blob_meta` alive                                  | FK lógico + reconcile |
| INV-DATA-TENANT-ISOLATION | Todo row / object / DO id inclui `tenant_id`; cross-tenant access = assertion fail | Middleware + TLA+     |
| INV-DATA-AUDIT-CHAIN      | `audit.hash_chain[i] == H(audit[i-1].content + audit[i].content)`                  | Daily verify          |
| INV-DATA-BILLING-RECONCILE | `Σ(usage_events) ≈ usage_counter` (±0.1%)                                         | Reconcile diário       |
| INV-DATA-MONOTONIC-TS      | `last_accessed_at` monotonic non-decreasing                                       | Writer enforces       |
| INV-DATA-ERASURE-COMPLETE | DSR erasure resolved ⇒ nenhum backend retorna dado do subject (exceto legal hold) | Test E2E              |

CRITICAL invariants (em **bold**) requerem TLA+ model (CTRL-FORMAL-001):

- **INV-DATA-TENANT-ISOLATION**
- **INV-DATA-BLOB-NO-ZOMBIE** (refcount correctness — ligado a INV-GC-001)

---

## 8. Migration strategy

### 8.1 Princípios

1. **Online migrations only** — zero downtime (PAT-MIGRATION-IDEM-001).
2. **Expand-migrate-contract**:
   - Expand: adicionar nova coluna / tabela (compatível com código existente).
   - Migrate: backfill + dual-write + switch reads.
   - Contract: remover coluna/tabela antiga.
3. **Reversible in 15 min**: se expand quebra, rollback rápido.
4. **Versioned via sqlx migrate** com tracking table `schema_migration`.

### 8.2 Backfill

- Nunca bloqueante.
- Batches de 10k rows; jitter entre batches.
- Progress em `corelink_migration_progress_ratio{migration}` gauge.
- Abort automático se p99 latência do hot path subir > 2× baseline.

### 8.3 Compatibilidade de dado histórico

- Blobs escritos em v1 continuam legíveis em v2 (formato = `digest = algo:hex`).
- Mudança de algo primary (ex: BLAKE3 → BLAKE3-2) exige fallback: aceitar ambos em read por ≥ 12 meses.

---

## 9. Ciclo de vida do dado

```
 Blob:
   created  →  referenced  →  soft-deleted  →  swept
       (PUT)       (refcount>0)   (refcount==0, 72h)  (GC sweep)

 AC Entry:
   created  →  hit_updated  →  expired(TTL)  →  deleted

 User:
   active  →  deactivated  →  erased(post-DSR)
       ↑              ↑               ↑
     signup      self/admin        DSR request

 DSR (canonical 7-state, Lote 10.11.0-bis):
   received  →  verified  →  queued  →  in_progress  →  completed
       ↓           ↓           ↓            ↓
     denied      denied      failed       failed
       (legal_hold | fraud_check | admin_override | jurisdiction_mismatch | duplicate_request)
       (failure_reason populated; on-call paged on 24h SLO breach)
```

Notas:
- `denied` é terminal (legal grounds documentados em `denial_reason`); `failed` é terminal mas sinaliza retentativa manual via on-call.
- Transição `queued → in_progress` é idempotente (PAT-RETRY-IDEMPOTENT-001).
- Receipt JWS gerado apenas em `completed` (CTRL-PRIV-DSR-RECEIPT canonical).

Cada transição emite evento CloudEvents (§7 obs).

---

## 10. Cardinalidade esperada (capacity planning)

Projeção Fase 1 (primeiros 12 meses GA):

| Entidade          | Estimativa ano 1   | Growth rate | Notas                             |
|-------------------|--------------------|-------------|------------------------------------|
| Accounts          | 1k                 | linear       | Predominantemente `team`           |
| Tenants           | 3k (1 conta ~ 3)   | linear       | —                                   |
| Users             | 15k                | linear       | —                                   |
| PATs              | 60k                | linear       | 4× users em média                  |
| Blobs (ativos)    | 500M               | Heavy-tail  | p99 dos tenants < 100k blobs       |
| AC entries         | 50M                | Corr. blobs | —                                   |
| Storage bytes      | 2 PB                | Corr. blobs | dedup médio 4×                     |
| Usage events       | 50B/ano             | Linear      | — Gravados append; agregados em D1 |
| Audit events       | 5B/ano              | Linear      | Append; Object Lock 7y             |

Essas estimativas informam:
- D1 shardagem por região (cada região ≤ 10 GB).
- R2 bucket count (audit separate from CAS separate from AC).
- Neon sizing (< 50 GB em ano 1, 1 instância primary + 2 read replicas).

---

## 11. Referências

### 11.1 Specs CoreLink relacionadas

- `storage_semantics_matrix.md` — comportamento por backend.
- `remote_cache_product_profile.md` — semântica CAS/AC.
- `auth_model.md` — entidades de identidade.
- `privacy_model.md` — categorias de dado.
- `observability_model.md` — schema de eventos/logs.
- `failure_modes.md` — FMs associados a data integrity.
- `resilience_patterns.md` — patterns de mitigation.

### 11.2 Externas

- **PostgreSQL docs** — `Neon` baseline.
- **SQLite D1** docs (Cloudflare).
- **R2 S3-compatible API** ref.
- **REAPI v2** — https://github.com/bazelbuild/remote-apis
- **Martin Kleppmann** — *Designing Data-Intensive Applications* (invariantes, migration patterns).
- **Uber** — blog "Datastore consistency at scale" (expand-contract).

---

## 12. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-23 | Gustavo Schneiter | Criação inicial do data model. |
| 0.2.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) — WI-S01-004 SEAL Lote | **§4.2 D1 schema canonical alignment** com a implementação SEALED em `crates/corelink-meta/`. (a) `blob_meta` DDL ganha `IF NOT EXISTS`, CHECK constraints (`size_bytes > 0`, `refcount >= 0`), partial indexes renomeados de `idx_blob_meta_deleted` para o par canonical `idx_blob_meta_tenant_alive` (alive set, AuthZ checks S-02) + `idx_blob_meta_gc_candidates` (GC sweep S-06), nomes que matcham WI-S01-004 §1 + SQL on-disk `migrations/d1/0001_blob_meta.sql`. (b) **`audit_outbox` DDL adicionado** — tabela referenciada por WI-S01-004 §1 + INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER + 7 outras spec docs (S-03 revocation, S-04 ttl-worker, S-05 reapi handler, S-09 chain), mas ausente do data_model.md original; canonical ddl agora está aqui (single source of truth). (c) Comentários inline citam canonical sources de cada coluna (UUIDv7 text §2.1; algo:hex §1; first-write refcount=1 sprint S-01 §1.4; ms timestamps §4.2). (d) Patches feitos no mesmo Lote da SEAL ceremony para evitar drift entre WI text + canonical ref + impl. |

---

**Fim de DATA-MODEL.** Alterações em entidades canônicas (§1) ou invariantes (§7) exigem ADR + re-check TLA+ correspondente + aprovação Architect + Data Engineer + SRE.
