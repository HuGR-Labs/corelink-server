---
id: "AUDIT-2026-05-02-ADVERSARIAL-S07"
type: "audit"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-02"
updated: "2026-05-02"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "adversarial-review", "s07", "dedup", "eviction", "quota", "lru", "wi-s07-005"]
---

# Adversarial review summary — S-07 implementation

> **Sprint:** S-07 · **WI:** WI-S07-005 §6.1 + §15 · **Mode:** internal aggregation across per-WI implementation rounds + cumulative pre-PRR sweep

This document aggregates ~20 adversarial scenarios catalogued across
WI-S07-001..004 implementation rounds. Per the 2026-04-30 protocol
shift, no per-WI codex was run — sprint-close Sonnet review (one
round of `general-purpose` agent with `model: sonnet` per charter)
covers the full S-07 corpus AFTER WI-S07-005 SEALs. This audit
captures the cumulative adversarial trace at SEAL time.

## 0. Scope

S-07 implementation scope:

- WI-S07-001 — dedup index trait + FindMissingBlobs orchestrator (commit `31d7e0f`)
- WI-S07-002 — eviction worker LRU + TTL + quota trigger (commit `7baa9d7`)
- WI-S07-003 — quota check middleware + DO actor model + reservation tracker (commit `002be3f`)
- WI-S07-004 — LRU tracker DO singleton async-batch (commit `8e9ec87`)
- WI-S07-005 — DASH-DEDUP + alerts + RB-FM-305/059 dry-runs + PRR ship gate (this Lote)

## 1. Adversarial scenarios (cumulative)

### 1.1 Dedup index (WI-S07-001)

1. **Cross-tenant existence-oracle attack via FindMissingBlobs.** Outcome:
   `DedupError::CrossTenantBlocked` arm enforces CTRL-ISO-005 explicitly;
   `dedup.cross_tenant.enabled = false` default; `prop_tenant_isolation`
   at 10k iter rejects every cross-tenant lookup.
2. **Re-upload race during eviction grace window** (same chunk_digest
   re-written while soft-delete pending). Outcome: chunks PK ON CONFLICT
   bumps refcount per Lote 10.7bis P0-1 (NOT 409 reject — that would
   break dedup since same chunk_digest legitimately appears in multiple
   manifests). `prop_idempotent` covers the canonical case at 10k iter.
3. **FindMissingBlobs batch overflow** (> MAX_FIND_MISSING_BATCH_SIZE).
   Outcome: explicit error returned at the trait surface; bounded
   iteration enforced; `prop_batch_above_max_rejected` pins the cap.
4. **Dedup ratio inflation via synthetic workload.** Outcome: SOTA bench
   §16 mandates 3 distinct workloads (Docker pulls, ML training, generic
   Bazel) before any marketing claim — cherry-picking a single workload
   would be rejected at PRR Architect signoff (R-S07-3 mitigation).

### 1.2 Eviction phase (WI-S07-002)

5. **Eviction observes blob ref'd by active `ac_meta.blob_refs` mid-
   sweep.** Outcome: pre-evict reachable check (canonical `json_each`
   SQL idiom inherited from S-06 reconcile) + `prop_evict_protect_if_re_referenced_strict_boundary`
   pin the off-by-one boundary at offsets 0/-1/+1; loosening to `<=`
   evict / `>` protect is a data-loss bug.
6. **Eviction R2 DELETE direct (would skip soft-delete + grace).**
   Outcome: structurally impossible — orchestrator type signature
   accepts `BlobMetaSoftDeleteStore` only (NOT `R2BlobStore`); soft-
   delete-first invariant pinned at construction. Physical delete is
   owned by S-06 GC physical-delete phase (post-grace strict-`>`).
7. **Eviction observes cascade dependency on chunks** (chunks lifecycle
   owned by S-06 GC via `chunks.refcount`). Outcome: `prop_blob_only_scope`
   asserts the orchestrator has NO chunks-table dependency by type
   signature (BLOB-scope per Lote 10.7bis P0-8); chunks reachability
   is exclusively S-06 GC's concern.
8. **Eviction TTL exceeds enterprise hard cap** (730d per ADR-0019).
   Outcome: `EvictionConfig::canonical()` pins the per-tier defaults
   + 730d hard cap; `prop_ttl_enterprise_cap_respected` rejects any
   override above the cap.
9. **Eviction quota-trigger fires twice for same tenant-region**
   (re-spawn race). Outcome: `worker::send_future()` fire-and-forget
   envelope per Lote 10.7bis R5 P0-3 simulated by
   `spawn_quota_trigger_in_memory`; `prop_quota_trigger_fires_at_95pct`
   asserts exact-boundary semantics + idempotent re-spawn.

### 1.3 Quota check (WI-S07-003)

10. **FM-059 race condition under high throughput** (1000 concurrent
    writes at 99.9% quota; canonical chaos scenario). Outcome:
    per-instance `Mutex<()>` decision_lock mirrors DO actor model;
    `prop_quota_atomic_no_race` asserts INV-QUOTA-ENFORCEMENT 0
    violations across 10k iter. RB-FM-059 dry-run validates the path
    end-to-end.
11. **Reservation TTL too short for multipart upload** (160 GiB at
    1 MB/s would take 160000s ≈ 1.85d; canonical TTL formula
    `min(7d, max(60s, req_bytes/1MB/s × 2))` per Lote 10.7bis R5 P0-2
    yields ≈ 3.7d which fits within the 7d hard cap). Outcome:
    `prop_reservation_ttl_size_proportional` + `prop_reservation_ttl_size_proportional_monotone`
    pin the formula; multipart upload no longer over-quota mid-flight.
12. **Reservation expiry leak** (TTL bug; expired reservations not
    cleaned up; FM-059 precursor). Outcome:
    `prop_reservation_expiry_releases_bytes` asserts the cleanup path
    monotonically releases `bytes_used` budget; the dashboard
    `Dedup_ReservationExpirySpike` SEV-2 alert fires within 15 min.
13. **Audit emit failure mid-quota-check.** Outcome: emit BEFORE state
    mutation per fail-closed envelope; emit failure surfaces
    `QuotaError::AuditEmissionFailed`; reservation NOT inserted; the
    request fails-closed with retry semantics.
14. **PROVISIONAL 429 transitional concern** (ADR-0020 boundary): the
    100% hard-block is owned by S-08 rate-limit DO with
    `Retry-After: days-until-month-reset`; S-07 emits a transitional
    PROVISIONAL 429. Outcome: documented explicit per WI-S07-003
    title row + scenario summary + design decision + API contract;
    PRR Architect cite-and-acknowledge required.

### 1.4 LRU tracker (WI-S07-004)

15. **Hot-path GET writes saturate D1 1k writes/s budget.** Outcome:
    `record_access` is fast-path enqueue (in-memory bounded queue
    `queue_size_max`) + `flush_batch` ≤ 250 rows per Lote 10.4bis;
    coalescing per `(tenant, digest)` latest-wins reduces D1 write
    amplification.
16. **Queue overflow drops most recent records** (FIFO drop semantics
    must be oldest-first to avoid data loss on the newest hot path).
    Outcome: `prop_overflow_drops_oldest` asserts FIFO semantics;
    `corelink_lru_dropped_total{reason=queue_full}` increments;
    SEV-2 alert at > 1k/min sustained 10 min.
17. **LRU drift exceeds threshold** (eviction observes
    `last_accessed_at` newer than DO buffered + D1 base UNION).
    Outcome: `prop_consistency_violation_detected_when_drift_exceeds_threshold`
    pins the canonical drift threshold; SEV-1 alert
    `Dedup_InvLruConsistencyViolation` fires within 5 min sustained.
18. **DO singleton flush stall** (long network pause; queue grows
    unbounded). Outcome: queue is bounded by `queue_size_max`; drop-
    counter increments past saturation; SLO probe
    `prop_check_duration_under_3ms_p99` asserts the canonical SLO
    envelope.

### 1.5 S-07 ship gate (WI-S07-005)

19. **DASH-DEDUP cardinality bomb** (per-tenant metrics × N tenants).
    Outcome: heatmap + breach-counter panels enforce `topk(50)` /
    `topk(20)` to bound cardinality; `Dedup_LruDriftP99High` fires
    on drift not on per-tenant rate.
20. **Alert noise during 2-week tune-in period** (false positives
    train oncall to ignore future SEV-3). Outcome: R-001 mitigation
    explicit in WI §28 risk register; threshold tunable via YAML
    revert; 2-week tune-in window mandatory before SLA claim.

The internal review surfaced **zero HIGH/CRITICAL** during S-07
implementation. The four prior WIs SEALed clean per spec contract
§20 v1.7.0..v1.10.0; trait-abstraction-defer items (real CF binding
+ 100k nightly + cargo-fuzz + criterion bench + chaos suite + 30d
sustained gates) are forward-looking with explicit revalidation
triggers.

## 2. Cross-WI invariant interaction matrix

| Invariant | WI source | WI consumer(s) | Cross-validation |
|---|---|---|---|
| INV-DEDUP-CONSISTENCY | WI-S07-001 | WI-S07-002 (re-upload race protection); WI-S06 (tombstone race) | `chunks` PK + ON CONFLICT bumps refcount; `prop_idempotent` 10k iter |
| INV-EVICT-SOFT-DELETE-FIRST | WI-S07-002 | WI-S07-005 (PRR ship gate) | type signature accepts `BlobMetaSoftDeleteStore` only; `prop_soft_delete_surfaces_as_missing` |
| INV-EVICT-CASCADE-PREVENTED | WI-S07-002 | WI-S07-005 (DASH panel 6) | canonical `json_each` SQL inherited from S-06; BLOB-scope per Lote 10.7bis P0-8 |
| INV-EVICT-TTL-CAP-RESPECTED | WI-S07-002 | WI-S07-005 (DASH panel 9 SOTA) | `prop_ttl_enterprise_cap_respected` 730d hard cap |
| INV-LRU-CONSISTENCY | WI-S07-004 | WI-S07-002 (race-aware reachable check) | `prop_consistency_violation_detected_when_drift_exceeds_threshold` |
| INV-QUOTA-ENFORCEMENT | WI-S07-003 | WI-S07-002 (95% trigger) | `Mutex<()>` decision_lock + `prop_quota_atomic_no_race` |
| INV-QUOTA-RESERVATION-TTL | WI-S07-003 | WI-S07-005 (DASH panel 10) | size-proportional formula + `prop_reservation_expiry_releases_bytes` |
| INV-GC-001 (inherited) | S-06 | WI-S07-002 (race-aware reachable check) | `corelink_evict_gc_invariant_violation_total` SEV-0 alert |
| INV-CAS-IMMUTABILITY (inherited) | S-02 | WI-S07-002 (eviction = metadata mark) | physical delete owned by S-06 post-grace |
| INV-TENANT-ISOLATION (inherited, TLA+) | S-01 | every WI | `prop_tenant_isolation` per-WI 10k iter |

## 3. Sign-off

| Role | Status | Date |
|---|---|---|
| Owner | ✅ ACKNOWLEDGED | 2026-05-02 |
| Engineer (lead) | ✅ APPROVED | 2026-05-02 |
| Architect (Crypto SME co-sign — INV-DEDUP-CONSISTENCY + CTRL-ISO-005 reviewed) | ⚠️ WAIVED (ADR-0034 dual-hat) | 2026-05-02 |

## 4. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-02 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial S-07 adversarial review aggregation (WI-S07-005 SEAL Lote). 20 scenarios catalogued; cumulative invariant interaction matrix; zero HIGH/CRITICAL. |

---

**End S-07 adversarial review v1.0.0.**
