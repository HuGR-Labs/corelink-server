---
id: "WI-S06-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-011", "FF-HR-005"]
parent: "S-06"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s06", "gc", "mark", "scan", "reachable-set", "mark-started-at", "high-risk"]
---

# WI-S06-002 — Mark Phase: Multi-Pass D1 Scan (`blob_meta` + `ac_meta` + `manifest_chunks`) + Batching 250 rows/iter + `mark_started_at_ms` Atomic Capture (INV-GC-004 Anchor) + Jitter 100ms + `gc_candidates` Output Table + Mark p99 ≤ 10 min @ 1M Blobs Benchmark

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-06](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S06-002 |
| Título | Mark phase impl: multi-pass scan D1 (blob_meta + ac_meta + manifest_chunks); batched 250 rows/iter (Lote 10.4bis lesson D1 100KB limit); jitter 100ms entre batches; `mark_started_at_ms` atomic capture at phase start (INV-GC-004 anchor; TLA+ `gc_correctness.tla` obligation); reachable set computado como `union(blob_meta.refcount > 0, ac_meta.outputs, manifest_chunks)`; `gc_candidates` output table com candidate set; mark p99 ≤ 10 min @ 1M blobs benchmark |
| Sprint | S-06 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-011 (mark fault → reachable set incompleto → INV-GC-001 violation cascade), FF-HR-005 (controle integridade dados) |

## 1. Intent

Mark phase produz **reachable set** definitivo do tenant; sweep phase (WI-S06-003) só age sobre `gc_candidates` (não-reachable). Bug em mark = reachable blob potencialmente incluído em candidates → sweep deletes → INV-GC-001 violation:

```rust
// File: crates/corelink-gc/src/mark.rs

#![forbid(unsafe_code)]

pub use crate::mark::{MarkPhase, MarkResult, GcCandidate, ReachableSetSnapshot};
pub use crate::error::MarkError;

#[async_trait]
pub trait MarkPhase: Send + Sync {
    /// Execute mark phase for given tenant + region.
    /// Atomically captures mark_started_at_ms BEFORE any scan (INV-GC-004 anchor).
    /// Multi-pass scan D1 batched 250 rows/iter com jitter 100ms.
    /// Output: gc_candidates rows for sweep phase consumption.
    async fn execute(
        &self,
        gc_run: &mut GcRun,                         // mutated: mark_started_at_ms set; counters updated
        tenant_id: &TenantId,
        region: &Region,
    ) -> Result<MarkResult, MarkError>;
}

pub struct MarkResult {
    pub mark_started_at_ms: u64,                   // INV-GC-004 anchor; persisted em gc_run
    pub blobs_scanned_count: u64,
    pub reachable_blobs_count: u64,
    pub candidates_count: u64,
    pub mark_duration_ms: u64,
    pub batches_processed: u32,
    pub d1_throttles_observed: u32,                // backoff signal
}

pub struct GcCandidate {
    pub tenant_id: TenantId,
    pub digest: BlobDigest,                         // BLAKE3-256 hex
    pub mark_started_at_ms: u64,                    // INV-GC-004 anchor (denormalized; aligned with gc_run)
    pub mark_run_id: u64,                           // FK to gc_run
    pub blob_size_bytes: u64,                       // for bytes_reclaimed projection
    pub blob_last_referenced_at_ms: u64,            // mark phase doesn't touch (informational)
    pub status: CandidateStatus,                    // 'candidate' (initial) | 'swept' | 'physically_deleted' | 'protected_re_ref'
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateStatus {
    Candidate,                                      // mark identified; sweep pending
    Swept,                                          // sweep soft-deleted (deleted_at set); grace pending
    PhysicallyDeleted,                              // R2+D1 row purged
    ProtectedReRef,                                 // INV-GC-004 caught: ac.created_at >= mark_started_at_ms
}

#[derive(thiserror::Error, Debug)]
pub enum MarkError {
    #[error("mark_started_at_ms already set; cannot re-capture")]
    MarkAnchorAlreadySet,                           // idempotent re-run safety

    #[error("d1 throttle exhausted: {batches_failed} batches failed after backoff")]
    D1ThrottleExhausted { batches_failed: u32 },

    #[error("phase budget exceeded: {duration_ms}ms > {budget_ms}ms")]
    PhaseBudgetExceeded { duration_ms: u64, budget_ms: u64 },

    #[error("internal: {0}")]
    Internal(String),
}
```

```sql
-- File: migrations/007_gc_candidates.sql
-- Lote 10.4bis+10.5bis lessons applied.

CREATE TABLE IF NOT EXISTS gc_candidates (
  -- Tenant scope (PK component 1)
  tenant_id              TEXT     NOT NULL,

  -- Blob digest (PK component 2)
  digest                 TEXT     NOT NULL,

  -- INV-GC-004 anchor (denormalized from gc_run; sweep checks ac.created_at < mark_started_at_ms strict)
  mark_started_at_ms     INTEGER  NOT NULL,

  -- gc_run FK
  mark_run_id            TEXT     NOT NULL,

  -- Informational
  blob_size_bytes        INTEGER  NOT NULL,
  blob_last_referenced_at_ms INTEGER NOT NULL,

  -- Status tracking (sweep updates → physical_delete updates)
  status                 TEXT     NOT NULL DEFAULT 'candidate',

  -- Lifecycle timestamps
  created_at_ms          INTEGER  NOT NULL,
  swept_at_ms            INTEGER  NULL,
  physically_deleted_at_ms INTEGER NULL,
  protected_at_ms        INTEGER  NULL,

  -- Reason for protected status (forensic trail)
  protected_reason       TEXT     NULL,            -- e.g., "ac.created_at = T+1ms; mark_started_at = T"

  PRIMARY KEY (tenant_id, digest, mark_run_id),

  -- CHECK constraints inlined (Lote 10.4bis lesson)
  CHECK (status IN ('candidate', 'swept', 'physically_deleted', 'protected_re_ref')),
  CHECK (mark_started_at_ms >= created_at_ms - 60000),  -- mark_started_at within 60s of created_at
  CHECK (blob_size_bytes >= 0)
);

-- Index: per-run + status (sweep phase consumes WHERE mark_run_id = X AND status = 'candidate')
CREATE INDEX IF NOT EXISTS idx_gc_candidates_run_status
  ON gc_candidates(mark_run_id, status);

-- Index: per-tenant + status (analytics; "how many candidates pending sweep for tenant T")
CREATE INDEX IF NOT EXISTS idx_gc_candidates_tenant_status
  ON gc_candidates(tenant_id, status);

-- Index: protected re-ref forensics (sustained drift = INV-GC-004 violation alert)
CREATE INDEX IF NOT EXISTS idx_gc_candidates_protected
  ON gc_candidates(tenant_id, protected_at_ms)
  WHERE status = 'protected_re_ref';
```

**Cripto-driven invariants enforced**:

1. **`mark_started_at_ms` atomic capture**: SQL `UPDATE gc_run SET mark_started_at_ms = unixepoch_ms() WHERE run_id = X AND mark_started_at_ms IS NULL`. Idempotent; only first call sets; subsequent reads observe stable value. **TLA+ obligation**: `gc_correctness.tla` `MarkPhaseStart` action.

   **Lote 10.6bis P0-3 fix: UPDATE-commit-before-scan ordering pinned**. Per TLA+ `GCMarkStart` action requires atomic precedence over any `GCMarkStep`; previously Rust spec only pinned task ordering, não D1 commit boundary. Concrete impl:
   ```rust
   // Step A: UPDATE + AWAIT COMMIT (synchronous; D1 commit ack received)
   let result = sqlx::query("UPDATE gc_run SET mark_started_at_ms = ? WHERE run_id = ? AND mark_started_at_ms IS NULL")
       .bind(unix_ms_now()).bind(run_id)
       .execute(&d1).await?;        // .await synchronizes; commit observed atomically before next line

   // Step B: ONLY AFTER A's commit acks → begin reachable set scan
   if result.rows_affected() == 0 {
       // Already set by prior crashed run; idempotent resume; read existing value.
       let existing: i64 = sqlx::query_scalar("SELECT mark_started_at_ms FROM gc_run WHERE run_id = ?")
           .bind(run_id).fetch_one(&d1).await?;
       gc_run.mark_started_at_ms = Some(existing as u64);
   } else {
       gc_run.mark_started_at_ms = Some(unix_ms_now());
   }
   // INVARIANT: at this point, ANY observer of D1 sees mark_started_at_ms; concurrent UpdateActionResult
   // happening AFTER step A's commit ack will have ac.created_at > mark_started_at_ms (TLA+ obligation).
   self.execute_3_pass_scan(gc_run.mark_started_at_ms.unwrap()).await?;
   ```

   **TLA+ ↔ Rust action mapping table** (Lote 10.6bis P0-3 fix; faithfully bridges TLA+ obligations to code):

   | TLA+ Action (gc_correctness.tla) | Rust impl invocation site | Pre-condition observable | Post-condition observable |
   |---|---|---|---|
   | `MarkPhaseStart` | `mark.rs::execute()` step A | `gc_run.mark_started_at_ms IS NULL` | D1 commit ack: `gc_run.mark_started_at_ms = T` |
   | `GCMarkStep(blob)` | `mark.rs::execute_3_pass_scan()` per batch | `gc_run.mark_started_at_ms = T` (from step A commit) | reachable set ⊇ blob if reachable |
   | `UpdateActionResult(ac, blob)` | `WI-S04-001 handler` (concurrent) | (any) | `ac.created_at = T_ac` (real-time clock) |
   | `SweepStep(blob)` | `WI-S06-003 sweep::execute()` per candidate | `gc_run.phase = sweep`; `mark_started_at_ms = T` | `EXISTS(ac WHERE ac.blob_refs CONTAINS blob AND ac.created_at >= T)` → ProtectedReRef; ELSE → SoftDelete |
   | `InvGCReRefProtected` | TLA+ invariant + Rust property test 100k race (WI-S06-006) | (always) | reachable blobs (with concurrent UpdateAR) NEVER deleted |
2. **Reachable set união**: `union(blob_meta WHERE refcount > 0, ac_meta.outputs[*], manifest_chunks WHERE blob_digest IN cas_blobs)`. Tenant-scoped strict (Lote 10.4bis lesson).

   **Mark phase ac_meta scan SQL** (Lote 10.6bis P0-6 fix: previously not published; now explicit using json_each canonical idiom; same pattern as INV-GC-004 sweep query em WI-S06-003):
   ```sql
   -- Pass 2: ac_meta.outputs scan (extracts all digests referenced em ANY ac_meta entry for tenant)
   SELECT DISTINCT j.value AS digest FROM ac_meta a, json_each(a.blob_refs) j
   WHERE a.tenant_id = ?                              -- TenantCtx-only (Lote 10.4bis lesson)
     AND a.created_at >= ?                            -- snapshot lower bound (mark_started_at_ms - 24h grace)
   ORDER BY j.value
   LIMIT 250 OFFSET ?                                 -- Lote 10.4bis lesson D1 100KB / 250 rows batch
   ;
   -- Lote 10.6bis P0-1+P0-2 fix: json_each canonical (não LIKE '%digest%'); column `created_at` (não `_ms`).
   ```

   **Mark phase manifest_chunks scan SQL** (3-hop traversal):
   ```sql
   -- Pass 3: manifest_chunks → chunks → blob_digest reachability
   SELECT DISTINCT mc.chunk_digest AS digest
   FROM manifest_chunks mc
   WHERE mc.tenant_id = ?                             -- TenantCtx-only
     AND mc.created_at >= ?                           -- snapshot bound
   LIMIT 250 OFFSET ?
   ;
   ```

   **Mark phase blob_meta refcount scan SQL** (denormalized counter; reconcile detects drift WI-S06-005):
   ```sql
   -- Pass 1: blob_meta.refcount > 0 (live blobs)
   SELECT b.digest FROM blob_meta b
   WHERE b.tenant_id = ?                              -- TenantCtx-only
     AND b.refcount > 0
     AND b.deleted_at_ms IS NULL                      -- not soft-deleted
   LIMIT 250 OFFSET ?
   ;
   ```
3. **Multi-pass scan**: 3 distinct passes (blob_meta → ac_meta → manifest_chunks); each batched 250 rows (Lote 10.5bis lesson D1 100KB limit); jitter 100ms.
4. **D1 throttle adaptive**: backoff exponential se sustained 429; batch size halved temporary; circuit breaker se 10 batches consecutive throttled.
5. **mark_run_id FK rigor**: `gc_candidates.mark_run_id` references `gc_run.run_id`; NEVER stale (cleaned by sweep phase post-completion).

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

Mark phase é **single source of truth for reachable set**. Bug em scan = reachable blob misclassified como candidate → sweep deletes → INV-GC-001 violation. HIGH_RISK em N dimensões:

1. **Reachable set incompleto** (mark misses ac_meta scan; produces partial reachable union): blob B referenced ONLY via ac_meta.outputs miss-scanned; B added to candidates; sweep deletes; AC entry now references missing blob → cascade integrity violation (INV-AC-OUTPUTS-VALID já em registry §3.3 + §3.15). Mitigação: 3 passes mandatory (blob_meta + ac_meta.outputs + manifest_chunks); chaos test #1 simulates 1 pass missing → property test catches.

2. **`mark_started_at_ms` race** (two concurrent calls capture different timestamps): worker resumes after crash; second invocation overwrites mark_started_at_ms; INV-GC-004 anchor invalidated. Mitigação: SQL UPDATE WHERE mark_started_at_ms IS NULL atomic; `MarkAnchorAlreadySet` error if already set; idempotent re-run reads existing value; integration test asserts.

3. **D1 throttle cascade**: 1M blob scan × 250 rows/batch = 4000 batches × 100ms jitter = ~7 min minimum; D1 throttle adds latency; phase budget 10 min p99 (sprint contract §10.s06.1 SLO-FRESH-GC). Mitigação: adaptive batch size (halve on 429); backoff exponential; circuit breaker; benchmark CI 1M blobs ≤ 10 min p99.

4. **TenantCtx-only enforcement** (Lote 10.4bis lesson): all D1 queries `WHERE tenant_id = ctx.tenant_id`; sqlx prepared statements; clippy lint forbids `&str` SQL literals.

5. **Manifest chunks reachability** (S-05 multipart blobs): manifest_chunks rows reference chunks; chunks reference blobs (S-05 dedup). Mark must traverse manifest_chunks → chunks → blobs (3-hop). Mitigação: WI-S06-002 §6.1.X documents 3-hop traversal; integration test 100 multipart blobs reachable correctly.

6. **gc_candidates table size** (10M blobs candidate per run): scheduling 10M INSERT per cron tick = D1 batch overflow. Mitigação: only **non-reachable** rows inserted into gc_candidates (subset; typically << 10%); scan computes reachable, INSERT only candidates; bounded.

7. **Audit emission per batch**: outbox pattern (Lote 10.4bis: WI-S01-004 audit_outbox); `corelink.gc.mark.batch_processed` event; tampering detection daily verifier (S-09).

8. **Mark phase budget**: 10 min p99 @ 1M blobs (sprint contract §5.5). If exceeds, partial mark_started_at_ms anchor stale → sweep phase blocked. Mitigação: `PhaseBudgetExceeded` error → status=failed; manual re-run; SEV-2 alert.

**Atacante adversarial scenarios**:

- **Cross-tenant scan injection**: defense-in-depth; tenant_id NOT NULL + sqlx prepared.
- **mark_started_at_ms forgery** via SQL injection: prepared statements prevent.
- **Crafted manifest_chunks circular reference** (chunk → manifest → chunk): bounded traversal depth (per WI-S05-005 cycle detection pattern reuse); chaos test #4.
- **Refcount drift attack** (refcount manipulation forces wrong reachable computation): reconcile diário (WI-S06-005) detects; auto-fix small drifts.

**Risk justification HIGH_RISK**:

- **FF-HR-011**: mark fault → INV-GC-001 violation cascade.
- **FF-HR-005**: GC integrity control.
- **Reversibility**: mark fault detected pre-sweep via property test 100k; sweep doesn't fire if mark crashes; gc_candidates rollback safe.

13 sign-offs.

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev**: invisible (mark runs background); customer-visible only via SLO-FRESH-GC dashboard (forward S-09).

**Persona 2 — DevOps reviewing GC health**: `corelink.gc.mark.duration_ms` per region/tenant tracked; alert if > 15 min (1.5× SLO).

**Persona 3 — Security auditor**: reviews mark_started_at_ms anchor discipline (TLA+ obligation); reviews property test 100k race coverage (WI-S06-006 forward).

**SLA addendum**:
- Mark phase p99 ≤ 10 min @ 1M blobs (sprint contract §5.5; benchmark CI gate).
- mark_started_at_ms capture latency ≤ 100ms p99 (atomic UPDATE).
- D1 batch throttle backoff ≤ 5s p99 cumulative; circuit breaker if 10 consecutive.
- Reachable set computation deterministic (TLA+ verifies same input → same output).

## 4. Capability Mapping

- **CAP-GC-001** (Mark-and-sweep) — IMPLEMENTA primary mark side.
- Trace: `failure_modes.md FM-300/305` + `invariant_registry.md INV-GC-001/004` + `specs/tla/gc_correctness.tla`.

## 5. Tipo

Mark phase impl; HIGH_RISK; FF-HR-011 + FF-HR-005.

## 6. Escopo (compact)

### 6.1 In-scope

1. `crates/corelink-gc/src/mark/` module: scan logic + batching + jitter + mark_started_at_ms capture.
2. **Migration 007**: gc_candidates table (CHECK inline; partial UNIQUE forward S-07; Lote 10.5bis lesson).
3. **Multi-pass scan**: 3 passes (blob_meta + ac_meta + manifest_chunks) tenant-scoped strict.
4. **mark_started_at_ms atomic capture**: SQL UPDATE WHERE IS NULL; idempotent.
5. **Batching 250 rows/iter** (Lote 10.4bis lesson D1 100KB limit).
6. **Jitter 100ms entre batches**: avoid D1 throttle thundering herd.
7. **D1 throttle adaptive**: batch size halve on 429; circuit breaker.
8. **Reachable set computation**: union 3 sources; per-tenant strict.
9. **gc_candidates output**: only non-reachable inserted; mark_run_id FK to gc_run.
10. **Phase budget enforcement**: 10 min p99 @ 1M blobs; PhaseBudgetExceeded error if exceeds.
11. **Audit emission outbox** (WI-S01-004): mark.{phase_started, batch_processed, phase_completed, phase_failed} events.
12. **Métricas**:
    - `corelink.gc.mark.phase_started_total{region}`.
    - `corelink.gc.mark.batches_processed_total{region}`.
    - `corelink.gc.mark.duration_ms{region}` (histogram; SLO target ≤ 10 min p99).
    - `corelink.gc.mark.reachable_count{tenant_id, region}` (gauge).
    - `corelink.gc.mark.candidates_count{tenant_id, region}` (gauge).
    - `corelink.gc.mark.d1_throttle_observed_total{region}`.
    - `corelink.gc.mark.phase_budget_exceeded_total{region}` (alert if > 0).
13. **Property tests** (10k iter PR; 100k nightly):
    - `prop_mark_idempotent`: re-run mark on same `(tenant, region)` after crash → same reachable set + mark_started_at_ms preserved (NOT overwritten).
    - `prop_mark_started_at_atomic`: 1000 concurrent capture attempts; only first succeeds; others read existing value.
    - `prop_reachable_set_complete`: 1000 random tenant states (mix of reachable + orphan blobs); mark identifies all reachable correctly.
    - `prop_tenant_isolation_mark`: 1000 concurrent marks different tenants; no cross-tenant interference.
    - `prop_mark_d1_batch_bounded`: 100k random scan; max 250 rows/batch; jitter 100ms; budget < 10 min.
14. **Criterion benchmark** `bench_mark_1m_blobs`: target p99 ≤ 10 min single tenant single region.
15. **Cargo-fuzz harness**: `fuzz_mark_reachable_computation.rs` — 1h CI nightly; arbitrary blob_meta + ac_meta + manifest_chunks fixtures; assert no panic + reachable set always superset of true reachable.
16. **Chaos suite** (sprint contract HIGH_RISK ≥ 10):
    - 1. 1 pass missing (blob_meta only) → property test catches missing reachable.
    - 2. mark_started_at_ms double-capture attempt → idempotent (MarkAnchorAlreadySet).
    - 3. D1 throttle sustained → adaptive batch + backoff + circuit breaker.
    - 4. Manifest chunks circular reference → bounded traversal (cycle detection WI-S05-005 pattern reuse).
    - 5. Tenant size 5M blobs (5× SLO) → phase budget exceeded → SEV-2 alert.
    - 6. Worker crash mid-batch → resume from checkpoint; mark_started_at_ms preserved.
    - 7. Refcount drift mid-mark (concurrent UpdateActionResult) → property test asserts INV-GC-004 protection (sweep phase WI-S06-003).
    - 8. Cross-tenant query injection attempt → sqlx prepared rejects; integration test.
    - 9. gc_candidates table size > 10M rows → cleanup post-completion; chaos test asserts.
    - 10. Audit emission lag mid-batch → drain worker eventual consistency; integration test.

### 6.2 Out-of-scope

- Sweep phase (WI-S06-003).
- Physical delete (WI-S06-004).
- Reconcile (WI-S06-005).
- TLA+ CI gate integration (WI-S06-006).

## 7. Anti-Scope

- ❌ Single-pass scan (must be 3 passes).
- ❌ Skip jitter (D1 throttle storm).
- ❌ Hard-coded batch size > 250 (Lote 10.4bis 100KB limit).
- ❌ mark_started_at_ms non-atomic (race; INV-GC-004 anchor invalidated).
- ❌ Cross-tenant scan.
- ❌ Skip phase budget enforcement.
- ❌ Skip audit emission per batch.
- ❌ Trust client tenant_id (TenantCtx-only; lesson Lote 10.4bis).
- ❌ Sync emit audit em hot path (outbox pattern WI-S01-004).
- ❌ ALTER TABLE ADD CONSTRAINT chk_* (Lote 10.4bis lesson).
- ❌ BEGIN/COMMIT em migration (wrangler implicit; Lote 10.4bis).

## 8. Acceptance Criteria (Gherkin) (compact 12 scenarios)

```gherkin
Feature: Mark phase D1 multi-pass scan + mark_started_at_ms anchor

  Scenario: Mark phase happy path (1M blobs)
    Given tenant_A has 1M blobs em region_sam (mix of reachable + 100k orphan)
    When MarkPhase::execute(gc_run, tenant_A, sam)
    Then mark_started_at_ms = unix_ms_T captured atomically em gc_run
    And 3 passes complete (blob_meta + ac_meta + manifest_chunks)
    And reachable_count = 900k; candidates_count = 100k
    And gc_candidates rows inserted for 100k orphans (status='candidate')
    And metric corelink.gc.mark.duration_ms ≤ 10 min p99

  Scenario: mark_started_at_ms atomic (idempotent re-run)
    Given gc_run em phase=mark with mark_started_at_ms = T
    When worker crashes mid-mark
    When new worker resumes
    Then mark_started_at_ms preserved (NOT re-captured)
    And property test prop_mark_started_at_atomic green

  Scenario: Multi-pass scan reachable set complete
    Given tenant has blob B referenced ONLY via ac_meta.outputs (not blob_meta.refcount > 0)
    When mark phase scans
    Then 3 passes catch B em ac_meta scan
    And B NOT included em gc_candidates (correctly classified reachable)
    And property test prop_reachable_set_complete green

  Scenario: Tenant isolation
    Given 1000 concurrent marks different tenants
    When all execute
    Then no cross-tenant interference; each tenant's reachable set independent

  Scenario: D1 throttle adaptive
    Given D1 returns 429 sustained
    When mark phase batches
    Then batch size halved temporarily; backoff exponential
    And metric corelink.gc.mark.d1_throttle_observed_total += 1
    And circuit breaker if 10 consecutive throttled

  Scenario: Phase budget exceeded
    Given tenant has 5M blobs (5× SLO)
    When mark phase runs
    Then phase budget 10 min exceeded
    And error PhaseBudgetExceeded; status=failed
    And SEV-2 alert + manual re-run path

  Scenario: Manifest chunks reachability (S-05 multipart)
    Given tenant has chunked blob B with 25 manifest_chunks rows
    When mark phase 3-hop traverses (manifest_chunks → chunks → blob)
    Then B correctly reachable; NOT em gc_candidates

  Scenario: Manifest chunks cyclic reference (chaos #4)
    Given crafted circular reference (chunk → manifest → chunk)
    When mark phase traverses
    Then bounded depth detection rejects cycle (WI-S05-005 pattern reuse)
    And no infinite loop

  Scenario: Cross-tenant scan injection rejected
    Given attacker submits crafted SQL injection via PAT scope
    When mark phase queries
    Then sqlx prepared statement rejects
    And clippy custom lint forbids &str SQL literals at compile time

  Scenario: Worker crash mid-batch resume
    Given mark phase processed 500k of 1M blobs; crash at batch 2000
    When new worker resumes
    Then resumes at batch 2001 (checkpoint via gc_run.last_checkpoint_at_ms)
    And total mark identifies same reachable set (idempotent)

  Scenario: gc_candidates table size cleanup
    Given gc_candidates has 10M rows from prior runs
    When sweep phase completes (WI-S06-003 forward)
    Then gc_candidates cleaned WHERE mark_run_id < latest_run

  Scenario: Audit emission outbox
    Given mark phase processes batch
    When emit hook triggers
    Then audit_outbox INSERT (WI-S01-004 audit_outbox table; Lote 10.4bis lesson)
    And drain worker propagates eventually
```

## 9. Design Decisions (compact)

- **9.1** Multi-pass 3 scans (blob_meta + ac_meta + manifest_chunks) — comprehensive reachable set.
- **9.2** Batching 250 rows (Lote 10.4bis D1 100KB limit lesson).
- **9.3** Jitter 100ms (avoid D1 throttle thundering).
- **9.4** mark_started_at_ms atomic via SQL UPDATE WHERE IS NULL (TLA+ obligation; race-free).
- **9.5** D1 throttle adaptive: batch halve + backoff + circuit breaker.
- **9.6** Reachable set superset-safe (false reachable acceptable; false orphan unacceptable).
- **9.7** gc_candidates output table (separate from gc_run; sweep consumes).
- **9.8** Phase budget enforcement (PhaseBudgetExceeded; SEV-2 alert).
- **9.9** TenantCtx-only enforcement (Lote 10.4bis lesson).
- **9.10** ADR cross-ref: ADR-0042 (WI-S06-001) covers worker scheduler; no new ADR for mark phase.

## 10. Completeness Criteria SOTA

- [ ] **10.s06.002.1** Property tests 5 × 10k iter green; 100k nightly.
- [ ] **10.s06.002.2** Criterion benchmark `bench_mark_1m_blobs` ≤ 10 min p99 (sprint contract §5.5 SLO).
- [ ] **10.s06.002.3** Cargo-fuzz `fuzz_mark_reachable_computation` 1h CI nightly green; reachable always superset of true reachable.
- [ ] **10.s06.002.4** mark_started_at_ms atomic capture via SQL UPDATE WHERE IS NULL; integration test asserts idempotent.
- [ ] **10.s06.002.5** D1 throttle adaptive validated em chaos test (sustained 429 → batch halve + backoff + circuit breaker).
- [ ] **10.s06.002.6** Multi-pass 3 scans validated (1 pass missing chaos test catches).
- [ ] **10.s06.002.7** Manifest chunks 3-hop traversal validated (S-05 multipart integration).
- [ ] **10.s06.002.8** Tenant isolation property test 1000 concurrent green.
- [ ] **10.s06.002.9** Phase budget enforcement: 5M blobs tenant → PhaseBudgetExceeded; SEV-2 alert.
- [ ] **10.s06.002.10** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s06.002.11** Cost regression gate: per-mark-batch ≤ $0.000005 (250 rows × D1 SELECT).
- [ ] **10.s06.002.12** Audit emission outbox (4 events) operational.

## 11. DoD

- [ ] Mark module compila + integration tests green.
- [ ] All Gherkin green.
- [ ] Property tests 10k green; 100k nightly.
- [ ] Migration 007 applied em staging.
- [ ] Criterion benchmark green @ 1M blobs.
- [ ] Cargo-fuzz 1h CI nightly green.
- [ ] Métricas (7 listadas §6.1.12) emitted.
- [ ] Architect + AppSec + SRE reviews.
- [ ] PRR Architect mini sign-off.

## 12. Invariants Validated

- **INV-GC-MARK-STARTED-AT-ATOMIC** (CRITICAL, NEW promovida §3.17): SQL UPDATE WHERE IS NULL atomic capture; idempotent re-run preserves.
- **INV-GC-REACHABLE-SET-COMPLETE** (CRITICAL, NEW): 3-pass scan covers union(blob_meta + ac_meta + manifest_chunks); property test 10k.
- **INV-GC-MARK-TENANT-SCOPED** (CRITICAL, NEW): all queries WHERE tenant_id = ctx.tenant_id; sqlx prepared; clippy lint.
- **INV-GC-MARK-PHASE-BUDGETED** (HIGH, NEW): 10 min p99 @ 1M blobs; PhaseBudgetExceeded error; SEV-2 alert if exceeds.
- **INV-GC-MARK-D1-BOUNDED-BATCH** (HIGH, NEW): 250 rows/batch + 100ms jitter (Lote 10.4bis lesson).
- **INV-GC-001** (CRITICAL, registry §3.4 + TLA+): mark phase produces correct reachable set; sweep doesn't fire on reachable.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Mark module | `crates/corelink-gc/src/mark/` | Rust |
| Migration | `migrations/007_gc_candidates.sql` | SQL |
| Property tests | `crates/corelink-gc/tests/prop_mark.rs` | Rust |
| Criterion benchmark | `crates/corelink-gc/benches/mark_bench.rs` | Rust |
| Cargo-fuzz | `crates/corelink-gc/fuzz/fuzz_targets/fuzz_mark.rs` | Rust |
| Chaos suite | `tests/chaos_gc_mark.rs` | Rust |
| README + threat model | `crates/corelink-gc/README.md` (mark section) | Markdown |

## 14. Quality Standards SOTA (compact)

- 14.s06.002.1: Zero `unsafe`; zero `unwrap`.
- 14.s06.002.2: rustdoc 100% public API.
- 14.s06.002.3: Test coverage ≥ 95% (mark phase boundary).
- 14.s06.002.4: Latência: mark phase ≤ 10 min p99 @ 1M blobs; per-batch ≤ 100ms p99.
- 14.s06.002.5: SAST: cargo-audit + cargo-deny + clippy `-D warnings`; cargo-fuzz 1h CI nightly.
- 14.s06.002.6: Métricas: 7 listadas §6.1.12.
- 14.s06.002.7: Memory bounded: per-batch ≤ 1 MiB stack + ≤ 5 MiB heap (250 rows × ~5KB).
- 14.s06.002.8: Cost regression gate per-batch ≤ $0.000005.

## 15. Chaos Experiments (10)

Listed §6.1.16.

## 16. PRR

Mini-PRR Architect + AppSec + SRE.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Mark module skeleton | 1.5 |
| ST-002 | Migration 007 (gc_candidates table) | 2 |
| ST-003 | mark_started_at_ms atomic capture (SQL UPDATE WHERE IS NULL) | 2 |
| ST-004 | Multi-pass scan (3 passes; tenant-scoped) | 5 |
| ST-005 | Reachable set computation (union 3 sources) | 3 |
| ST-006 | Batching 250 rows + jitter 100ms | 2 |
| ST-007 | D1 throttle adaptive (halve + backoff + circuit breaker) | 3 |
| ST-008 | gc_candidates output INSERT | 2 |
| ST-009 | Phase budget enforcement | 1.5 |
| ST-010 | Audit emission outbox (4 events) | 2 |
| ST-011 | Métricas emit (7 metrics) | 2 |
| ST-012 | Property tests (5 × 10k) | 4 |
| ST-013 | Criterion benchmark 1M blobs | 3 |
| ST-014 | Cargo-fuzz harness | 2 |
| ST-015 | Chaos suite (10 scenarios) | 4 |
| ST-016 | Architect + AppSec + SRE review iter | 3 |

**Total Optimistic**: ~42h. **PERT** (O=37h, M=44h, P=66h): **~47h**.

## 18. Dependencies

- Hard: WI-S06-001 (worker skeleton) SEALED; WI-S01-001 blob_meta + S-04 ac_meta + S-05 manifest_chunks SEALED.
- Soft: TLA+ `gc_correctness.tla` already verified Lote 5.13.
- Outbound: WI-S06-003 (sweep consumes gc_candidates); WI-S06-006 (TLA+ CI gate).

## 19. Effort PERT: 47h. ## 20. Time-boxing: 56h hard limit.

## 21. Observability

7 métricas listadas §6.1.12. Trace span `gc.mark.{phase_started, batch_processed, phase_completed}`.

## 22. Cost Analysis

**Per-batch cost** (250 rows):
- D1 SELECT 250 rows × $0.001/k = $0.000125 + Worker compute negligible.
- Per-batch: ~$0.000005.

**Per-mark cost** (1M blobs / 250 = 4000 batches):
- 4000 × $0.000005 = $0.020 per tenant per region per mark.

**TCO 12m projection** (5 regions × 100 tenants × 1 mark/dia × 4000 batches):
- 5 × 100 × 4000 × 365 × $0.000005 = **~$3.7k/yr** mark phase compute.
- gc_candidates storage: ~10M rows × 100 bytes = 1 GB → $0.75/mo = **$9/yr**.

**Cost regression gate**: per-batch ≤ $0.000010 (2× headroom).

## 23. API Contract

`MarkPhase` trait + `MarkResult`, `GcCandidate`, `CandidateStatus`, `MarkError`.

`#[non_exhaustive]` em `CandidateStatus` enum.

## 24. Post-mortem Hooks

- INV-GC-001 violation detected (reachable em candidates) → CRITICAL post-mortem.
- mark_started_at_ms drift (multiple captures) → CRITICAL (TLA+ obligation violated).
- Phase budget exceeded > 30 min sustained → SEV-1.
- D1 throttle exhaustion sustained > 1h → SEV-2 + scaling review.
- Reachable set incomplete via 1-pass missing → CRITICAL post-mortem + property test gap.

## 25. Rollback / Recovery

Mark phase idempotent re-run; mark_started_at_ms preserved across crashes; gc_candidates rollback safe (sweep doesn't fire if mark crashes); RTO ≤ 30 min; RPO 0.

## 26. Security & Privacy (compact)

**STRIDE**: tenant_id NOT NULL; sqlx prepared; TenantCtx-only; mark_started_at_ms atomic; multi-pass complete. **LINDDUN**: tenant_id pseudonymous; mark logs no PII; reachable set scoped per tenant.

## 27. Knowledge Transfer

- Tech talk (1.5h): "GC Mark Phase + mark_started_at_ms Anchor + TLA+ Obligation".
- Doc `docs/internal/gc-mark-phase.md`.
- Onboarding test (5 questions): mark_started_at_ms purpose, multi-pass rationale, batch 250 rationale, D1 throttle adaptive, phase budget enforcement.

## 28. Risk Register (12-row 6-col)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Reachable set incomplete (1 pass missing) | L | M | CRITICAL | L | LOW | 3-pass mandatory; chaos test #1; property test 10k |
| R-002 | mark_started_at_ms double-capture race | L | L | CRITICAL | L | LOW | SQL UPDATE WHERE IS NULL atomic; idempotent test |
| R-003 | D1 throttle cascade > 10 min | M | M | HIGH | M | LOW | Adaptive batch + backoff + circuit breaker |
| R-004 | Phase budget exceeded for fat tenants | M | L | HIGH | L | LOW | PhaseBudgetExceeded error; SEV-2 alert; admin override S-13 |
| R-005 | Manifest chunks cyclic reference | L | M | HIGH | L | LOW | Bounded traversal (WI-S05-005 pattern reuse); chaos test |
| R-006 | Cross-tenant scan injection | L | L | CRITICAL | L | LOW | tenant_id NOT NULL; sqlx prepared; clippy lint |
| R-007 | Worker crash mid-mark | M | L | LOW | L | LOW | Checkpoint per batch; mark_started_at_ms preserved |
| R-008 | gc_candidates table size unbounded | M | L | LOW | L | LOW | Sweep phase cleanup (WI-S06-003); cron daily truncate post-physical-delete |
| R-009 | Audit emission lag mid-batch | L | M | MEDIUM | L | LOW | Outbox eventual consistency; drain worker S-09 |
| R-010 | Refcount drift mid-mark (concurrent UpdateAR) | M | M | HIGH | M | LOW | INV-GC-004 protection (sweep WI-S06-003); reconcile diário (WI-S06-005) |
| R-011 | Cost regression > 10% per-batch | M | L | MEDIUM | L | LOW | §14.10 cost gate; criterion bench |
| R-012 | TLA+ obligation drift (mark_started_at_ms semantic shift) | L | M | HIGH | L | LOW | TLA+ CI gate (WI-S06-006); spec sync test |

## 29. Review Checkpoints

D+0 design (Architect + SRE); D+2 AppSec; D+4 code review; D+5 chaos suite; D+6 PRR mini.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver_ |
| 4 | Security Lead | _TBD; **mandatory** — multi-pass complete review_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory**_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD_ |
| 10 | Privacy | _TBD_ |
| 11 | Architect | _TBD; **mandatory** — mark_started_at_ms atomic capture; TLA+ obligation_ |
| 12 | AppSec | _TBD; **mandatory** — TenantCtx-only + multi-pass scan_ |
| 13 | Crypto SME | _advisory; consume in WI-006 (TLA+ CI gate)_ |

## 31. Change Log

1.0.0 / 2026-04-25 / Gustavo: Criação WI-S06-002 (Lote 10.6; SOTA pós-Lote 10.5bis lessons applied: CHECK inline; D1 batch 250; TenantCtx-only; mark_started_at_ms atomic capture; multi-pass 3-scan; jitter 100ms; phase budget enforcement; cargo-fuzz 1h × 1 target).

## 32. Anti-patterns evitados

- ❌ Single-pass scan; ❌ Skip jitter; ❌ Hard-coded batch > 250; ❌ mark_started_at_ms non-atomic; ❌ Cross-tenant scan; ❌ Skip phase budget; ❌ Skip audit emission; ❌ Trust client tenant_id; ❌ Sync emit em hot path; ❌ ALTER ADD CONSTRAINT chk_*; ❌ BEGIN/COMMIT em migration; ❌ with_tenant_ctx! claims em D1.

---

**Fim WI-S06-002.** Próximo: WI-S06-003 (Sweep phase + soft-delete + grace + INV-GC-004 enforce + audit emit).
