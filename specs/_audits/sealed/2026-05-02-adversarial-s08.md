---
id: "AUDIT-2026-05-02-ADVERSARIAL-S08"
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
tags: ["audit", "adversarial-review", "s08", "rate-limit", "quota-cas", "edge", "abuse", "circuit", "wi-s08-006"]
---

# Adversarial review summary — S-08 implementation

> **Sprint:** S-08 · **WI:** WI-S08-006 §6.1 + §15 · **Mode:** internal aggregation across per-WI implementation rounds + cumulative pre-PRR sweep

This document aggregates ~20 adversarial scenarios catalogued across
WI-S08-001..005 implementation rounds. Per the 2026-04-30 protocol
shift, no per-WI codex was run — sprint-close Sonnet review (one
round of `general-purpose` agent with `model: sonnet` per charter)
covers the full S-08 corpus AFTER WI-S08-006 SEALs. This audit
captures the cumulative adversarial trace at SEAL time.

## 0. Scope

S-08 implementation scope:

- WI-S08-001 — DO RateLimiter token bucket + state machine (commit `ae84983`)
- WI-S08-002 — CF edge per-IP rules + CIDR blocklist (commit `aa4d06b`)
- WI-S08-003 — Quota checker middleware atomic CAS (commit `c8c0587`)
- WI-S08-004 — Abuse detection heuristic + scoring (commit `675993e`)
- WI-S08-005 — Response code types + RFC 9331 headers + global circuit (commit `f60f90e`)
- WI-S08-006 — DASH-RATE + alerts + RB-FM-250 dry-run + PRR ship gate (this Lote)

## 1. Adversarial scenarios (cumulative)

### 1.1 Per-tenant DO RateLimiter (WI-S08-001)

1. **FM-059 race condition under high concurrent acquire** (1000 concurrent
   `try_acquire` at zero remaining tokens). Outcome: per-instance
   `Arc<Mutex<HashMap<BucketKey, TokenBucketState>>>` mirrors DO actor
   model serialisation; `prop_concurrent_acquire_does_not_double_spend`
   pins INV-AVAIL-ISOLATION at 10k iter PR / 100k iter nightly.
2. **Token bucket negative-token underflow via overshoot acquire**.
   Outcome: saturating-arithmetic safety + structurally non-negative
   tokens; `prop_token_bucket_never_exceeds_capacity` + `prop_overshoot_does_not_panic`.
3. **Cross-tenant rate-limit state leak via shared HashMap key collision**.
   Outcome: `BucketKey` composite tenant-leftmost; `prop_tenant_isolation`
   rejects every cross-tenant lookup at 10k iter.
4. **Refill rate division-by-zero on canceled tenant** (refill_rate = 0).
   Outcome: Lote 10.8bis P1-1 division-by-zero guard with 7-day
   saturation; `prop_canceled_tenant_saturation` pins the 7d ceiling.
5. **Plan-tier change does NOT propagate ≤ 5min** (INV-RATE-LIMIT-PROPORTIONALITY
   violation). Outcome: `corelink_ratelimit_plan_sync_lag_ms` gauge +
   SEV-3 alert sustained > 5min; cross-validated by
   `prop_token_bucket_proportionality`.

### 1.2 CF edge per-IP CIDR (WI-S08-002)

6. **Adversarial CIDR collision via `LIKE '%cidr%'` SQL substring attack**.
   Outcome: structurally impossible — longest-prefix-match canonical
   (NOT linear scan); IPv4 + IPv6 supported; hand-rolled bit-level
   matcher per WI-S08-002 §6.1. `prop_longest_prefix_match` 10k iter
   pins canonical semantics.
7. **NAT customer false-positive block** (multi-user behind single IP).
   Outcome: warn-mode first per sprint contract §15 R-S08-006
   mitigation; opt-in hard-block requires explicit admin action.
8. **Blocklist drift D1 ↔ CF List replica** (D1 source-of-truth +
   CF List replica). Outcome: `corelink_edge_cf_api_error_total{operation=reconcile}`
   SEV-2 alert sustained 5min; reconcile cycle re-triggered manually.
9. **Idempotent add CIDR repeatedly** (re-add same prefix). Outcome:
   `prop_idempotent_add` asserts no double-counting; tenant isolation
   round-trip pinned.

### 1.3 Quota CAS atomic check (WI-S08-003)

10. **FM-059 race condition under bounded retry loop** (CAS fails 3
    consecutive times = race detected). Outcome: bounded retry
    canonical 3 attempts; audit emit BEFORE write fail-closed mirroring
    S-07 sprint-close P1-1 fix; race detection rate
    `corelink_quota_cas_race_detected_total` SEV-3 informational.
    `prop_cas_no_double_spend` + `prop_cas_race_detected_retry_succeeds`
    pin canonical race-aware strict-< boundary.
11. **Retry-After regression** (canonical days-until-month-reset must
    always be within `[1, 31×86_400]`). Outcome:
    `prop_retry_after_days_until_month_reset` 10k iter; Howard Hinnant
    Gregorian primitives wasm32-clean; supersedes S-07 PROVISIONAL
    transitional 429 + Retry-After per ADR-0020 FROZEN.
12. **Audit emit failure mid-CAS-check**. Outcome: emit BEFORE state
    mutation per fail-closed envelope; emit failure surfaces
    `QuotaCasError::AuditEmissionFailed`; reservation NOT inserted.

### 1.4 Abuse heuristic detection (WI-S08-004)

13. **AutoSuspendForbidden trap on programmatic suspend attempts**
    (LGPD Art. 20 + GDPR Art. 22 humane response). Outcome:
    `AbuseError::AutoSuspendForbidden` arm carrying tenant_id + score
    for audit trail; `prop_suspend_idempotent` pins idempotence;
    `corelink_abuse_auto_suspend_attempts_total` SEV-1 LGPD canary.
14. **Cross-tenant feature leak via shared rolling-window state**.
    Outcome: per-instance `Arc<Mutex<HashMap<Uuid, AbuseRollingWindow>>>`
    mirrors DO actor model; `prop_per_tenant_isolation_concurrent`
    100 tenants 10k iter rejects every cross-tenant feature read.
15. **Score calibration drift** (FP > 5% upper OR TP < 80% lower).
    Outcome: Wilson 95% CI calibration sample n=50/50 per Lote
    10.8bis P0-E corrected; **0/50 FP (Wilson upper 0.071) + 50/50
    TP (Wilson lower 0.929)** well above acceptance bound.
    `Rate_AbuseCalibrationDrift` SEV-3 sustained 1h triggers shadow
    re-calibration workflow.
16. **NaN-poisoned input to abuse score compute** (e.g. 0.0/0.0 in
    cpu_wallclock_ratio). Outcome: NaN-safe AbuseFeatures constructor
    + `prop_score_in_range_0_1` clamps structurally [0.0, 1.0];
    `prop_score_monotonic_in_*` 4 monotonicity props pin canonical
    ordering.

### 1.5 RFC 9331 + global circuit (WI-S08-005)

17. **Single-signal false-positive trip** (one signal alone trips the
    circuit, converting regional rate spike into region-wide outage;
    R-S08-004 risk register MEDIUM impact CRITICAL). Outcome: multi-
    signal canonical (5xx_rate AND p99_latency); single-signal
    observed = `corelink_global_circuit_single_signal_alarm_total`
    SEV-3 informational (NOT trip); manual override audit emit
    mandatory.
18. **HalfOpen probe lets multiple through** (recovery state; should
    let exactly one through per WI-S08-005 §6.1). Outcome:
    `prop_half_open_lets_one_through` pins exactly-one canonical;
    audit emit `corelink.circuit.half_open_probe`.
19. **5-arm X-Rate-Limit-Type taxonomy non-exhaustive** (sixth arm
    needed; would silently cause SLI denominator drift). Outcome:
    `XRateLimitTypeKind` `#[non_exhaustive]` enum forces match-arm
    exhaustivity; `prop_x_rate_limit_type_5_arm_canonical` pins the
    canonical taxonomy at 10k iter.

### 1.6 S-08 ship gate (WI-S08-006)

20. **DASH-RATE cardinality bomb** (per-tenant heatmap × 100k tenants).
    Outcome: heatmap panels enforce `topk(50)` to bound cardinality;
    blocklist size gauge per family (IPv4/IPv6) only 2 series; SLO
    panel uses 30d window aggregate (not per-tenant). 14 panels total
    + 15 alert rules canonical (4 SEV-1 + 6 SEV-2 + 5 SEV-3 per Lote
    10.8-tris P1-NEW-1 corrected).

The internal review surfaced **zero HIGH/CRITICAL** during S-08
implementation. The five prior WIs SEALed clean per spec contract
§20 v1.3.0..v1.7.0; trait-abstraction-defer items (real CF binding +
100k nightly + cargo-fuzz + chaos suite + 30d sustained gates +
real CF DDoS-managed integration) are forward-looking with explicit
revalidation triggers.

## 2. Cross-WI invariant interaction matrix

| Invariant | WI source | WI consumer(s) | Cross-validation |
|---|---|---|---|
| INV-AVAIL-ISOLATION (HIGH; registry §3.8) | WI-S08-001 | WI-S08-004 (cross-tenant feature leak); WI-S08-006 (DASH panel 6 + Rate_InvAvailIsolationViolation SEV-1) | per-instance `Arc<Mutex<>>` mirroring DO actor model; `prop_tenant_isolation` per-WI 10k iter cumulative 50k |
| INV-RATE-LIMIT-PROPORTIONALITY (HIGH; new) | WI-S08-001 | WI-S08-006 (DASH panel 13 + Rate_RatelimitPlanSyncLag SEV-3) | `prop_token_bucket_proportionality` + tier-aware refill ladder canonical 5-tier per Lote 10.7bis P0-7 |
| INV-QUOTA-ENFORCEMENT (HIGH; registry §3.11) | WI-S08-003 | WI-S08-006 (DASH panel 8 + Rate_QuotaHardBlock100PctSustained SEV-2) | per-instance `Mutex` + `prop_cas_no_double_spend` race-aware strict-< |
| INV-TENANT-ISOLATION (CRITICAL, TLA+) | inherited S-01 | every S-08 WI | `prop_tenant_isolation` per-WI 10k iter; cross-validated cumulative |
| INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (CRITICAL, TLA+) | inherited S-01 | every S-08 WI | audit emit BEFORE state mutation per fail-closed envelope (Lote 10.6bis pattern + S-07 sprint-close P1-1 fix) |
| INV-CAS-IMMUTABILITY (inherited, TLA+) | S-02 | WI-S08-003 (atomic CAS does NOT mutate canonical CAS layer) | quota-CAS operates on `tenant_storage_state` row only; `tenant_quota` POLICY immutable |

## 3. Sign-off

| Role | Status | Date |
|---|---|---|
| Owner | ✅ ACKNOWLEDGED | 2026-05-02 |
| Engineer (lead) | ✅ APPROVED | 2026-05-02 |
| Architect (Crypto SME co-sign — INV-QUOTA-ENFORCEMENT atomic CAS race correctness reviewed; INV-AVAIL-ISOLATION DO actor serialisation reviewed) | ⚠️ WAIVED (ADR-0034 dual-hat) | 2026-05-02 |

## 4. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-02 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial S-08 adversarial review aggregation (WI-S08-006 SEAL Lote). 20 scenarios catalogued; cumulative invariant interaction matrix; zero HIGH/CRITICAL. |

---

**End S-08 adversarial review v1.0.0.**
