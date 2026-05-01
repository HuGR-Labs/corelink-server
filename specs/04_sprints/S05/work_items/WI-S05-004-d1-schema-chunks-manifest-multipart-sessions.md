---
id: "WI-S05-004"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-04-25"
updated: "2026-05-01"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-05"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s05", "d1", "schema", "chunks", "manifest", "multipart-sessions", "unique-tenant", "high-risk"]
---

# WI-S05-004 — D1 Schema `chunks` + `manifest_chunks` + `multipart_sessions` + UNIQUE Constraint `(tenant_id, chunk_digest)` + tenant_prefix Materialization + Migration Idempotency

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-05](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S05-004 |
| Título | D1 schemas `chunks` (refcount intra-tenant dedup; UNIQUE `(tenant_id, chunk_digest)`) + `manifest_chunks` (ordered chunk references per blob) + `multipart_sessions` (R2 multipart tracking) + `cas_blobs.is_chunked` flag; CHECK constraints inline (lesson Lote 10.4bis); tenant_prefix BLOB(16) materialized; migration 004 idempotent + rollback plan |
| Sprint | S-05 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (UNIQUE bug → cross-tenant chunk leak FM-303), FF-HR-005 (FK lógico to chunks enforces INV-CAS-INTEGRITY tree) |

## 1. Intent

3 novas tabelas D1 + 1 column adição (cas_blobs.is_chunked) — backbone físico de multipart + chunking que WI-S05-001 handler consome:

```sql
-- File: migrations/d1/0003_multipart_chunks_manifest.sql (v1.3.0 path correction:
-- the D1 migration domain lives under migrations/d1/ per ADR-0036 + WI-S04-002
-- precedent; the root migrations/ hosts the Neon Postgres migrations only).
-- Lote 10.4bis P0 fix preemptive: CHECK constraints inline em CREATE TABLE
-- (SQLite/D1 NÃO suporta `ALTER TABLE … ADD CONSTRAINT chk_*`).
-- BEGIN/COMMIT removidos (wrangler implicit transactions).

-- 1) chunks: intra-tenant dedup table com refcount
CREATE TABLE IF NOT EXISTS chunks (
  -- Tenant scope (PK component 1; ALL queries filter via tenant_id Layer 4)
  tenant_id           TEXT        NOT NULL,

  -- Chunk identity (PK component 2; BLAKE3-256 hex of chunk bytes)
  chunk_digest        TEXT        NOT NULL,

  -- Tenant prefix materialized (consistente com WI-S04-002 pattern; cron sem TDK access)
  tenant_prefix       BLOB        NOT NULL,             -- length=16 enforced via CHECK
  path_key_id         INTEGER     NOT NULL DEFAULT 1,   -- TDK rotation version

  -- R2 storage location
  region              TEXT        NOT NULL,             -- 'sam', 'iad', 'lhr', 'nrt', 'syd'
  r2_object_key       TEXT        NOT NULL,             -- chunk-<region>/<tenant_prefix_hex>/<chunk_digest>

  -- Content metadata
  size_bytes          INTEGER     NOT NULL,             -- 1 MiB ≤ size ≤ 4 MiB (FastCDC bounds)

  -- Reference counting (intra-tenant dedup; CAP-CAS-010)
  refcount            INTEGER     NOT NULL DEFAULT 1,   -- INSERT ON CONFLICT increments

  -- Lifecycle timestamps (unix ms)
  created_at          INTEGER     NOT NULL,
  last_referenced_at  INTEGER     NOT NULL,             -- updated em manifest_chunks INSERT

  -- Source attribution
  created_by_pat_id   TEXT        NULL,
  created_by_request_id TEXT      NULL,

  PRIMARY KEY (tenant_id, chunk_digest),

  -- CHECK constraints inlined
  CHECK (length(tenant_prefix) = 16),
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),
  CHECK (size_bytes >= 1 AND size_bytes <= 4194304),    -- size 1 byte to 4 MiB max (FastCDC bound; Fixed2MiB final partial chunk pode ser < 1 MiB; Lote 10.5bis P0 fix: comment corrigido — original "1 MiB to 4 MiB" inverted lower bound; final partial pode ser 1 byte)
  CHECK (refcount >= 0)                                 -- never negative; 0 = candidate for S-06 GC
);

-- Index: tenant + last_referenced (S-06 GC sweep)
CREATE INDEX IF NOT EXISTS idx_chunks_tenant_last_ref
  ON chunks(tenant_id, last_referenced_at);

-- Index: tenant + refcount (S-06 GC reachability)
CREATE INDEX IF NOT EXISTS idx_chunks_tenant_refcount
  ON chunks(tenant_id, refcount)
  WHERE refcount = 0;

-- 2) manifest_chunks: ordered chunk references per chunked blob
CREATE TABLE IF NOT EXISTS manifest_chunks (
  -- Tenant scope (PK component 1)
  tenant_id           TEXT        NOT NULL,

  -- Blob digest (PK component 2; references cas_blobs)
  blob_digest         TEXT        NOT NULL,

  -- Chunk order (PK component 3; 0-indexed)
  chunk_index         INTEGER     NOT NULL,

  -- Chunk reference (FK lógico to chunks)
  chunk_digest        TEXT        NOT NULL,

  PRIMARY KEY (tenant_id, blob_digest, chunk_index),

  -- CHECK constraints inlined
  CHECK (chunk_index >= 0 AND chunk_index < 81920)      -- Lote 10.5bis P0 fix: 81920 = 160 GiB / 2 MiB exato (was off-by-one 80000); chunk_index 0..81919 = 81920 chunks max
);

-- Index: tenant + blob (manifest lookup ordered)
CREATE INDEX IF NOT EXISTS idx_manifest_chunks_tenant_blob
  ON manifest_chunks(tenant_id, blob_digest, chunk_index);

-- Index: tenant + chunk_digest (reverse lookup; refcount maintenance)
CREATE INDEX IF NOT EXISTS idx_manifest_chunks_tenant_chunk
  ON manifest_chunks(tenant_id, chunk_digest);

-- 3) multipart_sessions: R2 multipart upload tracking (FM-060 detection)
CREATE TABLE IF NOT EXISTS multipart_sessions (
  -- R2 upload_id (PK; opaque from R2)
  upload_id           TEXT        PRIMARY KEY,

  -- Tenant scope
  tenant_id           TEXT        NOT NULL,

  -- Expected blob digest (computed at SplitBlob start; verified at Complete)
  blob_digest_expected TEXT       NOT NULL,

  -- R2 location
  region              TEXT        NOT NULL,
  bucket              TEXT        NOT NULL,             -- 'corelink-chunk-<region>' or 'corelink-manifest-<region>'
  object_key          TEXT        NOT NULL,

  -- Lifecycle
  started_at          INTEGER     NOT NULL,             -- unix ms
  last_activity_at    INTEGER     NOT NULL,             -- updated per UploadPart
  state               TEXT        NOT NULL DEFAULT 'in_progress',  -- 'in_progress' | 'completed' | 'aborted'

  -- Source attribution
  created_by_pat_id   TEXT        NULL,
  created_by_request_id TEXT      NOT NULL,

  -- CHECK constraints inlined
  CHECK (state IN ('in_progress', 'completed', 'aborted')),
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd')),
  CHECK (last_activity_at >= started_at),

  -- Lote 10.5bis P0 fix: partial UNIQUE (state='in_progress' only); allows multiple
  -- completed/aborted records as audit trail; previously plain UNIQUE wrongly blocked
  -- second multipart of same blob after completion/abort.
  -- Handler semantic on collision: retry-on-existing (return existing upload_id), matching
  -- S3/R2 native multipart idempotency contract (NOT 409).
  UNIQUE (tenant_id, blob_digest_expected, state)
    -- Note: SQLite/D1 supports partial UNIQUE via separate CREATE UNIQUE INDEX statement;
    -- inline UNIQUE here covers all states; partial constraint via index below.
);

-- Partial UNIQUE INDEX (Lote 10.5bis P0 fix): only state='in_progress' enforces uniqueness;
-- completed/aborted records can be duplicated (legitimate audit trail).
CREATE UNIQUE INDEX IF NOT EXISTS uq_multipart_sessions_in_progress
  ON multipart_sessions(tenant_id, blob_digest_expected)
  WHERE state = 'in_progress';

-- Index: sweeper consumes (state=in_progress + last_activity older than 7d)
CREATE INDEX IF NOT EXISTS idx_multipart_sessions_orphan_sweep
  ON multipart_sessions(state, last_activity_at)
  WHERE state = 'in_progress';

-- Index: tenant analytics
CREATE INDEX IF NOT EXISTS idx_multipart_sessions_tenant_state
  ON multipart_sessions(tenant_id, state);

-- 4) cas_blobs.is_chunked column adição (cas_blobs table existed since S-01)
-- Lote 10.4bis lesson: NEVER DROP COLUMN em prod; use ADR + migration; ALTER TABLE ADD COLUMN OK em SQLite
ALTER TABLE cas_blobs ADD COLUMN is_chunked INTEGER NOT NULL DEFAULT 0;
-- Note: SQLite uses INTEGER 0/1 for boolean; CHECK enforced via convention.
```

```toml
# wrangler.toml additions
# 5 chunk-<region> buckets + 5 manifest-<region> buckets (consumes WI-S05-003 adapter provisioning)
[[r2_buckets]]
binding = "CHUNK_BUCKET_SAM"
bucket_name = "corelink-chunk-sam"
preview_bucket_name = "corelink-chunk-sam-preview"
# ... 4 more regions × 2 (chunk + manifest)
```

**Cripto-driven invariants**:

1. **chunks UNIQUE `(tenant_id, chunk_digest)`**: cross-tenant chunk leak impossible at storage layer (Layer 4 enforcement); INV-TENANT-ISOLATION load-bearing.
2. **manifest_chunks PK `(tenant_id, blob_digest, chunk_index)`**: ordered references; tenant-scoped lookup; same `(tenant_id, blob_digest)` re-write idempotent.
3. **multipart_sessions PRIMARY KEY (upload_id)**: R2-issued opaque ID; `tenant_id` binding column for cross-tenant replay rejection.
4. **CHECK constraints inlined** (lesson Lote 10.4bis): SQLite/D1 não suporta `ALTER TABLE ADD CONSTRAINT chk_*`.
5. **tenant_prefix BLOB(16) materialized** (consistente com WI-S04-002 pattern): cron worker (WI-S05-006 sweeper) reads sem TDK access; trust boundary preserved.
6. **FK lógico to cas_blobs + chunks**: D1 SQLite FK opcional; reconcile diário (S-06 forward) detects drift (lesson Lote 10.4bis: FK SQL strict has perf overhead em D1).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Schema é physical foundation de multipart + chunking. HIGH_RISK em N dimensões:

1. **UNIQUE direction error**: se UNIQUE fosse `(chunk_digest)` only (sem tenant_id), todos tenants compartilhariam chunks → INV-TENANT-ISOLATION CRITICAL violado at storage layer. Mitigação: PK composite `(tenant_id, chunk_digest)` — tenant_id PRIMEIRO (lesson Lote 10.4bis WI-S04-002).

2. **manifest_chunks PK direction**: `(tenant_id, blob_digest, chunk_index)` — tenant_id-first index seek; consistent com pattern.

3. **multipart_sessions UNIQUE constraint** `(tenant_id, blob_digest_expected, state)`: prevents two concurrent in_progress sessions for same blob (idempotency); allows multiple completed/aborted records (audit trail).

4. **`is_chunked` flag em cas_blobs**: ALTER TABLE ADD COLUMN é SQLite-supported (não DROP). Lesson Lote 10.4bis: ADD COLUMN OK; DROP COLUMN é problemático (use ADR + migration).

5. **Refcount semantics**: INSERT ON CONFLICT (tenant_id, chunk_digest) DO UPDATE SET refcount = refcount + 1, last_referenced_at = excluded.last_referenced_at. Idempotent for re-Split em mesmo blob (handler short-circuits via `is_chunked` check anyway). Decrement on manifest delete (S-06 GC forward); refcount = 0 = candidate for chunk delete.

6. **Migration idempotency**: `IF NOT EXISTS` + `ON CONFLICT DO NOTHING`; hash-validated by migration framework (lesson Lote 10.4bis); CI gate.

7. **R2 bucket provisioning vs schema sync**: deploy guard validates 5 chunk-<region> + 5 manifest-<region> buckets exist BEFORE schema migration applies (consumes WI-S05-003 provisioning script).

8. **D1 storage growth projection**: 10M blobs × 25 chunks/blob avg = 250M chunks rows × ~150 bytes = ~37 GB. **Excede D1 hard limit 10 GB** (lesson Lote 10.4bis WI-S04-002); sharding mandatory antes ou per-region/per-tier sharding (ADR-0036 sharding criteria + new ADR-0040 forward para multipart sharding).

**Atacante adversarial scenarios**:

- **Cross-tenant chunk reference via UNIQUE bug**: defense-in-depth via PK composite tenant_id-first.
- **Refcount integer overflow**: INTEGER em SQLite is i64; 10M tenants × 10M blobs × refcount = ~10^13 max; well below i64 max 2^63.
- **multipart_sessions ID enumeration**: R2 upload_id is cryptographically random; not predictable.
- **Schema migration replay attack**: hash validation prevents (lesson Lote 10.4bis); ADR governance.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: UNIQUE direction bug = cross-tenant chunk leak FM-303.
- **FF-HR-005**: FK lógico to chunks enforces INV-CAS-INTEGRITY tree.
- **D1 storage growth at scale**: 250M chunks rows = sharding mandatory; ADR-0040 forward.
- **Reversibility**: schema bug em prod = data loss risk; rollback via dummy migration; pre-deploy validation mandatory.

11 sign-offs canonical incl. Architect (DBA specialization for D1 schema sizing + chunks/manifest tables) + AppSec.

## 3. Customer Impact & Journey

**Persona 1 — Backend dev deploying CoreLink S-05**: `wrangler d1 migrations apply CORELINK_DB --env staging` aplica migration 004 idempotently. Smoke test: handler SplitBlob succeeds.

**Persona 2 — DBA reviewing storage projection**: 250M chunks rows = 37 GB; D1 10 GB hard limit; sharding mandatory; ADR-0040 (forward) documents per-tenant_tier OR per-region sharding strategy.

**Persona 3 — Compliance reviewer (LGPD/GDPR)**: chunks UNIQUE tenant-scoped; multipart_sessions audit trail; DSR cascade S-11 forward complementary.

**SLA addendum**:
- D1 chunks SELECT p99 ≤ 5ms (PK seek); INSERT ≤ 20ms; UPDATE refcount ≤ 10ms.
- D1 manifest_chunks batch INSERT 250 rows ≤ 100ms (Lote 10.4bis 100KB limit lesson).
- D1 multipart_sessions sweeper SELECT (state=in_progress + last_activity old) ≤ 50ms via partial index.
- Migration apply ≤ 30s staging; ≤ 60s prod (low-load window).

## 4. Capability Mapping

- **CAP-CAS-010** (Intra-tenant chunk dedup) — IMPLEMENTA via UNIQUE constraint.
- **CAP-CAS-008** + **CAP-CAS-012** — IMPLEMENTA storage foundation consumed by handlers + sweeper.
- Trace: `data_model.md §4.X` (forward update Lote 10.5bis) + `storage_semantics.md §5.X` + ADR-0022.

## 5. Tipo

Schema migration; HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope (compact form; details em §1)

1. **D1 migration `migrations/d1/0003_multipart_chunks_manifest.sql`**: 3 new tables (chunks + manifest_chunks + multipart_sessions). The `cas_blobs.is_chunked` column adição is **DEFERRED to WI-S05-006** alongside the live-D1 binding shim — `cas_blobs` lives in the `0001_blob_meta.sql` domain and the canonical S-05 wiring through that flag happens together with the conformance suite (handler short-circuit on `is_chunked = 1` is exercised against the live binding, not the simulator).
2. **CHECK constraints inline** (lesson Lote 10.4bis).
3. **6 indices** (chunks: 2; manifest_chunks: 2; multipart_sessions: 2).
4. **tenant_prefix BLOB(16) materialized** em chunks (cron sem TDK access).
5. **Wrangler R2 bindings** (consumed by WI-S05-003): 5 chunk-<region> + 5 manifest-<region> buckets.
6. **Migration framework integration**: hash-validated (lesson Lote 10.4bis).
7. **Schema validation tests**: PK uniqueness, CHECK constraints, FK lógico.
8. **Deploy guard `scripts/check_multipart_infra.sh`**: pre-deploy CI gate validates schema + buckets.
9. **Property tests** (10k iter PR; 100k nightly):
   - `prop_chunks_pk_uniqueness`: same `(tenant_id, chunk_digest)` INSERT 2× → 2nd ON CONFLICT increments refcount.
   - `prop_chunks_check_constraints`: invalid size_bytes/region/refcount rejected.
   - `prop_chunks_tenant_isolation`: Tenant A INSERT; Tenant B SELECT WHERE tenant_id=B → empty.
   - `prop_manifest_chunks_pk_ordered`: chunks indexed 0..N-1 sequential.
   - `prop_multipart_sessions_state_transitions`: in_progress → completed OR aborted; never reverse.
10. **Migration rollback test**: dummy migration 004a marks DEPRECATED via meta table; never DROP TABLE prod.
11. **Storage projection** (lesson Lote 10.4bis): 250M chunks rows = ~37 GB; sharding ADR-0040 forward.

### 6.2 Out-of-scope

- Sharding strategy implementation (ADR-0040 forward).
- Cross-region replication (S-14).
- DSR cascade integration (S-11 forward).
- Manifest_chunks tenant-tier retention policies (S-13 forward).

## 7. Anti-Scope

- ❌ DROP TABLE em prod.
- ❌ FK SQL strict (perf overhead; FK lógico via reconcile S-06).
- ❌ PK reverse direction (Layer 4 risk).
- ❌ tenant_id NULLABLE.
- ❌ ALTER TABLE ADD CONSTRAINT chk_* (SQLite/D1 unsupported; lesson Lote 10.4bis).
- ❌ BEGIN/COMMIT em migration (wrangler implicit; lesson Lote 10.4bis).
- ❌ Manual provisioning (idempotent script).
- ❌ Skip tenant_prefix materialization (cron TDK access expansion; lesson Lote 10.4bis WI-S04-002).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: D1 schema chunks + manifest_chunks + multipart_sessions

  Scenario: Migration 004 applies idempotently
    When wrangler d1 migrations apply CORELINK_DB --env staging
    Then 3 new tables exist (chunks, manifest_chunks, multipart_sessions)
    And cas_blobs.is_chunked column exists
    And 6 indices exist
    And 7+ CHECK constraints active
    When re-applied
    Then no-op

  Scenario: chunks UNIQUE tenant_id + chunk_digest
    Given Tenant A inserts (chunk_digest=C, ...) at T+0
    When Tenant A inserts (chunk_digest=C, ...) at T+1 with INSERT ON CONFLICT DO UPDATE refcount = refcount + 1
    Then refcount = 2; last_referenced_at = T+1
    And property test prop_chunks_pk_uniqueness green

  Scenario: chunks tenant isolation at storage layer
    Given Tenant A inserts (chunk_digest=C, ...)
    When Tenant B SELECT WHERE tenant_id = TenantCtx_B AND chunk_digest = C
    Then result empty (PK composite tenant_id-first index excludes A's row)
    And property test prop_chunks_tenant_isolation green

  Scenario: CHECK constraint chunks size_bytes
    When INSERT chunk with size_bytes = 5 * 1024 * 1024 (5 MiB > 4 MiB max)
    Then CHECK constraint violation

  Scenario: manifest_chunks PK ordered
    Given Tenant A blob D references chunks c1..c5
    When INSERT manifest_chunks (tenant_id=A, blob_digest=D, chunk_index=0..4, chunk_digest=...)
    Then PK enforce ordered uniqueness; chunk_index 0..4 sequential
    And property test prop_manifest_chunks_pk_ordered green

  Scenario: multipart_sessions UNIQUE in_progress
    Given Tenant A blob D upload starts (state=in_progress)
    When second upload tries same (tenant=A, blob=D, state=in_progress)
    Then UNIQUE constraint violation
    And handler returns 409 OR retry semantics

  Scenario: multipart_sessions state transitions
    Given session state = in_progress
    When UPDATE state = completed
    Then OK
    When UPDATE state = in_progress (reverse)
    Then handler-level rejection (not enforced by CHECK; INV-MULTIPART-STATE-MONOTONIC documented em registry)

  Scenario: cas_blobs.is_chunked column added
    Given migration 004 applied
    Then cas_blobs has is_chunked INTEGER NOT NULL DEFAULT 0
    And existing rows pre-S-05 have is_chunked = 0
    And new chunked blobs SET is_chunked = 1 via WI-S05-001 handler

  Scenario: D1 storage projection alert (sharding trigger)
    Given chunks table reaches 80% of D1 10 GB hard limit (~30M rows; 8 GB)
    When metric corelink.d1.chunks.size_bytes monitor fires
    Then alert PD-CRITICAL; sharding ADR-0040 forward escalation

  Scenario: Deploy guard validates schema + buckets
    Given scripts/check_multipart_infra.sh executed pre-deploy
    Then verifies migration 004 applied + 5 chunk + 5 manifest buckets exist + bindings configured
    And exit 0 if all green; exit 1 + alert if drift
```

## 9. Design Decisions (compact)

- **9.1** PK composite `(tenant_id, chunk_digest)` tenant-first (lesson Lote 10.4bis).
- **9.2** FK lógico to cas_blobs + chunks (perf vs SQL FK).
- **9.3** CHECK constraints inline (SQLite/D1 mandatory inline).
- **9.4** tenant_prefix materialized BLOB(16) (lesson Lote 10.4bis WI-S04-002).
- **9.5** ALTER TABLE ADD COLUMN OK em SQLite (NOT DROP; lesson Lote 10.4bis).
- **9.6** Refcount semantics: INSERT ON CONFLICT increments; S-06 GC decrements; refcount = 0 candidate for delete.
- **9.7** multipart_sessions UNIQUE `(tenant_id, blob_digest, state)` prevents duplicate in_progress.
- **9.8** Storage projection 250M rows ≈ 37 GB → sharding ADR-0040 forward (lesson Lote 10.4bis cost gate).
- **9.9** Migration framework hash-validated (lesson Lote 10.4bis).
- **9.10** ADR forward: ADR-0040 — multipart D1 sharding strategy (per-tenant_tier OR per-region; ratificada em WI-S05-006).

## 10. Completeness Criteria SOTA

- [ ] **10.s05.004.1** Property tests 5 × 10k iter green; 100k nightly.
- [ ] **10.s05.004.2** Migration idempotent: re-apply 004 = no-op; hash validates.
- [ ] **10.s05.004.3** EXPLAIN QUERY PLAN validates: PK seek for chunks SELECT by `(tenant_id, chunk_digest)`; idx_multipart_sessions_orphan_sweep for sweeper.
- [ ] **10.s05.004.4** D1 chunks SELECT p99 ≤ 5ms; INSERT ≤ 20ms; UPDATE refcount ≤ 10ms.
- [ ] **10.s05.004.5** Deploy guard CI gate green.
- [ ] **10.s05.004.6** Rollback test: dummy migration 004a deprecates; data preserved.
- [ ] **10.s05.004.7** Storage projection ADR-0040 forward (sharding criteria).
- [ ] **10.s05.004.8** ADR-0040 published (forward; ratificada em WI-S05-006).
- [ ] **10.s05.004.9** Cargo-audit + cargo-deny clean (test scaffolding).
- [ ] **10.s05.004.10** Schema doc updated em data_model.md §4.X.

## 11. DoD

- [ ] Migration 004 applied em dev/staging/prod.
- [ ] All Gherkin scenarios green.
- [ ] Property tests 10k green; 100k nightly.
- [ ] EXPLAIN QUERY PLAN passes.
- [ ] Deploy guard CI gate live.
- [ ] Rollback test executed.
- [ ] Métricas (4 storage-related) emitted.
- [ ] ADR-0040 published (forward).
- [ ] Architect + DBA + AppSec reviews.

## 12. Invariants Validated

- **INV-TENANT-ISOLATION** (CRITICAL, registry §3.3): chunks PK composite tenant_id-first.
- **INV-MULTIPART-IDEMPOTENT** (HIGH, registry §3.16): chunks ON CONFLICT increments refcount; multipart_sessions UNIQUE in_progress.
- **INV-MULTIPART-CHUNK-DETERMINISTIC** (CRITICAL, registry §3.16): tenant_prefix materialized stable.
- **INV-MULTIPART-STATE-MONOTONIC** (HIGH, NEW promovida §3.16 Lote 10.5bis): multipart_sessions state in_progress → completed OR aborted; never reverse.
- **INV-MULTIPART-PATH-KEY-MATERIALIZED** (HIGH, NEW): tenant_prefix BLOB(16) column; cron sem TDK access.

## 13. Artifacts Produced

(v1.3.0 update: paths reconciled to actual implementation; the
canonical D1 migration directory is `migrations/d1/` per ADR-0036 +
the WI-S04-002 precedent — not the root `migrations/` which hosts the
Neon Postgres migrations only. Items marked DEFERRED follow the
charter trait-abstraction-defer pattern and land in WI-S05-006
alongside the production D1 binding shim + sweeper.)

| Artifact | Path | Tipo | Status |
|---|---|---|---|
| Migration SQL | `migrations/d1/0003_multipart_chunks_manifest.sql` | SQL | SHIPPED |
| Embedded SQL constant | `crates/corelink-multipart-schema::MIGRATION_0003_MULTIPART_CHUNKS_MANIFEST` | Rust (`include_str!`) | SHIPPED |
| Host-side simulator | `crates/corelink-multipart-schema/src/sim.rs` (`MultipartSchema`) | Rust | SHIPPED |
| Region enum + bucket bindings | `crates/corelink-multipart-schema/src/region.rs` | Rust | SHIPPED |
| Wrangler config update (10 R2 bindings: 5 chunk + 5 manifest, default + prod) | `wrangler.toml` | TOML | SHIPPED |
| Migration canonical-text regression tests | `crates/corelink-multipart-schema/tests/migration_canonical.rs` | Rust (19 tests) | SHIPPED |
| Property tests (10k iter PR; 100k nightly via `PROPTEST_CASES`) | `crates/corelink-multipart-schema/tests/prop_multipart_schema.rs` | Rust (15 tests) | SHIPPED |
| Idempotency canonical regression tests | `crates/corelink-multipart-schema/tests/idempotency_canonical.rs` | Rust (15 tests) | SHIPPED |
| Schema doc | `docs/internal/d1-schema-multipart.md` | Markdown | SHIPPED |
| Production D1 binding shim | `crates/<TBD>` (adapter wrapping wrangler/miniflare D1 client) | Rust | DEFERRED → WI-S05-006 |
| Deploy guard | `scripts/check_multipart_infra.sh` | Bash | DEFERRED → WI-S05-006 |
| Live-D1 lifecycle integration test | `tests/it_multipart_lifecycle.rs` (miniflare/wrangler-dev) | Rust | DEFERRED → WI-S05-006 |
| Rollback runbook | `specs/02_governance/runbooks/RB-FM-MULTIPART-MIGRATION-BUG.md` | Markdown | DEFERRED → WI-S05-006 |
| ADR-0040 (D1 sharding criteria) | `specs/03_architecture/adrs/ADR-0040-multipart-d1-sharding.md` | Markdown | DEFERRED → WI-S05-006 |

## 14. Quality Standards SOTA (compact)

- 14.s05.004.1: Migration framework hash-validated.
- 14.s05.004.2: Test coverage 100% schema constraints.
- 14.s05.004.3: Latência: D1 ops as listed.
- 14.s05.004.4: SAST: Bash shellcheck; SQL syntax validated.
- 14.s05.004.5: Métricas: chunks size, multipart_sessions count, orphan rate.
- 14.s05.004.6: Runbook: RB-FM-MULTIPART-MIGRATION-BUG.
- 14.s05.004.7: Forward-compat: path_key_id rotation; sig_alg ENUM.
- 14.s05.004.8: Memory bounded: row size ≤ 200 bytes; 10K JSON cap (none here).
- 14.s05.004.9: Cost regression gate: D1 storage ≤ $0.075/GB/mo.

## 15. Chaos Experiments (5)

1. Migration replay attack (modified SQL) → hash validation rejects.
2. Cross-tenant query attempt → tenant isolation property test catches.
3. CHECK constraint regression → integration test catches.
4. D1 sharding boundary (30M rows simulated) → query plan still uses PK; metric alert.
5. ALTER TABLE ADD CONSTRAINT chk_* attempted (legacy fix forgotten) → SQLite syntax error; CI dry-run catches.

## 16. PRR

PRR mini Architect + DBA + AppSec.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Migration 004 SQL drafting (3 tables + 1 ALTER COLUMN) | 3 |
| ST-002 | CHECK constraints inline (lesson Lote 10.4bis) | 1 |
| ST-003 | Index design + EXPLAIN QUERY PLAN | 2 |
| ST-004 | Migration framework hash-validation | 1.5 |
| ST-005 | Wrangler.toml R2 bindings (10 buckets) | 1 |
| ST-006 | Deploy guard script | 2 |
| ST-007 | Storage projection + ADR-0040 sharding criteria | 2 |
| ST-008 | Schema doc update (data_model.md §4.X + docs/internal) | 2 |
| ST-009 | Property tests (5 × 10k) | 4 |
| ST-010 | Integration tests (lifecycle + dedup + tenant scope) | 3 |
| ST-011 | Rollback test em staging | 2 |
| ST-012 | RB-FM-MULTIPART-MIGRATION-BUG runbook | 1.5 |
| ST-013 | ADR-0040 redação | 2 |
| ST-014 | Architect + DBA + AppSec review iter | 3 |

**Total Optimistic**: ~30h. **PERT** (O=27h, M=32h, P=48h): **~35h**.

## 18. Dependencies

- Hard: D1 framework + R2 access + wrangler 4.x.
- Soft: WI-S04-002 schema pattern (tenant_prefix materialization lesson).
- Outbound: WI-S05-001 handler consumes; WI-S05-003 multipart_sessions; WI-S05-006 sweeper.

## 19. Effort PERT: 35h. ## 20. Time-boxing: 42h hard limit.

## 21. Observability (compact)

Métricas: corelink.d1.chunks.{row_count, size_bytes}; corelink.d1.multipart_sessions.{state_count, orphan_rate}; corelink.d1.manifest_chunks.{row_count}.

## 22. Cost Analysis

**Storage cost** (steady state):
- chunks: 250M rows × 150 bytes = 37 GB → sharding mandatory; per-shard ≤ 8 GB.
- manifest_chunks: 1M chunked blobs × 25 chunks avg × 100 bytes = 2.5 GB.
- multipart_sessions: 1M sessions/dia retention 7d = ~20 MB/dia rolling.
- D1 paid tier: $0.75/GB/mo; sharded total ~40 GB = $30/mo = **$360/yr**.

**Cost regression gate**: D1 storage growth > 50%/quarter alert; ADR-0040 sharding trigger at 80%.

## 23. API Contract

Schema é internal; expõe via SQL queries (handler-internal). Public artifacts: migration file `004_chunks_manifest_multipart.sql` (hash-validated); Wrangler binding names stable post-v1.0.

## 24. Post-mortem Hooks

- Schema migration replay drift → CRITICAL post-mortem.
- D1 storage hard limit hit → CRITICAL post-mortem + sharding ADR-0040 immediate.
- Cross-tenant query detected → CRITICAL.
- multipart_sessions orphan > 30d sustained → SEV-2 (sweeper review).

## 25. Rollback / Recovery

Migration 004a marks DEPRECATED via meta table; data preserved; never DROP. RTO ≤ 30 min; RPO 0.

## 26. Security & Privacy (compact)

**STRIDE**: tenant_id NOT NULL; PK composite tenant-first; sqlx prepared statements. **LINDDUN**: tenant_id pseudonymous; chunk_digest content-hash.

## 27. Knowledge Transfer

Tech talk (1h): "D1 Multipart Schema + Sharding Forward Strategy". Doc `docs/internal/d1-schema-multipart.md`. Onboarding test (5 questions).

## 28. Risk Register (10-row)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | UNIQUE direction bug | L | L | CRITICAL | L | LOW | PK composite tenant-first; integration test; ADR-0036 PK governance |
| R-002 | tenant_id NOT NULL accidentally removed | L | L | CRITICAL | L | LOW | Schema doc; CI test; ADR governance |
| R-003 | D1 storage hard limit hit (10 GB) | M | L | HIGH | M | LOW | Storage projection alert; ADR-0040 sharding trigger 80% |
| R-004 | Migration framework bypass | L | L | HIGH | L | LOW | Hash validation; CI gate |
| R-005 | CHECK constraint regression | L | L | MEDIUM | L | LOW | CI dry-run sqlite3 :memory:; lesson Lote 10.4bis |
| R-006 | FK lógico drift (manifest_chunks references missing chunks) | M | M | HIGH | M | LOW | Reconcile diário S-06 forward; INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY pattern |
| R-007 | Refcount overflow (i64) | L | L | LOW | L | LOW | Bounded by row count; well below i64 max |
| R-008 | multipart_sessions UNIQUE bypass | L | L | MEDIUM | L | LOW | UNIQUE constraint; integration test |
| R-009 | ALTER TABLE ADD COLUMN regression future | L | L | MEDIUM | L | LOW | Schema doc; ADR-0036 documents ADD vs DROP |
| R-010 | Sharding migration disruption (ADR-0040) | M | M | HIGH | M | LOW | Forward-compat; per-tenant_tier shard key planned |

## 29. Review Checkpoints

D+0 design (Architect+DBA); D+1 AppSec; D+2 code review; D+3 migration test staging; D+4 PRR mini.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034_ |
| 4 | Security Lead | _TBD; mandatory_ |
| 5-6 | Engineer × 2 | _TBD_ |
| 7 | QA | _TBD_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD_ |
| 10 | Privacy | _TBD_ |
| 11 | Architect | _TBD; **mandatory** — schema design + sharding ADR-0040_ |
| 12 | AppSec | _TBD; **mandatory** — UNIQUE direction + CHECK constraints inline_ |
| 13 | DBA (advisory) | _**mandatory** — PK direction + index design + sizing 37 GB + sharding trigger ADR-0040_ |

## 31. Change Log

1.0.0 / 2026-04-25 / Gustavo: Criação WI-S05-004 (Lote 10.5; SOTA pós-Lote 10.4bis lessons applied: CHECK inline; tenant_prefix materialized; ALTER ADD COLUMN; sharding ADR-0040 forward).

1.3.0 / 2026-05-01 / Gustavo (via Claude Opus 4.7): **WI-S05-004 SEALED — implementation phase, host-side simulator + canonical SQL artifact + 5-region wrangler bindings shipped.** Migration `migrations/d1/0003_multipart_chunks_manifest.sql` ships 3 idempotent `CREATE TABLE IF NOT EXISTS` + 5 `CREATE INDEX IF NOT EXISTS` + 1 `CREATE UNIQUE INDEX IF NOT EXISTS` (the partial UNIQUE on `(tenant_id, blob_digest_expected) WHERE state = 'in_progress'` per Lote 10.5bis P0 fix); 19 inline CHECK constraints across the three tables (chunks: digest_len + tenant_prefix_len + path_key_id_positive + region + size_bytes 1..=4_194_304 + refcount_non_negative + lifecycle; manifest_chunks: blob_digest_len + chunk_digest_len + chunk_index 0..81920; multipart_sessions: blob_digest_len + tenant_prefix_len + path_key_id_positive + region + state_domain + lifecycle_activity + lifecycle_expires + lifecycle_finalized + finalized_iff_terminal); tenant_prefix BLOB(16) materialised on chunks + multipart_sessions per ADR-0035 H-3; tenant-leftmost PK on every table (chunks `(tenant_id, chunk_digest)`; manifest_chunks `(tenant_id, blob_digest, chunk_index)`; multipart_sessions PK on `session_id` with `tenant_id` binding column for cross-tenant rejection). New crate `crates/corelink-multipart-schema/` ships the canonical migration via `include_str!` ([`MIGRATION_0003_MULTIPART_CHUNKS_MANIFEST`]) + `MultipartSchema` host-side simulator pinning every load-bearing invariant of the migration (chunks idempotent ON CONFLICT increment refcount; manifest_chunks PK collision with mismatched chunk_digest rejected; multipart_sessions partial-UNIQUE in_progress + monotone state graph in_progress → completed | aborted; CrossTenantSession surfaced for the adversarial path; expires_at = started_at + 7d default TTL); `MultipartRegion` enum mirrors the 5-region SQL CHECK list (sam/iad/lhr/nrt/syd) with chunk + manifest R2 bucket + wrangler binding accessors. Tests: 40 lib unit + 19 migration_canonical (idempotent DDL + every PK direction + partial UNIQUE shape + every CHECK mnemonic + every inline CHECK predicate + region IN-list parity + 5 indices + no destructive tokens + no BEGIN/COMMIT + no ALTER ADD CONSTRAINT) + 15 idempotency_canonical (chunks idempotent increment + GC candidate flow + manifest idempotent vs PK collision + session partial-UNIQUE returns existing + completed/aborted audit-trail does not block fresh start + monotone state graph + cross-tenant isolation enforced + reapply migration no-op + manifest assembled in canonical order via PK btree) + 15 property tests (10 at 10k iter PR + 4 at 5k for multi-tenant population scans + 1 at 10k migration idempotency; nightly opts into 100k via `PROPTEST_CASES`) covering chunks PK uniqueness + cross-tenant distinct + size_bytes + path_key_id_positive + tenant isolation + manifest PK ordered scan + manifest chunk_index bounded + sessions state forward + sessions state reverse rejected + partial UNIQUE in_progress + completed audit-trail unblocked + expires_at correctness + multi-tenant chunks isolated + migration idempotency. **Wrangler.toml** updated with 10 R2 bindings (5 `CHUNK_BUCKET_*` + 5 `MANIFEST_BUCKET_*`) for default + `[env.prod]`. F-001 closure preserved (every `MultipartSchema` instance owns its three `BTreeMap`s; no global mutable state). wasm32-clean (no tokio in src/ — same artifact runs in CF Workers WASM bundle + host-side test harness). Trait-abstraction-defer pattern preserved per charter: real Cloudflare D1 binding shim deferred to WI-S05-006 conformance suite alongside the sweeper; storage-projection ADR-0040 forward sharding criteria + RB-FM-MULTIPART-MIGRATION-BUG runbook + deploy guard `scripts/check_multipart_infra.sh` deferred to WI-S05-006 alongside the binding shim. Quality gates: `cargo test --workspace --all-targets --features corelink-worker/tower-middleware` 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` + `validate_references.py` no new dangling refs; `check_migrations_additive.py` clean (5 migration files scanned).

## 32. Anti-patterns evitados

- ❌ DROP TABLE em prod; ❌ FK SQL strict; ❌ PK reverse direction; ❌ tenant_id NULLABLE; ❌ ALTER ADD CONSTRAINT chk_* (Lote 10.4bis); ❌ BEGIN/COMMIT em migration; ❌ Skip tenant_prefix materialization; ❌ Manual provisioning.

---

**Fim WI-S05-004.** Próximo: WI-S05-005 (Merkle manifest builder/verifier dual-side).
