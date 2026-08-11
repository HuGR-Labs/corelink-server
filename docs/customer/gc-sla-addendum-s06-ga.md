# CoreLink — Garbage Collection SLA Addendum (S-06 GA)

> **Sprint:** S-06 · **Lane:** HIGH_RISK · **Effective date:** TBD (post-S-20 GA + customer onboarding S-19)
> **Status:** SCAFFOLD — finalisation at S-19 onboarding SEAL

This document is the customer-facing SLA addendum covering CoreLink's
garbage-collection (GC) reclaim behaviour. It supplements the base
CoreLink SLA published at GA.

## 1. What CoreLink GC does

CoreLink reclaims storage that is no longer referenced by any live
ActionCache entry, manifest, or blob_meta refcount. The reclaim
operation is **conservative by design** — we never delete a blob that
is reachable via any of the three load-bearing reachability sets
(blob_meta refcount > 0, ac_meta.outputs, manifest_chunks).

## 2. Performance SLOs

| SLO | Target | Measurement |
|---|---|---|
| Mark phase p99 | ≤ 10 min @ 1M blobs per (tenant, region) | Daily cron tick + criterion bench (S-09 forward) |
| Sweep phase p99 | ≤ 5 min @ 100k candidates per (tenant, region) | Daily cron tick + DASH-GC panel 1 |
| Physical-delete phase p99 | ≤ 30 min @ 100k candidates per (tenant, region) | Hourly tick + DASH-GC panel 1 |
| Reconcile phase p99 | ≤ 1 h @ 1M blobs per (tenant, region) | Daily cron tick + DASH-GC panel 4 |
| Refcount drift global | < 0.1% sustained 7d | DASH-GC panel 4 + alert `GC_RefcountDriftGlobalHigh` |
| Refcount drift per-tenant | < 1% sustained 1h | DASH-GC panel 4 + alert `GC_RefcountDriftPerTenantHigh` |

## 3. Correctness invariants

| Invariant | Severity | Verification |
|---|---|---|
| INV-GC-001: reachable never deleted | CRITICAL | TLA+ `gc_correctness.tla::InvGCReachableNeverDeleted` + chaos test 30d sustained |
| INV-GC-004: mark-phase-aware re-ref safe | CRITICAL | TLA+ `gc_correctness.tla::InvGCReRefProtected` + 100k race property test cross-validation |
| INV-CAS-IMMUTABILITY | CRITICAL | reads after GC soft-delete respect tombstone (S-02 read path interaction) |

**Customer trust claim:** "CoreLink never lost a reachable blob across
30d sustained staging chaos under 1k QPS write load." Cross-validated
by the TLA+ formal verification obligation `InvGCReReachableNeverDeleted`
+ `InvGCReRefProtected` against the real Rust implementation per
WI-S06-006 100k race property test.

## 4. Customer-visible reclaim metric (CAP-GC-006)

**Roadmap — not yet wired.** The canonical metric
`corelink_gc_reclaimed_bytes_total{tenant_id, tier}` (aggregated as
`bytes_reclaimed_last_30d` per tenant_tier: free / solo / team /
business / enterprise) has no code emitter today; it exists only as a
Grafana dashboard query. Customers do not yet see this metric anywhere —
the customer dashboard exposure (S-16 forward) and the emitter itself
both remain to be built.

## 5. Soft-delete reversibility window

| Cache surface | Grace period | Reversibility |
|---|---|---|
| CAS (blob_meta) | 72h | re-upload of same digest OR admin undelete (S-13 admin plane). **Not yet built:** the only shipped admin GC route today is `POST /v1/admin/gc/trigger`; there is no `/v1/admin/gc/undelete` endpoint. The re-upload path is real (`undelete()` in `crates/corelink-gc/src/sweep.rs`, exercised via CAS re-upload), but a direct admin-triggered undelete endpoint is roadmap, not shipped. |
| AC (ac_meta) | 24h | re-upload (UpdateActionResult) OR admin endpoint |

After grace expires, the physical-delete worker (post-grace strict-`>`
boundary) removes the row from R2 + D1 atomically per the R2→D1
crash-recovery ordering invariant.

## 6. DSR erasure interaction

CoreLink supports LGPD/GDPR Article 16/17 erasure requests via DSR
(S-11 forward). DSR erasure bypasses the grace period for the
issuing tenant only — cross-tenant impact is structurally impossible
per INV-TENANT-ISOLATION.

## 7. Failure mode response targets

| Failure mode | Severity | Detection target | Remediation target | Customer notification target |
|---|---|---|---|---|
| FM-300 (refcount bug) | P1 (CRITICAL incident) | ≤ 5 min p95 via reconcile SEV-2 alert | ≤ 30 min p95 | ≤ 1h p95 |
| FM-404 (gc-write-race) | P1 (CRITICAL incident) | ≤ 1 min p95 via property test alert | ≤ 30 min p95 | per RB if customer impact |
| FM-305 (tombstone lost) | P1 | ≤ 5 min p95 via SLO-FRESH-GC sustained metric | ≤ 30 min p95 | per RB if customer impact |

Runbooks: [RB-FM-300](../../specs/05_quality/runbooks/RB-FM-300-gc-refcount-bug.md) /
[RB-FM-404](../../specs/05_quality/runbooks/RB-FM-404-gc-write-race.md) /
[RB-FM-305](../../specs/05_quality/runbooks/RB-FM-305-tombstone-lost.md).

## 8. Anti-promises

CoreLink does NOT promise:

- Aggressive GC (< 72h grace) — would require ADR + customer opt-in
  via admin API (S-13 forward).
- Customer-facing "force GC" surface — admin-only via S-13 admin
  plane.
- Cross-region replicated tombstones — grace period applies per
  region; cross-region replication (S-14) does NOT extend grace.
- DSR-bypass grace for non-DSR scenarios — grace is regulatory floor;
  bypass authorised only via DSR.

## 9. Change log

| Version | Date | Change |
|---|---|---|
| 1.0.0-scaffold | 2026-05-02 | Initial scaffold for WI-S06-007 SEAL; finalisation at S-19 onboarding SEAL. |

---

**End SLA addendum scaffold.**
