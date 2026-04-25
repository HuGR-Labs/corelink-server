---
id: "WI-S04-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-04"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s04", "ac", "schema", "d1", "r2", "migration", "high-risk"]
---

# WI-S04-002 — D1 `ac_meta` Schema Migration + R2 `ac-<region>` Bucket Provisioning + UNIQUE Constraint + Tenant-Scoped Indices + Migration Idempotency

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-04](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S04-002 |
| Título | D1 schema `ac_meta` (PRIMARY KEY composite, UNIQUE constraint, indices for tenant + TTL queries, FK lógico to blob_meta) + R2 `ac-<region>` bucket per region with lifecycle policies + migration idempotency + rollback plan |
| Sprint | S-04 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (schema bug em tenant scoping = FM-303 catastrophic), FF-HR-005 (FK to blob_meta enforces INV-AC-OUTPUTS-VALID at storage layer) |

## 1. Intent

Provisionar **D1 schema** `ac_meta` + **R2 bucket** `ac-<region>` per region (`sam`, `iad`, `lhr`, `nrt`, `syd`) — backbone físico do Action Cache que WI-S04-001 handlers consomem.

```sql
-- File: migrations/003_ac_meta.sql
BEGIN;

CREATE TABLE IF NOT EXISTS ac_meta (
  -- Tenant scope (PK component 1; ALL queries filter via tenant_id Layer 4)
  tenant_id           TEXT        NOT NULL,

  -- Action identity (PK component 2; REAPI v2 SHA-256 hex digest)
  action_digest       TEXT        NOT NULL,

  -- Result metadata
  result_hash         TEXT        NOT NULL,             -- SHA-256 of ActionResult proto canonical bytes
  blob_refs           TEXT        NOT NULL,             -- JSON array of digests (output_files + output_dirs)
  blob_refs_count     INTEGER     NOT NULL,             -- denormalized for cheap COUNT
  result_size_bytes   INTEGER     NOT NULL,             -- for cost/quota observability

  -- Lifecycle timestamps (unix ms)
  created_at          INTEGER     NOT NULL,
  last_hit_at         INTEGER     NOT NULL,
  expires_at          INTEGER     NULL,                 -- NULL = no expiry; per-tier override S-07/ADR-0019

  -- Crypto integrity binding
  sig_key_id          INTEGER     NOT NULL DEFAULT 1,   -- HKDF tenant_key version (rotation support)
  sig_alg             TEXT        NOT NULL DEFAULT 'hkdf-sha256',  -- WI-S04-004 algorithm

  -- Region scope (R2 envelope location)
  region              TEXT        NOT NULL,             -- 'sam', 'iad', 'lhr', 'nrt', 'syd'

  -- Source attribution (audit cross-check; reuse WI-S03-007 chain)
  created_by_pat_id   TEXT        NULL,                 -- PAT id at write time (NULLABLE for legacy entries)
  created_by_request_id TEXT      NULL,                 -- request_id correlation

  PRIMARY KEY (tenant_id, action_digest)
);

-- Index: tenant + TTL queries (eviction worker WI-S04-005)
CREATE INDEX IF NOT EXISTS idx_ac_meta_tenant_expires
  ON ac_meta(tenant_id, expires_at)
  WHERE expires_at IS NOT NULL;

-- Index: tenant + last_hit (cache analytics + LRU support)
CREATE INDEX IF NOT EXISTS idx_ac_meta_tenant_last_hit
  ON ac_meta(tenant_id, last_hit_at);

-- Index: region (rare; cross-region migration support; not hot path)
CREATE INDEX IF NOT EXISTS idx_ac_meta_region
  ON ac_meta(region)
  WHERE region IS NOT NULL;

-- CHECK constraints (defense-in-depth; D1 SQLite supports CHECK)
ALTER TABLE ac_meta ADD CONSTRAINT chk_ac_blob_refs_size
  CHECK (length(blob_refs) <= 10240);  -- 10 KiB max JSON array

ALTER TABLE ac_meta ADD CONSTRAINT chk_ac_blob_refs_count
  CHECK (blob_refs_count >= 0 AND blob_refs_count <= 4096);

ALTER TABLE ac_meta ADD CONSTRAINT chk_ac_result_size
  CHECK (result_size_bytes >= 0 AND result_size_bytes <= 1048576);  -- 1 MiB

ALTER TABLE ac_meta ADD CONSTRAINT chk_ac_region
  CHECK (region IN ('sam', 'iad', 'lhr', 'nrt', 'syd'));

ALTER TABLE ac_meta ADD CONSTRAINT chk_ac_sig_alg
  CHECK (sig_alg IN ('hkdf-sha256', 'hkdf-blake3'));  -- forward-compat

ALTER TABLE ac_meta ADD CONSTRAINT chk_ac_lifecycle
  CHECK (last_hit_at >= created_at AND (expires_at IS NULL OR expires_at >= created_at));

COMMIT;
```

```toml
# wrangler.toml additions (R2 bucket bindings per region)
[[r2_buckets]]
binding = "AC_BUCKET_SAM"
bucket_name = "corelink-ac-sam"
preview_bucket_name = "corelink-ac-sam-preview"

[[r2_buckets]]
binding = "AC_BUCKET_IAD"
bucket_name = "corelink-ac-iad"
preview_bucket_name = "corelink-ac-iad-preview"

[[r2_buckets]]
binding = "AC_BUCKET_LHR"
bucket_name = "corelink-ac-lhr"
preview_bucket_name = "corelink-ac-lhr-preview"

[[r2_buckets]]
binding = "AC_BUCKET_NRT"
bucket_name = "corelink-ac-nrt"
preview_bucket_name = "corelink-ac-nrt-preview"

[[r2_buckets]]
binding = "AC_BUCKET_SYD"
bucket_name = "corelink-ac-syd"
preview_bucket_name = "corelink-ac-syd-preview"
```

```bash
# R2 bucket provisioning (idempotent; CI script)
for region in sam iad lhr nrt syd; do
  bucket="corelink-ac-${region}"
  wrangler r2 bucket create "$bucket" --location "$region" || echo "Bucket $bucket already exists"
  wrangler r2 bucket lifecycle set "$bucket" --rule '
    {
      "id": "abort-multipart-incomplete-7d",
      "status": "Enabled",
      "abortIncompleteMultipartUpload": { "daysAfterInitiation": 7 }
    }'
  # Public access OFF; only Worker bindings can read/write
  wrangler r2 bucket cors set "$bucket" --rules '[]'
done
```

**Migration semantics**:
- Idempotent (`CREATE TABLE IF NOT EXISTS`, `ON CONFLICT DO NOTHING`).
- Forward-only (D1 não-suporta DDL DROP em prod sem ADR + waiver).
- Ran via wrangler migration framework: `wrangler d1 migrations apply CORELINK_DB --env staging`.
- Pre-migration check: `SELECT name FROM sqlite_master WHERE type='table' AND name='ac_meta'`; only proceed if absent.
- Rollback plan: in case of bug, **mark migration as no-op** via dummy migration 003a; never DROP TABLE in prod (data loss risk).

**Constraint cripto-driven**:
- `tenant_id TEXT NOT NULL` — UUID v7 string (S-03 WI-S03-005); sqlx `with_tenant_ctx!` macro guarantees `SET LOCAL app.current_tenant`.
- PRIMARY KEY composite `(tenant_id, action_digest)` — Layer 4 enforcement at storage layer; impossible cross-tenant via PK alone.
- `blob_refs TEXT NOT NULL` — JSON array; FK lógico (não FK SQL strict — D1 limitation + perf) to `blob_meta(tenant_id, digest)`; reconcile diário enforces (S-06 GC).
- `sig_key_id` — multi-key support (rotation); same pattern WI-S03-005 `*_key_id` columns.
- `region` validated via CHECK; ensures envelope path matches binding.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Schema é o **physical foundation** da AC; bug em PRIMARY KEY ou CHECK constraint propaga para todo data lifecycle. HIGH_RISK porque:

1. **PRIMARY KEY direction confusion**: se PK fosse `(action_digest, tenant_id)` em vez de `(tenant_id, action_digest)`, query `WHERE tenant_id = $1 AND action_digest = $2` ainda funciona, mas index seek é diferente. Pior: se PK fosse `(action_digest)` only (sem tenant_id), todos os tenants compartilhariam namespace = INV-AC-TENANT-SCOPED CRITICAL violado at storage layer. Mitigação: PK composite `(tenant_id, action_digest)` — tenant_id PRIMEIRO; queries leverage index efficiently; tenant scope hardcoded at storage layer.

2. **Foreign key lógico to blob_meta** (INV-AC-OUTPUTS-VALID): D1 SQLite tem FK suporte mas `PRAGMA foreign_keys=ON` deve ser explicit; em CF Workers pool semantics, PRAGMA é per-connection (inconsistente). Decisão: **FK lógico** (não SQL FK); reconcile diário (S-06 GC) detecta drift; UPDATE handler (WI-S04-001) faz check pré-persist (strict). Rationale: SQL FK em D1 = perf overhead + lock contention; reconcile + handler check = same INV protection.

3. **JSON column blob_refs**: D1 não-suporta JSON path indexing (SQLite JSON1 extension nativo mas perf marginal); queries que filtram por content do JSON são full-scan. Decisão: **denormalized `blob_refs_count`** column; rare ad-hoc queries fazem full-scan acceptable (S-06 GC reconcile is batch).

4. **Region validation drift**: novo region `gru` adicionado em S-14 mas CHECK constraint não atualizado → INSERT fails. Mitigação: migration 016+ atualiza CHECK constraint com new region; ADR-0036 documents region addition policy.

5. **expires_at NULL semantics**: NULL = no expiry (admin-set free-tier override pre-S-07) vs missing = error. Schema chooses NULL = no expiry; explicit; query filters `WHERE expires_at IS NULL OR expires_at > NOW()`.

6. **`created_by_pat_id` NULL**: legacy entries (pre-WI-S03-002 PAT migration) não têm PAT id. Mitigação: NULLABLE column; modern entries always populate (handler enforced); audit chain WI-S03-007 cross-references.

7. **R2 bucket provisioning vs Worker binding ordering**: deploy Worker before R2 bucket exists → 503 at runtime. Mitigação: CI script provisions R2 buckets BEFORE Worker deploy; `wrangler r2 bucket create` is idempotent; deploy guard validates bindings.

8. **Bucket name collision** (CF account-wide namespace): `corelink-ac-sam` exists in another tenant's account. Mitigação: account-isolated; CF R2 bucket names are account-scoped; not global.

9. **Lifecycle rule misconfiguration**: multipart abort > 7d may impact slow uploaders (rare; CoreLink doesn't expose direct R2 upload to clients). Mitigação: 7d is REAPI guidance; alert if multipart abort spike.

**Atacante adversarial scenarios**:

- **Malicious migration replay**: attacker re-runs migration 003 in prod after schema drifted; CREATE TABLE IF NOT EXISTS no-op'd; CHECK constraints unchanged. Mitigação: migration framework hash-validates SQL file; mismatch → reject; ADR-0036 documents migration governance.

- **Tenant_id NULL injection** via SQL: rare but defense-in-depth. Mitigação: NOT NULL constraint; sqlx prepared statement type-check; `with_tenant_ctx!` macro asserts.

- **Schema diff drift between staging + prod**: dev applies new migration in staging, forgets prod; handler in prod uses fields not present. Mitigação: CI gate `wrangler d1 migrations list` matches expected state in both envs; deploy fails if mismatch.

- **R2 bucket public access enabled accidentally**: customer/dev sets bucket public via CF dashboard; AC envelopes (containing tenant_id, command_line, env_vars) leaked. Mitigação: CI cron asserts CORS rules empty + public access OFF; alert if drift; runbook documents revert.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: PK direction or `tenant_id NOT NULL` constraint bug = cross-tenant catastrophic FM-303.
- **FF-HR-005**: FK lógico to blob_meta enforces INV-AC-OUTPUTS-VALID; schema-level enforcement boundary.
- **Reversibility**: schema bug em prod = data loss risk (cannot DROP TABLE); rollback via dummy migration only; pre-deploy validation mandatory.
- **Customer impact**: schema drift breaks all AC ops; full sprint downtime; staging gate critical.

13 sign-offs incl. Architect (schema design + migration governance), DBA (D1 + R2 sizing + indexing), AppSec (CORS + bucket public access guard).

## 3. Customer Impact & Journey

**Persona 1 — Backend dev deploying CoreLink**:
- `wrangler d1 migrations apply CORELINK_DB --env staging` → migration 003 applies idempotently.
- `bash scripts/provision_ac_buckets.sh staging` → R2 buckets provisioned in 5 regions.
- Smoke test: handler GET request → ac_meta SELECT works; INSERT works; CHECK constraints validate.
- Deploy guard: pre-deploy CI fails if migration not applied OR R2 bucket missing.

**Persona 2 — DBA reviewing index efficiency**:
- `EXPLAIN QUERY PLAN` para SELECT WHERE tenant_id = ? AND action_digest = ? → uses PK; cost negligible.
- `EXPLAIN QUERY PLAN` para SELECT WHERE tenant_id = ? AND expires_at < ? → uses idx_ac_meta_tenant_expires; cost ≤ 50ms p99 even at 1M rows.
- Reconcile query (S-06 GC): WHERE tenant_id = ? AND blob_refs JSON contains $digest → full scan; OK as batch op.

**Persona 3 — Compliance auditor reviewing data residency**:
- Audit query: `SELECT region, COUNT(*) FROM ac_meta GROUP BY region` per tenant.
- Tenant ABC enforces SAM-only via tenant_quota row; INSERT with region != 'sam' rejected at handler level (defense-in-depth; not at schema since schema is global).
- Cross-region migration tracked via S-14; not in S-04 scope.

**SLA addendum**:
- D1 ac_meta query p99: SELECT by PK ≤ 5ms; INSERT ≤ 20ms; UPDATE last_hit_at ≤ 10ms.
- R2 ac-<region> bucket: PUT envelope p99 ≤ 100ms; GET p99 ≤ 50ms (5KB envelopes).
- Migration apply: ≤ 30s in staging; ≤ 60s in prod (low-load window).
- Bucket provisioning: ≤ 2 min for 5 regions sequential; CI parallelizable.

## 4. Capability Mapping

- **CAP-AC-001**, **CAP-AC-002** — IMPLEMENTA storage layer foundation; consumed by WI-S04-001 handlers.
- **CAP-EVICT-002** (S-07) — IMPLEMENTA index foundation (idx_ac_meta_tenant_expires); supersede semantics in ADR-0019.
- Trace: `data_model.md §4.2 (ac_meta schema canonical)` + `storage_semantics.md §5.2 (R2 AC layout)` + `security_model.md §10 (key management — sig_key_id rotation)`.

## 5. Tipo

Schema migration + infra provisioning; HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **D1 migration `migrations/003_ac_meta.sql`**:
   - CREATE TABLE ac_meta with composite PK + columns + CHECK constraints (vide §1).
   - 3 indices: idx_ac_meta_tenant_expires, idx_ac_meta_tenant_last_hit, idx_ac_meta_region.
   - Idempotent (`IF NOT EXISTS`).
   - Hash-validated by migration framework.

2. **Wrangler R2 bucket bindings**:
   - 5 regions: sam, iad, lhr, nrt, syd.
   - `wrangler.toml` updated with bindings.
   - Preview bucket bindings for staging/dev.

3. **R2 bucket provisioning script `scripts/provision_ac_buckets.sh`**:
   - Idempotent (`wrangler r2 bucket create` returns ok if exists).
   - Lifecycle rule: abort multipart > 7d.
   - CORS empty (no public access).
   - Public access OFF (CF default).

4. **Migration framework integration**:
   - `wrangler d1 migrations create CORELINK_DB ac_meta_schema` generates 003 file.
   - `wrangler d1 migrations apply CORELINK_DB --env <env>` applies.
   - CI gate: `wrangler d1 migrations list` matches expected.

5. **Schema validation in tests**:
   - Integration test: post-migration, INSERT/SELECT/UPDATE work.
   - CHECK constraint tests: invalid region/sig_alg/blob_refs_count rejected.
   - PK uniqueness test: duplicate (tenant_id, action_digest) INSERT → ON CONFLICT DO UPDATE.

6. **Deploy guard `scripts/check_ac_infra.sh`**:
   - Pre-deploy: validate migration 003 applied in target env.
   - Validate R2 buckets exist for 5 regions.
   - Validate Worker bindings for 5 buckets.
   - Validate CORS rules empty + public access OFF.
   - Fail-loud if any check fails.

7. **Sample data seeder for staging** (DEV ONLY):
   - `scripts/seed_ac_staging.sh` populates ~100 ac_meta entries for testing.
   - NOT for prod; gated by `CORELINK_ENV=staging`.

8. **Schema documentation**:
   - `docs/internal/d1-schema-ac-meta.md` — column rationale, CHECK constraints, indices, migration governance.
   - Cross-link to data_model.md §4.2 canonical source.

9. **Métricas observability**:
   - `corelink.d1.ac_meta.row_count{tenant_tier}` (gauge; cardinality bounded by tenant_tier).
   - `corelink.d1.ac_meta.size_bytes{tenant_tier}` (gauge).
   - `corelink.r2.ac_bucket.object_count{region}` (gauge; emit via S-06 reconcile job).
   - `corelink.r2.ac_bucket.size_bytes{region}` (gauge).
   - `corelink.d1.migration.applied_total{migration_id}` (counter).
   - `corelink.d1.ac_meta.check_violation_total{constraint}` (counter; alert on > 0).

10. **Property tests** (10k iter PR; 100k nightly):
    - `prop_ac_meta_pk_uniqueness`: 1000 random (tenant, digest); INSERT same twice → 2nd ON CONFLICT.
    - `prop_ac_meta_check_constraints`: 1000 invalid blob_refs (oversize/malformed); 100% rejected.
    - `prop_ac_meta_tenant_isolation_at_storage`: Tenant A INSERT; Tenant B SELECT WHERE tenant_id=B → empty.
    - `prop_ac_meta_lifecycle_invariant`: random (created, hit, expires); INSERT respects last_hit ≥ created + (expires NULL OR ≥ created).
    - `prop_ac_meta_blob_refs_count_consistent`: blob_refs_count == JSON.array_length(blob_refs); always true.

11. **Migration rollback test**:
    - Test scenario: migration 003 applied → app deployed with WI-S04-001 handler → bug detected → rollback path documented.
    - Rollback approach: NEW migration 003a marks ac_meta DEPRECATED via metadata table; handler short-circuits; data preserved for manual recovery.
    - **NEVER** DROP TABLE in prod; ADR-0036 documents.

12. **R2 bucket ACL hardening test**:
    - CI cron: `wrangler r2 bucket policy get corelink-ac-<region>` validates no public ACL.
    - Alert if drift; runbook RB-FM-AC-BUCKET-LEAK forward.

13. **Sizing validation**:
    - Estimate row size: ~250 bytes per ac_meta row (action_digest 64 + result_hash 64 + blob_refs JSON ~50 + timestamps + others ~70).
    - 10M unique actions/tenant × 100 tenants = 1B rows = ~250 GB. D1 free tier 5GB; paid tier 100GB+ per DB.
    - Decision: **D1 sharded per tenant_tier OR per region** → S-09 forward (database scaling).
    - For S-04 GA: assume 100 tenants × 100k actions = 10M rows = 2.5 GB; fits paid D1 single shard.
    - ADR-0036 documents sharding criteria.

### 6.2 Out-of-scope (deferred)

- **D1 sharding strategy** (per-region OR per-tenant_tier): S-09 forward.
- **Cross-region replication** of ac_meta (multi-region failover): S-14.
- **R2 bucket replication** (R2 cross-region replication feature): S-14.
- **Schema versioning policy** (online schema migration tooling): S-13 admin plane.
- **Per-tenant region pinning enforcement at schema** (currently handler-level): S-04 stays handler-level; schema is global.
- **GC reconcile job** (INV-AC-OUTPUTS-VALID enforcement): WI-S06-XXX.
- **Migration framework hardening** (signed migrations, automated rollback runbooks): S-09 forward.

## 7. Anti-Scope

- ❌ DROP TABLE em prod (data loss; rollback via dummy migration).
- ❌ FK SQL strict to blob_meta (perf overhead; FK lógico via reconcile).
- ❌ Index sem WHERE clause (cardinality unnecessary cost).
- ❌ Public R2 ACL (security; CI enforces).
- ❌ tenant_id NULLABLE (Layer 4 violated).
- ❌ Composite PK reverse `(action_digest, tenant_id)` (index seek inefficient + tenant Layer 4 risk).
- ❌ Region as enum (D1 SQLite no enum; CHECK constraint surrogate).
- ❌ JSON path indexing (D1 SQLite limit; full-scan acceptable for batch).
- ❌ Migration framework bypass via raw SQL apply (audit trail loss).
- ❌ Bucket name without environment suffix in non-prod (use preview_bucket_name).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: D1 ac_meta schema + R2 bucket provisioning

  Background:
    Given staging D1 database CORELINK_DB exists
    Given wrangler CLI installed and authenticated to staging environment

  Scenario: Migration 003 applies idempotently
    When wrangler d1 migrations apply CORELINK_DB --env staging
    Then migration 003 status = applied
    And ac_meta table exists in D1
    And 3 indices exist (idx_ac_meta_tenant_expires, idx_ac_meta_tenant_last_hit, idx_ac_meta_region)
    And 6 CHECK constraints active (blob_refs_size, count, result_size, region, sig_alg, lifecycle)
    When migration 003 re-applied
    Then status = no-op (already applied)
    And no schema change

  Scenario: PK uniqueness enforced
    Given Tenant A inserts (action_digest=D, result=R1) at T+0
    When Tenant A inserts (action_digest=D, result=R2) at T+1 with INSERT ON CONFLICT DO UPDATE last_hit_at = excluded.last_hit_at
    Then row count for (Tenant A, D) = 1
    And result_hash = R1 (immutable; ON CONFLICT only updates last_hit_at)
    And last_hit_at = T+1 (updated)

  Scenario: tenant_id NOT NULL enforced
    When INSERT INTO ac_meta (action_digest, result_hash, ...) VALUES (..., ...)  -- tenant_id missing
    Then SQL error NOT NULL constraint violation
    And metric corelink.d1.ac_meta.check_violation_total{constraint="tenant_id_not_null"} incremented

  Scenario: CHECK constraint blob_refs size violation
    When INSERT with blob_refs = JSON array of 10000 chars (>10240 limit)
    Then SQL error CHECK constraint chk_ac_blob_refs_size violation
    And metric corelink.d1.ac_meta.check_violation_total{constraint="blob_refs_size"} incremented

  Scenario: CHECK constraint blob_refs_count violation
    When INSERT with blob_refs_count = 5000 (>4096 limit)
    Then SQL error CHECK constraint chk_ac_blob_refs_count violation

  Scenario: CHECK constraint region violation
    When INSERT with region = 'gru'  -- not in allowed list
    Then SQL error CHECK constraint chk_ac_region violation

  Scenario: CHECK constraint lifecycle violation
    When INSERT with last_hit_at < created_at
    Then SQL error CHECK constraint chk_ac_lifecycle violation

  Scenario: Tenant isolation at storage layer
    Given Tenant A inserts (action_digest=D, ...)
    When Tenant B SELECT WHERE tenant_id = TenantCtx_B AND action_digest = D
    Then result is empty (PK composite tenant_id-first index excludes A's row)
    And property test prop_ac_meta_tenant_isolation_at_storage green

  Scenario: Index efficiency (PK seek)
    Given ac_meta has 1M rows
    When EXPLAIN QUERY PLAN SELECT * FROM ac_meta WHERE tenant_id = ? AND action_digest = ?
    Then plan uses PK (covering index)
    And query latency p99 ≤ 5ms

  Scenario: Index efficiency (TTL eviction query)
    Given ac_meta has 1M rows
    When EXPLAIN QUERY PLAN SELECT * FROM ac_meta WHERE tenant_id = ? AND expires_at < NOW()
    Then plan uses idx_ac_meta_tenant_expires
    And query latency p99 ≤ 50ms

  Scenario: R2 buckets provisioned in 5 regions
    When bash scripts/provision_ac_buckets.sh staging
    Then 5 R2 buckets exist (corelink-ac-{sam,iad,lhr,nrt,syd})
    And lifecycle rule "abort-multipart-incomplete-7d" set on each
    And CORS rules empty
    And public access OFF
    When script re-run
    Then idempotent (no recreate; no error)

  Scenario: Worker binding validation
    Given wrangler.toml has 5 R2 bindings (AC_BUCKET_{SAM,IAD,LHR,NRT,SYD})
    When wrangler deploy --env staging
    Then deploy succeeds
    And handler at runtime can resolve bindings

  Scenario: Deploy guard fails if migration not applied
    Given migration 003 not applied in env=staging-test
    When bash scripts/check_ac_infra.sh staging-test
    Then exit code != 0
    And error message "ac_meta migration not applied"

  Scenario: Deploy guard fails if R2 bucket missing
    Given corelink-ac-syd bucket deleted
    When bash scripts/check_ac_infra.sh staging
    Then exit code != 0
    And error message "R2 bucket corelink-ac-syd missing"

  Scenario: Deploy guard fails if bucket has public ACL drift
    Given attacker enables public access on corelink-ac-sam via dashboard
    When CI cron runs scripts/check_ac_infra.sh
    Then alert PD-CRITICAL fired
    And runbook RB-FM-AC-BUCKET-LEAK forward referenced

  Scenario: Migration rollback (via dummy 003a)
    Given migration 003 applied in prod with bug detected
    When dev creates migration 003a marking ac_meta DEPRECATED via meta table
    And handler reads meta flag and short-circuits
    Then ac_meta data preserved (no DROP)
    And handler returns 503 with error_code COR_AC_DEPRECATED until 003b restoration

  Scenario: ON CONFLICT DO UPDATE preserves immutability
    Given ac_meta has (Tenant A, D, result_hash=H1, last_hit_at=T0)
    When INSERT ON CONFLICT (tenant_id, action_digest) DO UPDATE SET last_hit_at = excluded.last_hit_at, expires_at = excluded.expires_at
    With excluded.last_hit_at = T1, excluded.result_hash = H2 (mismatch)
    Then result_hash unchanged (H1)
    And last_hit_at = T1
    And handler logic detects mismatch (excluded.result_hash != current.result_hash) → 409 COR_AC_RESULT_HASH_MISMATCH (WI-S04-001 enforces; schema permits)

  Scenario: sig_key_id default + rotation forward-compat
    Given migration applied; sig_key_id default = 1
    When key rotation event in S-04 v1.1 increments to sig_key_id = 2
    Then existing rows with sig_key_id = 1 still valid (handler verifies with v1 key)
    And new rows have sig_key_id = 2 (handler signs with v2 key)
    And metric corelink.crypto.key_rotation.ac_sig_key_id{version} emitted
```

## 9. Design Decisions

### 9.1 Why composite PK `(tenant_id, action_digest)` (not single PK)

- Tenant Layer 4 enforced at storage layer; impossible cross-tenant via PK.
- D1 SQLite uses leftmost columns for PK index seek; tenant_id-first is optimal for WHERE tenant_id = ? AND action_digest = ?.
- Reverse `(action_digest, tenant_id)` would still work for full PK lookup but inefficient for tenant-scoped scans.

### 9.2 Why FK lógico (not SQL FK strict) to blob_meta

- D1 SQLite FK requires `PRAGMA foreign_keys=ON`; CF Workers pool semantics inconsistent (per-connection); risk of FK silently OFF.
- SQL FK locks contention on multi-row UPDATE in blob_meta (S-06 GC tombstone).
- FK lógico via reconcile diário (S-06) + handler check pré-persist (WI-S04-001) = same INV protection without lock contention.

### 9.3 Why CHECK constraints (defense-in-depth + handler validation)

- Handler (WI-S04-001) does all validation; schema CHECK redundant but defense-in-depth.
- Captures bugs where handler validation is bypassed (refactor, new entry point).
- Cost: CHECK is O(1) per row INSERT; negligible.

### 9.4 Why indexes WHERE clauses (not full)

- `idx_ac_meta_tenant_expires WHERE expires_at IS NOT NULL` — most rows have expires_at NULL initially (free tier no expiry); index size halved.
- `idx_ac_meta_region WHERE region IS NOT NULL` — region NULL rare (legacy); index size minimized.

### 9.5 Why denormalized blob_refs_count

- D1 SQLite JSON1 ext supports `json_array_length()` but not indexed.
- Denormalized count column = O(1) read for COUNT(*) per AC entry; no JSON parse.
- Trade-off: handler must populate count on INSERT; CHECK constraint validates count consistency (count ≤ 4096); reconcile detects drift.

### 9.6 Why region as TEXT + CHECK constraint (not enum)

- D1 SQLite has no native ENUM type.
- TEXT + CHECK is canonical pattern; allows new region with migration update.
- ADR-0036 documents region addition policy (require new migration; never allow free-text).

### 9.7 Why per-region R2 bucket (not single global bucket)

- R2 buckets are region-pinned (CF infra primary region); cross-region access incurs latency.
- Per-region bucket aligns with tenant region attribution (region column).
- Simplifies S-14 multi-region replication (rclone OR R2-native replication forward).

### 9.8 Why migration framework hash validation (not raw SQL apply)

- Migration framework computes hash of SQL file; mismatch = file modified after applied = drift.
- Raw `wrangler d1 execute` allows any SQL; bypasses audit trail.
- ADR-0036 documents migration governance.

### 9.9 Why NEVER DROP TABLE in prod

- D1 SQLite DROP TABLE = irreversible data loss.
- Rollback via NEW migration 003a marking deprecated (metadata flag) is safer.
- Manual data recovery path documented; ADR-0036.

### 9.10 ADR potencial?

Sim — **ADR-0036**: "AC schema design + migration governance + R2 bucket provisioning policy + region addition workflow". Whitelist em validate_references.py.

## 10. Completeness Criteria SOTA

- [ ] **10.s04.002.1** Property tests 10k iter (PR) + 100k nightly → 0 panics, 0 false-accepts (EVT-002):
  - prop_ac_meta_pk_uniqueness, prop_ac_meta_check_constraints, prop_ac_meta_tenant_isolation_at_storage, prop_ac_meta_lifecycle_invariant, prop_ac_meta_blob_refs_count_consistent.
- [ ] **10.s04.002.2** Migration idempotency: re-apply 003 = no-op; hash validates (EVT-002).
- [ ] **10.s04.002.3** EXPLAIN QUERY PLAN validates: PK seek for SELECT by (tenant_id, action_digest); idx_ac_meta_tenant_expires for TTL queries (EVT-021).
- [ ] **10.s04.002.4** D1 ac_meta SELECT p99 ≤ 5ms; INSERT ≤ 20ms; UPDATE ≤ 10ms; sustained 72h staging (EVT-021).
- [ ] **10.s04.002.5** R2 ac-<region> PUT p99 ≤ 100ms; GET p99 ≤ 50ms; sustained 72h (EVT-021).
- [ ] **10.s04.002.6** Deploy guard CI gate: pre-deploy validates migration applied + buckets exist + CORS empty + public OFF (EVT-002).
- [ ] **10.s04.002.7** Rollback test: dummy migration 003a deprecates; handler short-circuits; data preserved (EVT-017).
- [ ] **10.s04.002.8** Bucket ACL hardening test: CI cron asserts no public access; alert on drift (EVT-014).
- [ ] **10.s04.002.9** Sizing projection: 10M rows = 2.5 GB; fits D1 paid single shard; ADR-0036 documents sharding trigger.
- [ ] **10.s04.002.10** Cargo-audit + cargo-deny + clippy `-D warnings` (no Rust code in this WI but related test scaffolding).
- [ ] **10.s04.002.11** ADR-0036 published.
- [ ] **10.s04.002.12** Schema documentation `docs/internal/d1-schema-ac-meta.md` published.

## 11. DoD

- [ ] Migration 003 applied in dev/staging/prod environments.
- [ ] R2 buckets provisioned in 5 regions.
- [ ] Wrangler bindings configured.
- [ ] Deploy guard script `scripts/check_ac_infra.sh` green.
- [ ] All Gherkin scenarios green.
- [ ] Property tests 10k green em CI; 100k nightly green.
- [ ] EXPLAIN QUERY PLAN validation passes.
- [ ] Latency benchmarks green (D1 + R2).
- [ ] CORS + public access guard CI cron live.
- [ ] Rollback test executed in staging.
- [ ] Métricas (6 listadas §6.1.9) emitted.
- [ ] ADR-0036 published.
- [ ] Architect + DBA + AppSec reviews.
- [ ] PRR Architect mini sign-off (full ship em WI-S04-006).

## 12. Invariants Validated

- **INV-AC-TENANT-SCOPED** (CRITICAL, registry §3.3): PK composite tenant_id-first; impossible cross-tenant at storage layer.
- **INV-AC-OUTPUTS-VALID** (HIGH, registry §3.3): FK lógico to blob_meta; reconcile diário + handler check.
- **INV-AC-IDEMPOTENT** (HIGH, NEW WI-S04-001 promotes; this WI provides PK enforcement): ON CONFLICT DO UPDATE last_hit_at only; result_hash immutable at storage layer (ON CONFLICT does not modify result_hash; CHECK enforces lifecycle).
- **INV-AC-RESULT-HASH-IMMUTABLE** (HIGH, NEW): ON CONFLICT clause does NOT include result_hash in UPDATE list; immutable enforced.
- **INV-DATA-AC-REFS-EXIST** (HIGH, registry §4.2 cross-ref): blob_refs JSON each digest exists in blob_meta; reconcile.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Migration SQL | `migrations/003_ac_meta.sql` | SQL |
| Wrangler config update | `wrangler.toml` | TOML |
| R2 bucket provisioning script | `scripts/provision_ac_buckets.sh` | Bash |
| Deploy guard script | `scripts/check_ac_infra.sh` | Bash |
| CI cron ACL hardening | `.github/workflows/ac-bucket-acl-cron.yml` | YAML |
| Sample data seeder (DEV) | `scripts/seed_ac_staging.sh` | Bash |
| Schema doc | `docs/internal/d1-schema-ac-meta.md` | Markdown |
| Property tests | `tests/prop_ac_meta_schema.rs` | Rust |
| Integration tests | `tests/it_ac_meta_lifecycle.rs` | Rust |
| Rollback runbook | `specs/02_governance/runbooks/RB-FM-AC-MIGRATION-BUG.md` | Markdown |
| Bucket leak runbook | `specs/02_governance/runbooks/RB-FM-AC-BUCKET-LEAK.md` | Markdown |
| ADR-0036 | `specs/02_governance/decisions/ADR-0036-ac-schema-migration-governance.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s04.002.1** Migration framework integration; hash-validated; never raw SQL apply em prod.
- **14.s04.002.2** Schema doc updated (data_model.md §4.2 already canonical; this WI adds index rationale + check constraint rationale).
- **14.s04.002.3** Test coverage: 100% schema constraints (CHECK + PK + NOT NULL + UNIQUE).
- **14.s04.002.4** Latência: D1 ops as listed; R2 ops as listed.
- **14.s04.002.5** SAST: Bash scripts shellcheck clean; SQL syntax validated.
- **14.s04.002.6** Métricas: 6 listadas §6.1.9.
- **14.s04.002.7** Runbooks: RB-FM-AC-MIGRATION-BUG + RB-FM-AC-BUCKET-LEAK.
- **14.s04.002.8** Forward-compat: sig_key_id rotation; sig_alg ENUM expandable; new region via migration.
- **14.s04.002.9** Memory bounded: row size ≤ 250 bytes typical; 10K JSON cap.
- **14.s04.002.10** Cost regression: D1 storage ≤ $0.01/GB/mo cap; R2 storage ≤ $0.015/GB/mo cap; provision cost gate.

## 15. Chaos Experiments

1. **Migration replay attack**: attacker re-runs migration 003 with modified SQL (extra column added). Hypothesis: hash validation rejects; CI gate red. Procedure: chaos PR; validate.

2. **R2 bucket public ACL drift**: attacker enables public access on bucket via dashboard. Hypothesis: CI cron detects within 1h; alert + auto-revert via runbook. Procedure: chaos toggle in staging; validate alert + revert.

3. **D1 partial outage during migration**: migration 003 partially applied (CREATE TABLE ok, INDEX failed). Hypothesis: framework rolls back transaction; partial state cleaned. Procedure: simulate D1 timeout mid-migration.

4. **Tenant_id NULL injection** via crafted INSERT: validate NOT NULL constraint catches; metric increments. Procedure: chaos integration test.

5. **PK collision storm**: 1000 req/s INSERT same (tenant_id, action_digest); validate ON CONFLICT semantics + no UNIQUE violation log. Procedure: load test.

6. **CHECK constraint regression**: schema migration 003a accidentally drops CHECK constraint chk_ac_region; validate test integration catches.

7. **R2 bucket region misalignment**: handler accidentally writes to ac-iad bucket while tenant region attribution = sam. Validate: handler binding selection logic (per WI-S04-001) covers; mismatch = 500 internal error + alert.

8. **D1 sharding boundary** (10M+ rows hypothetical): simulate large dataset via SQL fixtures; validate query plan still uses PK; latency degradation tracked. Procedure: load test 10M rows; alert if p99 > 5ms.

9. **Migration framework drift**: dev applies migration via raw `wrangler d1 execute` instead of framework. Hypothesis: framework `migrations list` detects drift; alert + manual reconcile.

10. **Bucket lifecycle rule misconfiguration**: chaos PR sets multipart abort to 1d (too aggressive); validate detection; revert.

11. **R2 bucket bindings misordered** in wrangler.toml; deploy succeeds but handler at runtime resolves wrong bucket. Validate: integration test catches; binding name lookup is exact.

## 16. PRR

PRR HIGH_RISK 13 sign-offs gated em WI-S04-006. Este WI mini-PRR Architect + DBA + AppSec.

- [ ] All Gherkin green.
- [ ] Property + chaos green.
- [ ] EXPLAIN QUERY PLAN validation passes.
- [ ] Latency benchmarks green.
- [ ] Deploy guard CI gate live.
- [ ] CORS hardening cron live.
- [ ] ADR-0036 published.
- [ ] Schema doc published.
- [ ] Rollback test executed.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Migration 003 SQL drafting + CHECK constraints | 2.5h |
| ST-002 | Index design + EXPLAIN QUERY PLAN validation | 2h |
| ST-003 | Migration framework integration + hash validation | 1.5h |
| ST-004 | Wrangler.toml R2 bindings update | 0.5h |
| ST-005 | Provisioning script `provision_ac_buckets.sh` | 2h |
| ST-006 | Deploy guard `check_ac_infra.sh` | 2h |
| ST-007 | CI cron bucket ACL hardening | 1.5h |
| ST-008 | Sample data seeder (DEV) | 1h |
| ST-009 | Schema documentation | 2h |
| ST-010 | Property tests (5 properties × 10k iter) | 3h |
| ST-011 | Integration tests (lifecycle + ON CONFLICT) | 3h |
| ST-012 | Rollback test in staging | 2h |
| ST-013 | Latency benchmarks (D1 + R2) | 2h |
| ST-014 | Sizing analysis + ADR-0036 sharding criteria | 2h |
| ST-015 | RB-FM-AC-MIGRATION-BUG + RB-FM-AC-BUCKET-LEAK | 2h |
| ST-016 | ADR-0036 redação | 2h |
| ST-017 | Architect + DBA + AppSec review iteration | 2h |
| ST-018 | Métricas emit + dashboard widget | 1.5h |

**Total Optimistic**: ~32h. **PERT** (O=28h, M=33h, P=50h): **~36h**.

## 18. Dependencies

### Hard blockers

- D1 framework available (already; S-01).
- R2 access in CF account (already).
- Wrangler CLI installed (dev tooling).

### Soft blockers

- DBA + Architect availability (review).
- ADR-0019 ratified (TTL ownership boundary; impacts expires_at semantics).

### Outbound

- WI-S04-001 (handler) consumes schema.
- WI-S04-003 (Merkle codec) referenced by blob_refs JSON.
- WI-S04-005 (TTL worker) consumes idx_ac_meta_tenant_expires.
- WI-S06-XXX (GC reconcile) consumes FK lógico boundary.

## 19. Effort PERT

O: 28h, M: 33h, P: 50h → PERT **36h**.

## 20. Time-boxing

**42h hard limit**. Se exceder → escalation: split em "schema migration" + "infra provisioning" sub-WIs.

## 21. Observability

6 métricas listadas §6.1.9. Trace span (rare; schema é mostly compile-time):
- `d1.migration.apply` — duration, status.
- `r2.bucket.provision` — bucket_name, region, status.
- `d1.ac_meta.check_violation` — constraint, tenant_id (sampled).

Logs structured JSON; INFO em apply success; WARN em CHECK violation; ERROR em provisioning fail.

Dashboard widget DASH-AC infra:
- D1 ac_meta row count + size per tenant_tier.
- R2 ac-<region> object count + size per region.
- CHECK violation rate (alert if > 0/h sustained).
- Bucket ACL drift status (boolean per region).

## 22. Cost Analysis

**Migration cost** (one-time):
- D1 migration apply: free tier; <1s execution.
- R2 bucket provisioning: free; CF dashboard ops.

**Storage cost** (steady state):
- D1 ac_meta storage: 10M rows × 250 bytes = 2.5 GB.
  - D1 paid tier: $0.75/GB/mo × 2.5 GB = ~$2/mo = **$24/yr**.
- R2 ac-<region> storage: 10M unique × 5 KB envelopes = 50 GB total (5 regions × 10 GB each).
  - R2 storage: $0.015/GB/mo × 50 GB = $0.75/mo = **$9/yr**.

**Operations cost**:
- D1 SELECT/INSERT/UPDATE: integrated with WI-S04-001 cost analysis.
- R2 PUT/GET/DELETE: integrated with WI-S04-001.

**TCO 12m projection**:
- Storage: ~$33/yr (D1 + R2 storage at 10M rows).
- Operations: covered in WI-S04-001 (~$23.7k/yr).
- **Total schema infra: ~$33/yr** (storage only; ops in handler WI).

**Cost regression gate**:
- D1 storage ≤ $0.01/GB/mo cap (within $0.75/GB/mo paid tier).
- R2 storage ≤ $0.015/GB/mo cap (R2 standard).
- Alert if storage growth > 50%/quarter (suggests schema bug or load growth).

**Comparison vs alternatives**:
- Postgres (Neon paid): $0.10/GB/mo × 2.5 GB = $3/mo + $0.50/M ops; total ~$60/yr at this scale.
- DynamoDB: $0.25/GB/mo + $1.25/M writes = ~$200/yr.
- D1 + R2: **$33/yr** (Cloudflare-native; near-zero ops).

## 23. API Contract

Schema é internal infra; expõe via SQL queries (handler-internal). Public artifacts:
- Migration file `003_ac_meta.sql` (deterministic; hash-validated).
- Wrangler binding names (stable post v1.0): `AC_BUCKET_{SAM,IAD,LHR,NRT,SYD}`.
- Deploy guard script `check_ac_infra.sh` (CI integration).

D1 query patterns documented in §1 (CREATE TABLE) + handler examples (WI-S04-001).

## 24. Post-mortem Hooks

- Migration replay drift detected → CRITICAL post-mortem + framework hardening review.
- Public bucket ACL drift sustained > 1h → CRITICAL post-mortem + breach notification consideration (envelope contains tenant_id + command_line).
- CHECK constraint violation > 10/day → SEV-2 (handler validation drift).
- D1 query latency p99 > 50ms sustained 1h → SEV-2 (sharding trigger; ADR-0036 escalation).
- Rollback test failed → SEV-1 (rollback path bug).
- Bucket provisioning failure in prod → SEV-1 + manual intervention.

## 25. Rollback / Recovery

- Migration rollback: NEW migration 003a marks DEPRECATED via meta table; handler short-circuits; data preserved.
- R2 bucket rollback: cannot delete bucket if non-empty; lifecycle rule update reversible via re-apply.
- Wrangler binding rollback: revert wrangler.toml + redeploy.
- RTO: ≤ 30 min (migration deprecation deploy).
- RPO: 0 (data preserved; no DROP).

Fallback: handler returns 503 `COR_AC_DEPRECATED` if 003a deprecated flag set; Bazel client retries with backoff.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: tenant_id NOT NULL enforced; sqlx prepared statements prevent SQL injection.
- **Tampering**: CHECK constraints validate at storage layer (defense-in-depth); migration framework hash validation prevents file tampering.
- **Repudiation**: migration applied event in audit chain (S-09 forward); ALL DDL ops logged.
- **Information disclosure**: R2 bucket public access OFF (CI enforces); CORS empty (no browser leak); per-region buckets isolated per CF account.
- **DoS**: D1 batch limit ~100 statements; INSERT ON CONFLICT atomic; R2 PUT idempotent.
- **Elevation of privilege**: schema is global (not per-tenant); handler enforces tenant scope; `with_tenant_ctx!` macro cross-cuts.

**LINDDUN delta**:
- **Linkability**: tenant_id UUID v7 pseudonymous; action_digest content-hash (non-PII).
- **Identifiability**: ActionResult metadata may contain command_line + env_vars (PII risk customer-side); R2 envelope signed but PLAINTEXT (not encrypted at rest within R2; SSE-S3 default encrypts disk-level only). FUTURE: per-tenant envelope encryption (S-04 v1.1 + ADR-0021 forward).
- **Non-repudiation**: append-only audit chain (S-09); migration ops logged.
- **Detectability**: bucket ACL drift detection cron; CHECK violation metrics.
- **Disclosure of information**: bucket public OFF enforced; per-region isolation; tenant_prefix path scoping (Layer 4).
- **Unawareness**: schema documented in data_model.md + docs/internal; ADR-0036 governance.
- **Non-compliance**: LGPD Art. 38 + GDPR Art. 32 satisfied via tenant scoping + audit + DSR S-11.

## 27. Knowledge Transfer

- **Tech talk** (1h): "D1 ac_meta Schema Design + R2 Bucket Provisioning + Migration Governance".
- **Doc** `docs/internal/d1-schema-ac-meta.md` — column rationale, indices, CHECK constraints.
- **Doc** `docs/internal/r2-bucket-provisioning.md` — bucket policies, ACL hardening, lifecycle rules.
- **ADR-0036** — design rationale.
- **Workshop** (1h): com Architect + DBA + Backend authors (downstream WIs consuming schema).
- **Onboarding test** (5 questions): PK direction rationale, FK lógico vs SQL FK, CHECK constraint enforcement, migration governance, R2 ACL hardening.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | PK composite reverse `(action_digest, tenant_id)` regression | L | L | CRITICAL | L | LOW | Migration hash validation; integration test EXPLAIN QUERY PLAN; ADR-0036 documents PK direction |
| R-002 | tenant_id NOT NULL constraint accidentally removed in future migration | L | L | CRITICAL | L | LOW | CHECK constraint review in ADR; test prop_ac_meta_tenant_isolation_at_storage; CI gate |
| R-003 | R2 bucket public ACL drift | L | M | HIGH | L | LOW | CI cron checks every 1h; alert + auto-revert via runbook; ADR-0036 documents |
| R-004 | Migration framework bypass via raw SQL apply | L | L | HIGH | L | LOW | Framework hash validation; CI gate; runbook RB-FM-AC-MIGRATION-BUG |
| R-005 | D1 storage growth exceeds paid tier (sharding trigger) | M | L | MEDIUM | L | LOW | Sizing projection; ADR-0036 sharding criteria; storage growth metric alert |
| R-006 | CHECK constraint regression (chk_ac_region drops in future) | L | L | MEDIUM | L | LOW | Schema doc; ADR-0036; integration test catches |
| R-007 | FK lógico drift (blob_refs references missing blob_meta entries) | M | M | HIGH | M | LOW | Reconcile diário (S-06); INV-AC-OUTPUTS-VALID daily check; alert on > 5 records |
| R-008 | Migration replay drift (file modified after applied) | L | L | HIGH | L | LOW | Hash validation; CI gate; runbook |
| R-009 | Per-tenant region pinning bypass at schema (handler-only) | M | M | MEDIUM | M | LOW | Schema is global; handler enforces region match; metric drift detection |
| R-010 | DROP TABLE in prod (irreversible) | L | L | CRITICAL | L | LOW | Migration framework forbids DROP; ADR-0036 explicit; senior review required |
| R-011 | Wrangler binding name mismatch at deploy time | L | L | HIGH | L | LOW | Deploy guard validates bindings; CI gate; handler binding lookup is exact |
| R-012 | Bucket lifecycle rule misconfiguration breaks slow uploaders | L | L | LOW | L | LOW | 7d abort multipart per REAPI; alert if abort spike; runbook |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + DBA review schema + indices + CHECK constraints.
2. **AppSec (D+1)**: AppSec review CORS + bucket public access + migration governance.
3. **Code (D+2)**: peer review (1 engineer + 1 DBA).
4. **Migration test (D+3)**: apply in staging; rollback test; latency benchmark.
5. **Adversarial (pre-merge D+4)**: red team — migration replay, ACL drift, tenant_id NULL injection.
6. **PRR (D+5)**: Architect mini sign-off (full ship gate em WI-S04-006).

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; CORS + bucket ACL hardening review_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD; bucket public access policy review_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; **mandatory** — schema design + migration governance_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; **mandatory** — CORS + bucket ACL + migration replay_ | _pending_ | _pending_ |
| 13 | DBA (advisory) | _mandatory; PK direction + index design + sizing projection_ | _pending_ | _pending_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S04-002 (Lote 10.4); SOTA pós-Lote 10.3bis. |

## 32. Anti-patterns evitados

- ❌ DROP TABLE in prod (irreversible).
- ❌ FK SQL strict (perf overhead; FK lógico via reconcile).
- ❌ PK reverse direction.
- ❌ tenant_id NULLABLE.
- ❌ Public R2 ACL.
- ❌ Raw SQL migration apply (framework bypass).
- ❌ JSON path indexing (D1 SQLite limit).
- ❌ Region as free-text (CHECK constraint enforces).
- ❌ Index without WHERE clause (cardinality cost).
- ❌ Composite PK action_digest-first (Layer 4 risk).
- ❌ Bucket name without env suffix in non-prod.
- ❌ Manual provisioning (idempotent script).

---

**Fim WI-S04-002.** Próximo: WI-S04-003 (corelink-ac crate — Merkle dual-side codec + verifier + INV-AC-OUTPUTS-VALID enforcement).
