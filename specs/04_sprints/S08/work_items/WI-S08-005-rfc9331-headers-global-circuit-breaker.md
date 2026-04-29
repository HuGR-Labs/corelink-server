---
id: "WI-S08-005"
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
  - "OBSERVABILITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
tags: ["wi", "s08", "rfc-9331", "rate-limit-headers", "global-circuit-breaker", "sli-distinction", "high-risk"]
---

# WI-S08-005 — Response Code Types + RFC 9331 RateLimit Headers + Global Circuit Breaker Camada 4 (`crates/corelink-rate-limit-headers` + `crates/corelink-global-circuit`; consolidates 429 responses from camadas 1-3 WI-S08-001/002/003 + camada 4 global circuit; RFC 9331 (IETF RateLimit + RateLimit-Policy headers stable; supersedes custom X-RateLimit-* legacy); 5 X-Rate-Limit-Type discriminators tenant_quota|per_ip|per_pat|over_quota|global_circuit_open sprint contract §5 R-S08-8; SLI distinction critical: within-plan-429 counted em SLI numerator (bug; sprint contract §7.10.s08.1) vs over-quota-429 NOT counted (legitimate); Retry-After RFC 6585 always present; global circuit breaker DO `GlobalRateLimiter-<region>` trip em error_rate > 50% sustained 5min com hysteresis para prevent flapping; degrade_mode=emergency fallback; multi-signal trigger sprint contract §15 R-S08-004 + standard circuit-breaker hysteresis pattern (Netflix Hystrix / Resilience4j canonical); CAP-RATE-004 system-wide DoS mitigation)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-08](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S08-005 |
| Título | Tower middleware response wrapper consolidando 429 de camadas 1-3 (WI-S08-001 per-tenant DO + WI-S08-002 CF edge per-IP + WI-S08-003 quota + per-PAT) + camada 4 global circuit breaker DO `GlobalRateLimiter-<region>`; RFC 9331 IETF headers `RateLimit: limit=<n>, remaining=<n>, reset=<seconds>` + `RateLimit-Policy: <window>;w=<seconds>` (supersedes legacy custom `X-RateLimit-*` per IETF stability); 5 discriminators `X-Rate-Limit-Type` tenant_quota|per_ip|per_pat|over_quota|global_circuit_open (sprint contract §5 R-S08-8); SLI distinction CRITICAL (sprint contract §7.10.s08.1): within-plan-429 (`tenant_quota` type) → emite `corelink_rate_limited_within_quota_total` → conta em SLO-AVAIL-CAS-GET denominador (é bug nosso = SLI failure); over-quota-429 (`over_quota` type) → emite `corelink_rate_limited_over_quota_total` → NÃO conta no denominador (legitimate; tenant excedeu plan; SLI pass); `Retry-After` RFC 6585 always present (seconds-until-refill realistic; sprint contract §5 R-S08-9); global circuit breaker DO trip em error_rate > 50% sustained 5min via multi-signal trigger sprint contract §15 R-S08-004 + standard hysteresis pattern (NOT single metric — combination 5xx + p99-latency + DO error_rate); hysteresis recovery 90% under threshold sustained 2min antes de re-engage; degrade_mode=emergency fallback returns 429 todos requests (graceful shed); CAP-RATE-004 system-wide DoS mitigation |
| Sprint | S-08 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-RATE-001 camada 4 global circuit é security control; bypass = catastrophic AVAIL outage cross-tenant); FF-HR-002 (global circuit FP catastrophic full outage; sprint contract §15 R-S08-004) |

## 1. Intent

Response wrapper + RFC 9331 + global circuit constituem a **system-wide observability + emergency primitive** do CoreLink: (a) headers padronizados RFC 9331 garantem clientes Bazel/Buck2 retry corretamente; (b) X-Rate-Limit-Type 5 discriminators permite SLI distinction within-quota vs over-quota (sprint contract §7.10.s08.1 SLI correctness); (c) global circuit breaker é the last-resort defense (camada 4 of 4 PAT-RATE-LIMIT-001) quando camadas 1-3 são insuficientes ou system-wide failure inicia (cascading DOS / control-plane outage). Hysteresis previne flapping (single-metric trip = false-positive vulnerability).

```rust
// File: crates/corelink-rate-limit-headers/src/lib.rs

#![forbid(unsafe_code)]

/// Tower middleware: wraps response with RFC 9331 headers + X-Rate-Limit-Type discrimination
/// + Retry-After. Idempotent on 429 responses from camadas 1-4.
pub fn rate_limit_headers_layer<S>() -> tower::Layer<S> { /* ... */ }

pub struct RateLimitHeaders {
    pub limit: u64,                                  // RFC 9331 RateLimit limit field
    pub remaining: u64,                              // RFC 9331 RateLimit remaining field
    pub reset_seconds: u64,                          // RFC 9331 RateLimit reset field
    pub policy_window_seconds: u64,                  // RFC 9331 RateLimit-Policy w field
    pub retry_after_seconds: u64,                    // RFC 6585 Retry-After
    pub x_rate_limit_type: RateLimitType,            // 5 discriminators
}

#[derive(strum::Display)]
pub enum RateLimitType {
    #[strum(serialize = "tenant_quota")]
    TenantQuota,                                     // WI-S08-001 per-tenant DO; SLI failure bug
    #[strum(serialize = "per_ip")]
    PerIp,                                           // WI-S08-002 CF edge; legitimate edge
    #[strum(serialize = "per_pat")]
    PerPat,                                          // WI-S08-003 PAT misuse; legitimate
    #[strum(serialize = "over_quota")]
    OverQuota,                                       // WI-S08-003 storage 100% / bandwidth; legitimate; NOT SLI fail
    #[strum(serialize = "global_circuit_open")]
    GlobalCircuitOpen,                               // this WI camada 4 emergency; SLI fail
}

impl RateLimitType {
    /// Returns true if this 429 type is bug nosso (SLI failure denominator counted)
    /// vs legitimate (NOT counted). Lote 10.8bis P1-3 (R5): GlobalCircuitOpen requires
    /// trip_reason context — ManualOverride trips (planned drill / load shed) are
    /// intentional and excluded from SLI denominator.
    pub fn counts_against_sli(&self, trip_reason: Option<&TripReason>) -> bool {
        match self {
            RateLimitType::TenantQuota => true,           // bug nosso
            RateLimitType::GlobalCircuitOpen => match trip_reason {
                Some(TripReason::ManualOverride { .. }) => false,  // planned drill; SLI exclusion
                _ => true,                                          // bug nosso (system overload)
            },
            RateLimitType::PerIp => false,                // legitimate edge
            RateLimitType::PerPat => false,               // legitimate PAT misuse
            RateLimitType::OverQuota => false,            // legitimate over-plan
        }
    }
}

// File: crates/corelink-global-circuit/src/lib.rs

#[async_trait]
pub trait GlobalCircuitBreaker: Send + Sync {
    /// Atomic check + record observation; returns Ok(allowed) if circuit closed,
    /// Err(CircuitOpen) if tripped; race-free via DO actor.
    async fn check_and_record(
        &self,
        observation: HealthObservation,
    ) -> Result<CircuitState, CircuitError>;

    /// Manual override (admin S-13): force circuit open OR closed (emergency control).
    async fn manual_override(
        &self,
        admin_ctx: &AdminCtx,
        target_state: CircuitState,
        reason: String,
    ) -> Result<(), CircuitError>;

    /// Get current state + recent observations (for dashboard WI-S08-006).
    async fn get_state(&self) -> Result<CircuitStateSnapshot, CircuitError>;
}

pub struct HealthObservation {
    pub timestamp_ms: i64,                           // unix ms
    pub status: ObservationStatus,                   // Success | ClientError4xx | ServerError5xx
    pub latency_p99_us: u64,                         // microseconds
    pub do_error_rate_5m: f64,                       // 5min DO error rate observed [0.0, 1.0]
}

pub enum CircuitState {
    Closed,                                          // normal; all requests pass
    Open { tripped_at_ms: i64, reason: TripReason },  // emergency; all requests 429 GlobalCircuitOpen
    HalfOpen { since_ms: i64 },                      // hysteresis; sample 10% requests
}

pub enum TripReason {
    Error5xxRateExceeded { observed: f64, threshold: f64 },
    LatencyP99Exceeded { observed_us: u64, threshold_us: u64 },
    DoErrorRateExceeded { observed: f64, threshold: f64 },
    MultiSignalCombined,                              // ≥ 2 signals tripped (sprint contract §15 R-S08-004 multi-signal canonical (Netflix Hystrix pattern))
    ManualOverride { admin_id: String, reason: String },
}

pub struct CircuitStateSnapshot {
    pub state: CircuitState,
    pub recent_observations: Vec<HealthObservation>,
    pub trips_history: Vec<(i64, TripReason)>,        // (tripped_at_ms, reason)
    pub current_5xx_rate: f64,
    pub current_p99_us: u64,
    pub current_do_error_rate: f64,
}

#[derive(thiserror::Error, Debug)]
pub enum CircuitError {
    #[error("circuit open: tripped_at={tripped_at_ms} reason={reason}; retry_after={retry_after_secs}s")]
    CircuitOpen { tripped_at_ms: i64, reason: String, retry_after_secs: u64 },

    #[error("DO backend error: {0}")]
    DoBackendError(String),

    #[error("audit emit failed (fail-closed): {0}")]
    AuditEmitFailed(String),

    #[error("admin authorization failed: {0}")]
    AdminAuthFailed(String),
}
```

**Cripto-driven invariants enforced**:

1. **RFC 9331 IETF compliance** (sprint contract §6 DoD §14.s08.5):
   - `RateLimit: limit=<n>, remaining=<n>, reset=<seconds>` (IETF stable; supersedes legacy `X-RateLimit-*`).
   - `RateLimit-Policy: <window>;w=<seconds>` (policy disclosure; e.g., `100;w=60` = 100 requests per 60s window).
   - Always present on 429 responses; optionally on 200 responses (informational; client tracking).
   - Custom `X-Rate-Limit-Type` (CoreLink-specific; 5 discriminators) is **adicional** ao RFC 9331; coexistem.

2. **5 discriminators canonical** (sprint contract §5 R-S08-8 absorbed):
   - `tenant_quota` (WI-S08-001 per-tenant DO) — SLI failure bug nosso.
   - `per_ip` (WI-S08-002 edge) — legitimate edge defense.
   - `per_pat` (WI-S08-003 PAT misuse) — legitimate.
   - `over_quota` (WI-S08-003 storage/bandwidth 100%) — legitimate over-plan.
   - `global_circuit_open` (this WI camada 4 emergency) — SLI failure (system overload).

3. **SLI distinction CRITICAL** (sprint contract §7.10.s08.1 absorbed):
   - **Within-plan 429** (`tenant_quota` + `global_circuit_open`): emite `corelink_rate_limited_within_quota_total` → conta em SLO-AVAIL-CAS-GET denominador → SLI failure.
   - **Legitimate 429** (`per_ip` + `per_pat` + `over_quota`): emite `corelink_rate_limited_over_quota_total` → NÃO conta no denominador → SLI pass.
   - Distinction critical for error budget correctness; senão error budget exhausts em legitimate over-quota scenarios + obscures real failures.

4. **Retry-After RFC 6585 always present** (sprint contract §5 R-S08-9 absorbed):
   - Realistic seconds-until-refill computed por discriminator type:
     - `tenant_quota`: `(amount_needed - tokens_remaining) / refill_rate_per_sec`.
     - `per_ip`: 60s (CF Ruleset mitigation_timeout).
     - `per_pat`: `(amount_needed - tokens_remaining) / pat_cap_rate`.
     - `over_quota` (storage): days-until-month-reset (or 0 if pay-per-use).
     - `over_quota` (bandwidth): seconds-until-next-month-1st-UTC (chrono crate Lote 10.5bis).
     - `global_circuit_open`: 60s minimum (hysteresis recovery sample interval).

5. **Global circuit breaker multi-signal trigger** (sprint contract §15 R-S08-004 + standard circuit-breaker hysteresis pattern (Netflix Hystrix / Resilience4j canonical)):
   - **Single-signal trigger é false-positive vulnerability** (sprint contract §15 R-S08-004); thus require **≥ 2 signals tripped** OR **manual override**:
     - Signal A: `error_rate_5xx > 0.5 sustained 5min`.
     - Signal B: `p99_latency > 5×SLO sustained 5min`.
     - Signal C: `DO_error_rate > 0.3 sustained 5min`.
   - Trip if (A AND B) OR (A AND C) OR (B AND C) OR ManualOverride.
   - **Single-signal alarm only** (NOT trip): SEV-3 alert; investigation; circuit remains closed.

6. **Hysteresis recovery** (prevent flapping; canonical pattern):
   - Open → HalfOpen: signals < 90% of threshold sustained 2min.
   - HalfOpen → Closed: signals < 50% of threshold sustained 2min PLUS 90% of HalfOpen sample requests succeed.
   - HalfOpen → Open: any signal trips again (immediate revert).
   - Sample rate 10% em HalfOpen state (gradual recovery; prevent thundering herd).

7. **Per-region scope** (NOT global global): DO `GlobalRateLimiter-<region>` per CF region; multi-region trip requires ≥ 2 regions tripped (rare; cascading failure pattern). Sprint contract §10 anti-scope cross-region federation.

8. **Manual override always available** (sprint contract §15 R-S08-004 absorbed): admin S-13 can force circuit open (drill / planned shedding) OR closed (override false-positive trip). Audit emit fail-closed em manual override.

9. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id from middleware; AdminCtx for manual override.

10. **Audit fail-closed** (Lote 10.6bis pattern absorbed): all circuit transitions audited; manual overrides audited; fail-closed if audit emit fails.

11. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): `worker::send_future()`; NEVER `tokio::spawn`.

12. **Cross-WI orphaned reservation behavior (Lote 10.8bis CI-3 corrected)**: when global circuit opens em WI-S08-005 middleware, all subsequent requests short-circuit with 429 GlobalCircuitOpen ANTES de chegar em WI-S08-003 quota_layer. This means in-flight reservations granted ANTES do circuit trip can orphan: write proceeds (or fails) without reaching `confirm_storage`/`release_storage_reservation`. Behavior:
    - WI-S08-003 DO `Quota-<tenant_id>` alarm sweep (5min cycle) auto-releases reservations at TTL (size-proportional; default 60s minimum); orphaned reservations clean up automatically.
    - During circuit-open extended event (> reservation_ttl): sweep cycle releases stale reservations preventing inflated `bytes_used + active_reserved`.
    - On circuit close: WI-S08-001 + WI-S08-003 resume normally; spurious StorageOver rejections from inflated active_reserved bounded ≤ next sweep cycle.
    - SEV-3 alert if `corelink.quota.reservations_active{tenant_id}` > 100 sustained (WI-S08-003 §6.1.10 metric); operator visibility.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + race-correctness justification)

Response wrapper + RFC 9331 + global circuit breaker constitute **the system-wide observability + emergency layer**. Without them: (a) Bazel/Buck2 clients can't reliably retry (legacy `X-RateLimit-*` headers vary across vendors); (b) SLI inflation occurs (legitimate over-quota counts as failure → false error budget exhaust); (c) cascading failures cascade indefinitely (camadas 1-3 are per-tenant/IP/PAT scoped; system-wide overload bypasses).

**Why RFC 9331 (NOT custom)** (sprint contract §14.s08.5 explicit): IETF stable headers `RateLimit` + `RateLimit-Policy`; canonical Bazel + Buck2 + Stripe SDK + GitHub SDK consume this format; custom `X-RateLimit-*` legacy é vendor-specific and inconsistent. Adopting RFC 9331 = customer SDK works without per-vendor adaptation. Custom `X-Rate-Limit-Type` is **CoreLink-specific extension** for SLI distinction (RFC 9331 doesn't define type discrimination).

**Why 5 discriminators specifically** (sprint contract §5 R-S08-8): each maps a different camada layer + economic semantic. Customer SDK can branch behavior: `over_quota` → upgrade prompt; `tenant_quota` → exponential backoff (transient); `global_circuit_open` → halt + 60s wait; `per_ip` → notify ops (NAT scenario); `per_pat` → revoke + rotate.

**Why SLI distinction CRITICAL** (sprint contract §7.10.s08.1): SLO-AVAIL-CAS-GET denominator must exclude legitimate over-quota responses. Example: tenant uses 200% of plan → 50% requests over-quota → if counted in denominator, SLI = 50% → false SLO breach → unnecessary pager. Correct behavior: over-quota requests NOT in numerator + NOT in denominator (excluded entirely); only within-quota requests counted; SLO measures CoreLink performance NOT customer plan adherence.

**Why global circuit breaker camada 4** (sprint contract §4 CAP-RATE-004): camadas 1-3 are scoped (tenant/IP/PAT); system-wide outage (control-plane down, DO storage layer overloaded, cross-region cascading failure) bypasses all 3. Camada 4 is last-resort: trip → 429 all requests + 60s recovery sample. Better to shed 100% requests for 60s than to cascade-fail indefinitely.

**Why multi-signal trigger** (sprint contract §15 R-S08-004 + standard circuit-breaker hysteresis pattern (Netflix Hystrix / Resilience4j canonical); sprint contract §15 R-S08-004): single-signal trip = false-positive vulnerability. Example: SLO-AVAIL-CAS-GET uses error_rate_5xx; if S-09 metrics endpoint blip causes 5xx for ALL request types simultaneously → single-signal trip → full outage from observability glitch. Multi-signal: ≥ 2 signals tripped concurrently (5xx + latency + DO error) → real systemic failure → trip justified.

**Why hysteresis recovery**: single threshold transition = flapping. Example: error_rate=0.501 trips circuit; recovers to 0.499; signal noise oscillates; circuit flaps every 5min destroying availability. Hysteresis: open at 0.5; recovery at 0.45 sustained 2min (prevents oscillation). Sample 10% in HalfOpen prevents thundering herd on full re-engage.

**Adversarial scenarios**:
- **SLI inflation attack** (tenant intentionally exceeds quota to inflate denominator): cannot — over-quota responses NOT in denominator (sprint contract §7.10.s08.1).
- **False-positive global trip** (S-09 metrics glitch): single-signal-only → SEV-3 alarm but NOT trip; multi-signal required.
- **Hysteresis flapping**: 2min recovery threshold; sample 10% prevents thundering herd.
- **Manual override race** (admin opens + closes simultaneously): DO actor serializes; audit emit fail-closed; deterministic outcome.
- **HalfOpen bypass attempt** (adversary requests during HalfOpen 10% sample): DO actor selects sample deterministically; no per-request gaming.
- **Cross-region cascade**: per-region scoped; multi-region requires ≥ 2 tripped (rare); deferred S-14 federation.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: CTRL-RATE-001 camada 4; bypass = catastrophic full outage.
- **FF-HR-002**: global circuit FP catastrophic full outage (sprint contract §15 R-S08-004 critical impact).
- 11 sign-offs canonical + chaos suite + property test 100k race iterations on multi-signal trigger.

## 3. Customer Impact & Journey

**Persona 1 — Bazel client (within plan)**: receives 200 OK response with informational `RateLimit: limit=200, remaining=185, reset=15` + `RateLimit-Policy: 200;w=60`. Client tracks; pre-emptively backoffs if remaining < 20.

**Persona 2 — Bazel client (over plan)**: receives 429 with `X-Rate-Limit-Type: tenant_quota` + `Retry-After: 1` + `RateLimit: limit=200, remaining=0, reset=1`. Client retries after 1s; succeeds. SLI: counted in denominator (within-plan 429 = bug nosso = SLI failure).

**Persona 3 — Bazel client (over quota plan limit)**: receives 429 with `X-Rate-Limit-Type: over_quota` + `Retry-After: 1296000` (15 days until month reset) + `RateLimit: limit=N, remaining=0, reset=1296000`. Client UI prompts "upgrade plan"; SDK halts retries (Retry-After too long). SLI: NOT counted (legitimate; sprint contract §7.10.s08.1).

**Persona 4 — Bazel client (NAT corporate)**: receives 429 with `X-Rate-Limit-Type: per_ip` + `Retry-After: 60`. Customer admin notified; NAT allowlist pre-arranged via S-13 (WI-S08-002 humane appeal). SLI: NOT counted.

**Persona 5 — Bazel client (PAT compromised)**: receives 429 with `X-Rate-Limit-Type: per_pat` + `Retry-After: 1`. SDK reports SEV-1 PagerDuty (PAT misuse pattern); admin revokes PAT (S-03 RUNBOOK-AUTH-003).

**Persona 6 — Bazel client (global circuit open)**: receives 429 with `X-Rate-Limit-Type: global_circuit_open` + `Retry-After: 60`. SDK halts ALL requests for 60s; system-wide alert; oncall paged. SLI: counted (system overload = bug nosso).

**Persona 7 — DevOps reviewing**: DASH-RATE (WI-S08-006) shows: SLI-AVAIL-CAS-GET % within-plan + count over-quota separately + circuit state per region; alert SEV-1 on circuit open; no false-positive on legitimate over-quota.

**SLA addendum**:
- RFC 9331 headers always present on 429 (compliance).
- SLI distinction implemented (within-quota vs over-quota separate counters).
- Global circuit trip latency: ≤ 5min from threshold breach (multi-signal observation window).
- Recovery latency: ≥ 2min hysteresis; gradual via HalfOpen 10% sample.
- Manual override SLA: ≤ 1min from admin S-13 trigger.

## 4. Capability Mapping

- **CAP-RATE-004** (Global rate limit circuit breaker camada 4) — IMPLEMENTA primary.
- Trace: `security_model.md CTRL-RATE-001` (camada 4) + `resilience_patterns.md PAT-CIRCUIT-001 (Lote 10.8bis P1-4 corrected; canonical name em resilience_patterns.md line 126)` + `slo_catalog.md SLI-AVAIL-CAS-GET (within-quota distinction)` + `failure_modes.md FM-201 (Config change causa rate-limit drop; canonical FM lookup; Lote 10.8bis P1-3 corrected — FM-251 actually é "Credential stuffing / brute force") + FM-401 (thundering herd)` + sprint contract §7.10.s08.1 (SLI correctness) + RFC 9331 (IETF) + RFC 6585 (Retry-After).

## 5. Tipo

Tower middleware response wrapper + DO singleton global circuit breaker + RFC 9331 IETF compliance; HIGH_RISK; FF-HR-005 + FF-HR-002.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-rate-limit-headers/` module** — Tower middleware wrapping responses with RFC 9331 + X-Rate-Limit-Type + Retry-After.

2. **RFC 9331 IETF compliance** (sprint contract §14.s08.5):
   - `RateLimit: limit=<n>, remaining=<n>, reset=<seconds>` (IETF stable).
   - `RateLimit-Policy: <window>;w=<seconds>` (policy disclosure).
   - On 429 responses always; optional on 200 responses.
   - Legacy `X-RateLimit-*` NOT emitted (avoid customer SDK ambiguity).

3. **5 X-Rate-Limit-Type discriminators**:
   - `tenant_quota` | `per_ip` | `per_pat` | `over_quota` | `global_circuit_open`.
   - SLI distinction via `RateLimitType::counts_against_sli()` method.
   - 2 separate counters: `corelink_rate_limited_within_quota_total{type, tenant_id}` + `corelink_rate_limited_over_quota_total{type, tenant_id}`.

4. **Retry-After RFC 6585 always present** com type-specific computation:
   - `tenant_quota`: from WI-S08-001 RateLimitResult.retry_after_seconds.
   - `per_ip`: 60s (CF Ruleset mitigation_timeout canonical).
   - `per_pat`: from WI-S08-003 PatRateResult.retry_after_seconds.
   - `over_quota` (storage): days-until-month-reset (or 0 pay-per-use).
   - `over_quota` (bandwidth): seconds-until-next-month-1st-UTC (chrono crate Lote 10.5bis).
   - `global_circuit_open`: 60s (hysteresis sample interval).

5. **`crates/corelink-global-circuit/` module** — DO `GlobalRateLimiter-<region>` per-region:
   - State: `state: CircuitState, recent_observations: VecDeque<HealthObservation>, trips_history: Vec<(i64, TripReason)>`.
   - DO alarm 60s sweep: trim observations buffer, check signals, update state, snapshot to D1 (Lote 10.5bis batch ≤ 250).
   - Re-arm AT START (Lote 10.4bis lesson).

6. **Multi-signal trigger** (sprint contract §15 R-S08-004 + standard circuit-breaker pattern absorbed):
   ```rust
   pub fn evaluate_signals(observations: &VecDeque<HealthObservation>, thresholds: &Thresholds, now_ms: i64) -> Option<TripReason> {
       let window_5min: Vec<&HealthObservation> = observations.iter()
           .filter(|o| o.timestamp_ms > now_ms - 5 * 60 * 1000)
           .collect();

       let total = window_5min.len() as f64;
       if total < 100.0 {                                // insufficient data; do not trip
           return None;
       }

       let error_5xx_rate = window_5min.iter()
           .filter(|o| matches!(o.status, ObservationStatus::ServerError5xx))
           .count() as f64 / total;
       let p99_us = percentile(&window_5min.iter().map(|o| o.latency_p99_us).collect(), 99.0);
       // Lote 10.8bis P1-2 (R5): use most recent observation (NOT average of overlapping
       // 5-min rolling rates, which double-counts and biases toward LATE detection).
       // Each observation.do_error_rate_5m is already a rolling 5min rate; averaging
       // across observations produces moving-average-of-moving-average smoothing.
       let do_err_rate = window_5min.last()
           .map(|o| o.do_error_rate_5m)
           .unwrap_or(0.0);

       let signal_a = error_5xx_rate > thresholds.error_5xx;             // > 0.5
       let signal_b = p99_us > thresholds.p99_latency_us;                 // > 5×SLO
       let signal_c = do_err_rate > thresholds.do_error_rate;             // > 0.3

       let signals_tripped = [signal_a, signal_b, signal_c].iter().filter(|x| **x).count();

       if signals_tripped >= 2 {
           // Multi-signal trip (CANONICAL sprint contract §15 R-S08-004 + standard circuit-breaker pattern)
           Some(TripReason::MultiSignalCombined)
       } else if signals_tripped == 1 {
           // Single-signal alarm only (NOT trip); SEV-3 alert
           emit_metric("corelink.global_circuit.single_signal_alarm_total", 1.0);
           None
       } else {
           None
       }
   }
   ```

7. **Hysteresis recovery**:
   ```rust
   pub fn evaluate_hysteresis(state: &CircuitState, signals: &SignalReadings, since_ms: i64) -> CircuitState {
       match state {
           CircuitState::Open { tripped_at_ms, reason } => {
               if all_signals_under_90pct_threshold(signals) && elapsed_since(since_ms) >= 2 * 60 * 1000 {
                   CircuitState::HalfOpen { since_ms: now() }
               } else {
                   state.clone()
               }
           }
           CircuitState::HalfOpen { since_ms } => {
               if all_signals_under_50pct_threshold(signals) && half_open_sample_success_rate() >= 0.9 && elapsed_since(*since_ms) >= 2 * 60 * 1000 {
                   CircuitState::Closed
               } else if any_signal_trips_again(signals) {
                   CircuitState::Open { tripped_at_ms: now(), reason: TripReason::MultiSignalCombined }   // immediate revert
               } else {
                   state.clone()
               }
           }
           CircuitState::Closed => {
               if let Some(reason) = evaluate_signals_for_trip(signals) {
                   CircuitState::Open { tripped_at_ms: now(), reason }
               } else {
                   state.clone()
               }
           }
       }
   }
   ```

8. **HalfOpen sample 10%** (gradual recovery):
   ```rust
   pub fn allow_halfopen(observation_id: u64, sample_rate: f64) -> bool {
       (observation_id % 10) < (sample_rate * 10.0) as u64   // deterministic 10% sampling
   }
   ```

9. **Manual override** (admin S-13 dependency; staging stub OK):
   - `POST /v1/admin/global_circuit/override` (AdminCtx) → force open/closed.
   - Audit fail-closed; SEV-2 alert per-override.

10. **D1 migrations** (NEW tables; CHECK inline per Lote 10.5bis):
    ```sql
    CREATE TABLE global_circuit_state (
        region TEXT PRIMARY KEY,                       -- "iad" | "sam" | etc.
        state TEXT NOT NULL,                           -- closed | open | half_open
        state_since INTEGER NOT NULL,                  -- unix ms; canonical no _ms suffix per Lote 10.7bis P0-3
        last_trip_reason TEXT,
        snapshotted_at INTEGER NOT NULL,
        CHECK (state IN ('closed', 'open', 'half_open'))
    );

    CREATE TABLE global_circuit_trips_history (
        region TEXT NOT NULL,
        tripped_at INTEGER NOT NULL,                   -- unix ms; canonical no _ms suffix
        recovered_at INTEGER,                          -- unix ms; canonical no _ms suffix
        trip_reason TEXT NOT NULL,
        signal_5xx_rate REAL,
        signal_p99_us INTEGER,
        signal_do_error_rate REAL,
        manual_override_admin_id TEXT,                 -- non-null if ManualOverride
        manual_override_reason TEXT,
        PRIMARY KEY (region, tripped_at),
        CHECK (recovered_at IS NULL OR recovered_at >= tripped_at),
        CHECK (signal_5xx_rate IS NULL OR (signal_5xx_rate >= 0.0 AND signal_5xx_rate <= 1.0)),
        CHECK (signal_do_error_rate IS NULL OR (signal_do_error_rate >= 0.0 AND signal_do_error_rate <= 1.0))
    );

    CREATE INDEX idx_trips_recent ON global_circuit_trips_history(region, tripped_at DESC);
    ```

11. **Audit fail-closed pattern** (Lote 10.6bis absorbed): emit `corelink.global_circuit.{trip, recover, half_open, manual_override}`; fail-closed if emit fails.

12. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 absorbed): `worker::send_future()`; NEVER `tokio::spawn`.

13. **Métricas** (Prometheus convention):
    - `corelink.rate_limited_within_quota_total{type, tenant_id}` (counter; SLI failure denominator).
    - `corelink.rate_limited_over_quota_total{type, tenant_id}` (counter; legitimate; NOT in SLI).
    - `corelink.global_circuit.state{region}` (gauge 0=closed, 1=half_open, 2=open).
    - `corelink.global_circuit.trips_total{region, reason}` (counter; **alert SEV-1 if > 0**).
    - `corelink.global_circuit.recoveries_total{region}` (counter; informational).
    - `corelink.global_circuit.single_signal_alarm_total{region, signal}` (counter; SEV-3 alert).
    - `corelink.global_circuit.half_open_duration_ms{region}` (histogram).
    - `corelink.global_circuit.manual_override_total{region, target_state}` (counter; **alert SEV-2 if > 0**).
    - `corelink.rate_limit_headers.rfc9331_compliance_total{type, version}` (counter; informational).

14. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_rfc9331_headers_format`: 10k random RateLimit values; assert RFC 9331 syntax compliance via parser roundtrip.
    - `prop_sli_distinction`: 10k random 429 types; assert counts_against_sli() correct mapping.
    - `prop_retry_after_realistic`: 10k random retry_after_seconds; assert ≥ 0 + ≤ 30 days max.
    - `prop_multi_signal_trigger_no_single_signal_trip`: 100k random single-signal scenarios; assert 0 trips (only multi-signal).
    - `prop_hysteresis_no_flapping`: 10k random oscillation patterns; assert no flapping (state transitions bounded).
    - `prop_halfopen_sample_deterministic`: 10k observations; assert sample rate exactly 10%.
    - `prop_manual_override_audited`: 1k overrides; assert all audited (LGPD trail compliance).

15. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 12):
    - 1. **5xx cascade** (50% requests 500 sustained 5min): trip via signal A; SEV-1 alert; recover via hysteresis.
    - 2. **Latency cascade** (p99 > 5×SLO sustained): trip via signal B.
    - 3. **DO outage cascade** (DO error rate > 30%): trip via signal C.
    - 4. **Single-signal false-positive resistance**: only signal A breached; assert NO trip; SEV-3 alarm only.
    - 5. **Multi-signal real failure** (A+B simultaneously): trip immediately (multi-signal canonical).
    - 6. **Hysteresis no-flap**: oscillating signal at threshold; assert no flapping.
    - 7. **HalfOpen recovery**: signals drop; verify gradual 10% sample; full recovery after 2min sustained.
    - 8. **HalfOpen revert** (signal trips during HalfOpen): immediate revert to Open.
    - 9. **Manual override open** (admin drill): admin S-13 force open; verify all 429 + audit emit.
    - 10. **Manual override closed** (false-positive recovery): admin force close; audit emit; SEV-2 alert.
    - 11. **SLI denominator correctness**: 1000 over-quota requests + 100 within-quota; assert SLO metric only counts within-quota.
    - 12. **RFC 9331 compliance**: emit headers; parse via 3rd-party parser (BuildBuddy/Bazel SDK fixture); assert format compliance.

### 6.2 Out-of-scope (deferred)

- Cross-region circuit federation (per-region independent; deferred S-14).
- Custom retry-after computation per customer SDK (use canonical RFC 6585).
- Customer-tunable hysteresis thresholds (admin S-13 future).
- Detailed observability per-customer headers (informational on 200 deferred Phase 2; emit on 429 only initial).
- Legacy `X-RateLimit-*` backward-compat shim (anti-scope; RFC 9331 canonical).

## 7. Anti-Scope

- ❌ Single-signal trigger (sprint contract §15 R-S08-004 + standard circuit-breaker pattern; multi-signal canonical).
- ❌ No hysteresis (flapping vulnerability).
- ❌ Legacy `X-RateLimit-*` headers (RFC 9331 IETF stable canonical).
- ❌ SLI conflation (within-quota + over-quota same counter).
- ❌ Retry-After missing (RFC 6585 mandatory).
- ❌ Cross-region federation initial.
- ❌ TenantCtx bypass.
- ❌ AdminCtx bypass on manual override.
- ❌ `tokio::spawn` em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ Skip alarm re-arm AT START (Lote 10.4bis).
- ❌ Skip manual override audit (LGPD trail).

## 8. Acceptance Criteria (Gherkin) — 12 scenarios

```gherkin
Feature: Response Code Types + RFC 9331 Headers + Global Circuit Breaker

  Scenario: RFC 9331 headers on 429 within-quota
    Given tenant T tokens=0; refill_rate=200 RPS
    When request triggers 429 (tenant_quota)
    Then response includes RateLimit: limit=200, remaining=0, reset=1
    Then response includes RateLimit-Policy: 200;w=60
    Then response includes X-Rate-Limit-Type: tenant_quota
    Then response includes Retry-After: 1
    Then status code = 429

  Scenario: RFC 9331 headers on 429 over-quota
    Given tenant T bandwidth_egress=100GiB sustained; max=100GiB; period=2026-04
    When download blob (egress)
    Then response 429 X-Rate-Limit-Type: over_quota
    Then RateLimit-Policy disclosed
    Then Retry-After = secs_until_2026-05-01_00:00:00Z (~5 days)

  Scenario: SLI distinction within-quota counted
    Given 100 requests; 50 within-plan-429 (tenant_quota); 50 success
    When metrics computed
    Then corelink_rate_limited_within_quota_total = 50
    Then SLO-AVAIL-CAS-GET denominator = 100; numerator = 50; SLI = 50%
    Then SLI failure (bug nosso identified)

  Scenario: SLI distinction over-quota NOT counted
    Given 100 requests; 50 over-quota-429; 50 success
    When metrics computed
    Then corelink_rate_limited_over_quota_total = 50
    Then SLO-AVAIL-CAS-GET denominator = 50 (over-quota excluded); numerator = 50; SLI = 100%
    Then SLI pass (legitimate over-plan; sprint contract §7.10.s08.1)

  Scenario: Multi-signal global circuit trip
    Given region iad; signal A (5xx > 0.5) sustained 5min
    Given simultaneously signal B (p99 > 5×SLO) sustained 5min
    When evaluate_signals
    Then signals_tripped = 2 (A+B)
    Then CircuitState = Open { reason: MultiSignalCombined }
    Then SEV-1 alert; corelink.global_circuit.trips_total{region=iad, reason=MultiSignalCombined} +=1
    Then audit emit corelink.global_circuit.trip

  Scenario: Single-signal NO trip (false-positive resistance)
    Given region iad; signal A (5xx > 0.5) sustained 5min ONLY
    Given signals B + C below threshold
    When evaluate_signals
    Then signals_tripped = 1 (A only)
    Then NO trip; CircuitState remains Closed
    Then SEV-3 alarm: single_signal_alarm_total{region=iad, signal=A} +=1
    Then alert investigation; circuit unchanged

  Scenario: Hysteresis recovery Open → HalfOpen
    Given CircuitState = Open { tripped_at = T0 }
    Given all signals under 90% of threshold sustained 2min
    When evaluate_hysteresis at T0+2min
    Then CircuitState transitions to HalfOpen { since = T0+2min }
    Then audit emit corelink.global_circuit.half_open

  Scenario: HalfOpen → Closed recovery
    Given CircuitState = HalfOpen { since = T0 }
    Given all signals under 50% of threshold sustained 2min
    Given half_open_sample_success_rate ≥ 0.9
    When evaluate_hysteresis at T0+2min
    Then CircuitState transitions to Closed
    Then audit emit corelink.global_circuit.recover

  Scenario: HalfOpen revert on signal re-trip
    Given CircuitState = HalfOpen
    Given signal trips again (any signal exceeds threshold)
    When evaluate_hysteresis
    Then CircuitState transitions immediately back to Open
    Then audit emit corelink.global_circuit.trip { reason: hysteresis_revert }

  Scenario: Manual override admin force open
    Given admin AdminCtx valid auth from S-13
    Given CircuitState = Closed
    When POST /v1/admin/global_circuit/override {region: "iad", target: open, reason: "drill"}
    Then CircuitState transitions to Open { reason: ManualOverride { admin_id, reason: "drill" } }
    Then audit emit corelink.global_circuit.manual_override; SEV-2 alert
    Then all requests in iad return 429 X-Rate-Limit-Type: global_circuit_open

  Scenario: HalfOpen sample 10%
    Given CircuitState = HalfOpen
    Given 1000 requests arrive
    When allow_halfopen called for each (deterministic by observation_id)
    Then exactly 100 requests pass (10% sample)
    Then 900 requests return 429 X-Rate-Limit-Type: global_circuit_open

  Scenario: Audit fail-closed manual override
    Given admin POST /v1/admin/global_circuit/override
    Given audit_outbox INSERT fails (D1 throttle)
    When transaction commits
    Then transaction ABORTED: state unchanged
    Then admin receives CircuitError::AuditEmitFailed
    Then SEV-2 alert (override denied due to audit fail)
```

## 9. Design Decisions

- 9.1: RFC 9331 IETF (NOT custom `X-RateLimit-*`); customer SDK universal compliance.
- 9.2: 5 X-Rate-Limit-Type discriminators (sprint contract §5 R-S08-8 canonical).
- 9.3: SLI distinction: within-quota in denominator (bug); over-quota excluded (legitimate; sprint contract §7.10.s08.1).
- 9.4: Multi-signal trigger (sprint contract §15 R-S08-004 + standard circuit-breaker pattern absorbed; sprint contract §15 R-S08-004).
- 9.5: Hysteresis recovery (no flapping; canonical pattern).
- 9.6: HalfOpen 10% sample (gradual recovery; no thundering herd).
- 9.7: Per-region circuit (NOT global global; deferred S-14 federation).
- 9.8: Manual override always available (admin S-13).
- 9.9: TenantCtx-only (Lote 10.4bis); AdminCtx for manual override.
- 9.10: Audit fail-closed (Lote 10.6bis absorbed).
- 9.11: NEW migrations global_circuit_state + global_circuit_trips_history.
- 9.12: NO new ADR (extends CTRL-RATE-001 + CAP-RATE-004 sprint contract canonical).
- 9.13: Retry-After type-specific computation; chrono for bandwidth reset (Lote 10.5bis lesson absorbed).

## 10. Completeness Criteria SOTA

- [ ] **10.s08.005.1** Module compila + integration tests green.
- [ ] **10.s08.005.2** All 12 Gherkin scenarios green.
- [ ] **10.s08.005.3** Property tests 7 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- [ ] **10.s08.005.4** Chaos suite 12 scenarios green.
- [ ] **10.s08.005.5** **RFC 9331 compliance**: 3rd-party parser fixture (BuildBuddy/Bazel SDK) accepts headers (sprint contract §14.s08.5).
- [ ] **10.s08.005.6** **SLI distinction implemented**: separate counters within-quota vs over-quota; SLO-AVAIL-CAS-GET denominator excludes over-quota (sprint contract §7.10.s08.1).
- [ ] **10.s08.005.7** Multi-signal trigger validated (NO single-signal trip; 100k property test).
- [ ] **10.s08.005.8** Hysteresis no-flapping validated.
- [ ] **10.s08.005.9** Métricas (9) emitted; trips_total alerts SEV-1 if > 0.
- [ ] **10.s08.005.10** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s08.005.11** Cost regression gate per-request ≤ $0.0000001 (header injection).
- [ ] **10.s08.005.12** **D1 migrations** (2 NEW tables; CHECK inline per Lote 10.5bis).
- [ ] **10.s08.005.13** Manual override audit trail (LGPD compliance).
- [ ] **10.s08.005.14** Recovery latency hysteresis ≥ 2min validated.

## 11. DoD

- [ ] Module compila + tests green; all Gherkin/property/chaos green; 11 sign-offs canonical (HIGH_RISK).

## 12. Invariants Validated

- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): camada 4 last-resort defense.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+): all circuit transitions audited.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): per-region scope; per-tenant headers.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| RateLimitHeaders module | `crates/corelink-rate-limit-headers/` | Rust |
| Tower middleware response wrapper | `crates/corelink-worker/src/middleware/rate_limit_headers.rs` | Rust |
| GlobalCircuitBreaker module | `crates/corelink-global-circuit/` | Rust |
| DO singleton impl | `crates/corelink-global-circuit/src/do_singleton.rs` | Rust |
| Manual override admin endpoint | `crates/corelink-worker/src/admin/global_circuit.rs` | Rust |
| D1 migrations | `migrations/00X_global_circuit_state.sql`, `migrations/00X_global_circuit_trips_history.sql` | SQL |
| Property tests | `crates/corelink-global-circuit/tests/prop_circuit.rs` | Rust |
| Chaos suite | `tests/chaos_global_circuit.rs` | Rust |
| RFC 9331 fixture parser | `tests/fixtures/rfc9331_parser.rs` | Rust |
| Wrangler DO binding | `wrangler.toml` (additions; `GlobalRateLimiter-<region>`) | TOML |

## 14. Quality Standards SOTA

- 14.s08.005.1: Zero unsafe; zero unwrap em production paths.
- 14.s08.005.2: rustdoc 100% public API.
- 14.s08.005.3: Test coverage ≥ 90%.
- 14.s08.005.4: Header injection latency ≤ 0.5ms p99.
- 14.s08.005.5: Circuit state check latency ≤ 1ms p99.
- 14.s08.005.6: SAST clean.
- 14.s08.005.7: Métricas (9 §6.1.13).
- 14.s08.005.8: Cost regression gate per-request ≤ $0.0000001.
- 14.s08.005.9: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); column drift no `_ms` suffix (Lote 10.7bis P0-3); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis); `corelink_time::secs_until_next_month_first_utc_midnight()` for bandwidth Retry-After (Lote 10.8bis P0-D correct chrono primitive).
- 14.s08.005.10: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s08.005.11: Multi-signal trigger (sprint contract §15 R-S08-004 + standard circuit-breaker pattern absorbed).
- 14.s08.005.12: RFC 9331 IETF compliance via 3rd-party parser fixture.

## 15. Chaos Experiments (12)

§6.1.15 enumerated.

## 16. PRR

HIGH_RISK 11 sign-offs canonical PRR; sprint contract §6 DoD enforces.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + RateLimitHeaders + RateLimitType | 1 |
| ST-002 | Tower middleware response wrapper | 2 |
| ST-003 | RFC 9331 header generation + Retry-After type-specific | 1.5 |
| ST-004 | SLI distinction counters | 1 |
| ST-005 | DO `GlobalRateLimiter-<region>` state machine | 2.5 |
| ST-006 | Multi-signal trigger logic + thresholds | 1.5 |
| ST-007 | Hysteresis recovery + HalfOpen 10% sample | 2 |
| ST-008 | Manual override admin endpoint (S-13 stub) | 1 |
| ST-009 | D1 migrations (2 new tables) | 0.5 |
| ST-010 | Métricas (9) emit | 1 |
| ST-011 | RFC 9331 fixture parser (3rd-party SDK) | 1 |
| ST-012 | Property tests (7 × 10k; 100k nightly) | 3 |
| ST-013 | Chaos suite (12) | 2.5 |

**Total**: ~20.5h. **PERT** O=15h M=18h P=24h: **~19h** (sprint contract estimate 8h; revised significantly upward por: scope expansion para include camada 4 global circuit + multi-signal trigger + hysteresis + RFC 9331 fixture parser + 7 property tests; sprint contract estimate underestimated WI complexity).

## 18. Dependencies

- Hard: S-03 SEALED (TenantCtx + AdminCtx); WI-S08-001 SEALED (RateLimitResult.retry_after_seconds); WI-S08-002 SEALED (per-IP retry computation); WI-S08-003 SEALED (PatRateResult + bandwidth retry); WI-S08-004 SEALED (abuse response gradient downgrade tier).
- Soft: S-09 SEALED OR em paralelo (PromQL signals + dashboard); S-13 admin plane (manual override staging stub OK); WI-S08-006 (DASH-RATE consumes metrics).

## 19. Effort PERT: ~19h. ## 20. Time-boxing: 24h hard limit.

## 21. Observability

9 metrics §6.1.13. Trace span `rate_limit_headers.{wrap, retry_after_compute}` + `global_circuit.{check, trip, recover, half_open, manual_override}`.

## 22. Cost Analysis

- Per-request: ~$0.0000001 (header injection negligible; circuit check sub-ms).
- TCO 12m: 5 regions × 1 DO × continuous × $0.0000001 = ~$50/yr — trivial.
- D1 storage: trips history ~ 100 bytes × 10 trips/region/yr × 5 regions = 5 KB — trivial.
- **Cost saved by global circuit**: prevents catastrophic cascading failure (full outage cost $$$/hour customer SLA breach + reputational harm + emergency response cost).

## 23. API Contract

- Public: `RateLimitHeaders` + `RateLimitType` enum + `GlobalCircuitBreaker` trait + `CircuitState`, `TripReason`, `CircuitError` types; `#[non_exhaustive]`.
- HTTP response: 429 + RFC 9331 (RateLimit + RateLimit-Policy) + X-Rate-Limit-Type + Retry-After.
- HTTP admin: `POST /v1/admin/global_circuit/override` (AdminCtx required).

## 24. Post-mortem Hooks

- Global circuit trip detected → CRITICAL post-mortem (was real failure or false-positive?).
- INV-AVAIL-ISOLATION violation via circuit gap → CRITICAL.
- Single-signal alarm sustained → SEV-3 5-Why (real failure trending OR observability glitch).
- Hysteresis flap detected → SEV-2; threshold tuning.
- Manual override during off-hours → 5-Why (planned drill OR emergency).
- SLI denominator drift (over-quota counted) → SEV-1; SLI correctness regression.
- RFC 9331 parser fail (customer SDK rejected headers) → SEV-2; format compliance regression.

## 25. Rollback / Recovery

- Rollback: revert Tower middleware + DO disabled; circuit absent; camadas 1-3 backstop (degraded resilience).
- Recovery: D1 state durable; cold start recovers from snapshot; alarm re-armed AT START.
- RTO ≤ 5min; RPO ≤ 5min.

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx/AdminCtx S-03; admin endpoints AdminCtx-gated.
- T(ampering): D1 audit append-only; circuit state versioned.
- R(epudiation): audit fail-closed; manual overrides traceable.
- I(nformation disclosure): rate limit metadata é OK to disclose (RFC 9331 spec); per-tenant scope.
- D(enial of Service): circuit breaker IS DoS mitigation.
- E(scalation of Privilege): AdminCtx required for override.

**LINDDUN**:
- L(inkability): per-tenant headers; per-region circuit; no cross-tenant linkability.
- I(dentifiability): tenant_id em metrics; no PII em headers.
- N(on-repudiation): audit append-only.
- D(etectability): customer can read RateLimit headers (transparent).
- D(isclosure): RFC 9331 spec discloses limits intentionally (customer benefit).
- U(nawareness): RateLimit-Policy header informs customer of plan.
- N(on-compliance): N/A (rate limiting NOT "significant decision affecting individual"; LGPD/GDPR not applicable).

## 27. Knowledge Transfer

Tech talk (1.5h): "S-08 Rate Limit Headers + Global Circuit: RFC 9331 + Multi-Signal + Hysteresis"; doc `docs/dev/rate-limit-headers-architecture.md`; onboarding test 8 questions: RFC 9331 vs legacy X-RateLimit-* rationale, 5 discriminators + SLI distinction (sprint contract §7.10.s08.1), multi-signal trigger (sprint contract §15 R-S08-004 + standard hysteresis pattern), hysteresis recovery no-flapping, HalfOpen 10% sample no-thundering-herd, per-region scope, manual override audit (LGPD trail), Retry-After type-specific computation.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Single-signal false-positive trip | M | M | CRITICAL | L | LOW | Multi-signal canonical (Lote 10.7bis P0-9); 100k property test |
| R-002 | Hysteresis flapping | L | L | MEDIUM | L | LOW | 2min recovery + HalfOpen 10% sample; property test |
| R-003 | SLI denominator regression (over-quota counted) | L | M | CRITICAL | L | LOW | Counts_against_sli explicit; property test 100k |
| R-004 | RFC 9331 parser rejection (customer SDK) | L | M | MEDIUM | L | LOW | 3rd-party fixture parser test (BuildBuddy/Bazel SDK) |
| R-005 | Manual override race | L | L | MEDIUM | L | LOW | DO actor serialize; audit fail-closed |
| R-006 | Cross-region cascade (multi-region trip) | L | L | HIGH | L | LOW | Per-region scope; multi-region requires ≥ 2 (rare) |
| R-007 | Retry-After computation drift (chrono) | L | L | LOW | L | LOW | Lote 10.5bis chrono lesson absorbed; integration test |
| R-008 | INV-AVAIL-ISOLATION via circuit gap | L | M | CRITICAL | L | LOW | Per-region; chaos test 30d |
| R-009 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-010 | Audit fail silently | L | M | MEDIUM | L | LOW | Fail-closed bias; reconcile catches |
| R-011 | Cost regression header injection | L | L | LOW | L | LOW | §14.s08.005.8 gate |
| R-012 | Manual override unauthorized (compromised AdminCtx) | L | L | HIGH | L | LOW | S-03 AdminCtx + audit trail; revocation S-13 |

## 29. Review Checkpoints

D+0 design (Architect; multi-signal + hysteresis); D+2 AppSec (TenantCtx + AdminCtx + audit fail-closed); D+3 SRE (signal threshold tuning); D+4 code review; D+5 RFC 9331 fixture parser validation; D+6 chaos validation; D+7 PRR HIGH_RISK 11 sign-offs canonical.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation; **emphatic on signal threshold tuning**_ |
| 4 | Security Lead | _TBD; **mandatory** — INV-AVAIL-ISOLATION + AdminCtx_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos + property test 100k_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; mandatory — RFC 9331 IETF compliance + RateLimit headers compliance review_ |
| 10 | Privacy | _TBD; mandatory — manual override audit trail_ |
| 11 | Architect | _TBD; **mandatory emphatic** — multi-signal trigger + hysteresis + per-region scope_ |
| 12 | SRE peer / NetSec advisor | _**substitutes Crypto SME** — circuit breaker design + signal threshold tuning_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.8) | Criação WI-S08-005; HIGH_RISK; SOTA pós-Lote 10.7bis lessons absorbed: multi-signal trigger (sprint contract §15 R-S08-004 driven; standard circuit-breaker pattern (Netflix Hystrix / Resilience4j Rust emerging)); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (P1-3); audit fail-closed (Lote 10.6bis); alarm re-arm AT START (Lote 10.4bis); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis); column drift no `_ms` suffix (P0-3); chrono `tomorrow_at_utc_midnight()` for bandwidth Retry-After (Lote 10.5bis); 5-tier canonical baselines (P0-7). RFC 9331 IETF stable canonical (sprint contract §14.s08.5). 5 X-Rate-Limit-Type discriminators (sprint contract §5 R-S08-8). SLI distinction CRITICAL implementação (sprint contract §7.10.s08.1). Multi-signal trigger NOT single-signal (sprint contract §15 R-S08-004). Hysteresis no-flapping. HalfOpen 10% sample no-thundering-herd. NEW migrations global_circuit_state + global_circuit_trips_history. Manual override admin S-13 audit trail. Sprint contract estimate 8h revised upward to ~19h porque scope expansion include camada 4 global circuit + multi-signal + hysteresis + RFC 9331 fixture parser. SRE peer / NetSec advisor substitutes Crypto SME (circuit breaker design). |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.8bis) | R4+R5 review remediation: P0-B multi-signal trigger ~12 occurrences misattributing "Lote 10.7bis P0-9 race-aware" replaced com canonical "sprint contract §15 R-S08-004 + standard circuit-breaker pattern (Netflix Hystrix / Resilience4j)"; P1-3 FM-251 → FM-201 canonical (FM-251 = "Credential stuffing" não "rate FP"); P1-4 PAT-CIRCUIT-BREAKER-001 → PAT-CIRCUIT-001 canonical (resilience_patterns.md line 126); R5 P1-2 do_error_rate_5m averaging methodology — replaced sum-of-overlapping-rolling-rates / total (double-counting; biases LATE detection) com most-recent observation; R5 P1-3 ManualOverride TripReason excluded from counts_against_sli() (planned drill = intentional NOT bug); CI-3 orphaned reservation behavior documented (DO alarm sweep auto-recovers em ≤ next sweep cycle). Aggregate score post-bis target ≥ 8.5/10 (R4 7.6 + R5 7.0 baselines). |
| 1.2.0 | 2026-04-25 | Gustavo (Lote 10.8-tris **SEALED**) | Sonnet R5 round-2 review tris-validation pass: tris score 8.5/10 from 7.0 round-1 (+1.5 delta; highest absolute among 6 WIs). 0 NEW findings em este WI (P0-D + R5 P1-2/3 + CI-3 all PASSED tris validation). **WI sealed pre-implementation**. |

## 32. Anti-patterns evitados

- ❌ Single-signal trigger (sprint contract §15 R-S08-004 + standard circuit-breaker pattern); ❌ No hysteresis (flapping); ❌ Legacy X-RateLimit-* (RFC 9331 canonical); ❌ SLI conflation; ❌ Retry-After missing; ❌ Cross-region federation initial; ❌ TenantCtx bypass; ❌ AdminCtx bypass on override; ❌ tokio::spawn em CF Workers; ❌ Skip alarm re-arm AT START; ❌ Skip manual override audit (LGPD trail).

---

**Fim WI-S08-005.** Próximo: WI-S08-006 (DASH-RATE dashboard + alerts + SLI distinction operational visibility).
