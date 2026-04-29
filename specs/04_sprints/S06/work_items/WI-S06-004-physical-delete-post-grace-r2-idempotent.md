---
id: "WI-S06-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.2.0"
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
  - "STORAGE-SEMANTICS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s06", "gc", "physical-delete", "r2", "idempotent", "post-grace", "high-risk"]
---

# WI-S06-004 — Physical Delete Worker (Hourly Cron) + Post-Grace `deleted_at < now - grace_period` Filter + R2 DeleteObject + D1 Row Purge + PAT-RETRY-IDEMPOTENT-001 + bytes_reclaimed Tracking + DSR Erasure Bypass Path

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-06](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S06-004 |
| Título | Hourly cron physical-delete worker; consume `gc_candidates` (status='swept') filter `blob_meta.deleted_at < now - grace_period`; R2 DeleteObject (idempotent PAT-RETRY-IDEMPOTENT-001) + D1 row purge **with idempotent crash-recovery** (NOT 2PC ACID-atomic; Lote 10.6bis P0-2 framing fix; recovery via WI-S06-005 reconcile orphan detection); bytes_reclaimed tracking; DSR erasure bypass path (S-11 forward signal acelera grace = 0; **Ed25519 DPO-signed + tenant+digest-scoped + replay-protected via UNIQUE signal_id**; Lote 10.6bis P0-3 auth depth specified pre-impl + Crypto SME MANDATORY); chaos test R2 partial outage; FM-305 (tombstone lost) detection signal |
| Sprint | S-06 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-011 (irreversível; bug = data loss permanente), FF-HR-005 (controle integridade dados; FM-305 mitigation) |

## 1. Intent

Physical delete é **irreversível**; runs hourly; consume swept candidates apenas APÓS grace period; idempotent (R2 DeleteObject + D1 row purge) per PAT-RETRY-IDEMPOTENT-001:

```rust
// File: crates/corelink-gc/src/physical_delete.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait PhysicalDeletePhase: Send + Sync {
    /// Hourly cron entry point.
    /// Consumes gc_candidates WHERE status='swept' AND deleted_at < (now - grace_period).
    /// R2 DeleteObject + D1 row purge atomic per candidate.
    /// Idempotent (PAT-RETRY-IDEMPOTENT-001).
    async fn execute(
        &self,
        gc_run: &mut GcRun,
        tenant_id: &TenantId,
        region: &Region,
    ) -> Result<PhysicalDeleteResult, PhysicalDeleteError>;
}

pub struct PhysicalDeleteResult {
    pub candidates_processed: u64,
    pub blobs_deleted_count: u64,
    pub blobs_skipped_grace_pending: u64,         // grace not yet expired
    pub blobs_skipped_dsr_bypass: u64,            // DSR erasure path
    pub bytes_reclaimed: u64,
    pub r2_delete_failed_count: u64,              // graceful 503; retry next hour
    pub d1_purge_failed_count: u64,
    pub duration_ms: u64,
}

#[derive(thiserror::Error, Debug)]
pub enum PhysicalDeleteError {
    #[error("r2 backend error: {0}")]
    R2BackendError(String),

    #[error("d1 backend error: {0}")]
    D1BackendError(String),

    #[error("phase budget exceeded: {duration_ms}ms > {budget_ms}ms")]
    PhaseBudgetExceeded { duration_ms: u64, budget_ms: u64 },

    #[error("audit emission failed; physical delete aborted (fail-closed)")]
    AuditEmissionFailed,
}
```

**Cripto-driven invariants enforced**:

1. **Grace period boundary STRICT enforcement**: `WHERE blob_meta.deleted_at < (unix_ms_now() - grace_period_ms)`. Grace = 72h CAS / 24h AC (sprint contract §5.3). Boundary check === Lote 10.4bis lesson D1 query precision.

2. **R2 DeleteObject idempotent**: re-call em already-deleted blob = 200 OK (S3-compatible idempotent semantics). PAT-RETRY-IDEMPOTENT-001 (resilience patterns).

3. **Eventually-consistent R2→D1 ordering with idempotent crash-recovery (PAT-RETRY-IDEMPOTENT-001)** (Lote 10.6bis P0-2 framing fix — NOT 2PC ACID-atomic; R2 + D1 cannot be transactionally atomic in Cloudflare Workers; cross-system consistency via crash-recovery + reconcile):
   - R2 DeleteObject FIRST.
   - On R2 success → D1 row purge (D1 batch contains DELETE blob_meta + DELETE gc_candidate + INSERT audit_outbox + conditional `WHERE refcount = 0` predicate per P0-4).
   - On R2 fail → preserve D1 row (no orphan ref); next hourly cron retries.
   - On D1 fail post R2 success → R2 deleted but D1 row remains; next cron tick re-attempts D1 purge (idempotent: D1 row already gone? skip).
   - **Cross-system invariant**: R2 + D1 are **NOT** transactionally atomic; (a) R2-first ordering ensures D1 never references missing R2 *during the visibility window*; (b) WI-S06-005 reconcile detects R2-success/D1-fail orphan within 24h cron cycle. Downstream implementers MUST NOT assume 2PC; reconcile orphan detection is the safety net.

4. **DSR erasure bypass path** (Lote 10.6bis P0-3 + Lote 10.6-tris NEW-P1-1 enforcement mechanism — auth depth specified pre-impl; Crypto SME MANDATORY): signal from S-11 forward; aceleração grace = 0; immediate physical-delete; CTRL-PRIV-014 alignment. Auth requirements:
   - **(a) Authenticated**: DSR signal MUST carry `signed_payload` with DPO authority signature. Verify via Ed25519 (preferred) OR HMAC-with-rotated-key against known DPO pubkey set.
     - **(a.i) Rotation enforcement** (Lote 10.6-tris NEW-P1-1): pubkey table `dsr_dpo_pubkeys(key_id PRIMARY KEY, public_key_bytes BLOB(32) NOT NULL, created_at_ms INTEGER NOT NULL, expires_at_ms INTEGER NOT NULL, revoked_at_ms INTEGER, region TEXT NOT NULL, CHECK (expires_at_ms = created_at_ms + 7776000000))` — `expires_at_ms` enforced at INSERT to `created_at_ms + 90d` (7776000000ms). Runtime verification rejects pubkeys with `now > expires_at_ms OR revoked_at_ms IS NOT NULL`.
     - **(a.ii) Fail-closed runtime check**: if no valid (non-expired, non-revoked) pubkey exists for DPO authority, DSR path returns `DsrBypassError::NoValidPubkey` + SEV-1 alert + audit emit `corelink.gc.physical_delete.dsr_bypass_no_valid_pubkey` (NOT silent pass; bypass refused; grace continues to apply).
     - **(a.iii) Revocation propagation**: cross-region invalidation via per-region D1 row `UPDATE dsr_dpo_pubkeys SET revoked_at_ms = now()`. Propagation latency bound ≤15min via S-14 forward (BYOK + multi-region replication); pre-S-14 era requires manual per-region DPO key operation tracked em `dsr_dpo_pubkey_revocations` audit table. Cross-region replication SLA documented as P1 dependency on S-14.
     - **(a.iv) Rotation reminder CI gate**: `validate_dsr_pubkey_expiry.py` (NEW; weekly cron via GitHub Actions) — warns 14d before expiry; FAILS new DSR signal verification at 0d expiry; emits `corelink.dsr.pubkey_expiry_warning` metric (gauge: days_until_expiry).
   - **(b) Authorized scope**: `(tenant_id, digest)` pair specifically; `(tenant_id, '*')` and `('*', '*')` REJECTED. Wildcard scope = SEV-1 + audit alert.
   - **(c) Pre-execution audit**: emit `corelink.gc.physical_delete.dsr_bypass_received` with full provenance (signal_id, dpo_authority_id, signed_payload_digest, tenant_id, digest, received_at_ms, dpo_pubkey_id) BEFORE bypass executes — forensic trail even if subsequent bypass succeeds maliciously.
   - **(d) Replay-protected**: DSR signal IDs persisted em `dsr_signals_processed(signal_id PRIMARY KEY, processed_at_ms, region)`; UNIQUE constraint rejects replay. Cross-region replay protection: signal_id is globally unique (UUIDv7 + region prefix); cross-region check via S-14 forward.
   - **(e) Forge rejection audit**: emit `corelink.gc.physical_delete.dsr_bypass_invalid_signature` on signature verify fail; SEV-1; **bypass refused**.

5. **Tenant-scoped strict** (Lote 10.4bis lesson).

6. **Audit emit fail-closed**: same pattern WI-S06-003; D1 batch atomic.

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

Physical delete é **point of no return**. Bug em grace boundary check OR DSR bypass = data loss permanente. HIGH_RISK em N dimensões:

1. **Grace boundary off-by-one**: `< (now - grace_period)` vs `<= (now - grace_period)`. Strict `<` é correto (delete fires AFTER grace fully expired); `<=` deletes 1ms early. Mitigação: explicit `<` em SQL; CHECK chaos test boundary; integration test asserts.

2. **R2 DeleteObject failure cascade**: R2 down 1h; physical-delete pauses; next hourly cron retries (idempotent). Mitigação: graceful 503 + retry; PAT-RETRY-IDEMPOTENT-001; orphan_count metric (sustained > 1% = R2 reliability issue SEV-2).

3. **R2 success + D1 purge failure** (orphan D1 row references missing R2): inconsistent state. Mitigação: D1 row preserved; next hourly cron re-attempts (idempotent); reconcile (WI-S06-005) catches.

4. **DSR erasure bypass timing race**: DSR signal arrives mid-physical-delete tick; signal processed next tick; bounded latency 1h max. Mitigação: signal queue persisted; chaos test #5 simulates.

5. **Tenant isolation**: per-region per-tenant cron iteration; cross-tenant impossible by tenant_id NOT NULL + sqlx prepared.

6. **Audit emit fail-closed** (lesson WI-S06-003): R2+D1+audit must be atomic; if audit fails, R2 deleted but D1 preserved; next tick reconciles. Trade-off acceptable: audit miss is recoverable via S-09 chain integrity drift detection.

7. **Phase budget** (Lote 10.6bis P0-5 re-derivation com D1 batch ≤250 row Lote 10.5bis explicit): sprint contract §5.4 R-S06-9.1 ≤30 min p99 @ 100k candidates per (tenant, region) hourly tick. **Correct derivation**: R2 DeleteObject 50ms × 100k / bounded_concurrency 8 ≈ 625s ≈ 10.4min R2 work; D1 batch share — each batch contains 3 rows per candidate (DELETE blob_meta + DELETE gc_candidate + INSERT audit_outbox) so 250-row D1 batch caps at ~83 candidates → 100k / 83 = ~1200 batches × D1_batch_p99(100ms) / concurrency 8 ≈ 15s D1 work; total ≈ 11min p99 @ 100k. Budget headroom 30min/11min = 2.7×. SEV-2 alert threshold 25min sustained. Mitigação: bounded concurrency 8; D1 batch ≤250 explicit; phase budget enforcement.

8. **Forensic trail**: each physical-delete emit `corelink.gc.physical_delete.executed` event com forensic data (digest, bytes_reclaimed, deleted_at, mark_run_id).

**Atacante adversarial scenarios**:

- **DSR erasure bypass replay attack**: DSR signal replayed múltiplas vezes; idempotent (already deleted); audit emits multiple "no-op" events.
- **Cross-tenant injection**: defense-in-depth.
- **Race storm post-grace boundary**: simultaneous physical-delete + customer re-upload for same digest within 1ms boundary; race resolved via D1 row state (deleted vs new INSERT); WI-S01-001 CAS write handler integration.

**Risk justification HIGH_RISK**:

- **FF-HR-011**: irreversível; bug = data loss permanente.
- **FF-HR-005**: FM-305 (tombstone lost) mitigation; reclaim measurable.

11 sign-offs (canonical Lote 10.6 cycle 4 alignment with sprint.md §14 + _spec_contract §2 HIGH_RISK lane).

## 3. Customer Impact & Journey

**Persona 1 — Customer**: invisible (background); customer-visible via `bytes_reclaimed_last_30d` metric (forward S-09 + S-16 dashboard).

**Persona 2 — DevOps**: monitor `corelink.gc.physical_delete.{r2_failed, d1_failed}_total`; alert if > 5% sustained.

**Persona 3 — DPO (DSR erasure)**: regulatory request triggers bypass; audit chain captures DSR event forensic.

**SLA addendum**:
- Physical delete cron hourly (5 regions independent).
- Phase budget 30 min p99 @ 100k candidates.
- R2 DeleteObject p99 ≤ 100ms.
- DSR erasure bypass latency ≤ 1h (next cron tick).
- Idempotent re-run safe (PAT-RETRY-IDEMPOTENT-001).

## 4. Capability Mapping

- **CAP-GC-001** (Mark-and-sweep) — IMPLEMENTA primary physical-delete side.
- **CAP-GC-006** (Storage bytes reclaimed tracking) — IMPLEMENTA primary.
- Trace: `failure_modes.md FM-305` + `resilience_patterns.md PAT-RETRY-IDEMPOTENT-001`.

## 5. Tipo

Physical delete worker; HIGH_RISK; FF-HR-011 + FF-HR-005.

## 6. Escopo (compact)

### 6.1 In-scope

1. `crates/corelink-gc/src/physical_delete/` module.
2. **Hourly cron DO** (separate from mark/sweep cron; 5 regions); alarm re-arm at start (Lote 10.4bis lesson).
3. **Grace boundary SQL filter**: `WHERE status='swept' AND blob_meta.deleted_at < (now - grace_period_ms)` strict `<`.
4. **R2→D1 ordering with idempotent crash-recovery** (Lote 10.6bis P0-2 framing fix; NOT 2PC ACID-atomic; recovery via WI-S06-005 reconcile orphan detection within 24h).
5. **R2 DeleteObject idempotent** + retry exponential backoff.
6. **D1 row purge with conditional `WHERE refcount = 0` predicate** (Lote 10.6bis P0-4 race fix): `DELETE FROM blob_meta WHERE digest = ? AND refcount = 0 AND deleted_at < (now - grace_period_ms)` — predicate fails if customer CAS write incremented refcount in race window between physical-delete tick start and D1 commit; row preserved if race detected; reconcile catches orphan in next 24h cron.
7. **bytes_reclaimed tracking**: sum blob_size_bytes per run; persist em gc_run + audit emit + customer-visible metric.
8. **DSR erasure bypass signal handler** (S-11 forward; staging stub OK): immediate physical-delete grace=0.
9. **Bounded concurrency** 8 parallel physical-deletes per tenant per region (avoid R2 rate limit storm).
10. **Audit emission outbox** (WI-S01-004): physical_delete.{started, executed, r2_failed, d1_failed, completed, dsr_bypass} events.
11. **Métricas**:
    - `corelink.gc.physical_delete.cron_fired_total{region}`.
    - `corelink.gc.physical_delete.executed_total{tenant_id, region}`.
    - `corelink.gc.physical_delete.bytes_reclaimed_total{tenant_id, region}`.
    - `corelink.gc.physical_delete.r2_failed_total{region}` (alert > 5% sustained).
    - `corelink.gc.physical_delete.d1_failed_total{region}` (alert > 5% sustained).
    - `corelink.gc.physical_delete.dsr_bypass_total{tenant_id}`.
    - `corelink.gc.physical_delete.phase_duration_ms{region}` (histogram; SLO ≤ 30 min p99).
12. **Property tests** (10k iter PR; 100k nightly):
    - `prop_physical_delete_idempotent`: re-run on already-deleted = no-op.
    - `prop_grace_boundary_strict`: `deleted_at = (now - grace)` exactly → NOT deleted (boundary; strict `<`).
    - `prop_tenant_isolation`: 1000 concurrent across tenants; no interference.
    - `prop_r2_d1_crash_recovery` (Lote 10.6bis P0-2 renamed; was prop_r2_d1_atomic): simulate R2 success + D1 fail; D1 row preserved; reconcile detects orphan; next tick recovers via idempotent retry. NOT a 2PC atomicity property.
    - `prop_dsr_bypass_immediate`: valid DSR signal (Ed25519 verified) grace=0; immediate physical-delete; integration test.
    - `prop_physical_delete_re_upload_race` (Lote 10.6bis P0-4 NEW): 1k threads physical-delete vs CAS write same digest within 100ms window; assert conditional `WHERE refcount = 0` predicate prevents dangling ac_meta; assert no INV-CAS-IMMUTABILITY violation.
    - `prop_dsr_signature_forge_rejected` (Lote 10.6bis P0-3 NEW): inject DSR signals with (a) invalid Ed25519 signature; (b) wildcard scope; (c) replayed signal_id; assert each rejected with appropriate audit emit; bypass refused.
13. **Chaos suite** (sprint contract HIGH_RISK ≥ 10):
    - 1. Grace boundary off-by-one (deleted_at = exact boundary) → strict `<` rejects; chaos test asserts.
    - 2. R2 outage 1h mid-cron → physical-delete pauses; next tick retries; idempotent.
    - 3. R2 success + D1 fail → D1 row preserved; reconcile catches; chaos test asserts no orphan.
    - 4. Cross-tenant injection → sqlx prepared rejects.
    - 5. DSR bypass timing race (signal mid-tick) → next tick processes; bounded latency 1h.
    - 6. Bounded concurrency storm 1000 parallel → semaphore caps at 8; metric alert.
    - 7. Phase budget exceeded (1M candidates) → SEV-2 alert.
    - 8. Audit emit fail → physical-delete ROLLBACK; SEV-1.
    - 9. **Customer re-upload race post-grace boundary** (Lote 10.6bis P0-4 RE-FRAMED): physical-delete D1 commit at T+25ms; customer CAS write INSERT-OR-IGNORE at T+15ms (sees stale blob_meta row pre-physical-delete-D1-commit) → customer-cached pointer becomes dangling at T+25ms after physical-delete D1 commits. **Resolution**: physical-delete D1 batch uses **conditional predicate** `DELETE FROM blob_meta WHERE digest = ? AND refcount = 0 AND deleted_at < (now - grace_period_ms)` — customer's CAS write S-01 increments refcount to 1 in race window → conditional predicate fails → DELETE no-op → row preserved. Hard-to-reach race (customer INSERT commits BEFORE physical-delete predicate evaluates AND physical-delete commits AFTER): WI-S06-005 reconcile detects R2-deleted/D1-still-present orphan within 24h. Property test `prop_physical_delete_re_upload_race` 1k threads physical-delete vs CAS write; assert no dangling ac_meta.
    - 10. Worker crash mid-batch → resume from checkpoint; idempotent.
    - 11. **Cron tick missed** (CF Workers DO alarm not fired) → next tick double-load 200k candidates → phase budget exceeded → SEV-2 alert sustained 25min; auto-shed via priority queue (oldest-grace-expired first).
    - 12. **Cron clock skew across regions** (5 regions, NTP drift 1s) → grace boundary ambiguous at 1ms-from-boundary; resolution: D1 server-side `unix_timestamp_ms()` evaluated at D1 query time (NOT at Worker tick time); cross-region clock skew bounded by D1 single-region authoritative timestamp.
    - 13. **DSR signal forged (invalid Ed25519 signature)** (Lote 10.6bis P0-3 NEW): bypass refused; `corelink.gc.physical_delete.dsr_bypass_invalid_signature` audit emit; SEV-1 alert; `dsr_signals_processed` row NOT inserted (signal_id not consumed; no replay protection burnt on forged signal).

### 6.2 Out-of-scope

- Reconcile (WI-S06-005).
- TLA+ CI gate (WI-S06-006).
- DASH-GC dashboard (WI-S06-007).
- Customer dashboard reclaim metric (S-16 forward).

## 7. Anti-Scope

- ❌ Grace `<=` boundary (must be strict `<`).
- ❌ Skip R2→D1 ordering with idempotent crash-recovery discipline (Lote 10.6bis P0-2 framing; NOT 2PC).
- ❌ **Claim "atomic" semantics for R2 + D1** (Lote 10.6bis P0-2; cross-system NOT ACID-atomic).
- ❌ Cross-tenant physical-delete.
- ❌ Skip audit emit (fail-closed mandatory).
- ❌ DSR signal without DPO signature (Lote 10.6bis P0-3 — Ed25519 mandatory).
- ❌ DSR signal wildcard scope (Lote 10.6bis P0-3 — `(tenant_id, digest)` pair only).
- ❌ DSR signal replay (Lote 10.6bis P0-3 — UNIQUE signal_id mandatory).
- ❌ Skip pre-execution DSR audit emit (Lote 10.6bis P0-3 — forensic trail before bypass).
- ❌ Skip conditional `WHERE refcount = 0` predicate on physical-delete D1 batch (Lote 10.6bis P0-4 — race-protection mandatory).
- ❌ Customer-triggered force physical-delete (admin-only S-13).
- ❌ Trust client tenant_id (TenantCtx-only Lote 10.4bis).
- ❌ Hard-coded grace period (env-config).
- ❌ Retry without backoff (R2 rate limit cascade).

## 8. Acceptance Criteria (Gherkin) (compact 10 scenarios)

```gherkin
Feature: Physical delete worker post-grace + idempotent

  Scenario: Physical delete happy path post-grace
    Given gc_candidate status='swept'; blob_meta.deleted_at = T (T < now - 72h)
    When hourly cron fires
    Then SQL WHERE filter selects candidate
    And R2 DeleteObject called (idempotent)
    And D1 batch (blob_meta DELETE + gc_candidate DELETE + audit_outbox INSERT) atomic
    And metric bytes_reclaimed += blob.size_bytes
    And metric corelink.gc.physical_delete.executed_total +1

  Scenario: Grace period strict boundary (NOT yet expired)
    Given deleted_at = (now - 72h + 1ms) (1ms before boundary)
    When cron fires
    Then SQL filter `< (now - 72h)` does NOT match (strict <)
    And blob NOT physically deleted
    And property test prop_grace_boundary_strict green

  Scenario: R2 outage retry idempotent
    Given R2 DeleteObject returns 5xx for 1h
    When cron fires
    Then physical-delete pauses; D1 row preserved; metric r2_failed_total +1
    When R2 recovers
    Then next cron tick retries; idempotent (R2 returns 200 OR delete already happened)

  Scenario: R2 success + D1 fail (orphan recovery)
    Given R2 DeleteObject succeeds
    Given D1 batch fails (D1 throttle simulated)
    Then D1 row preserved (still references missing R2)
    When next cron tick
    Then D1 batch retries; idempotent

  Scenario: DSR erasure bypass immediate (valid signature)
    Given DSR signal received for tenant T digest D with valid Ed25519 DPO signature
    Given signal_id NOT in dsr_signals_processed (no replay)
    Given scope = (tenant_id, digest) pair (no wildcard)
    When physical-delete cron fires
    Then pre-execution audit emit corelink.gc.physical_delete.dsr_bypass_received WITH provenance
    Then DSR-flagged candidates bypass grace check
    And immediate R2 DeleteObject + D1 purge
    And dsr_signals_processed.signal_id INSERTED (replay protection burnt)
    And audit emit corelink.gc.physical_delete.dsr_bypass

  Scenario: DSR signal invalid Ed25519 signature → REJECTED (Lote 10.6bis P0-3)
    Given DSR signal with forged signature
    When physical-delete cron processes signal
    Then signature verification fails
    And bypass REFUSED (no R2 DeleteObject; no D1 purge)
    And audit emit corelink.gc.physical_delete.dsr_bypass_invalid_signature
    And SEV-1 alert fired
    And dsr_signals_processed row NOT inserted (signal_id preserved for forensic)

  Scenario: DSR signal wildcard scope → REJECTED (Lote 10.6bis P0-3)
    Given DSR signal with scope (tenant_id, '*') OR ('*', '*')
    When physical-delete cron processes signal
    Then bypass REFUSED
    And audit emit corelink.gc.physical_delete.dsr_bypass_wildcard_scope
    And SEV-1 alert fired

  Scenario: DSR signal replay → idempotent no-op (Lote 10.6bis P0-3)
    Given DSR signal with signal_id ALREADY in dsr_signals_processed
    When physical-delete cron processes signal
    Then UNIQUE constraint rejects re-insertion
    And bypass NOT re-executed (idempotent)
    And audit emit corelink.gc.physical_delete.dsr_bypass_replay_detected (forensic; SEV-2 informational)

  Scenario: Idempotent re-run on already-deleted
    Given blob already physically deleted (R2 DeleteObject returns 200; D1 row gone)
    When cron tick processes
    Then no-op; no double-emit audit
    And property test prop_physical_delete_idempotent green

  Scenario: Tenant isolation
    Given 1000 concurrent physical-deletes different tenants
    When all execute
    Then no cross-tenant interference; per-region per-tenant scoping

  Scenario: Bounded concurrency
    Given 1000 candidates queued
    Then per-tenant semaphore caps at 8 parallel R2 DeleteObject
    And metric concurrency_limited_total tracks

  Scenario: Customer re-upload race post-grace boundary (Lote 10.6bis P0-4 RE-FRAMED)
    Given physical-delete tick starts at T for digest D in region_sam
    Given customer CAS write INSERT-OR-IGNORE fires at T+15ms (sees stale blob_meta row pre-physical-delete D1 commit)
    Given customer's CAS write S-01 increments blob_meta.refcount = 1 in race window
    When physical-delete D1 batch executes at T+25ms with conditional WHERE refcount = 0 predicate
    Then DELETE no-op (refcount = 1 fails predicate)
    And blob_meta row preserved (NOT deleted)
    And R2 object preserved (R2 DeleteObject NOT called when D1 predicate would fail; verify-then-delete order)
    And customer's ac_meta pointer remains valid (no dangling)
    When subsequent reconcile cron tick (WI-S06-005)
    Then orphan check: blob_meta exists; R2 exists; consistent
    And no INV-CAS-IMMUTABILITY violation

  Scenario: Audit emit fail-closed
    Given audit_outbox INSERT fails
    When physical-delete batch executes
    Then D1 batch ROLLBACK (R2 already deleted; D1 row preserved; reconcile detects orphan)
    And SEV-1 alert
```

## 9-32 (compact)

### 9. Design Decisions

- 9.1: Hourly cron (não daily) — minimize latency between grace expiry and physical delete.
- 9.2: **R2→D1 ordering with idempotent crash-recovery (PAT-RETRY-IDEMPOTENT-001)** (Lote 10.6bis P0-2; NOT 2PC ACID-atomic; recovery via WI-S06-005 reconcile orphan detection).
- 9.3: Strict `<` grace boundary (TLA+-aligned semantics).
- 9.4: Bounded concurrency 8 (R2 rate limit safe).
- 9.5: **DSR bypass via Ed25519-signed signal queue** (Lote 10.6bis P0-3): DPO signature mandatory; (tenant_id, digest) scope; UNIQUE signal_id replay protection; pre-execution audit; **Crypto SME MANDATORY** (was advisory). S-11 forward.
- 9.6: PAT-RETRY-IDEMPOTENT-001 reuse.
- 9.7: TenantCtx-only (Lote 10.4bis).
- 9.8: Audit fail-closed (consistency com WI-S06-003 sweep).
- 9.9: **Phase budget separate sprint contract §5.4 R-S06-9.1**: ≤30 min p99 @ 100k candidates; arithmetic re-derived (Lote 10.6bis P0-5) with D1 batch ≤250 row Lote 10.5bis lesson explicit.
- 9.10: ADR forward — ADR-0042 (WI-001) covers; no new ADR.
- 9.11: **Conditional D1 predicate `WHERE refcount = 0`** (Lote 10.6bis P0-4): physical-delete race-protection against customer CAS write INSERT in tick window.
- 9.12: D1 batch ≤250 row Lote 10.5bis lesson explicit; ~83 candidates per batch; ~1200 batches @ 100k.

### 10. Completeness Criteria

- [ ] **10.s06.004.1** Property tests 5 × 10k green; 100k nightly.
- [ ] **10.s06.004.2** Chaos suite 10 scenarios green.
- [ ] **10.s06.004.3** Grace boundary strict `<` validated em integration test.
- [ ] **10.s06.004.4** R2 outage 1h chaos test + idempotent recovery.
- [ ] **10.s06.004.5** DSR bypass immediate (forward S-11 stub).
- [ ] **10.s06.004.6** Bounded concurrency 8 validated.
- [ ] **10.s06.004.7** Phase budget 30 min @ 100k + SEV-2 alert.
- [ ] **10.s06.004.8** Métricas (7) emitted.
- [ ] **10.s06.004.9** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s06.004.10** Cost regression gate per-physical-delete ≤ $0.000010.

### 11. DoD

- [ ] Module compila + integration tests green; All Gherkin green; Property 10k green; 100k nightly green; Migration cron DO em wrangler.toml; Architect + AppSec + SRE reviews; PRR mini.

### 12. Invariants Validated

- **INV-GC-PHYSICAL-DELETE-IDEMPOTENT** (HIGH, NEW promovida §3.17): re-run no-op (PAT-RETRY-IDEMPOTENT-001).
- **INV-GC-GRACE-BOUNDARY-STRICT** (CRITICAL, NEW): SQL `<` not `<=`; reversibility window respected.
- **INV-GC-R2-D1-ORDERING** (HIGH, NEW): R2 DeleteObject before D1 purge; orphan recoverable; reconcile detects.
- **INV-GC-DSR-BYPASS-AUTHORIZED** (HIGH, NEW): DSR signal verified pre-bypass; S-11 forward auth.
- **INV-GC-002** (MEDIUM, registry §3.4): orphan eventualmente deletado.
- **INV-CAS-IMMUTABILITY** (CRITICAL, registry §3.3): post-physical-delete reads return 404.

### 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Physical delete module | `crates/corelink-gc/src/physical_delete/` | Rust |
| Property tests | `crates/corelink-gc/tests/prop_physical_delete.rs` | Rust |
| Chaos suite | `tests/chaos_gc_physical_delete.rs` | Rust |
| Wrangler cron DO binding | `wrangler.toml` (additions) | TOML |

### 14. Quality Standards

- 14.s06.004.1: Zero unsafe; zero unwrap.
- 14.s06.004.2: rustdoc 100%.
- 14.s06.004.3: Test coverage ≥ 90%.
- 14.s06.004.4: Latência: per-delete p99 ≤ 100ms; phase ≤ 30 min p99 @ 100k.
- 14.s06.004.5: SAST clean.
- 14.s06.004.6: Métricas: 7 §6.1.11.
- 14.s06.004.7: Memory bounded ≤ 1 MiB stack.
- 14.s06.004.8: Cost regression gate per-delete ≤ $0.000010.

### 15. Chaos Experiments (10)

§6.1.13.

### 16. PRR

Mini-PRR Architect + AppSec + SRE.

### 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton | 1 |
| ST-002 | Hourly cron DO + alarm re-arm | 2 |
| ST-003 | Grace boundary SQL filter | 1.5 |
| ST-004 | R2 DeleteObject + retry idempotent | 2.5 |
| ST-005 | D1 batch atomic (R2-then-D1) | 2 |
| ST-006 | bytes_reclaimed tracking + métricas | 2 |
| ST-007 | DSR bypass signal handler stub | 2 |
| ST-008 | Bounded concurrency semaphore | 1.5 |
| ST-009 | Audit emission outbox (6 events) | 2 |
| ST-010 | Property tests (5 × 10k) | 4 |
| ST-011 | Chaos suite (10) | 4 |
| ST-012 | Architect + AppSec + SRE review iter | 3 |

**Total**: ~28h. **PERT** O=24h M=29h P=44h: **~31h**.

### 18-32 (compact final)

- **18 Dependencies**: Hard WI-001..003 SEALED; WI-S01-004 audit_outbox; soft S-11 DSR stub; outbound WI-005 reconcile.
- **19 Effort PERT 31h**; **20 Time-boxing 38h hard limit**.
- **21 Observability**: 7 métricas §6.1.11; trace `gc.physical_delete.{phase, executed, dsr_bypass}`.
- **22 Cost**: per-delete ~$0.000005 (R2 DeleteObject $4.5/M + D1 batch + audit). TCO 12m: 5 × 100 × 1k × 365 × $0.000005 = ~$913/yr.
- **23 API Contract**: PhysicalDeletePhase trait + result types; `#[non_exhaustive]`.
- **24 Post-mortem**: grace boundary off-by-one detected → CRITICAL (data loss reversibility broken); R2 outage > 4h sustained → SEV-1; DSR bypass timing > 1h → SEV-2.
- **25 Rollback**: physical-delete irreversível; rollback only via PRE-physical-delete state preservation; RTO N/A; RPO 0 within grace window.
- **26 Security**: tenant_id NOT NULL; sqlx prepared; DSR signal authenticated; audit fail-closed.
- **27 Knowledge Transfer**: Tech talk (1h); doc `gc-physical-delete.md`; onboarding test 5 questions.
- **28 Risk Register** (12-row; Lote 10.6bis expansion): grace boundary bug L M CRITICAL L LOW (chaos test); R2 outage cascade M M HIGH M LOW (retry); D1 fail post-R2 L M MEDIUM L LOW (reconcile catches); cross-tenant injection L L CRITICAL L LOW (sqlx prepared); **DSR bypass auth defect** (Lote 10.6bis P0-3) L L CRITICAL L LOW (Ed25519 verify + pre-exec audit + replay protection); **DSR bypass timing race** L M MEDIUM L LOW; bounded concurrency miss M L LOW L LOW; phase budget exceeded L L MEDIUM L LOW; audit emit fail L M MEDIUM L LOW; **customer re-upload race undetected** (Lote 10.6bis P0-4) L M MEDIUM L LOW (conditional refcount=0 predicate + reconcile orphan detection); **R2/D1 cross-system "atomic" framing misuse** (Lote 10.6bis P0-2) L M MEDIUM L LOW (spec wording fixed; downstream impl assumes crash-recovery); cost regression M L MEDIUM L LOW.
- **29 Review Checkpoints**: D+0 design; D+1 AppSec; D+3 code review; D+4 chaos; D+5 PRR mini.
- **30 Sign-off (HIGH_RISK 11)**: Final Approver Gustavo; SRE Lead waiver ADR-0034; Security Lead mandatory; Engineer mandatory; QA mandatory; Product Gustavo; Compliance mandatory (DSR LGPD Art. 16); Privacy mandatory (DSR bypass authorization review); Architect mandatory (R2→D1 crash-recovery ordering); AppSec mandatory (cross-tenant + DSR scope rejection); **Crypto SME MANDATORY (Lote 10.6bis P0-3 promoted from advisory; non-waivable for DSR Ed25519 verify + key rotation cadence + signal_id UNIQUE constraint)**.
- **31 Change Log**: 1.0.0 / 2026-04-25 / Gustavo (Lote 10.6); 1.1.0 / 2026-04-25 / Gustavo (Lote 10.6bis Part 2a P0 fixes: P0-2 atomic→crash-recovery wording; P0-3 DSR Ed25519 + scope + replay + Crypto SME mandatory; P0-4 conditional refcount=0 predicate + race re-frame; P0-5 phase budget arithmetic re-derived with D1 batch ≤250); 1.2.0 / 2026-04-25 / Gustavo (Lote 10.6-tris Sonnet R5 NEW-P1-1 fix: DSR Ed25519 key rotation enforcement mechanism specified — `dsr_dpo_pubkeys` schema with expires_at_ms = created_at + 90d CHECK constraint; runtime fail-closed on no-valid-pubkey; cross-region revocation propagation via S-14 forward; `validate_dsr_pubkey_expiry.py` weekly CI gate; signal_id global uniqueness via UUIDv7+region prefix; pre-execution audit captures dpo_pubkey_id provenance).
- **32 Anti-patterns**: ❌ Grace `<=`; ❌ Skip R2→D1 crash-recovery ordering; ❌ Claim "atomic" cross-system semantics; ❌ Cross-tenant; ❌ Skip audit; ❌ DSR signal sem Ed25519 sig; ❌ DSR wildcard scope; ❌ DSR replay; ❌ Skip pre-exec DSR audit; ❌ Skip conditional refcount=0 predicate; ❌ Trust client tenant_id; ❌ Hard-coded grace; ❌ Retry sem backoff.

---

**Fim WI-S06-004.** Próximo: WI-S06-005 (Refcount reconciliation + auto-fix).
