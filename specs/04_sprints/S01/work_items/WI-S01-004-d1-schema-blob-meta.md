---
id: "WI-S01-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-01"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s01", "cas", "d1", "schema", "blob-meta", "refcount"]
---

# WI-S01-004 — D1 `blob_meta` Schema + Migration + Refcount Transactional

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-01](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S01-004 |
| Título | D1 `blob_meta` schema + migration + refcount transacional |
| Sprint | S-01 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (tenant_id em todas as queries; bug → cross-tenant), FF-HR-005 (CTRL-ISO-002 AuthZ check on storage call depende de blob_meta lookup) |

## 1. Intent

Schema canônico D1 para `blob_meta` (per-region) com migration idempotente + refcount column transactional + UNIQUE constraint `(tenant_id, digest)` + tombstone semantics:

```sql
CREATE TABLE IF NOT EXISTS blob_meta (
    digest           TEXT NOT NULL,           -- canonical 'algo:hex' (e.g. 'blake3:a1b2c3...'; per data_model.md §1 L71 + §2.1 L94)
    tenant_id        TEXT NOT NULL,           -- UUIDv7 canonical text form (per data_model.md §4.2 L254 + §2.1 L91)
    refcount         INTEGER NOT NULL DEFAULT 1 CHECK (refcount >= 0),  -- canonical data_model.md §4.2 (S-06 GC mark depends on refcount > 0 reachability; first write yields refcount=1 per sprint.md §1.4)
    size_bytes       INTEGER NOT NULL CHECK (size_bytes > 0),
    created_at       INTEGER NOT NULL,        -- Unix epoch milliseconds (canonical data_model.md §4.2; SQLite INTEGER é 64-bit; safe ~ano 292B)
    last_accessed_at INTEGER NOT NULL,
    deleted_at       INTEGER,                 -- NULL = alive; non-NULL = tombstoned
    PRIMARY KEY (tenant_id, digest)
);

CREATE INDEX idx_blob_meta_tenant_alive
    ON blob_meta(tenant_id, deleted_at)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_blob_meta_gc_candidates
    ON blob_meta(deleted_at)
    WHERE deleted_at IS NOT NULL;

-- Audit outbox table (consumed por WI-S01-005 Outbox Pattern; vide ADR-0027)
CREATE TABLE IF NOT EXISTS audit_outbox (
    id           TEXT PRIMARY KEY,            -- UUIDv7 text form
    tenant_id    TEXT NOT NULL,
    digest       TEXT,                        -- nullable (não-blob events)
    request_id   TEXT NOT NULL,               -- client-provided idempotency key
    event_type   TEXT NOT NULL,               -- corelink.cas.put_completed | poisoning_attempt | ...
    payload_json TEXT NOT NULL,               -- CloudEvents 1.0 envelope
    enqueued_at  INTEGER NOT NULL,
    emitted_at   INTEGER,                     -- NULL = pending drain; non-NULL = sent to S-09 chain
    UNIQUE (request_id, event_type)           -- idempotent re-INSERT em retry
);

CREATE INDEX idx_audit_outbox_pending
    ON audit_outbox(enqueued_at)
    WHERE emitted_at IS NULL;
```

Migration `migrations/001_blob_meta.sql` aplica idempotently via `CREATE TABLE IF NOT EXISTS`. Insert path usa `INSERT OR IGNORE` (idempotent INSERT — INV-CAS-IMMUTABILITY).

## 2. Narrative (HIGH_RISK ≥ 300)

`blob_meta` é a single source of truth para metadata de blobs em CAS. Todo write check tenant ownership + integrity context aqui antes de R2 PutObject; todo read AuthZ check (CTRL-ISO-002 em S-02) faz `SELECT 1 FROM blob_meta WHERE digest=? AND tenant_id=? AND deleted_at IS NULL`.

Bugs catastróficos:
1. **Missing UNIQUE `(tenant_id, digest)`**: permite duplicate rows → race INSERT cria 2 rows mesmo digest mesmo tenant → refcount diverge → INV-GC-003 violation → potential data loss em GC sweep.
2. **Race em refcount UPDATE não-atomic**: dois UpdateActionResult concurrent referenciando mesmo digest → refcount += 1 dois twice em separate transactions → drift positivo (ok mas wasteful) OR `read-then-write` sem transação → drift negativo (catastrophic; GC deleta blob ainda referenced).
3. **Migration drift**: schema migrations applied diferente em diferentes regiões → S-14 cross-region inconsistency.
4. **Tombstone bypass**: query sem `WHERE deleted_at IS NULL` retorna soft-deleted blobs → INV-CAS-IMMUTABILITY violation (read after GC).

Mitigação:
1. **PRIMARY KEY (tenant_id, digest)**: D1 SQLite enforces; INSERT OR IGNORE → idempotent (segundo INSERT é silent no-op; INV-CAS-IMMUTABILITY enforced).
2. **Refcount via single-statement atomic UPDATE com RETURNING**: `UPDATE blob_meta SET refcount = refcount + 1, last_accessed_at = ?2 WHERE tenant_id=?1 AND digest=?3 RETURNING refcount` — D1 single-row update é atomic em SQLite engine.
   - **CRITICAL — D1 Worker binding constraints**: D1 não suporta multi-statement client-driven transactions over the Worker binding. `BEGIN/UPDATE/COMMIT` em separate `db.prepare()` calls executa em autocommit mode, perdendo ACID grouping (gotcha; será test-flaky em load). Multi-statement work usa `db.batch([stmt1, stmt2, ...])` que D1 executa como single transaction. Schema design favorece single-row UPDATE com RETURNING wherever possível.
   - **Exemplo correto** (Rust com `worker` crate): `let stmt = db.prepare("UPDATE blob_meta SET refcount=refcount+1 WHERE tenant_id=?1 AND digest=?2 RETURNING refcount").bind(&[tenant_id, digest])?; let row = stmt.first().await?;`
   - **Exemplo correto multi-statement** (vide WI-S01-005 outbox pattern): `db.batch([stmt_insert_blob_meta, stmt_insert_audit_outbox]).await?;` — atomic.
   - **CHECK constraint** `CHECK (refcount >= 0)` mandatory; previne bug em decrement_refcount onde silently goes negative; violation retorna error em `db.batch`.
3. **Migration framework**: `migrations/<NNN>_<name>.sql` com `CREATE IF NOT EXISTS`; CI validate per region D1 schema sync.
4. **Index design**: `idx_blob_meta_tenant_alive` is partial index `WHERE deleted_at IS NULL` → AuthZ checks são O(log n) only sobre alive blobs; tombstoned não bloat index.
5. **GC index** `idx_blob_meta_gc_candidates` partial sobre `deleted_at IS NOT NULL` → S-06 sweep efficient.

**Risk justification HIGH_RISK:**
- **FF-HR-002**: cada query depende de `tenant_id` filter; bug → cross-tenant exposure indireta.
- **FF-HR-005**: schema serve como AuthZ enforcement layer (CTRL-ISO-002 alignment).
- **Reversibility**: schema migration backwards é hard; ALTER TABLE em D1 SQLite tem restrições; rollback requer dump + restore.

## 3. Customer Impact & Journey

**JTBD:** "Como tenant, preciso garantia de consistência forte entre R2 (blob) e D1 (metadata) — após PUT bem sucedido, GET subsequent always retorna meu blob."

**Journey:**
- **Direct**: PUT/GET hot path bate D1.
- **Customer-visible**: latência (D1 query p99 ≤ 5ms — within SLO budget).
- **Indirect**: foundation pra S-04 AC outputs reachability, S-06 GC mark phase, S-10 billing usage tracking.

## 4. Capability Mapping

- **CAP-CAS-001/002/003** (CAS write path) — schema é foundation.
- Trace: `data_model.md §4.2 (blob_meta schema)` + `storage_semantics_matrix.md §3 (D1 semantics)` + `auth_model.md §8.1 layer 3` (AuthZ check pre-R2 reads em S-02).

## 5. Tipo e Classificação

Foundation (data layer); HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Migration `migrations/001_blob_meta.sql`**: DDL CREATE TABLE + 2 indexes; idempotent.
2. **D1 client wrapper** em `crates/corelink-worker/src/storage/d1_blob_meta.rs`:
   - `BlobMetaStore::insert(tenant_id, digest, size_bytes) -> Result<InsertOutcome>` (returns Inserted | AlreadyExists for idempotent semantics).
   - `BlobMetaStore::get(tenant_id, digest) -> Result<Option<BlobMetaRow>>`.
   - `BlobMetaStore::increment_refcount(tenant_id, digest) -> Result<()>`.
   - `BlobMetaStore::decrement_refcount(tenant_id, digest) -> Result<RefcountResult>`.
   - `BlobMetaStore::soft_delete(tenant_id, digest) -> Result<()>` (tombstone).
3. **Per-region D1 instance binding** via wrangler.toml: 3 regiões.
4. **Migration runner script** em `scripts/migrate_d1.sh` (manual run pre-deploy).
5. **CI gate**: schema validation cross-region (`scripts/check_d1_schema.py` planned forward).

### 6.2 Out-of-scope

- AC schema (`ac_meta` em WI-S04-002).
- Multipart schema (`chunks` + `manifest_chunks` em WI-S05-004).
- Audit chain schema (em S-09).

## 7. Anti-Scope

- ❌ ORM (sqlx OR diesel) — overkill para SQL simples; raw queries via D1 binding.
- ❌ Schema versioning via app metadata (use git+migrations files only).
- ❌ Custom SQL function (D1 SQLite supports limited; avoid for portability).
- ❌ Global tenant_id index (privacy + cardinality issue).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: blob_meta schema + refcount

  Background:
    Given Tenant A with tenant_id "uuid-A"

  Scenario: First INSERT — happy path
    When BlobMetaStore.insert(uuid-A, D_X, 5000) called
    Then row exists: (tenant_id=uuid-A, digest=D_X, refcount=1, size_bytes=5000, deleted_at=NULL)
    And InsertOutcome::Inserted returned

  Scenario: Idempotent duplicate INSERT
    Given row exists for (uuid-A, D_X)
    When BlobMetaStore.insert(uuid-A, D_X, 5000) called again
    Then InsertOutcome::AlreadyExists returned
    And row count == 1 (no duplicate row)

  Scenario: PRIMARY KEY enforcement
    Given row exists (uuid-A, D_X)
    When raw INSERT OR FAIL attempted with same (uuid-A, D_X)
    Then SQLITE_CONSTRAINT_PRIMARYKEY error

  Scenario: Cross-tenant same digest is allowed
    Given row exists (uuid-A, D_X)
    When BlobMetaStore.insert(uuid-B, D_X, 5000) called
    Then both rows exist (different tenants OK; same content)
    And refcount independent per tenant

  Scenario: Atomic refcount increment
    Given row (uuid-A, D_X, refcount=1) (canonical first-write default)
    When 100 concurrent increment_refcount called
    Then final refcount == 101 (no race; D1 transaction atomic)

  Scenario: Soft-delete tombstone
    When BlobMetaStore.soft_delete(uuid-A, D_X) called
    Then deleted_at is set to current timestamp
    And subsequent get(uuid-A, D_X) returns row (alive flag check is consumer responsibility)
    And idx_blob_meta_tenant_alive partial index excludes this row

  Scenario: Migration idempotent
    Given migrations/001_blob_meta.sql applied once
    When applied again on same D1 instance
    Then no error (CREATE IF NOT EXISTS)
    And schema unchanged
```

## 9. Design Decisions

### 9.1 Why D1 (vs Neon Postgres)

D1 é colocated com Workers em mesma região; latência sub-ms para queries simples. Neon é used for slowly-changing data (tenant config; auth tables — em S-03 WI-S03-005). Blob metadata é hot path; D1 SQLite engine + per-region replication = perfect fit.

### 9.2 Why PRIMARY KEY (tenant_id, digest)

Composite primary key combina:
- Uniqueness enforcement (cada tenant tem at most 1 blob per digest).
- Index covering for AuthZ checks `WHERE tenant_id=? AND digest=?`.
- Insertion order locality (tenant blobs stored together; better cache behavior).

### 9.3 Why partial indexes (`WHERE deleted_at IS NULL` / `IS NOT NULL`)

Tombstoned blobs accumulate até physical delete (S-06 grace period 72h). Sem partial index, queries sobre alive set scan tombstones too. Partial index keeps alive-set queries O(log n_alive) sem n_total.

### 9.4 Why INTEGER timestamps (Unix epoch milliseconds)

D1 SQLite TEXT date format slow + indexable inconsistente. INTEGER unix epoch (milliseconds canonical per data_model.md §4.2) = atomic compare + sortable + index-friendly. SQLite **INTEGER é 64-bit nativo** (não 32-bit; signed 32-bit rolls em 2038, unsigned 32-bit em 2106 — ambos irrelevantes aqui). Y2106/2038 não-aplicável; safe até ~ano 292B (signed 64-bit max). Unidade ms (não s) alinha com `last_accessed_at` hot-path em S-07 + audit chain timestamps em S-09.

### 9.5 Why per-tenant blob (não cross-tenant CAS dedup)

Decisão **deliberada**: cross-tenant content addressing seria storage-saving mas:
- Privacy violação (content fingerprinting cross-customer): atacante PUT digest_X em tenant A; verifica via FindMissingBlobs no tenant B se "missing" — leak de existence.
- Billing complexity: quem paga storage de blob shared? First-writer? Last-reader?
- Deletion isolation: tenant A LGPD erasure cannot delete blob ainda em use por tenant B.
- Trade-off: aceita storage waste (cross-tenant duplication) em troca de isolation hard-by-construction. Future S-XX pode revisitar com privacy-preserving CAS dedup (e.g., CR-Lite tickets).

### 9.6 Why audit_outbox em mesmo schema (não separate D1 DB)

Outbox pattern (ADR-0027) requires **single transaction** abrange `blob_meta` INSERT + `audit_outbox` INSERT. D1 não tem cross-database transactions; portanto `audit_outbox` mora no mesmo D1 instance. Drain worker emit ao S-09 chain remoto (separado).

### 9.7 Migration rollback policy (additive-only com CI enforcement)

Migrations são **additive-only** (CREATE TABLE/COLUMN, never DROP/ALTER). Disruptive change requer ADR + dual-write window. CI gate `scripts/check_migrations_additive.py` diff vs main; fails em DROP/ALTER COLUMN. Vide WI-S01-007 §6.1 para CI integration.

### 9.5 ADR potencial?

Não. Schema é canonical em data_model.md; este WI é implementation.

## 10. Completeness Criteria SOTA

- [ ] **10.4.1** Migration idempotent verified em 3 regiões staging (EVT-027).
- [ ] **10.4.2** Property test 10k iter atomic refcount race (EVT-002).
- [ ] **10.4.3** D1 query p99 ≤ 5ms para AuthZ check (criterion benchmark).
- [ ] **10.4.4** Cross-region schema sync verified via `check_d1_schema.py` planned.
- [ ] **10.4.5** Cost regression gate: D1 query cost per op (EVT-002).

## 11. DoD

- [ ] Migration `001_blob_meta.sql` applied em 3 regiões.
- [ ] BlobMetaStore impl + integration test.
- [ ] Atomic refcount property test green.
- [ ] Index strategy benchmark validated.
- [ ] Migration runner script.
- [ ] Code review + Architect (D1 schema review) + DBA-equivalent.

## 12. Invariants

- **INV-CAS-IMMUTABILITY** (CRITICAL): PRIMARY KEY enforces; INSERT OR IGNORE idempotent.
- **INV-GC-003** (HIGH): refcount consistency via transactional updates.
- **INV-TENANT-ISOLATION** (CRITICAL): tenant_id em PK + indexes.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Migration DDL | `migrations/001_blob_meta.sql` | SQL |
| BlobMetaStore | `crates/corelink-worker/src/storage/d1_blob_meta.rs` | Rust |
| Migration runner | `scripts/migrate_d1.sh` | Bash |
| Property test | `crates/corelink-worker/tests/prop_d1_refcount.rs` | Rust test |

## 14. Quality Standards SOTA

- **14.4.1** Zero unsafe; zero unwrap.
- **14.4.2** Documentação rustdoc.
- **14.4.3** Test coverage ≥ 90%.
- **14.4.4** D1 query p99 ≤ 5ms (AuthZ hot path).
- **14.4.5** SAST clean.
- **14.4.6** Métricas: `corelink.d1.blob_meta.query_duration_seconds_bucket{op}`.
- **14.4.7** Runbook: nenhum novo (failures cobertos em RB-FM-057 D1 failover).
- **14.4.8** Breaking schema changes = bump migration version + ADR.
- **14.4.9** Index strategy reviewed by Architect.
- **14.4.10** Cost regression gate.

## 15. Chaos Experiments

1. **D1 primary failover**: verify queries failover to read replica gracefully.
2. **Concurrent refcount race**: 100 increments + 100 decrements concurrent → final value correct.
3. **Migration replay**: apply migration twice → idempotent.

## 16. PRR

PRR doc + Architect (schema review) + DBA-equivalent.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Migration DDL design + review | 2h |
| ST-002 | BlobMetaStore impl (insert/get/refcount/soft_delete) | 4h |
| ST-003 | Atomic refcount transaction | 2h |
| ST-004 | Migration runner script | 1.5h |
| ST-005 | Per-region D1 binding wrangler.toml | 1h |
| ST-006 | Property test 10k race | 2.5h |
| ST-007 | Integration test E2E | 2h |
| ST-008 | Métricas | 1h |
| ST-009 | rustdoc + examples | 1h |
| ST-010 | PRR + schema review walkthrough | 1.5h |

**Total**: ~18.5h Optimistic; PERT ~22h.

## 18. Dependencies

- WI-S01-001 SEALED (TenantPath crate; tenant_id type).
- D1 instance provisioned (Terraform pre-S-01).

### Outbound

- WI-S01-005 (REAPI handler) usa BlobMetaStore.
- WI-S04-002 (AC schema) reuses migration framework pattern.
- WI-S06-002 (GC mark) lê blob_meta.

## 19. Effort Estimate (PERT)

O: 16h, M: 18.5h, P: 32h → PERT 20.7h.

## 20. Time-boxing

- 24h hard limit.
- Escalation ST-002 > 5h → Architect.

## 21. Observability

`corelink.d1.blob_meta.query_duration_seconds{op}`; alert p99 > 10ms sustained 5min.

## 22. Cost Analysis

D1 read $1/M ops; write $1/M ops. TCO 12m com 100M ops/dia: ~$36k/yr.

## 23. API Contract Impact

`BlobMetaStore` trait semver-locked; migrations versioned semver.

## 24. Post-mortem Hooks

- Refcount drift detected → CRITICAL post-mortem (FM-300 risk).
- Schema sync mismatch cross-region → 5-Why obrigatório.
- D1 query p99 > 50ms sustained → post-mortem.

## 25. Rollback / Recovery

- Schema rollback complex em SQLite; mitigate via additive-only migrations.
- Refcount drift recovery via reconcile job (S-06).

## 26. Security & Privacy

STRIDE: tampering — atomic transactions enforce; info disclosure — tenant_id em PK + partial indexes.
LINDDUN: linkability — tenant_id é pseudonymous UUIDv7 (canonical per data_model.md §2.1).

## 27. Knowledge Transfer

Doc `docs/internal/d1-schema-pattern.md` — partial index strategy + atomic refcount pattern reusable em outros tables.

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Race em refcount UPDATE | M | M | HIGH | M | LOW | Atomic SQL UPDATE single-row + property test 10k |
| R-002 | Schema drift cross-region | M | M | MEDIUM | M | LOW | check_d1_schema.py CI gate planned |
| R-003 | Migration breaks rollback | L | M | HIGH | L | LOW | Additive-only policy + ADR per disruptive change |
| R-004 | Partial index queries não usados (planner miss) | L | L | LOW | L | LOW | EXPLAIN QUERY PLAN test em CI |
| R-005 | Tombstone leak (query sem WHERE deleted_at IS NULL) | M | M | HIGH | M | LOW | Code review checklist + integration test |

## 29. Review Checkpoints

1. Design review (D+0): Architect approves schema.
2. Code review (D+2): peer + Architect.
3. Pre-merge: PRR + schema sync verify.

## 30. Sign-off (HIGH_RISK 11 canonical)

[12 roles incl. Architect (schema) + Security].

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S01-004 (Lote 10.1). |

## 32. Anti-patterns evitados

- ❌ Query sem WHERE tenant_id (cross-tenant scan).
- ❌ Read-modify-write refcount (race-prone).
- ❌ Schema migrations sem versioning git (drift risk).
- ❌ ORM heavy (overkill).
- ❌ Tombstone bypass em read path.

---

**Fim WI-S01-004.** Próximo: WI-S01-005 (REAPI BatchUpdateBlobs handler).
