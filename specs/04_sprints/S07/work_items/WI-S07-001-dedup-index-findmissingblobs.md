---
id: "WI-S07-001"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-28"
lane: "STANDARD"
parent: "S-07"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "STORAGE-SEMANTICS"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s07", "dedup", "chunks", "findmissingblobs", "reapi", "standard"]
---

# WI-S07-001 — Dedup Lookup via `chunks` table (PK `(tenant_id, chunk_digest)` + `refcount`; pre-existing S-05 schema; NO new index/migration needed) + FindMissingBlobs Otimizado (REAPI v2 §FindMissingBlobs RPC; client skip re-upload of chunks already in tenant; baseline ≥ 2.5× dedup ratio sustained 7d staging) + INV-DEDUP-CONSISTENCY enforcement (Lote 10.7bis P0-1 fix: was incorrectly `UNIQUE INDEX manifest_chunks(tenant_id, chunk_digest)` which would break dedup — multiple manifests share chunks; corrected to `chunks` table which already has correct PK from S-05 WI-S05-004)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-07](../_spec_contract.md) (sprint contract; sprint.md not yet authored — defer to S-07-bis if full sprint doc needed) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S07-001 |
| Título | Dedup lookup via `chunks` table O(1) PK lookup (Lote 10.7bis P0-1 fix: pre-existing S-05 WI-S05-004 schema com `PRIMARY KEY (tenant_id, chunk_digest)` + `refcount` column; NO new migration needed; was incorrectly proposing UNIQUE INDEX em `manifest_chunks` which would BREAK dedup since same chunk_digest legitimately appears em multiple manifests for different blobs); REAPI v2 `FindMissingBlobs` handler returns absent-only chunks (skip-upload optimization); INV-DEDUP-CONSISTENCY already enforced by `chunks` PK; cross-tenant dedup OFF default (CTRL-ISO-005 + `dedup.cross_tenant.enabled=false`); benchmark ≥ 2.5× dedup ratio sustained 7d staging (target SOTA 3×) |
| Sprint | S-07 |
| Lane | STANDARD |
| Forcing factors | none (no CRITICAL touched; INV-TENANT-ISOLATION inherited via tenant_id scope) |

## 1. Intent

Adicionar **dedup intra-tenant** O(1) reaproveitando S-05 `chunks` table (Lote 10.7bis P0-1 fix; original spec incorrectly referenced `manifest_chunks`): `chunks` table tem `PRIMARY KEY (tenant_id, chunk_digest) + refcount` purpose-built for dedup; PK lookup é O(log N); REAPI v2 `FindMissingBlobs` retorna apenas chunks **genuinamente ausentes**, eliminando re-upload bytes redundantes (target: ≥ 50% bytes saved em workload Docker pulls):

```rust
// File: crates/corelink-dedup/src/lib.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait DedupIndex: Send + Sync {
    /// O(1) lookup: returns true se chunk_digest existe em D1 `chunks` table (Lote 10.7bis P0-1 fix)
    /// para o tenant — content-addressable hit; cliente pode pular upload.
    /// Tenant-scoped strict (INV-TENANT-ISOLATION); cross-tenant rejeitado
    /// pre-impl (CTRL-ISO-005; dedup.cross_tenant.enabled MUST be false).
    async fn chunk_exists(
        &self,
        tenant_ctx: &TenantCtx,
        chunk_digest: &Digest,
    ) -> Result<bool, DedupError>;

    /// Batch: filter input list returning only chunks NOT YET present em tenant.
    /// REAPI v2 FindMissingBlobs handler usa este método.
    /// Bounded D1 batch ≤ 250 chunks per query (Lote 10.5bis lesson).
    async fn find_missing(
        &self,
        tenant_ctx: &TenantCtx,
        chunk_digests: &[Digest],
    ) -> Result<Vec<Digest>, DedupError>;
}

#[derive(thiserror::Error, Debug)]
pub enum DedupError {
    #[error("d1 backend error: {0}")]
    D1BackendError(String),

    #[error("batch size {got} exceeds limit {limit}")]
    BatchTooLarge { got: usize, limit: usize },

    #[error("cross-tenant dedup attempted (CTRL-ISO-005 violation; dedup.cross_tenant.enabled=false)")]
    CrossTenantBlocked,
}
```

**Cripto-driven invariants enforced**:

1. **Tenant-scoped dedup via `chunks` table PRIMARY KEY** (Lote 10.7bis P0-1 fix; uses pre-existing S-05 schema):
   ```sql
   -- ALREADY EXISTS in `chunks` table (S-05 WI-S05-004 §6.1 lines 56-101):
   --   tenant_id           TEXT        NOT NULL,
   --   chunk_digest        TEXT        NOT NULL,
   --   r2_object_key       TEXT        NOT NULL,
   --   refcount            INTEGER     NOT NULL DEFAULT 1,  -- 1:1 (tenant_id, chunk_digest) → chunk_body via PK + refcount
   --   created_at          INTEGER     NOT NULL,
   --   last_referenced_at  INTEGER     NOT NULL,
   --   deleted_at          INTEGER     NULL,                -- soft-delete grace (S-06 inheritance)
   --   PRIMARY KEY (tenant_id, chunk_digest),               -- UNIQUE per tenant; enforces INV-DEDUP-CONSISTENCY
   --   CHECK (refcount >= 0)
   --
   -- S-07 Lote 10.7bis P0-1 fix: NO new migration needed. The `chunks` table PK
   -- (tenant_id, chunk_digest) IS the dedup uniqueness constraint. Original Lote 10.7
   -- spec proposed UNIQUE INDEX em `manifest_chunks` which would break dedup (same
   -- chunk_digest legitimately appears em multiple blobs' manifests; UNIQUE rejects
   -- second blob's INSERT). The `chunks` table is purpose-built for dedup; reverse
   -- lookup index `idx_manifest_chunks_tenant_chunk` (S-05 line 128) handles
   -- "which blobs reference chunk X" queries.
   ```
   - **Why `chunks` PK (NOT manifest_chunks UNIQUE)**: dedup canonical model — N blobs share 1 chunk via N rows em `manifest_chunks` referencing 1 row em `chunks` (refcount = N). UNIQUE em manifest_chunks would block N>1.
   - **INV-DEDUP-CONSISTENCY enforcement**: `chunks` PK guarantees 1:1 `(tenant_id, chunk_digest) → chunk_body bytes` via content-addressing (BLAKE3); INSERT ON CONFLICT increments refcount.
   - **Tenant-scoped (NOT global)**: CTRL-ISO-005 — cross-tenant dedup is existence oracle (THR-I-004 em security_model.md §6); off by default.

2. **CTRL-ISO-005 enforcement** (cross-tenant gate):
   - Code path explicitly `if !config.dedup.cross_tenant_enabled { return Err(CrossTenantBlocked); }` em qualquer chamada cross-tenant.
   - `config.dedup.cross_tenant_enabled` defaults `false`; flipping to `true` requires ADR + BYOE (S-14) + Privacy Lead signoff.
   - CI grep gate (NOT clippy::disallowed_method which is path-based; Lote 10.7bis P1-2 lesson absorbed) — `! grep -rn -E 'FROM\s+chunks\s+WHERE\s+(?!.*tenant_id)' src/ tests/` em CI workflow forbids `chunks` SELECT without `tenant_id` filter; cargo-spellcheck OR AST-grep custom rule alternative for stronger enforcement.

3. **D1 batch ≤ 250 row constraint** (Lote 10.5bis lesson): `find_missing` chunks input list capped at 250 per query; chunked iteration if input larger.

4. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id extracted from `TenantCtx` Tower middleware (S-03 WI-S03-003) — NEVER from request body, query params, or headers.

5. **Cripto-grade chunk_digest** (S-05 inheritance): chunk_digest is BLAKE3-256 of chunk bytes (deterministic; collision-resistant ≥ 128-bit security); index integrity follows from BLAKE3 collision-resistance.

## 2. Narrative (≥ 200 palavras STANDARD lane + dedup risk justification)

Dedup intra-tenant é a **economia direta** do CoreLink: para um workload Docker típico (multi-stage builds com layer reuse), dedup ratio chega a 3-5× via chunk-level CAS. Sem index UNIQUE em `(tenant_id, chunk_digest)`, lookups são O(N) D1 scan — incompatível com hot path < 2ms p99. Com UNIQUE INDEX, lookup é O(log N) B-tree ≈ O(1) em D1 SQLite (typical ~0.5ms p99 em 30M-row index).

`FindMissingBlobs` é a RPC REAPI v2 que **multiplica o ROI**: client envia lista de chunk digests; server retorna só os ausentes; client skip-uploads chunks existentes. Em workload Docker pull típico (~500 chunks, 50% redundancy típico), transferência cai de 1 GiB para ~500 MiB = **50% bandwidth saved customer-side**, com latency saved (~3s reduzido a ~1.5s em 100 Mbps).

**Risk justification STANDARD lane**:
- **Não toca CRITICAL invariants**: INV-TENANT-ISOLATION é HERDADO via tenant_id scope (UNIQUE constraint enforces); INV-CAS-IMMUTABILITY preserved (dedup é metadata index, NOT body mutation); INV-GC-001 untouched (S-06 enforcement).
- **Reversível**: drop INDEX possível (ADR + downtime ~5min D1 ALTER); fallback to scan (slower but correct).
- **Bounded blast radius**: dedup error → cliente uploads chunk redundante (waste); NEVER data loss.
- **Cross-tenant gate strict**: code path defaults disabled; flipping requires ADR.

**Adversarial scenarios considered**:
- **Existence oracle attack** (CTRL-ISO-005): atacante envia FindMissingBlobs com guessed digests; server vaza "chunk existe em qualquer tenant" se cross-tenant dedup ativo. Mitigação: tenant-scoped strict; cross-tenant disabled default.
- **Dedup ratio inflation** (false positive): bug em chunker BLAKE3 → digests duplicados artificialmente; ratio measurement falso. Mitigação: property test 10k iter cobrindo determinism (S-05 inheritance).
- **D1 index size explosion**: 30M chunks per tenant × 32-byte digest + ~24 bytes overhead = ~1.7 GB `chunks` table size; abaixo 10 GB D1 limit per shard mas merece monitoring. Mitigação: alert metric `corelink.d1.chunks.size_bytes` SEV-2 > 80% (consistent com ADR-0040 multipart D1 sharding trigger).

## 3. Customer Impact & Journey

**Persona 1 — Bazel/Buck2 build engineer**: invisible direct API; observable via `corelink_dedup_ratio{type=chunk}` metric (forward DASH-DEDUP em WI-S07-005); customer dashboard "bytes saved this month" forward S-16.

**Persona 2 — Docker registry user**: `docker push image:v2` after `image:v1` already cached → most layer chunks already exist; FindMissingBlobs returns absent layer chunks only; skip-upload existing → **measured bandwidth saved**.

**Persona 3 — DevOps reviewing dedup ratio**: DASH-DEDUP shows ratio per-tenant + per-tier; anomaly detection (ratio drop > 30% WoW) triggers SEV-3 (forward WI-S07-005).

**SLA addendum**:
- `chunk_exists` p99 ≤ 2ms (D1 index lookup).
- `find_missing` p99 ≤ 50ms @ 250-chunk batch.
- Dedup ratio target ≥ 2.5× sustained 7d staging (per-tenant aggregate).
- INV-DEDUP-CONSISTENCY 0 violations (UNIQUE constraint enforced).

## 4. Capability Mapping

- **CAP-DEDUP-001** (Intra-tenant chunk dedup) — IMPLEMENTA primary index + lookup.
- **CAP-DEDUP-002** (Dedup ratio metric) — IMPLEMENTA emit (consumed by WI-S07-005 dashboard).
- Trace: `data_model.md §4.1` + `WI-S05-004 §6.1 chunks table` (PK + refcount; canonical dedup table) + `security_model.md CTRL-ISO-005` + `invariant_registry.md INV-DEDUP-CONSISTENCY`.

## 5. Tipo

Indexed lookup library + REAPI handler; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-dedup/` module** — DedupIndex trait + D1 impl + tests.
2. **NO new D1 migration** (Lote 10.7bis P0-1 fix): `chunks` table from S-05 WI-S05-004 §6.1 lines 56-101 already has `PRIMARY KEY (tenant_id, chunk_digest)` which IS the dedup uniqueness constraint. Reverse lookup index `idx_manifest_chunks_tenant_chunk` (line 128) already exists for "which blobs reference chunk X" queries. **Original Lote 10.7 spec proposed `CREATE UNIQUE INDEX idx_manifest_chunks_tenant_digest ON manifest_chunks(tenant_id, chunk_digest)` — this was WRONG**: `manifest_chunks` is a per-blob ordered list (PK includes blob_digest); same chunk_digest legitimately appears in multiple blobs' manifests; UNIQUE constraint would reject second blob's INSERT, breaking dedup entirely. Migration cancelled.
3. **`chunk_exists` lookup** (uses `chunks` table PK): O(log N) D1 SELECT `WHERE tenant_id = ? AND chunk_digest = ? AND deleted_at IS NULL LIMIT 1`; returns bool. The `deleted_at IS NULL` predicate ensures soft-deleted chunks (S-06 grace window 72h) are treated as missing — forcing client to re-upload, which acts as undelete (re-INSERT increments refcount; `deleted_at` reset). Crypto SME advisory note: this semantic chosen for safety (skip-upload of soft-deleted = race with physical delete).
4. **`find_missing` batch** (uses `chunks` table): D1 SELECT `WHERE tenant_id = ? AND chunk_digest IN (?, ?, ...) AND deleted_at IS NULL`; input chunked at 250 per query (D1 batch ≤250 lesson); returns Vec<Digest> of NOT-found (set difference em caller). Uses `chunks` PK for O(log N) per lookup; total `find_missing` cost = O(K log N) where K = batch size.
5. **REAPI v2 FindMissingBlobs handler** em `crates/corelink-worker/src/handlers/find_missing_blobs.rs`:
   - gRPC service signature per REAPI v2 spec: `rpc FindMissingBlobs(FindMissingBlobsRequest) returns (FindMissingBlobsResponse)`.
   - Tower auth_stack (S-03 WI-S03-003) extracts TenantCtx.
   - Body decode → list of `Digest` (typed; algo + hash bytes).
   - Invokes `DedupIndex::find_missing(tenant_ctx, digests)`.
   - Returns `FindMissingBlobsResponse { missing_blob_digests: Vec<Digest> }`.
6. **CTRL-ISO-005 cross-tenant gate**:
   - Config struct `DedupConfig { cross_tenant_enabled: bool /* default false */ }`.
   - All DedupIndex methods MUST receive TenantCtx; cross-tenant calls return `Err(CrossTenantBlocked)`.
   - **CI grep gate** (Lote 10.7bis P1-2 fix; clippy::disallowed_method is path-based not content-based, infeasible para SQL string inspection): `! grep -rn -E 'FROM\s+chunks\s+WHERE\s+(?!.*tenant_id)' src/ tests/` em CI workflow forbids `chunks` SELECT without `tenant_id` filter; alternative: AST-grep custom rule on `sqlx::query!` macro arguments OR cargo-spellcheck regex rule.
7. **Métricas**:
   - `corelink.dedup.lookup_total{result=hit|miss}` (counter).
   - `corelink.dedup.lookup_duration_us` (histogram; SLO ≤ 2ms p99).
   - `corelink.dedup.find_missing_total{tenant_id}` (counter).
   - `corelink.dedup.find_missing_duration_ms` (histogram; SLO ≤ 50ms p99 @ 250-batch).
   - `corelink.dedup.bytes_saved_total{tenant_id}` (counter; sum of chunk.size_bytes for hit responses; consumed by DASH-DEDUP).
   - `corelink.dedup.cross_tenant_blocked_total` (counter; alert > 0 = security concern).
8. **Property tests** (10k iter PR; 100k nightly):
   - `prop_dedup_idempotent`: same chunk_digest queried 1k times → same result.
   - `prop_dedup_tenant_isolation`: 1k queries different tenants; same chunk_digest; assert independent results (no cross-tenant leak).
   - `prop_find_missing_batch_chunked`: input 1k digests; assert chunked 250-per-query; result equivalent to 1k independent lookups.
   - `prop_dedup_consistency`: insert same `(tenant_id, chunk_digest)` row N times → INSERT ON CONFLICT increments refcount to N; PK enforces 1:1 (tenant_id, chunk_digest) → chunk_body. (Lote 10.7bis P0-1 fix: original spec incorrectly framed as "UNIQUE rejects duplicate"; correct dedup semantic é refcount increment.)
   - `prop_cross_tenant_blocked`: attempt cross-tenant query → `Err(CrossTenantBlocked)` always.
9. **Chaos suite** (≥ 6 STANDARD lane; SOTA aim ≥ 8):
   - 1. D1 index missing (drop test) → fallback to scan; alert SEV-2.
   - 2. Cross-tenant query attempt → blocked + audit emit.
   - 3. Batch size > 250 → `Err(BatchTooLarge)` + 400.
   - 4. D1 throttle on lookup → exponential backoff retry.
   - 5. Index size > 80% D1 limit → alert SEV-2; sharding plan triggered.
   - 6. Duplicate INSERT same (tenant_id, chunk_digest) → ON CONFLICT bumps refcount (canonical dedup semantic per Lote 10.7bis P0-1; aligns prop_dedup_consistency property at §6.X above; was incorrectly framed as 409 reject in original spec — duplicate is normal dedup hit, not error).
   - 7. FindMissingBlobs with 0 input → returns empty Vec (not error).
   - 8. FindMissingBlobs with 1k input → chunked 4×250 batches; all results aggregated.

### 6.2 Out-of-scope (deferred)

- Cross-tenant dedup (CAP-DEDUP-* extension; ADR + BYOE S-14 forward).
- Content-defined chunking adaptive (FastCDC-PCI variant; out-of-scope per sprint contract §10).
- Delta compression (research-grade; pós-GA).
- Dedup metrics dashboard (delegate WI-S07-005).

## 7. Anti-Scope

- ❌ Cross-tenant dedup query path (CTRL-ISO-005 violation).
- ❌ Skip TenantCtx (multi-tenant injection vector).
- ❌ Plain `chunks` SELECT without `tenant_id` filter (custom CI grep gate enforces).
- ❌ D1 batch > 250 rows per query (Lote 10.5bis lesson).
- ❌ Skip property test for tenant isolation.
- ❌ Cache dedup results em local memory without TTL (stale read on chunk evict).
- ❌ Hard-coded batch limit (config-driven; default 250).

## 8. Acceptance Criteria (Gherkin) — 8 scenarios

```gherkin
Feature: Dedup index + FindMissingBlobs

  Scenario: chunk_exists hit (chunk already in tenant; uses `chunks` table PK)
    Given chunks table contains row (tenant=T, chunk_digest=D, refcount≥1, deleted_at IS NULL)
    When chunk_exists(T, D) called
    Then SELECT 1 FROM chunks WHERE tenant_id=T AND chunk_digest=D AND deleted_at IS NULL LIMIT 1
    Then returns Ok(true) em ≤ 2ms p99 (PK lookup O(log N))
    And metric corelink.dedup.lookup_total{result=hit} += 1

  Scenario: chunk_exists miss (chunk not in tenant; OR soft-deleted in grace)
    Given chunks does NOT contain (tenant=T, chunk_digest=D) OR row exists with deleted_at IS NOT NULL (S-06 grace)
    When chunk_exists(T, D) called
    Then returns Ok(false)
    Then client must re-upload (acts as undelete via INSERT ON CONFLICT incrementing refcount)
    And metric corelink.dedup.lookup_total{result=miss} += 1

  Scenario: find_missing returns absent-only (uses `chunks` table)
    Given tenant T has chunks [D1, D2, D3] em chunks table (refcount≥1, deleted_at IS NULL each)
    Given client requests find_missing(T, [D1, D4, D5])
    When handler invoked
    Then SELECT chunk_digest FROM chunks WHERE tenant_id=T AND chunk_digest IN (D1,D4,D5) AND deleted_at IS NULL
    Then result set = [D1] (present subset)
    Then returns Ok([D4, D5])  // set difference: input \ present
    And bytes_saved_total += sum(D1 size_bytes for refcount-fresh chunks)

  Scenario: find_missing chunked at 250
    Given client requests find_missing(T, 1000 digests)
    When handler invoked
    Then 4 D1 SELECTs of 250 each (NOT a single SELECT of 1000)
    And aggregated result returned em ≤ 200ms p99

  Scenario: Cross-tenant query blocked (CTRL-ISO-005)
    Given config.dedup.cross_tenant_enabled = false
    Given attacker issues query with tenant_id=T2 but TenantCtx says T1
    When middleware extracts TenantCtx (T1)
    Then query uses T1 (NOT T2 from request body)
    And tenant T2's chunks invisible to T1's query

  Scenario: Duplicate chunk INSERT increments refcount (NOT rejected; correct dedup semantic)
    Given (tenant=T, chunk_digest=D) already em chunks table com refcount=1
    When second blob B2 references chunk D via SplitBlob (S-05); INSERT ON CONFLICT into chunks
    Then PK constraint hit; ON CONFLICT increments refcount to 2 (sqlx UPSERT semantic)
    Then INV-DEDUP-CONSISTENCY preserved (chunk_body 1:1 cripto-grade via BLAKE3; refcount = N referencing manifests)
    Then NO audit anomaly (legitimate dedup hit; expected behavior)
    Note: this scenario clarifies why UNIQUE INDEX em manifest_chunks (original Lote 10.7 spec) was wrong — manifest_chunks INSERT CAN have multiple rows per chunk_digest (one per blob); chunks INSERT ON CONFLICT increments refcount instead.

  Scenario: FindMissingBlobs REAPI v2 conformance
    Given REAPI v2 client sends FindMissingBlobsRequest with 100 digests
    When handler invoked via gRPC
    Then response = FindMissingBlobsResponse { missing_blob_digests: Vec<Digest> }
    And response per REAPI v2 spec field types (algo + hash bytes)

  Scenario: Batch > 250 rejected
    Given find_missing input 1001 digests in single batch (NOT chunked by caller)
    When handler invoked
    Then returns Err(BatchTooLarge { got: 1001, limit: 250 })  // single-batch limit
    But the trait impl chunks internally — this scenario applies to direct sqlx access (developer error catch)
```

## 9. Design Decisions

- 9.1: D1 UNIQUE INDEX on `(tenant_id, chunk_digest)` — O(log N) lookup; enforces INV-DEDUP-CONSISTENCY.
- 9.2: CTRL-ISO-005 cross-tenant gate via config flag (default false); ADR-required to enable.
- 9.3: D1 batch ≤ 250 rows per `find_missing` query (Lote 10.5bis lesson absorbed).
- 9.4: TenantCtx-only enforcement (Lote 10.4bis lesson).
- 9.5: BLAKE3-256 chunk_digest inherited from S-05 (collision-resistant ≥128-bit).
- 9.6: REAPI v2 FindMissingBlobs RPC compliance (NOT custom CoreLink RPC).
- 9.7: Metrics emit per Prometheus convention (counter + histogram).
- 9.8: Property test 10k iter PR; 100k nightly.

## 10. Completeness Criteria SOTA

- [ ] **10.s07.001.1** Pre-existing S-05 `chunks` PK schema verified deploy-compatible (Lote 10.7bis P0-1: NO new migration; PK lookup `(tenant_id, chunk_digest)` already in WI-S05-004); `EXPLAIN QUERY PLAN` confirms index hit on chunk_exists query.
- [ ] **10.s07.001.2** `chunk_exists` p99 ≤ 2ms criterion benchmark green.
- [ ] **10.s07.001.3** `find_missing` p99 ≤ 50ms @ 250-batch criterion benchmark green.
- [ ] **10.s07.001.4** Property tests 5 × 10k green; 100k nightly green sustained 7d.
- [ ] **10.s07.001.5** Chaos suite 8 scenarios green.
- [ ] **10.s07.001.6** REAPI v2 FindMissingBlobs conformance test green (against bazelbuild/remote-apis test fixtures).
- [ ] **10.s07.001.7** CTRL-ISO-005 enforcement: 100 cross-tenant attack attempts blocked; audit emit verified.
- [ ] **10.s07.001.8** Métricas (6) emitted; cross_tenant_blocked_total alerts SEV-2 if > 0.
- [ ] **10.s07.001.9** Cargo-audit + cargo-deny + clippy clean; **CI grep gate** (Lote 10.7bis P1-2 fix) forbids `chunks` SELECT sem tenant_id (replaces infeasible clippy::disallowed_method claim).
- [ ] **10.s07.001.10** Cost regression gate: per-lookup ≤ $0.0000003 (D1 read share).
- [ ] **10.s07.001.11** Dedup ratio measurable: ≥ 2.5× sustained 7d staging em ≥ 3 tenants Docker workload.

## 11. DoD

- [ ] Module compila + integration tests green.
- [ ] All Gherkin scenarios green.
- [ ] Property tests 10k green.
- [ ] Pre-existing S-05 `chunks` PK schema verified in staging (Lote 10.7bis P0-1: NO new migration needed).
- [ ] FindMissingBlobs REAPI v2 conformance green.
- [ ] Métricas em DASH-DEDUP forward (consumed by WI-S07-005).
- [ ] Architect + AppSec + SRE reviews.

## 12. Invariants Validated

- **INV-DEDUP-CONSISTENCY** (HIGH, registry §3.12 L162 — already exists; Lote 10.7-tris cycle 6 section ref fix): 1:1 (tenant_id, chunk_digest) → chunk_body enforced by `chunks` PK from S-05.
- **INV-TENANT-ISOLATION** (CRITICAL, registry §3.1 + TLA+ tenant_isolation.tla): tenant_id scope strict; cross-tenant blocked.
- **INV-CAS-IMMUTABILITY** (CRITICAL, registry §3.2 L86 — Lote 10.7-tris cycle 6 section ref fix; was incorrectly §3.3): dedup is metadata-only; chunk body untouched.
- **INV-NEG-CACHE-MONOTONIC** (HIGH, registry §3.6): not directly applicable (dedup is positive cache); consistent.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Dedup module | `crates/corelink-dedup/` | Rust |
| FindMissingBlobs handler | `crates/corelink-worker/src/handlers/find_missing_blobs.rs` | Rust |
| ~~D1 migration~~ | ~~`migrations/00X_dedup_index.sql`~~ — NOT NEEDED (Lote 10.7bis P0-1: pre-existing S-05 `chunks` PK suffices) | — |
| Property tests | `crates/corelink-dedup/tests/prop_dedup.rs` | Rust |
| Chaos suite | `tests/chaos_dedup.rs` | Rust |
| REAPI conformance integration | `tests/reapi_findmissing_conformance.rs` | Rust |

## 14. Quality Standards SOTA

- 14.s07.001.1: Zero unsafe; zero unwrap em production paths.
- 14.s07.001.2: rustdoc 100% public API.
- 14.s07.001.3: Test coverage ≥ 90% module.
- 14.s07.001.4: Latency: chunk_exists ≤ 2ms p99; find_missing ≤ 50ms p99 @ 250-batch.
- 14.s07.001.5: SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`).
- 14.s07.001.6: Métricas (6 §6.1.7 enumerated).
- 14.s07.001.7: Memory bounded ≤ 100 KiB stack per request.
- 14.s07.001.8: Cost regression gate per-lookup ≤ $0.0000003; per-find_missing-batch ≤ $0.000005.
- 14.s07.001.9: CI grep gate (Lote 10.7bis P1-2) enforces tenant_id em `chunks` queries (CI fail without).
- 14.s07.001.10: D1 batch ≤ 250 row constraint enforced em find_missing (Lote 10.5bis lesson).

## 15. Chaos Experiments (8 — STANDARD floor 6 + 2 margin)

§6.1.9 enumerated.

## 16. PRR

STANDARD lane — sprint review (5 sign-offs); not full HIGH_RISK PRR.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + DedupIndex trait | 1 |
| ST-002 | Verify pre-existing S-05 `chunks` PK deploy-compatibility (Lote 10.7bis P0-1: NO new migration; replaces removed migration sub-task) | 0.5 |
| ST-003 | chunk_exists + find_missing impl + sqlx prepared | 3 |
| ST-004 | FindMissingBlobs REAPI handler | 3 |
| ST-005 | CTRL-ISO-005 cross-tenant gate + custom clippy lint | 2 |
| ST-006 | Métricas (6) emit | 1.5 |
| ST-007 | Property tests (5 × 10k) | 3 |
| ST-008 | Chaos suite (8) | 2.5 |
| ST-009 | REAPI conformance test | 1 |
| ST-010 | Architect + AppSec review iter | 1.5 |

**Total**: ~20h. **PERT** O=18h M=20h P=28h: **~21h** (matches sprint contract estimate 20h).

## 18. Dependencies

- Hard: S-05 SEALED (`chunks` table + `manifest_chunks` table base); S-03 WI-S03-003 SEALED (TenantCtx middleware).
- Soft: WI-S07-005 (dashboard consumes metrics; can stub interim).

## 19. Effort PERT: ~21h. ## 20. Time-boxing: 28h hard limit.

## 21. Observability

6 metrics §6.1.7. Trace span `dedup.{lookup, find_missing}`. Custom log `corelink.dedup.lookup{tenant_id, chunk_digest_hex8, result, latency_us}` (debug-level; sampled 1% prod).

## 22. Cost Analysis

- Per-lookup: ~$0.0000003 (D1 read share).
- Per-find_missing-batch (250): ~$0.000005 (250 × $0.0000003 + audit emit ~$0.0000005).
- TCO 12m: 5 regions × 1k tenants × 100 lookups/dia × 365 dias × $0.0000003 = ~$55/yr. Marginal.
- Index storage: 30M chunks × ~56 bytes per index entry = ~1.7 GB; em D1 ~$0.75/mo per shard.

## 23. API Contract

- Public: `DedupIndex` trait + `Digest`, `DedupConfig`, `DedupError` types; `#[non_exhaustive]` em error enum.
- gRPC: REAPI v2 `FindMissingBlobs` (no CoreLink-custom RPC; spec compliance).

## 24. Post-mortem Hooks

- Cross-tenant dedup leak detected (cross_tenant_blocked > 0 OR audit anomaly) → CRITICAL post-mortem + Security review.
- Dedup ratio drops > 50% WoW sustained → 5-Why + chunker bug investigation.
- INV-DEDUP-CONSISTENCY violated (UNIQUE constraint bypass somehow) → SEV-1 + D1 schema audit.

## 25. Rollback / Recovery

- Rollback: NO new index introduced em Lote 10.7bis P0-1 fix; uses `chunks` table PK from S-05. Rollback = revert handler code (FindMissingBlobs); dedup lookup falls back to existing S-05 patterns (chunks PK + manifest_chunks reverse lookup).
- Recovery: no rebuild needed — `chunks` PK is base storage (S-05 canonical); refcount derivable via reconcile S-06 WI-S06-005 if drift detected; no data loss.
- RTO ≤ 15min; RPO 0 (index reproducible from base table).

## 26. Security & Privacy

**STRIDE delta**:
- **I (Information disclosure)**: cross-tenant existence oracle blocked via CTRL-ISO-005 + tenant-scoped INDEX.
- **T (Tampering)**: UNIQUE constraint enforces 1:1; tampering via SQL injection blocked via sqlx prepared.
- **D (DoS)**: D1 batch ≤ 250 limits single-request cost; FindMissingBlobs rate limit S-08 forward.
- **R (Repudiation)**: lookups audit-logged (sampled 1%); find_missing requests fully audit-logged.

**LINDDUN delta**:
- **L (Linkability)**: cross-tenant dedup OFF prevents cross-tenant blob existence linkability.
- **I (Identifiability)**: chunk_digest hex first 8 chars only em logs (BLAKE3-256 truncated; not personal data).
- **N (Non-repudiation)**: REAPI conformance includes audit per Bazel client_id.
- **D (Detectability)**: prevented per CTRL-ISO-005.

## 27. Knowledge Transfer

Tech talk (1h): "S-07 Dedup Architecture: BLAKE3 + UNIQUE Index + REAPI v2 FindMissingBlobs"; doc `docs/dev/dedup-overview.md`; onboarding test 5 questions: BLAKE3 collision-resistance, INV-DEDUP-CONSISTENCY rationale, CTRL-ISO-005 enforcement, REAPI v2 FindMissingBlobs spec compliance, D1 batch ≤250 lesson.

## 28. Risk Register (8-row)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | UNIQUE constraint violation false positive (chunker non-determinism bug) | L | H | MEDIUM | L | LOW | Property test prop_chunker_determinism (S-05 inheritance) |
| R-002 | D1 index size > 80% D1 10 GB limit | M | M | MEDIUM | M | LOW | Monitor metric; sharding plan ADR-0040 forward |
| R-003 | Cross-tenant existence oracle leak | L | M | HIGH | L | LOW | CTRL-ISO-005 + custom clippy lint + audit + chaos test |
| R-004 | Lookup latency > 2ms p99 (D1 throttle) | M | L | MEDIUM | L | LOW | Backoff retry; circuit breaker; cache miss tolerated |
| R-005 | FindMissingBlobs REAPI conformance regression | L | M | MEDIUM | L | LOW | Conformance test em CI nightly |
| R-006 | Dedup ratio < 2.5× target em customer workload real | M | H | LOW (metric) | L | LOW | Baseline 3 workloads pre-claim public; tune chunker if low |
| R-007 | Cross-tenant flag accidentally enabled em prod | L | L | CRITICAL | L | LOW | Default false em config; CI grep gate; ADR required |
| R-008 | TenantCtx middleware bypass somehow | L | M | CRITICAL | L | LOW | TenantCtx-only Lote 10.4bis lesson; integration test; clippy lint |

## 29. Review Checkpoints

D+0 design (Architect); D+2 AppSec (CTRL-ISO-005); D+5 code review (Engineer peer); D+7 sprint review (5 sign-offs).

## 30. Sign-off (STANDARD 5)

| # | Role | Status |
|---|---|---|
| 1 | Owner / Final Approver (Gustavo) | _pending_ |
| 2 | Engineer (peer) | _TBD; mandatory_ |
| 3 | QA | _TBD; mandatory — chaos + property test green_ |
| 4 | AppSec | _TBD; mandatory — CTRL-ISO-005 + cross-tenant gate verified_ |
| 5 | SRE | _TBD; mandatory — D1 index size monitoring + alerting_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.7) | Criação WI-S07-001; SOTA pós-Lote 10.6-tris lessons absorbed: TenantCtx-only; D1 batch ≤250; CHECK inline; custom clippy lint para tenant_id enforcement; CTRL-ISO-005 cross-tenant gate default false. **DEFEITO INTRODUZIDO**: spec proposed UNIQUE INDEX em `manifest_chunks(tenant_id, chunk_digest)` — incorrect; would break dedup since same chunk_digest legitimately appears em multiple blobs' manifests. |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.7bis Agent R4 P0-1 + Sonnet R5 P0-1 fix) | **P0-1 corrected**: dedup table is `chunks` (S-05 WI-S05-004 §6.1; PK `(tenant_id, chunk_digest) + refcount` purpose-built); UNIQUE INDEX em `manifest_chunks` removed (would break dedup). Migration cancelled (NO new schema needed). `chunk_exists`/`find_missing` queries `chunks` table com `deleted_at IS NULL` predicate (S-06 grace window respect). Duplicate INSERT semantic corrected: PK ON CONFLICT increments refcount (não rejects). **P1-2 corrected**: clippy::disallowed_method claim replaced com CI grep gate (`! grep -rn -E 'FROM\s+chunks\s+WHERE\s+(?!.*tenant_id)' src/ tests/`) — clippy::disallowed_method is path-based not content-based; infeasible para SQL string inspection. Cross-WI consistency: usa same `chunks` table que S-05 + S-06 GC. |

## 32. Anti-patterns evitados

- ❌ Cross-tenant dedup query (CTRL-ISO-005); ❌ manifest_chunks SELECT sem tenant_id; ❌ D1 batch > 250; ❌ Trust client tenant_id; ❌ Hard-coded batch limit; ❌ Skip property test tenant isolation; ❌ Cache dedup local sem TTL; ❌ Skip TenantCtx middleware.

---

**Fim WI-S07-001.** Próximo: WI-S07-002 (Eviction worker LRU + TTL + quota trigger).
