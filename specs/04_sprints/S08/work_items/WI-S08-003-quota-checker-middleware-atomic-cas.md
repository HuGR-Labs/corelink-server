---
id: "WI-S08-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-002"]
parent: "S-08"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "SECURITY-MODEL"
  - "DATA-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "AUTH-MODEL"
tags: ["wi", "s08", "quota", "rate-limit", "per-pat", "atomic-cas", "do-actor", "bandwidth-monthly", "high-risk"]
---

# WI-S08-003 — Quota Checker Middleware (Atomic CAS via DO Actor) + Per-PAT Rate Camada 3 + Monthly Bandwidth Quota (`crates/corelink-quota`; DO `Quota-<tenant_id>` race-free serialization; canonical `tenant_storage_state.bytes_used` Lote 10.7bis P0-2 absorbed — phantom `tenant_quota.bytes_used` from sprint contract REJECTED; `tenant_quota.max_storage_bytes` POLICY immutable; race-aware strict-< predicate analogous a S-06 INV-GC-004 + S-07 WI-S07-002 absorbed; CAP-QUOTA-001 hard-block 100% boundary com S-07 ≤95% eviction trigger ADR-0020 FROZEN; CAP-QUOTA-002 monthly bandwidth via DO `BandwidthTracker-<tenant>-<YYYY-MM>` reset 1st UTC; per-PAT rate camada 3 of 4 PAT-RATE-LIMIT-001; INV-QUOTA-ENFORCEMENT atomic enforcement)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-08](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S08-003 |
| Título | Quota checker middleware unificado (storage hard-block 100% + bandwidth monthly + per-PAT rate); atomic CAS via DO actor model (race-free serialization NOT D1 SQLite atomic — D1 sem native CAS); DO `Quota-<tenant_id>` shared com S-07 WI-S07-003 reservation DO (single DO; multiple methods); canonical `tenant_storage_state.bytes_used` (Lote 10.7bis P0-2 NEW table absorbed; sprint contract phantom `tenant_quota.bytes_used` REJECTED — current sprint contract §5 R-S08-5 will need correction in `tris` cycle); race-aware strict-< pre-write predicate `bytes_used + request_bytes < max_storage_bytes` (analogous a S-06 INV-GC-004 + S-07 WI-S07-002 lessons absorbed); CAP-QUOTA-001 boundary com S-07: S-07 owns ≤95% eviction trigger; S-08 owns 100% hard-block + 95-100% transition window; ADR-0020 FROZEN; bandwidth monthly via DO `BandwidthTracker-<tenant>-<YYYY-MM>` aggregator (egress + ingress separately tracked); reset 1st UTC monthly atomic via DO alarm (Lote 10.8bis P0-D corrected: chrono crate `next_month_first_utc_midnight()` canonical primitive; previous fabricated `tomorrow_at_utc_midnight()` reference REJECTED — wrong semantics); per-PAT rate camada 3 of 4 (sprint contract §5 R-S08-3): `pat_rate.check{pat_id}` cap = 10× tenant refill_rate detects PAT misuse; response 429 + `X-Rate-Limit-Type: over_quota | per_pat` (canonical 5-enum em WI-S08-005; bandwidth excedence é semantically over-plan → subsumed sob `over_quota` discriminator; reason field em audit log distingue storage vs bandwidth — Lote 10.8bis Phase 1 P0-A absorbed) |
| Sprint | S-08 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-QUOTA-001 + per-PAT camada 3 security controls; bypass = AVAIL-ISOLATION violation cross-tenant), FF-HR-002 (cross-tenant SLO degradation se quota deficit) |

## 1. Intent

Quota checker é o **enforcement primitive** de tenant economic isolation (storage + bandwidth + PAT-scoped). Sem isso: tenant pode escrever sem limit (cost overrun), bandwidth flood degrada todos via shared egress, PAT comprometido continues to operate at full tenant rate. Atomic enforcement via DO actor (race-free) compõe defense-in-depth com WI-S08-001 (per-tenant rate) + WI-S08-002 (per-IP edge). Boundary canonical com S-07 (eviction) + ADR-0020 FROZEN.

```rust
// File: crates/corelink-quota/src/lib.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait QuotaChecker: Send + Sync {
    /// Atomic pre-write check; returns Ok(reservation) if (bytes_used + request_bytes < max_storage_bytes),
    /// Err(Over) if would-exceed; race-free via DO actor; reservation TTL size-proportional
    /// (Lote 10.7bis R5 P0-2 lesson absorbed: 60s minimum / max(60s, request_bytes/1MB/s × 2x safety) capped 7d).
    async fn check_storage_and_reserve(
        &self,
        tenant_ctx: &TenantCtx,
        request_bytes: u64,
    ) -> Result<StorageReservation, QuotaError>;

    /// Confirm reservation after write success (decrement reservation; commit bytes_used delta).
    /// Idempotent on reservation_id.
    async fn confirm_storage(
        &self,
        tenant_ctx: &TenantCtx,
        reservation_id: ReservationId,
        actual_bytes: u64,
    ) -> Result<(), QuotaError>;

    /// Release reservation after write failure (decrement reservation only; bytes_used unchanged).
    /// Idempotent.
    async fn release_storage_reservation(
        &self,
        tenant_ctx: &TenantCtx,
        reservation_id: ReservationId,
    ) -> Result<(), QuotaError>;

    /// Monthly bandwidth check + atomic decrement (via DO `BandwidthTracker-<tenant>-<YYYY-MM>`).
    async fn check_bandwidth(
        &self,
        tenant_ctx: &TenantCtx,
        direction: BandwidthDirection,             // Egress | Ingress
        bytes: u64,
    ) -> Result<BandwidthCheckResult, QuotaError>;

    /// Per-PAT misuse DETECTION (NOT aggregate enforcement; camada 1 owns aggregate).
    /// Lote 10.8bis P0-C correction: returns Ok always (request not blocked); emits
    /// `corelink.quota.pat_misuse_detected_total{pat_id}` metric SEV-2 alert se
    /// per-PAT rate > misuse threshold. Aggregate per-tenant rate enforced em camada 1
    /// (WI-S08-001 DO RateLimiter); camada 3 é purely observability-on-PATs.
    async fn check_pat_rate(
        &self,
        tenant_ctx: &TenantCtx,
        pat_id: &PatId,
    ) -> Result<PatRateResult, QuotaError>;
}

pub struct StorageReservation {
    pub reservation_id: ReservationId,             // ULID; idempotency key
    pub bytes_reserved: u64,
    pub expires_at_ms: i64,                        // size-proportional TTL
    pub current_used_bytes: u64,
    pub max_storage_bytes: u64,
    pub utilization_pct: f64,                      // 0.0 to 1.0; ≥ 0.95 SEV-3 alert (S-07 boundary)
}

pub struct BandwidthCheckResult {
    pub allowed: bool,
    pub bytes_consumed_this_month: u64,
    pub max_bytes_this_month: u64,
    pub period: String,                            // "YYYY-MM"
    pub seconds_until_reset: u64,
}

pub struct PatRateResult {
    pub allowed: bool,                             // ALWAYS true post-Lote 10.8bis P0-C (misuse-detector NOT enforcer)
    pub pat_id: PatId,
    pub observed_rate_per_sec: f64,                // observed rolling rate per PAT
    pub misuse_threshold_per_sec: f64,             // = 10× tenant refill (alarm threshold; NOT cap)
    pub misuse_detected: bool,                     // true if observed > threshold; emits SEV-2 alert
}

#[derive(thiserror::Error, Debug)]
pub enum QuotaError {
    #[error("storage quota exceeded: used={used} max={max} request={request} → would_exceed by {overshoot}")]
    StorageOver { used: u64, max: u64, request: u64, overshoot: u64 },

    #[error("bandwidth quota exceeded: month={period} consumed={consumed} max={max}; resets in {reset_secs}s")]
    BandwidthOver { period: String, consumed: u64, max: u64, reset_secs: u64 },

    // PatRateOver REMOVED per Lote 10.8bis P0-C: per-PAT camada 3 é misuse-detector NOT enforcer.
    // Compromised PAT detection: per-PAT observed rate > 10× tenant refill triggers SEV-2 alert + admin revoke flow.
    // Request blocking handled by camada 1 (WI-S08-001 per-tenant DO; aggregate enforcement).

    #[error("reservation expired: {reservation_id}")]
    ReservationExpired { reservation_id: String },

    #[error("DO backend error: {0}")]
    DoBackendError(String),

    #[error("D1 backend error: {0}")]
    D1BackendError(String),

    #[error("audit emit failed (fail-closed; transaction aborted): {0}")]
    AuditEmitFailed(String),

    #[error("plan unknown for tenant {tenant_id}")]
    PlanUnknown { tenant_id: String },
}
```

**Cripto-driven invariants enforced**:

1. **INV-QUOTA-ENFORCEMENT** (HIGH; registry §3.11): atomic pre-write check via DO actor model:
   - DO `Quota-<tenant_id>` shared com S-07 WI-S07-003 reservation pattern (single DO; multiple methods; reduces cold-start cost).
   - Race-free serialization: concurrent `check_storage_and_reserve` calls serialized; deterministic outcome.
   - **Race-aware strict-< predicate** (S-06 INV-GC-004 + S-07 WI-S07-002 lessons absorbed): `bytes_used + request_bytes < max_storage_bytes` (NOT ≤; strict prevents boundary race).
   - D1 backing for durability + admin visibility; DO is hot-path authoritative; D1 is snapshot replica + cold-start recovery.

2. **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant DO; cross-tenant impossible by design.

3. **CTRL-QUOTA-001** (security_model.md §300; sprint contract §4 CAP-QUOTA-001): hard-block 100% boundary com S-07.

4. **ADR-0020 FROZEN** boundary canonical (Lote 10.7bis Phase 3 absorbed):
   - **S-07 owns**: ≤95% storage utilization → eviction trigger (CAP-EVICT-003); 80% email defer to S-13.
   - **S-08 owns**: 100% storage utilization → hard-block 429 + `X-Rate-Limit-Type: over_quota`; 95-100% transition window monitored (alert SEV-3 customer notification SEV-2 internal).
   - **No overlap**: S-07 CAP-EVICT-003 ≤95% trigger; S-08 CAP-QUOTA-001 100% hard-block; transition handled by S-08 (95% → 100% rapid spike = eviction insufficient; hard-block kicks in).

5. **Canonical `tenant_storage_state.bytes_used`** (Lote 10.7bis P0-2 NEW table absorbed):
   - **REJECT phantom column** `tenant_quota.bytes_used` referenced em sprint contract §5 R-S08-5 (will require sprint contract correction em `tris` cycle review feedback).
   - Canonical: `tenant_storage_state.bytes_used` (mutable telemetry; running counter); `tenant_quota.max_storage_bytes` (POLICY; immutable per period).
   - DO reads/writes `tenant_storage_state.bytes_used`; D1 batch ≤ 250 (Lote 10.5bis canonical).

6. **Size-proportional reservation TTL** (Lote 10.7bis R5 P0-2 lesson absorbed):
   ```rust
   pub fn reservation_ttl(request_bytes: u64) -> Duration {
       let bytes_per_sec_assumed = 1_000_000.0;   // 1 MB/s pessimistic floor
       let safety_multiplier = 2.0;
       let secs = (request_bytes as f64 / bytes_per_sec_assumed) * safety_multiplier;
       let bounded = secs.max(60.0).min(7.0 * 24.0 * 3600.0);  // [60s, 7d]
       Duration::from_secs(bounded as u64)
   }
   ```
   60s minimum (small writes); 7d maximum (160 GiB upload @ 100 Mbps takes ~218min — far below 7d). Reservation auto-released at TTL via DO alarm sweep.

7. **DO routing via `tenant.primary_region`** (Lote 10.7bis P0-9 lesson absorbed): DO `Quota-<tenant_id>` resolved within tenant primary region; cross-region requests route via primary; SLA p99 ≤ 5ms within primary (slightly higher than RateLimiter ≤3ms because quota CAS is atomic D1 backing; not pure in-memory).

8. **Monthly bandwidth canonical reset** (chrono crate `next_month_first_utc_midnight()` correct primitive — Lote 10.8bis P0-D fix; previous reference to `tomorrow_at_utc_midnight()` was semantically wrong: returns next-day midnight NOT next-month-1st-UTC):
   - DO `BandwidthTracker-<tenant>-<YYYY-MM>` aggregator with separate counters: `egress_bytes`, `ingress_bytes`.
   - Reset trigger: 1st UTC of next month at 00:00:00.000Z (atomic via DO alarm scheduled at exact next-month boundary; Lote 10.4bis re-arm AT START).
   - **Canonical chrono utility** (defined em `crates/corelink-time/src/lib.rs`):
     ```rust
     /// Returns the UTC midnight of the 1st day of the next calendar month.
     /// Handles December → January year increment (e.g., 2026-12 → 2027-01-01 00:00:00Z).
     /// Lote 10.8bis P0-D corrects prior misuse of `tomorrow_at_utc_midnight()`.
     pub fn next_month_first_utc_midnight(now: DateTime<Utc>) -> DateTime<Utc> {
         let (year, month) = if now.month() == 12 {
             (now.year() + 1, 1u32)
         } else {
             (now.year(), now.month() + 1)
         };
         let next_first = NaiveDate::from_ymd_opt(year, month, 1)
             .expect("year+month always valid post-increment");
         next_first.and_hms_opt(0, 0, 0)
             .expect("00:00:00 always valid")
             .and_utc()
     }

     /// Seconds until next-month-1st UTC midnight from `now_ms`. Used for Retry-After header
     /// em over_quota responses (bandwidth excedence). Returns u64 seconds; min 0.
     pub fn secs_until_next_month_first_utc_midnight(now_ms: i64) -> u64 {
         let now = DateTime::from_timestamp_millis(now_ms).expect("valid unix ms");
         let next_first = next_month_first_utc_midnight(now);
         (next_first.timestamp() - now.timestamp()).max(0) as u64
     }
     ```
   - Old DO snapshot persisted to D1 `bandwidth_history(tenant_id, period TEXT, egress_bytes, ingress_bytes, snapshotted_at)`; new DO `BandwidthTracker-<tenant>-<YYYY-MM+1>` boots fresh.
   - **Determinism**: NOT sliding window; calendar boundary canonical (sprint contract §10.s08.5).
   - **Property test obrigatório** (P0-D fallout): boundary cases `next_month_first_utc_midnight` testado em (a) Jan 1 00:00:01 → next Feb 1; (b) Dec 31 23:59:59 → next Jan 1 (year increment); (c) Feb 28 non-leap → Mar 1; (d) Feb 28 leap year (2028) → Feb 29 → Mar 1.

9. **Per-PAT misuse detection camada 3** (Lote 10.8bis P0-C correction; sprint contract §5 R-S08-3 amend queued Phase 6): observability-on-PATs (NOT enforcement; aggregate enforcement em camada 1):
   ```rust
   pub fn pat_rate_misuse_threshold_for_tier(tier: &Tier) -> f64 {
       let (tenant_refill, _burst) = WI_S08_001::refill_rate_for_tier(tier);
       tenant_refill * 10.0   // misuse alarm threshold (NOT enforcement cap; aggregate em camada 1)
       // free 100 RPS / solo 500 / team 2000 / business 10000 / enterprise 100000
   }
   ```
   Architectural correction: aggregate per-tenant rate enforced em camada 1 (WI-S08-001 DO RateLimiter). Camada 3 emits SEV-2 metric alert when single PAT observed rate > 10× tenant_refill (compromised credential signal); request NOT blocked at camada 3 (camada 1 already throttles aggregate). Compromised PAT signature: single PAT >> tenant_aggregate / num_PATs, indicating attacker maxing one credential while others idle. Detection latency ≤ 5min via metric aggregation; admin revoke flow S-03 RUNBOOK-AUTH-003.

10. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id from Tower middleware (S-03 WI-S03-003); pat_id from same TenantCtx (Lote 10.4bis structure absorbed).

11. **Audit fail-closed** (Lote 10.6bis pattern absorbed): all quota mutations emit audit events; fail-closed if audit emit fails (transaction aborted).

12. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): `worker::send_future()`; NEVER `tokio::spawn`; `async_lock::Semaphore` if needed (Lote 10.3-tris lesson).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + race-correctness justification)

Quota checker é **the economic isolation primitive** do CoreLink — sem isso, INV-QUOTA-ENFORCEMENT inviável (silent over-write); cost discipline broken; per-PAT misuse undetected. 3 enforcement domains unificados em single DO `Quota-<tenant_id>` (shared com S-07 WI-S07-003 reservation): (a) storage hard-block 100%; (b) monthly bandwidth; (c) per-PAT rate camada 3. Single DO consolida cold-start cost (one DO per tenant for all quota operations).

**Why DO actor model (NOT D1 atomic CAS)**: D1 SQLite-based; **no native CAS** — atomic UPDATE-WHERE-condition exists but limited by SQLite single-writer transaction; under contention (1k QPS concurrent writes from same tenant) D1 would serialize at SQLite WAL lock level (latency p99 50–200ms unacceptable). DO actor model: in-memory state + single-threaded actor = race-free + sub-ms latency + durable storage backed. Sprint contract §6 DoD explicit: "Quota check atomic: DO actor model guarantees".

**Why race-aware strict-< predicate** (S-06 INV-GC-004 + S-07 WI-S07-002 lessons absorbed): `bytes_used + request_bytes ≤ max` permits boundary race where 2 concurrent reservations both see exactly-equal-to-max state and both pass; correct predicate is `bytes_used + request_bytes < max` (strict). Catches edge case where last byte of quota disputed by 2 concurrent uploads.

**Why size-proportional reservation TTL** (Lote 10.7bis R5 P0-2 lesson): 60s fixed TTL (initial sprint contract assumption) breaks multipart uploads — 160 GiB @ 100 Mbps takes ~218min; reservation auto-released mid-upload would corrupt counter (write proceeds without active reservation; bytes_used not properly tracked). Formula: `max(60s, request_bytes/1MB/s × 2x safety)` capped 7d. Lote 10.7bis P0-2 absorbed.

**Why ADR-0020 FROZEN boundary com S-07** (Lote 10.7bis Phase 3 absorbed): without explicit boundary, S-07 eviction + S-08 hard-block would conflict (which trigger fires? when?). ADR-0020: S-07 owns ≤95% (eviction primary; soft pressure); S-08 owns 100% (hard-block residual; eviction insufficient). 95-100% transition window: S-08 monitors; alert SEV-3 (customer-facing) + SEV-2 (internal) signaling eviction lag.

**Why monthly bandwidth NOT sliding window** (sprint contract §10.s08.5): determinism canonical; calendar boundary 1st UTC midnight. Sliding window would create gaming: upload at end-of-window when sliding overlaps; calendar rigid. chrono crate `next_month_first_utc_midnight()` (Lote 10.8bis P0-D corrected primitive — previous reference to `tomorrow_at_utc_midnight()` from Lote 10.5bis lineage was fabricated; that lesson actually concerned partial UNIQUE indexes em S-05 dedup) computes deterministic next-reset boundary handling December → January year increment.

**Why per-PAT camada 3 é DETECTOR não enforcer** (Lote 10.8bis P0-C absorbed; sprint contract §5 R-S08-3 requer correção em `tris` cycle):

CRITICAL CLARIFICATION: Per-PAT camada 3 é a **misuse-detection layer**, NOT a rate-enforcement layer. Camada 1 (WI-S08-001 per-tenant DO RateLimiter) **já enforces aggregate per-tenant rate** (e.g., 200 RPS team plan). Per-PAT cap = `10× tenant_refill_rate` (e.g., 2000 RPS team) é o **misuse alarm threshold**: single PAT consuming > 2000 RPS = signal of compromised credential OR misconfigured client. **Isso NÃO blocks the request** (camada 1 already 429s when tenant aggregate exceeded); apenas emits SEV-2 alert.

Math correctness: 5 PATs × 200 RPS each = 1000 RPS aggregate; camada 1 throttles to 200 RPS sustained; camada 3 detection threshold (2000 RPS per PAT) triggers ZERO false-positives em legitimate usage (typical PAT 5–10% of tenant rate). Compromised PAT signals: single PAT > 2000 RPS observed during compromise scenario (attacker maxing one credential while others idle); detection latency ≤ 5min via metric aggregation; admin revoke flow S-03 RUNBOOK-AUTH-003.

Why threshold = 10× tenant: balances false-positive (high enough to not flag legitimate burst usage) vs false-negative (low enough to detect deliberate compromise; saturating attacks typically hit ≥ 10× tenant aggregate before being throttled by camada 1). Compromised PAT mathematical signature: single PAT >> aggregate / num_PATs (e.g., 1 PAT consuming 5× aggregate = anomaly).

Sprint contract §5 R-S08-3 amend (will be applied em this Lote 10.8bis Phase 6 spread sprint contract correction): "PAT-scoped rate **observability**: `corelink_quota_pat_misuse_detected_total{pat_id}` SEV-2 alert when per-PAT observed rate > 10× tenant refill (compromised credential signal); enforcement of aggregate per-tenant rate handled em camada 1 (WI-S08-001 DO)."

**Adversarial scenarios**:
- **Race at boundary** (2 concurrent writes at exhausted state): DO actor serializes; first succeeds (bytes_used + req < max); second sees updated bytes_used; deterministic strict-< rejection.
- **Reservation orphan** (write fails after reserve, no release call): DO alarm sweep auto-releases at expiry; bytes_used decrement reverts; eventual consistency.
- **Plan downgrade mid-flight** (max_storage_bytes decreased while bytes_used > new max): pre-existing reservations honored (TTL); new check_and_reserve returns Err(StorageOver) immediately; eviction (S-07) drives down bytes_used; no data loss (eviction is soft-delete grace).
- **Bandwidth reset race at midnight** (tenant uploads at 23:59:59.999 UTC; counter decrements at 00:00:00.000Z): chrono crate atomic check; either old DO accepts (last-millisecond) or new DO accepts (first-millisecond); never both; never neither.
- **PAT compromise** (attacker uses PAT at 100× tenant rate): camada 3 cap 10× → 90% requests rejected; SEV-2 alert; admin revokes PAT (S-03 RUNBOOK-AUTH-003).
- **DO restart amid in-flight reservation**: durable storage persists pending reservations + bytes_used; cold start recovers state from D1 snapshot; alarm re-armed AT START (Lote 10.4bis).
- **Monthly bandwidth wraparound** (tenant exceeds quota at 28th of month): Err(BandwidthOver); customer notified; reset 1st of next month; chrono crate computes reset_secs deterministic.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: CTRL-QUOTA-001 + per-PAT security controls; bypass = AVAIL-ISOLATION violation.
- **FF-HR-002**: per-tenant bulkhead failure; cross-tenant SLO degradation se quota deficit.
- 12 sign-offs + chaos 30d sustained + property test 100k race iterations on atomic CAS predicate.

## 3. Customer Impact & Journey

**Persona 1 — Customer (storage 90%)**: tenant's bytes_used = 90 GiB / 100 GiB plan; upload 5 GiB blob. Pre-check: 90 + 5 = 95 GiB < 100 GiB → reservation granted (TTL ~10min); upload proceeds; on success: confirm_storage (bytes_used = 95 GiB); utilization 95% → SEV-3 customer notification "approaching quota"; S-07 eviction trigger fires.

**Persona 2 — Customer (storage 100%)**: tenant bytes_used = 99.5 GiB; upload 1 GiB. Pre-check: 99.5 + 1 = 100.5 GiB ≥ 100 GiB → Err(StorageOver); response 429 + X-Rate-Limit-Type: over_quota + Retry-After: days-until-month-reset (or 0 if pay-per-use plan). Customer prompted to upgrade plan OR delete content. SLI: not counted in numerator (legitimate over-quota; sprint contract §7.10.s08.1).

**Persona 3 — Customer (bandwidth 95%)**: tenant consumed 95 GiB / 100 GiB monthly egress; download 10 GiB blob. Pre-check: 95 + 10 = 105 GiB ≥ 100 GiB → Err(BandwidthOver); response 429 + X-Rate-Limit-Type: over_quota + Retry-After: seconds-until-month-reset (audit log `reason=bandwidth_egress_exceeded` para discriminação observability; canonical 5-enum em WI-S08-005 preserved; Lote 10.8bis P0-A). Customer self-service: upgrade plan OR wait for reset.

**Persona 4 — Customer (PAT misuse detected)**: tenant team tier (200 RPS aggregate, enforced em camada 1); PAT-A configured for CI/CD; attacker steals PAT-A; floods at 5000 RPS attempting saturation. Camada 1 (WI-S08-001 per-tenant DO) caps aggregate at 200 RPS sustained; 96% of attacker requests already 429'd with `X-Rate-Limit-Type: tenant_quota` (camada 1 enforcement). Camada 3 (this WI) detects single PAT-A consuming > 200×10 = 2000 RPS observed (despite camada 1 throttling, attacker's intent is visible em request rate before throttle); emits `corelink.quota.pat_misuse_detected_total{pat_id=PAT-A}` SEV-2 alert; admin notification PagerDuty; admin revokes PAT (S-03 RUNBOOK-AUTH-003). Lote 10.8bis P0-C correction: per-PAT camada 3 é misuse DETECTOR; aggregate enforcement is camada 1.

**Persona 5 — DevOps reviewing**: DASH-RATE (WI-S08-006) shows per-tenant bytes_used utilization + bandwidth-month-consumed + per-PAT rate distribution; alert if per-PAT rate > 10× WoW (legitimate growth signal vs misuse).

**SLA addendum**:
- Quota check overhead: ≤ 5ms p99 (DO + D1 backing).
- Monthly bandwidth reset: deterministic 1st UTC midnight.
- Reservation TTL: size-proportional formula min 60s / max 7d.
- INV-QUOTA-ENFORCEMENT: 0 silent over-write em chaos test 30d.
- PAT rate cap detection: SEV-2 alert ≤ 5min from misuse pattern.

## 4. Capability Mapping

- **CAP-QUOTA-001** (Storage hard-block 100% boundary com S-07) — IMPLEMENTA primary.
- **CAP-QUOTA-002** (Monthly bandwidth) — IMPLEMENTA primary.
- **CAP-RATE-003** (Per-PAT camada 3) — IMPLEMENTA primary.
- Trace: `security_model.md CTRL-QUOTA-001 + CTRL-RATE-001` + `invariant_registry.md INV-QUOTA-ENFORCEMENT + INV-AVAIL-ISOLATION` + `failure_modes.md FM-059 (quota race) + FM-201 (Config change causa rate-limit drop; Lote 10.8bis P1-3 corrected) + FM-255 (over-quota)` + ADR-0020 FROZEN + S-07 WI-S07-003 (DO `Quota-<tenant_id>` shared).

## 5. Tipo

DO singleton + QuotaChecker trait + Tower middleware + D1 backing; HIGH_RISK; FF-HR-005 + FF-HR-002.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-quota/` module** — QuotaChecker trait + DO impl (shared com S-07 WI-S07-003) + Tower middleware + tests.

2. **DO `Quota-<tenant_id>` extended** (single DO; multi-method; shared com S-07 WI-S07-003 reservation pattern):
   - State: `bytes_used: u64, reservations: HashMap<ReservationId, ReservationEntry>, pending_release_alarms: BinaryHeap<(expiry_ms, ReservationId)>, bandwidth_egress_bytes: u64, bandwidth_ingress_bytes: u64, bandwidth_period: String "YYYY-MM", per_pat_state: HashMap<PatId, TokenBucketState>`.
   - DO routing via `tenant.primary_region` (Lote 10.7bis P0-9).
   - DO alarm 60s sweep: (a) release expired reservations; (b) snapshot to D1 (Lote 10.5bis batch ≤ 250); (c) check bandwidth period transition (cross 1st-UTC-midnight → reset).
   - DO alarm re-arm AT START (Lote 10.4bis lesson absorbed).

3. **Atomic check_and_reserve** (race-aware strict-< predicate; S-06 INV-GC-004 + S-07 WI-S07-002 absorbed):
   ```rust
   pub fn check_and_reserve(&mut self, request_bytes: u64, now_ms: i64) -> Result<StorageReservation, QuotaError> {
       let active_reserved: u64 = self.reservations.values()
           .filter(|r| r.expires_at_ms > now_ms)
           .map(|r| r.bytes_reserved)
           .sum();

       let total_committed = self.bytes_used + active_reserved + request_bytes;

       if total_committed < self.max_storage_bytes {
           // STRICT-< predicate (race-aware Lote 10.7bis P0-6 absorbed):
           let reservation_id = ulid::generate();
           let ttl = reservation_ttl(request_bytes);
           let entry = ReservationEntry {
               bytes_reserved: request_bytes,
               expires_at_ms: now_ms + ttl.as_millis() as i64,
               created_at: now_ms,                               // canonical no _ms suffix per Lote 10.7bis P0-3
           };
           self.reservations.insert(reservation_id, entry);
           // Schedule alarm for release
           Ok(StorageReservation {
               reservation_id,
               bytes_reserved: request_bytes,
               expires_at_ms: entry.expires_at_ms,
               current_used_bytes: self.bytes_used,
               max_storage_bytes: self.max_storage_bytes,
               utilization_pct: (self.bytes_used + active_reserved) as f64 / self.max_storage_bytes as f64,
           })
       } else {
           // Lote 10.8bis P1-6 (R5): canonical overshoot é total_committed - max (NOT max-1);
           // strict-< predicate already ensured total_committed >= max em este branch.
           let overshoot = total_committed.saturating_sub(self.max_storage_bytes);
           Err(QuotaError::StorageOver {
               used: self.bytes_used,
               max: self.max_storage_bytes,
               request: request_bytes,
               overshoot,
           })
       }
   }
   ```

4. **confirm_storage** (post-write commit; idempotent on reservation_id):
   ```rust
   pub fn confirm_storage(&mut self, reservation_id: ReservationId, actual_bytes: u64) -> Result<(), QuotaError> {
       let entry = self.reservations.remove(&reservation_id)
           .ok_or(QuotaError::ReservationExpired { reservation_id: reservation_id.to_string() })?;
       // actual_bytes may differ from bytes_reserved (compression / actual upload size)
       // Atomic decrement of reserved + increment of bytes_used
       self.bytes_used = self.bytes_used.saturating_add(actual_bytes);
       // Audit emit fail-closed
       Ok(())
   }
   ```

5. **Monthly bandwidth check** (DO `BandwidthTracker-<tenant>-<YYYY-MM>` ou inlined em `Quota-<tenant_id>` com period transition):
   ```rust
   pub fn check_bandwidth(&mut self, direction: BandwidthDirection, bytes: u64, now_ms: i64) -> Result<BandwidthCheckResult, QuotaError> {
       let current_period = utc_period_for_ts(now_ms);   // "YYYY-MM"
       if current_period != self.bandwidth_period {
           // Period boundary transition (1st UTC midnight)
           self.snapshot_bandwidth_to_d1();              // batch ≤ 250 (Lote 10.5bis)
           self.bandwidth_period = current_period;
           self.bandwidth_egress_bytes = 0;
           self.bandwidth_ingress_bytes = 0;
       }
       let counter = match direction {
           BandwidthDirection::Egress => &mut self.bandwidth_egress_bytes,
           BandwidthDirection::Ingress => &mut self.bandwidth_ingress_bytes,
       };
       let max_bytes = match direction {
           BandwidthDirection::Egress => self.max_egress_bytes_monthly,
           BandwidthDirection::Ingress => self.max_ingress_bytes_monthly,
       };
       let new_total = counter.saturating_add(bytes);
       if new_total < max_bytes {                         // strict-<
           *counter = new_total;
           Ok(BandwidthCheckResult { allowed: true, bytes_consumed_this_month: new_total, max_bytes_this_month: max_bytes, period: self.bandwidth_period.clone(), seconds_until_reset: corelink_time::secs_until_next_month_first_utc_midnight(now_ms) })
       } else {
           Err(QuotaError::BandwidthOver {
               period: self.bandwidth_period.clone(),
               consumed: *counter,
               max: max_bytes,
               reset_secs: corelink_time::secs_until_next_month_first_utc_midnight(now_ms),
           })
       }
   }
   ```

6. **Per-PAT rate check camada 3** (token bucket per-PAT; cap = 10× tenant refill_rate):
   ```rust
   // Lote 10.8bis P0-C correction: per-PAT camada 3 é DETECTOR não enforcer.
   // Aggregate rate enforced em camada 1 (WI-S08-001 DO RateLimiter); this method emits SEV-2 alert
   // when per-PAT observed rate exceeds 10× tenant_refill threshold (compromised credential signal).
   // Returns Ok always (request not blocked); admin notification via metric.

   pub fn check_pat_rate(&mut self, pat_id: &PatId, now_ms: i64) -> Result<PatRateResult, QuotaError> {
       let misuse_threshold = pat_rate_misuse_threshold_for_tier(&self.plan_tier);  // = 10× tenant refill_rate
       let state = self.per_pat_state.entry(pat_id.clone()).or_insert(PatRateState::new(now_ms));

       // Update rolling rate (1min window EWMA)
       state.update_rolling_rate(now_ms);

       let observed_rate = state.observed_rate_per_sec;
       let misuse_detected = observed_rate > misuse_threshold;

       if misuse_detected {
           // Emit SEV-2 alert (NOT block request; aggregate enforcement em camada 1)
           emit_metric("corelink.quota.pat_misuse_detected_total", 1.0, &[("pat_id", pat_id.as_str())]);
           audit_emit("corelink.quota.pat_misuse_detected", &PatMisuseEvent {
               pat_id: pat_id.clone(),
               tenant_id: self.tenant_id.clone(),
               observed_rate,
               threshold: misuse_threshold,
               detected_at_ms: now_ms,
           })?;
       }

       // Always allowed at camada 3 (detector); camada 1 owns rate enforcement
       Ok(PatRateResult {
           allowed: true,
           pat_id: pat_id.clone(),
           observed_rate_per_sec: observed_rate,
           misuse_threshold_per_sec: misuse_threshold,
           misuse_detected,
       })
   }

   pub fn pat_rate_misuse_threshold_for_tier(tier: &Tier) -> f64 {
       let (tenant_refill, _burst) = WI_S08_001::refill_rate_for_tier(tier);
       tenant_refill * 10.0   // misuse alarm threshold (NOT cap; aggregate em camada 1)
       // free 100 RPS / solo 500 / team 2000 / business 10000 / enterprise 100000
   }
   ```

7. **Tower middleware integration** em CAS PUT (write paths; storage check + bandwidth ingress) + CAS GET (bandwidth egress) + AC PUT (storage + ingress) + AC GET (egress):
   ```rust
   pub fn quota_layer<S>() -> tower::Layer<S> {
       // Pre-write: check_storage_and_reserve (CAS PUT) ou check_bandwidth (egress) ou check_pat_rate (todos);
       // Post-write success: confirm_storage; failure: release_storage_reservation.
       // 429 + X-Rate-Limit-Type {over_quota | per_pat} + Retry-After.
       // Bandwidth excedence subsumed sob `over_quota` (canonical 5-enum em WI-S08-005; Lote 10.8bis P0-A);
       // audit log `reason` field distingue: storage_hard_block | bandwidth_egress | bandwidth_ingress.
   }
   ```

8. **D1 migrations** (canonical `tenant_storage_state` Lote 10.7bis P0-2 absorbed; NEW tables for bandwidth + PAT rate state):
   ```sql
   -- tenant_storage_state JÁ existe em data_model.md §4.2 (Lote 10.7bis P0-2 NEW).
   -- WI-S08-003 reads/writes existing table; no schema change required for storage.

   CREATE TABLE bandwidth_state (
       tenant_id TEXT NOT NULL,
       period TEXT NOT NULL,                           -- "YYYY-MM"
       egress_bytes INTEGER NOT NULL DEFAULT 0,
       ingress_bytes INTEGER NOT NULL DEFAULT 0,
       last_synced_at INTEGER NOT NULL,                -- unix ms; canonical no _ms suffix per Lote 10.7bis P0-3
       PRIMARY KEY (tenant_id, period),
       CHECK (egress_bytes >= 0),
       CHECK (ingress_bytes >= 0)
   );

   CREATE TABLE bandwidth_history (
       tenant_id TEXT NOT NULL,
       period TEXT NOT NULL,
       egress_bytes INTEGER NOT NULL,
       ingress_bytes INTEGER NOT NULL,
       snapshotted_at INTEGER NOT NULL,
       PRIMARY KEY (tenant_id, period)
   );

   CREATE TABLE quota_reservations (
       reservation_id TEXT PRIMARY KEY,                -- ULID
       tenant_id TEXT NOT NULL,
       bytes_reserved INTEGER NOT NULL,
       expires_at INTEGER NOT NULL,                    -- unix ms; canonical no _ms suffix
       created_at INTEGER NOT NULL,                    -- unix ms; canonical no _ms suffix
       confirmed_at INTEGER,                           -- NULL until confirm_storage; canonical no _ms suffix
       released_at INTEGER,                            -- NULL until release; canonical no _ms suffix
       CHECK (bytes_reserved > 0),
       CHECK (expires_at > created_at),
       CHECK (confirmed_at IS NULL OR confirmed_at >= created_at),
       CHECK (released_at IS NULL OR released_at >= created_at)
   );

   CREATE INDEX idx_reservations_active ON quota_reservations(tenant_id, expires_at)
       WHERE confirmed_at IS NULL AND released_at IS NULL;

   -- Lote 10.8bis P0-C correction: NOT a token bucket (no bucket = no enforcement);
   -- detection-only: stores rolling rate observation per PAT for SEV-2 alert when threshold exceeded.
   CREATE TABLE pat_rate_observation (
       pat_id TEXT PRIMARY KEY,
       tenant_id TEXT NOT NULL,
       observed_rate_per_sec REAL NOT NULL DEFAULT 0,   -- 1min EWMA rolling rate
       last_observation_at INTEGER NOT NULL,            -- unix ms; canonical no _ms suffix
       misuse_threshold_per_sec REAL NOT NULL,          -- = 10× tenant refill_rate (alarm threshold; NOT enforcement)
       last_misuse_detected_at INTEGER,                 -- NULL until detected; canonical no _ms suffix
       last_synced_at INTEGER NOT NULL,                 -- unix ms; canonical no _ms suffix
       CHECK (observed_rate_per_sec >= 0),
       CHECK (misuse_threshold_per_sec > 0),
       CHECK (last_misuse_detected_at IS NULL OR last_misuse_detected_at >= last_observation_at - 60000)
   );

   CREATE INDEX idx_pat_observation_tenant ON pat_rate_observation(tenant_id);
   CREATE INDEX idx_pat_observation_recent_misuse ON pat_rate_observation(last_misuse_detected_at)
       WHERE last_misuse_detected_at IS NOT NULL;
   ```

9. **Audit fail-closed** (Lote 10.6bis pattern absorbed): emit `corelink.quota.{check_storage, confirm_storage, release_storage, check_bandwidth, period_reset, check_pat_rate, pat_misuse_detected}` audit events; fail-closed if audit emit fails.

10. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): `worker::send_future()`; NEVER `tokio::spawn` ou `wasm_bindgen_futures::spawn_local`. `async_lock::Semaphore` if needed (Lote 10.3-tris).

11. **Métricas** (Prometheus convention; aligned RFC 9331):
    - `corelink.quota.storage_check_total{tenant_id, result=allowed|over}` (counter; SLI source).
    - `corelink.quota.storage_utilization_pct{tenant_id}` (gauge; alert SEV-3 ≥ 0.95; SEV-2 ≥ 1.0 — boundary com S-07 ADR-0020).
    - `corelink.quota.reservations_active{tenant_id}` (gauge; alert SEV-3 ≥ 100 sustained — possible orphan reservations).
    - `corelink.quota.bandwidth_consumed_pct{tenant_id, direction=egress|ingress}` (gauge; alert SEV-3 ≥ 0.95).
    - `corelink.quota.bandwidth_period_reset_total{tenant_id}` (counter; informational).
    - `corelink.quota.pat_rate_check_total{tenant_id, pat_id, result=allowed|over}` (counter; SLI source).
    - `corelink.quota.pat_misuse_detected_total{tenant_id, pat_id}` (counter; **alert SEV-2 if any > 0**).
    - `corelink.quota.middleware_duration_us` (histogram; SLO ≤ 5ms p99).
    - `corelink.quota.cross_tenant_violation_total` (counter; **alert SEV-1 if > 0; INV-AVAIL-ISOLATION canary**).

12. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 lesson absorbed):
    - `prop_atomic_cas_strict_lt`: 10k random concurrent reservations at boundary; assert no over-commit; strict-< predicate honored.
    - `prop_reservation_ttl_release`: random TTL + release sequences; assert bytes_used decrements correctly; no orphan.
    - `prop_bandwidth_period_transition`: simulate 1st-UTC-midnight crossing at random ms offsets; assert deterministic reset; old period snapshotted to D1.
    - `prop_pat_rate_cap`: 1k PATs × 1k requests each; assert per-PAT ≤ 10× tenant cap.
    - `prop_concurrent_reservations_serialization`: 1000 concurrent check_and_reserve same DO; assert serialization (DO actor); deterministic outcome.
    - `prop_reservation_expiry_safety`: clock skew + TTL boundary; assert no spurious release; monotonic clamp.

13. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 12):
    - 1. **Storage 100% hard-block** (tenant bytes_used = 99% sustained; eviction lag) → S-07 trigger insufficient; S-08 100% hard-block kicks in; 429 + over_quota.
    - 2. **Reservation orphan** (reserve, write fails, no release call): DO alarm sweep auto-releases at TTL; bytes_used unchanged; integration test verifies.
    - 3. **Plan downgrade max_storage decreased**: pre-existing reservations honored; new check_and_reserve fails immediately; eviction (S-07) drives bytes_used down; no data loss.
    - 4. **Bandwidth reset race at midnight** (uploads at 23:59:59.999 UTC + 00:00:00.001 UTC): chrono atomic check; deterministic period assignment; no double-counting.
    - 5. **Multipart upload 160 GiB at 100 Mbps**: reservation TTL = 218min × 2x safety = ~7.3h; under 7d cap; reservation persists full upload duration; bytes_used delta correct on completion.
    - 6. **PAT compromise** (single PAT 50× tenant rate): camada 3 cap 10× kicks in; 80% requests rejected; SEV-2 alert; admin revokes.
    - 7. **DO restart amid in-flight reservations**: durable storage persists; cold start recovers state from D1 snapshot; alarm re-armed AT START; pending alarms re-scheduled.
    - 8. **D1 outage 30min**: DO continues authoritative; reservations + bytes_used in-memory; on D1 recovery, sync resumes; no enforcement gap.
    - 9. **Audit emit fail mid-confirm**: confirm_storage rolls back? NO — fail-closed bias toward over-counting (charge tenant for legitimate use); reconcile catches drift; SEV-1 alert.
    - 10. **Bandwidth reset partial completion** (DO crash mid-snapshot to D1): D1 snapshot row may be incomplete; on DO recovery, retry snapshot atomic; idempotent on (tenant_id, period) PK.
    - 11. **Concurrent reservation + confirm + release race**: DO actor serializes; deterministic outcome; final bytes_used reflects only confirmed writes; released reservations no-op on bytes_used.
    - 12. **Cross-tenant injection attempt**: TenantCtx says T1; request body claims T2; middleware uses T1 (Lote 10.4bis); T2 quota state untouched.

### 6.2 Out-of-scope (deferred)

- Customer self-service quota dashboard UI (S-13 admin plane).
- Quota auto-grow on demand (anti-pattern; explicit upgrade flow in S-10 billing).
- Per-PAT scope-aware rate (different caps per scope read/write); deferred S-09+.
- Cross-region quota federation (per-region independent; deferred S-14).
- Quota refund on evicted content (S-07 boundary; S-07 owns refund post-eviction; sprint contract §10 anti-scope).
- Cost-based quota (USD per hour); deferred S-10 billing.

## 7. Anti-Scope

- ❌ D1-only atomic CAS (latency unacceptable; SQLite WAL serialization).
- ❌ Phantom column `tenant_quota.bytes_used` (Lote 10.7bis P0-2 REJECTED; canonical `tenant_storage_state.bytes_used`).
- ❌ Non-strict ≤ predicate (boundary race; Lote 10.7bis P0-6 lesson; strict-< canonical).
- ❌ 60s fixed reservation TTL (multipart fail; size-proportional canonical Lote 10.7bis R5 P0-2).
- ❌ Sliding-window bandwidth (gaming; calendar-month canonical sprint contract §10.s08.5).
- ❌ Cross-tenant DO sharing.
- ❌ TenantCtx bypass.
- ❌ `tokio::spawn` em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ Skip alarm re-arm AT START (Lote 10.4bis).
- ❌ Skip strict-< predicate (race-aware boundary).

## 8. Acceptance Criteria (Gherkin) — 11 scenarios

```gherkin
Feature: Quota Checker Middleware (Storage + Bandwidth + Per-PAT)

  Scenario: Storage check_and_reserve within quota
    Given tenant T (team plan; max_storage_bytes=100 GiB) com bytes_used=90 GiB
    Given no active reservations
    When check_storage_and_reserve(request_bytes=5 GiB)
    Then DO atomic predicate: 90+5=95 < 100 (strict-<)
    Then reservation_id ULID generated; expires_at = now + reservation_ttl(5 GiB)
    Then StorageReservation { bytes_reserved=5GiB, current_used_bytes=90GiB, utilization_pct=0.95 }
    Then SEV-3 alert (boundary com S-07 ≤95% trigger)

  Scenario: Storage hard-block at 100%
    Given tenant T bytes_used=99.5 GiB; max=100 GiB; no active reservations
    When check_storage_and_reserve(request_bytes=1 GiB)
    Then 99.5+1=100.5 ≥ 100 (strict-<)
    Then Err(StorageOver { used=99.5GiB, max=100GiB, request=1GiB, overshoot=0.5GiB+1B })
    Then middleware response 429 + X-Rate-Limit-Type: over_quota + Retry-After: days-until-reset
    Then SLI metric: NOT counted in numerator (legitimate over-quota; sprint contract §7.10.s08.1)

  Scenario: Reservation TTL size-proportional (multipart 160 GiB)
    Given upload 160 GiB blob at 100 Mbps assumed
    When reservation_ttl(160 GiB) computed
    Then ttl = max(60s, 160e9/1e6 × 2) = 320000s ~ 89h
    Then bounded to 7d max → ~89h ≤ 168h → 89h
    Then reservation persists full multipart upload (~218min @ 100 Mbps)
    Then no orphan; on completion confirm_storage commits bytes_used delta

  Scenario: Reservation orphan auto-release
    Given reservation R created at T0 (TTL 60s; 1 KiB write)
    Given write process crashed at T0+10s (no release call)
    When DO alarm fires at T0+60s
    Then sweep detects expired R; auto-release; bytes_used unchanged
    Then audit emit corelink.quota.release_storage; reason=ttl_expired

  Scenario: Bandwidth monthly reset at 1st UTC midnight
    Given tenant T bandwidth_period="2026-04"; egress=80 GiB consumed
    When system clock crosses 2026-05-01 00:00:00.000Z
    Then DO check_bandwidth detects current_period="2026-05" != stored "2026-04"
    Then snapshot bandwidth_egress=80GiB to bandwidth_history table (PK: tenant_id, period)
    Then reset egress_bytes=0; ingress_bytes=0; bandwidth_period="2026-05"
    Then audit emit corelink.quota.period_reset

  Scenario: Bandwidth over (95% near limit)
    Given tenant T bandwidth_egress=95 GiB / 100 GiB monthly cap
    When download 10 GiB blob (egress)
    Then 95+10=105 ≥ 100 (strict-<)
    Then Err(BandwidthOver { period="2026-04", consumed=95GiB, max=100GiB, reset_secs=secs_until_2026-05-01_00:00:00Z })
    Then 429 + X-Rate-Limit-Type: over_quota + Retry-After: reset_secs (canonical 5-enum; audit log reason=bandwidth_egress_exceeded discrimina from storage_hard_block; Lote 10.8bis P0-A)

  Scenario: PAT misuse DETECTION (compromised PAT) — Lote 10.8bis P0-C correction
    Given tenant T (team plan; refill_rate=200 RPS aggregate, enforced em camada 1)
    Given misuse_threshold = 200 × 10 = 2000 RPS (alarm threshold; NOT enforcement cap)
    Given PAT-A floods attempting 5000 RPS
    When camada 1 (WI-S08-001 DO) throttles tenant aggregate to 200 RPS sustained (96% of attacker requests 429 com X-Rate-Limit-Type: tenant_quota)
    When camada 3 (this WI) check_pat_rate(PAT-A) per request observes per-PAT rate
    Then per-PAT observed_rate > 2000 RPS misuse_threshold (attacker intent visible em request rate)
    Then misuse_detected = true; SEV-2 alert: corelink.quota.pat_misuse_detected_total{pat_id=PAT-A}
    Then audit emit corelink.quota.pat_misuse_detected (PAT-A; tenant T; observed_rate; threshold)
    Then admin revokes PAT-A (S-03 RUNBOOK-AUTH-003)
    Then camada 3 returns Ok (request NOT blocked at camada 3; camada 1 owns aggregate enforcement)

  Scenario: Concurrent reservations boundary race
    Given tenant T bytes_used=99 GiB; max=100 GiB
    Given 2 concurrent check_storage_and_reserve(request_bytes=0.6 GiB each)
    When DO actor serializes
    Then first reservation: 99+0.6=99.6 < 100 → granted
    Then second reservation: 99+0.6+0.6=100.2 ≥ 100 → Err(StorageOver) (active_reserved counted)
    Then NO over-commit (strict-< race-aware predicate)

  Scenario: Plan downgrade max_storage decreased
    Given tenant T plan team→solo (max_storage 100 GiB → 20 GiB)
    Given tenant bytes_used = 50 GiB (over new limit; was under old)
    When new check_storage_and_reserve(1 GiB)
    Then 50+1=51 ≥ 20 → Err(StorageOver)
    Then existing reservations honored (TTL); no retroactive cancel
    Then S-07 eviction kicks in (≤95% trigger; soft pressure → drives bytes_used down)
    Then no data loss (eviction is soft-delete grace per S-06 INV-GC-004)

  Scenario: Cross-tenant injection blocked
    Given attacker JWT for tenant T1
    Given request body claims tenant_id=T2
    When middleware extracts TenantCtx (T1; Lote 10.4bis)
    Then DO `Quota-<T1>` invoked (NOT T2)
    Then T2 quota state untouched
    Then audit emit corelink.quota.cross_tenant_attempt; SEV-1 alert

  Scenario: Audit fail-closed mid-confirm
    Given check_storage_and_reserve succeeds; reservation R granted
    Given write succeeds; confirm_storage called
    Given audit_outbox INSERT fails (D1 throttle)
    When middleware completes
    Then bytes_used updated in DO memory (NOT rolled back; eventual consistency)
    Then SEV-1 alert; reconcile job catches drift
    Note: fail-closed bias toward over-counting (charge tenant); reconcile detects audit gap
```

## 9. Design Decisions

- 9.1: DO actor model (NOT D1 atomic CAS) — sub-ms + race-free serialization.
- 9.2: Single DO `Quota-<tenant_id>` shared com S-07 WI-S07-003 reservation pattern (cold-start cost amortized).
- 9.3: Race-aware strict-< predicate (S-06 INV-GC-004 + S-07 WI-S07-002 lessons absorbed).
- 9.4: Size-proportional reservation TTL (Lote 10.7bis R5 P0-2 absorbed).
- 9.5: Canonical `tenant_storage_state.bytes_used` (Lote 10.7bis P0-2 NEW table; sprint contract phantom REJECTED).
- 9.6: ADR-0020 FROZEN boundary com S-07 (Lote 10.7bis Phase 3 absorbed).
- 9.7: Calendar-month bandwidth reset (NOT sliding window; sprint contract §10.s08.5).
- 9.8: Per-PAT camada 3 é misuse DETECTOR não enforcer (Lote 10.8bis P0-C correction; aggregate enforcement em camada 1 WI-S08-001 DO; threshold = 10× tenant refill alarm signal; sprint contract §5 R-S08-3 amend queued Phase 6).
- 9.9: TenantCtx-only (Lote 10.4bis); pat_id from same TenantCtx.
- 9.10: Audit fail-closed bias toward over-counting (Lote 10.6bis pattern adapted).
- 9.11: NEW migrations bandwidth_state + bandwidth_history + quota_reservations + pat_rate_state.
- 9.12: NO new ADR (extends ADR-0020 boundary; CTRL-QUOTA-001 + CTRL-RATE-001 canonical).
- 9.13: Sprint contract §5 R-S08-5 phantom column will require correction in `tris` cycle review feedback.

## 10. Completeness Criteria SOTA

- [ ] **10.s08.003.1** Module compila + integration tests green.
- [ ] **10.s08.003.2** All 11 Gherkin scenarios green.
- [ ] **10.s08.003.3** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- [ ] **10.s08.003.4** Chaos suite 12 scenarios green.
- [ ] **10.s08.003.5** **INV-QUOTA-ENFORCEMENT 30d sustained chaos zero violations** (sprint contract §6 DoD).
- [ ] **10.s08.003.6** Quota check overhead ≤ 5ms p99 criterion benchmark.
- [ ] **10.s08.003.7** Reservation TTL formula validated em multipart upload (160 GiB @ 100 Mbps).
- [ ] **10.s08.003.8** Bandwidth period reset deterministic via `corelink_time::next_month_first_utc_midnight()` (Lote 10.8bis P0-D correct chrono primitive; property test boundary: Jan 1, Dec 31 → year increment, Feb 28 non-leap, Feb 28/29 leap year).
- [ ] **10.s08.003.9** Per-PAT cap detected misuse (5x tenant rate) within 5min SEV-2 alert.
- [ ] **10.s08.003.10** Métricas (9) emitted; cross_tenant_violation_total alerts SEV-1 if > 0; pat_misuse_detected SEV-2.
- [ ] **10.s08.003.11** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s08.003.12** Cost regression gate per-check ≤ $0.000001.
- [ ] **10.s08.003.13** **D1 migrations** (4 NEW tables; CHECK constraint inline per Lote 10.5bis; D1 batch ≤250).
- [ ] **10.s08.003.14** Boundary com S-07 ADR-0020 verified em integration test (≤95% trigger eviction; 100% hard-block transition).

## 11. DoD

- [ ] Module compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK).

## 12. Invariants Validated

- **INV-QUOTA-ENFORCEMENT** (HIGH; registry §3.11): atomic enforcement via DO actor; race-aware strict-< predicate; chaos test 30d zero violations.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant DO; cross-tenant impossible.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): inherited via DO ID + TenantCtx middleware.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+): all quota mutations audited via S-04 corelink.audit_log.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| QuotaChecker module | `crates/corelink-quota/` | Rust |
| DO singleton impl (extended) | `crates/corelink-quota/src/do_singleton.rs` | Rust |
| Tower middleware | `crates/corelink-worker/src/middleware/quota.rs` | Rust |
| D1 migrations | `migrations/00X_bandwidth_state.sql`, `migrations/00X_bandwidth_history.sql`, `migrations/00X_quota_reservations.sql`, `migrations/00X_pat_rate_state.sql` | SQL |
| Property tests | `crates/corelink-quota/tests/prop_quota.rs` | Rust |
| Chaos suite | `tests/chaos_quota.rs` | Rust |
| Wrangler DO binding | `wrangler.toml` (extends `Quota-<tenant_id>` from S-07) | TOML |

## 14. Quality Standards SOTA

- 14.s08.003.1: Zero unsafe; zero unwrap em production paths.
- 14.s08.003.2: rustdoc 100% public API.
- 14.s08.003.3: Test coverage ≥ 90%.
- 14.s08.003.4: Latência: middleware ≤ 5ms p99; DO check ≤ 2ms p99.
- 14.s08.003.5: SAST clean.
- 14.s08.003.6: Métricas (9 §6.1.11).
- 14.s08.003.7: Memory bounded ≤ 5KB DO state per tenant (reservations + bandwidth + per-PAT).
- 14.s08.003.8: Cost regression gate per-check ≤ $0.000001.
- 14.s08.003.9: TenantCtx-only (Lote 10.4bis); DO routing primary_region (Lote 10.7bis P0-9); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical (Lote 10.7bis P0-7); column drift no `_ms` suffix (Lote 10.7bis P0-3); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis).
- 14.s08.003.10: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s08.003.11: Race-aware strict-< predicate (S-06 INV-GC-004 + S-07 WI-S07-002 lessons absorbed).
- 14.s08.003.12: Size-proportional reservation TTL (Lote 10.7bis R5 P0-2 absorbed).

## 15. Chaos Experiments (12)

§6.1.13 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR; sprint contract §6 DoD enforces.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + QuotaChecker trait | 1.5 |
| ST-002 | DO singleton state + check_and_reserve atomic CAS | 3 |
| ST-003 | confirm_storage + release_storage + reservation TTL alarm | 2 |
| ST-004 | Bandwidth check + period reset chrono | 2 |
| ST-005 | Per-PAT rate camada 3 (token bucket) | 1.5 |
| ST-006 | Tower middleware integration | 2 |
| ST-007 | D1 migrations (4 new tables) | 1 |
| ST-008 | Métricas (9) emit | 1 |
| ST-009 | Property tests (6 × 10k; 100k nightly) | 4 |
| ST-010 | Chaos suite (12) | 3 |
| ST-011 | Crypto SME advisory review (race correctness atomic CAS) | 1 |
| ST-012 | Sprint contract correction sketch (phantom column tris feedback) | 0.5 |

**Total**: ~22.5h. **PERT** O=18h M=20h P=28h: **~21h** (sprint contract estimate 16h; revised upward por scope: 4 migrations + 3 enforcement domains + race-aware + size-proportional TTL).

## 18. Dependencies

- Hard: S-03 SEALED (TenantCtx middleware com pat_id); S-07 WI-S07-003 SEALED (DO `Quota-<tenant_id>` shared); ADR-0020 FROZEN (boundary; Lote 10.7bis Phase 3); WI-S08-001 SEALED (per-PAT cap formula 10× tenant refill_rate canonical from §3.1).
- Soft: S-09 (PagerDuty integration; SEV-1/2 alerts); S-13 (admin endpoint plan_id sync; staging stub OK); WI-S08-005 (RFC 9331 headers); WI-S08-006 (DASH-RATE consumes metrics).

## 19. Effort PERT: ~21h. ## 20. Time-boxing: 28h hard limit.

## 21. Observability

9 metrics §6.1.11. Trace span `quota.{check_storage, confirm, release, check_bandwidth, period_reset, check_pat, cold_start}`.

## 22. Cost Analysis

- Per-check: ~$0.000001 (DO read + write + occasional D1 sync).
- TCO 12m: 5 regions × 10k tenants × 10k requests/sec × 86400 × 365 × $0.000001 = ~$1576800/yr — significant but constrained by per-region per-tenant scale.
- DO storage: ~5KB × 100k tenants = 500 MB total cluster — under 32 MiB per DO instance (1 instance per tenant).
- D1 storage: bandwidth_state monthly × 12 = ~5 KB/tenant/year; reservations transient (TTL release).
- **Cost saved by hard-block**: tenant infinite write would cost $0.005/GiB R2 storage indefinite; quota enforcement prevents.

## 23. API Contract

- Public: `QuotaChecker` trait + `StorageReservation`, `BandwidthCheckResult`, `PatRateResult`, `QuotaError` types; `#[non_exhaustive]`.
- HTTP: 429 + `Retry-After` + `X-Rate-Limit-Type {over_quota | per_pat}` (canonical 5-enum em WI-S08-005; bandwidth excedence subsumed sob `over_quota`; audit log `reason` field discriminates storage_hard_block vs bandwidth_{egress,ingress} for observability; Lote 10.8bis Phase 1 P0-A) (delegate WI-S08-005 RFC 9331 headers wrap).

## 24. Post-mortem Hooks

- INV-QUOTA-ENFORCEMENT violation detected (silent over-write) → CRITICAL post-mortem.
- INV-AVAIL-ISOLATION violation (cross_tenant_violation > 0) → CRITICAL post-mortem.
- Reservation orphan rate > 0.1% sustained → SEV-2; alarm sweep gap.
- Bandwidth period reset failure (chrono drift) → SEV-1; data loss potential.
- PAT misuse detected for enterprise customer → no-blame post-mortem; calibration signal.
- Customer "rate-limited within plan" (within-plan 429 anomaly) → bug investigation.
- 95-100% storage transition window detected eviction lag (S-07 trigger insufficient → S-08 hard-block engages) → SEV-3; eviction policy review.

## 25. Rollback / Recovery

- Rollback: revert Tower middleware mount; quota enforcement disabled; bulkhead camada 3 lost (cost overhead but no breakage).
- Recovery: DO state durable; cold start recovers from D1 snapshot; reservations TTL alarm re-armed AT START.
- RTO ≤ 5min; RPO ≤ 5min (D1 snapshot interval).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx middleware S-03; PAT scope check.
- T(ampering): DO actor model + audit append-only.
- R(epudiation): audit fail-closed; all quota mutations traceable.
- I(nformation disclosure): bytes_used metric per-tenant; no cross-tenant disclosure.
- D(enial of Service): quota enforcement IS DoS mitigation (S-08 purpose).
- E(scalation of Privilege): per-PAT cap detects compromised PAT.

**LINDDUN**:
- L(inkability): per-tenant DO; no cross-tenant linkability.
- I(dentifiability): pat_id in metrics; redact in audit logs (privacy_model.md).
- N(on-repudiation): audit append-only.
- D(etectability): per-PAT misuse detection.
- D(isclosure): bytes_used not customer-PII.
- U(nawareness): customer self-service via S-13 dashboard (deferred).
- N(on-compliance): LGPD Art. 20 N/A (quota enforcement is automated but not "significant decision affecting individual"); GDPR Art. 22 N/A.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-08 Quota Checker: Atomic CAS via DO Actor + Race-Aware Predicate + ADR-0020 Boundary"; doc `docs/dev/quota-architecture.md`; onboarding test 8 questions: DO actor vs D1 CAS rationale, race-aware strict-< predicate (S-06+S-07 lessons), size-proportional TTL (Lote 10.7bis R5), canonical tenant_storage_state vs phantom tenant_quota.bytes_used (P0-2), ADR-0020 boundary 95-100%, calendar-month bandwidth reset determinism, per-PAT cap = 10× tenant refill, audit fail-closed bias toward over-counting.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-QUOTA-ENFORCEMENT violation (silent over-write) | L | M | CRITICAL | L | LOW | DO actor + strict-< + chaos 30d; SEV-1 alert |
| R-002 | INV-AVAIL-ISOLATION violation cross-tenant | L | M | CRITICAL | L | LOW | Per-tenant DO; chaos test |
| R-003 | Reservation orphan accumulation | M | L | MEDIUM | L | LOW | DO alarm sweep TTL release; alert SEV-3 ≥ 100 active |
| R-004 | Race at boundary (concurrent reservations) | L | L | MEDIUM | L | LOW | Strict-< predicate (S-06+S-07 absorbed); property test 100k |
| R-005 | Bandwidth period reset race at midnight | L | L | LOW | L | LOW | chrono atomic; deterministic period assignment |
| R-006 | Plan downgrade orphan reservations | L | L | LOW | L | LOW | TTL honored; eviction (S-07) drives bytes_used down |
| R-007 | PAT cap false-positive (legitimate burst) | M | M | MEDIUM | L | LOW | Burst capacity 5× cap; tunable via admin S-13 |
| R-008 | DO cold start latency > 50ms | M | L | LOW | L | LOW | Sticky placement; cache D1 5min em DO |
| R-009 | Audit fail silently | L | M | MEDIUM | L | LOW | Fail-closed bias; reconcile catches |
| R-010 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3 lesson; worker::send_future |
| R-011 | Phantom column tenant_quota.bytes_used regression | L | M | MEDIUM | L | LOW | Lote 10.7bis P0-2 absorbed; canonical tenant_storage_state; sprint contract correction tris cycle |
| R-012 | Cost regression > 10% quota check | M | L | MEDIUM | L | LOW | §14.s08.003.8 gate |

## 29. Review Checkpoints

D+0 design (Architect; race-aware predicate); D+2 AppSec (TenantCtx + audit + PAT cap); D+3 Crypto SME (race correctness atomic CAS); D+4 code review; D+6 chaos validation; D+7 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — INV-AVAIL-ISOLATION + per-PAT misuse detection_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos + property test 100k race_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory** — quota enforcement boundary ADR-0020_ |
| 10 | Privacy | _TBD; **mandatory** — pat_id redaction em audit logs_ |
| 11 | Architect | _TBD; **mandatory** — DO actor + race-aware predicate + 95-100% boundary_ |
| 12 | Crypto SME | _**mandatory** — race correctness atomic CAS (NOT cripto-load-bearing per se; race-correctness IS load-bearing)_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.8) | Criação WI-S08-003; HIGH_RISK; SOTA pós-Lote 10.7bis lessons absorbed: canonical tenant_storage_state.bytes_used (P0-2; sprint contract phantom REJECTED — correction queued tris cycle); race-aware strict-< predicate (S-06 INV-GC-004 + S-07 WI-S07-002 lessons); size-proportional reservation TTL (R5 P0-2); ADR-0020 FROZEN boundary com S-07 (Phase 3); DO routing primary_region (P0-9); 5-tier canonical (P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (P1-3); audit fail-closed (Lote 10.6bis); alarm re-arm AT START (Lote 10.4bis); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis); column drift no `_ms` suffix (P0-3); chrono `tomorrow_at_utc_midnight()` deterministic (Lote 10.5bis). NEW migrations bandwidth_state + bandwidth_history + quota_reservations + pat_rate_state. DO `Quota-<tenant_id>` shared com S-07 WI-S07-003 reservation pattern (cold-start cost amortized). Per-PAT cap 10× tenant refill_rate camada 3 of 4 PAT-RATE-LIMIT-001. |

## 32. Anti-patterns evitados

- ❌ D1 atomic CAS (latency unacceptable); ❌ Phantom column tenant_quota.bytes_used (Lote 10.7bis P0-2 REJECTED); ❌ Non-strict ≤ predicate (boundary race); ❌ 60s fixed TTL (multipart fail; Lote 10.7bis R5 P0-2); ❌ Sliding-window bandwidth (gaming); ❌ Cross-tenant DO; ❌ TenantCtx bypass; ❌ tokio::spawn em CF Workers; ❌ Skip alarm re-arm AT START; ❌ Skip strict-< race-aware predicate; ❌ Audit fail-open (silent over-write).

---

**Fim WI-S08-003.** Próximo: WI-S08-004 (Abuse detection heurística + scoring multi-feature; humane LGPD response; CAP-ABUSE-001 + CAP-ABUSE-002).
