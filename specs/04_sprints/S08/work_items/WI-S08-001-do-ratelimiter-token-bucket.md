---
id: "WI-S08-001"
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
tags: ["wi", "s08", "rate-limit", "token-bucket", "do-actor", "ratelimiter", "high-risk"]
---

# WI-S08-001 — Per-Tenant DO RateLimiter Token Bucket (`crates/corelink-rate-limiter`; DO `rate-limiter-<tenant_id>` resolved via `tenant.primary_region` Lote 10.7bis P0-9 lesson; refill_rate from Plan tier 5-arm match free/solo/team/business/enterprise per data_model.md §3 + slo_catalog §3.1; INV-RATE-LIMIT-PROPORTIONALITY enforcement; INV-AVAIL-ISOLATION inheritance via per-tenant DO singleton; race-free serialization via DO actor model; sub-ms latency via sticky placement)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-08](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S08-001 |
| Título | DO singleton `rate-limiter-<tenant_id>` per-tenant token bucket; resolved via `tenant.primary_region` (Lote 10.7bis P0-9 lesson absorbed); refill_rate proportional ao tenant Plan (free/solo/team/business/enterprise — 5 tiers canonical Lote 10.7bis P0-7); state machine: `tokens: f64, last_refill_at_ms: i64, plan_refill_rate_per_sec: f64`; check_and_consume(amount) atomic DO actor (race-free serialization eliminating FM-401 thundering herd at boundary); INV-RATE-LIMIT-PROPORTIONALITY (`refill_rate × window` consistent com plan; mudança plan reflete ≤5min via DO config sync); INV-AVAIL-ISOLATION inheritance (per-tenant DO; cross-tenant impossible by design); plan upgrade DO config sync via S-13 admin push; chaos test 30d sustained zero false positives em legitimate workloads |
| Sprint | S-08 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-RATE-001 security control; bypass = AVAIL-ISOLATION violation), FF-HR-002 (cross-tenant SLO degradation) |

## 1. Intent

Per-tenant DO token bucket é o **bulkhead primário** do CoreLink multi-tenant: garante INV-AVAIL-ISOLATION (tenant A flood não degrada SLO tenant B) via state-isolated DO actor. Token bucket math canonical (Lamport-style stable refill); race-free via DO single-threaded actor; latency sub-ms via sticky placement em primary_region.

```rust
// File: crates/corelink-rate-limiter/src/lib.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Atomic check-and-consume; returns Ok(remaining_tokens) if allowed,
    /// Err(RateLimitError::Exhausted) if would-exceed; refill computed lazily on each call.
    /// Race-free via DO actor model (single-threaded serialization).
    async fn check_and_consume(
        &self,
        tenant_ctx: &TenantCtx,
        amount: f64,                              // typically 1.0; bandwidth-weighted sometimes >1
    ) -> Result<RateLimitResult, RateLimitError>;

    /// Plan upgrade reflects ≤ 5min (DO config sync via S-13 admin push).
    async fn update_plan(
        &self,
        tenant_ctx: &TenantCtx,
        new_refill_rate_per_sec: f64,
        new_burst_capacity: f64,
    ) -> Result<(), RateLimitError>;
}

pub struct RateLimitResult {
    pub allowed: bool,
    pub tokens_remaining: f64,
    pub burst_capacity: f64,
    pub refill_rate_per_sec: f64,
    pub retry_after_seconds: Option<u64>,         // None if allowed; Some(secs) if Exhausted
}

#[derive(thiserror::Error, Debug)]
pub enum RateLimitError {
    #[error("rate limit exhausted: requested {requested}, available {available}; retry after {retry_after_secs}s")]
    Exhausted { requested: f64, available: f64, retry_after_secs: u64 },

    #[error("DO backend error: {0}")]
    DoBackendError(String),

    #[error("plan unknown for tenant {tenant_id}")]
    PlanUnknown { tenant_id: String },
}
```

**Cripto-driven invariants enforced**:

1. **INV-RATE-LIMIT-PROPORTIONALITY** (HIGH; registry §3.12): `refill_rate × window` always proportional to tenant Plan tier:
   - **5-tier canonical** (Lote 10.7bis P0-7 absorbed): free/solo/team/business/enterprise.
   - **Refill rates** (per `slo_catalog.md §3.1` interpolation; baselines from `_spec_contract §16` external benchmarks):
     - `free`: 10 RPS sustained, 50 burst.
     - `solo`: 50 RPS sustained, 200 burst.
     - `team`: 200 RPS sustained, 1000 burst.
     - `business`: 1000 RPS sustained, 5000 burst.
     - `enterprise`: 10000 RPS sustained, 50000 burst (negotiable per contract; admin override via S-13).
   - **Plan change propagation**: ≤ 5min via DO config sync (S-13 admin push); plan_id read from Neon `subscription` table (data_model.md §3.4 line 202) replicated to per-region D1 cache.

2. **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant DO singleton; cross-tenant state contamination architecturally impossible (CF DO ID per-tenant).

3. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id from Tower middleware (S-03 WI-S03-003); NEVER request body.

4. **DO routing via `tenant.primary_region`** (Lote 10.7bis P0-9 lesson absorbed): DO ID `rate-limiter-<tenant_id>` resolved within tenant's primary region (data_model.md `tenant.primary_region`); cross-region requests routed via primary_region edge; SLA p99 ≤ 3ms within primary region.

5. **Token bucket math correctness**:
   - **Lazy refill**: `now_tokens = min(burst_capacity, last_tokens + (now - last_refill_at) * refill_rate_per_sec)`.
   - **Atomic decrement**: `if now_tokens >= amount { tokens = now_tokens - amount; allowed=true; } else { retry_after = (amount - now_tokens) / refill_rate_per_sec; allowed=false; }`.
   - **f64 precision**: token counts em f64 (monotonic time delta * refill_rate); precision ≥ 9 decimal digits sufficient for 10000 RPS × 1ms granularity.
   - **Crypto SME advisory**: token bucket is NOT cripto-load-bearing (no key material; deterministic math); but race-correctness IS load-bearing.

6. **Race-free serialization**: CF DO actor model serializes all `check_and_consume` calls; concurrent requests can't both pass at boundary — this is the canonical FM-401 (thundering herd) mitigation.

7. **DO state machine** (durable storage):
   - `tokens: f64` — current token count.
   - `last_refill_at_ms: i64` — unix ms; monotonic; updated each call.
   - `plan_refill_rate_per_sec: f64`.
   - `plan_burst_capacity: f64`.
   - `plan_updated_at_ms: i64` — sync watermark from S-13.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + race-correctness justification)

DO RateLimiter é **the primary bulkhead** do CoreLink — sem isso, INV-AVAIL-ISOLATION é violated by design (tenant A flood saturates shared resources). 4-camadas em S-08 (this WI = camada 1 per-tenant; camada 2 per-IP em WI-002; camada 3 per-PAT em WI-003 quota-middleware; camada 4 global circuit em WI-005 RFC 9331 wrapper) compõem defense-in-depth.

**Why DO actor model** (vs naive D1 atomic UPDATE): D1 latency p99 ~10-50ms; rate limit hot path budget ≤ 3ms (sprint contract §10.s08.2). DO sticky placement em primary_region é sub-ms; single-threaded serialization race-free; durable storage persists across Worker restart (cold start recovers state from D1 backup snapshot every 5min).

**Why `tenant.primary_region` routing** (Lote 10.7bis P0-9 lesson absorbed): cross-region requests adicionariam ~50-100ms RTT cross-region; SLA p99 ≤ 3ms only achievable within primary region. Multi-region tenants deferred to S-14 BYOK + replication; GA single-region per tenant (most tenants single-region per pricing tier).

**Why 5-tier canonical refill rates** (Lote 10.7bis P0-7 absorbed): tier vocabulary canonical em data_model.md §3 Plan + slo_catalog §3.1 + WI-S07-002 ttl_for_tier; rate limits MUST be 5-arm match for cross-WI consistency. Refill rates inspired por Stripe API baseline 25 RPS livemode; CoreLink team tier 200 RPS reflects build-system characteristics (parallel client requests).

**Why lazy refill (vs cron-tick)**: lazy compute é O(1) per check; cron-tick é O(N tenants); at 100k tenants × 1Hz cron = 100k DO writes/sec — DO would saturate. Lazy refill on-each-call distributes work to active tenants only.

**Adversarial scenarios**:
- **Race at boundary** (2 concurrent requests at exhausted state): DO actor serializes; first request succeeds (tokens ≥ amount); second request sees tokens<amount; deterministic.
- **Plan downgrade racy** (tenant downgrades free→canceled mid-flight): DO sees plan_refill_rate=0; subsequent calls Exhausted (correct fail-closed behavior).
- **Clock skew** (Worker clock skewed forward 1s): tokens over-refill; tenant gets brief excess capacity; bounded ≤ 1s drift; INV-RATE-LIMIT-PROPORTIONALITY softly violated for ≤ 1s window. Mitigation: `last_refill_at_ms` monotonic via `if now < last { now = last }` clamp.
- **DO restart amid in-flight check**: durable storage persists tokens; cold start recovers state; next call uses recovered tokens.
- **Plan upgrade cascade** (1000 tenants upgrade simultaneously): each DO updates independently via S-13 push; no thundering herd cross-DO.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: CTRL-RATE-001 security control; bypass = AVAIL-ISOLATION violation cross-tenant.
- **FF-HR-002**: per-tenant bulkhead failure permits cross-tenant degradation (sprint contract §2 explicit).
- 13 sign-offs + chaos 30d sustained + property test 100k race iterations.

## 3. Customer Impact & Journey

**Persona 1 — Customer (within plan)**: Bazel client at 50 RPS sustained on team plan (200 RPS limit) → never hit rate limit; oblivious to RateLimiter existence; latency neutral (sub-ms middleware overhead).

**Persona 2 — Customer (over plan)**: Bazel parallel build spike to 250 RPS on team plan → first 200 succeed; remaining 50 receive `429 + X-Rate-Limit-Type: tenant_quota + Retry-After: 0.25` (250ms until next refill); Bazel auto-retries; build proceeds slightly slower; customer sees "build OK em 1.5min vs 1.0min".

**Persona 3 — Customer (abuse)**: tenant continuously 250 RPS sustained 1h → 80% requests 429; abuse_score (WI-S08-004) increments; admin notification triggered; customer self-service inspection via `/v1/admin/abuse_score` endpoint (WI-S08-004).

**Persona 4 — DevOps reviewing**: DASH-RATE (WI-S08-006) shows per-tenant token consumption + 429 rate; alert SEV-2 if tenant 429 rate > 10× WoW (signals legitimate growth → upgrade-tier pitch).

**SLA addendum**:
- Rate limit middleware overhead: ≤ 3ms p99 (sprint contract §10.s08.2).
- Plan change propagation: ≤ 5min (sprint contract §8 INV-RATE-LIMIT-PROPORTIONALITY).
- INV-AVAIL-ISOLATION: 0 cross-tenant state contamination (chaos test 30d).
- DO cold start latency: ≤ 50ms p99 (acceptable; cold start is rare).

## 4. Capability Mapping

- **CAP-RATE-001** (Per-tenant token bucket) — IMPLEMENTA primary.
- Trace: `security_model.md CTRL-RATE-001` + `invariant_registry.md INV-AVAIL-ISOLATION + INV-RATE-LIMIT-PROPORTIONALITY` + `failure_modes.md FM-401 thundering herd` + ADR-0020 (Quota ownership boundary).

## 5. Tipo

DO singleton + RateLimiter trait + CF Workers integration; HIGH_RISK; FF-HR-005 + FF-HR-002.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-rate-limiter/` module** — RateLimiter trait + DO impl + tests.
2. **DO singleton** `rate-limiter-<tenant_id>` per-tenant:
   - DO ID resolved via `tenant.primary_region` (Lote 10.7bis P0-9 lesson).
   - State em DO durable storage: `tokens: f64, last_refill_at_ms: i64, plan_refill_rate_per_sec: f64, plan_burst_capacity: f64, plan_updated_at_ms: i64`.
   - DO alarm: every 5min, snapshot state to D1 `rate_limiter_state` table (cold-start recovery).
   - DO alarm re-arm AT START (Lote 10.4bis lesson).
3. **Token bucket math** (lazy refill canonical):
   ```rust
   pub fn check_and_consume(&mut self, amount: f64, now_ms: i64) -> RateLimitResult {
       // Clamp monotonic: if now < last_refill, use last_refill (clock skew safety)
       let now_ms = now_ms.max(self.last_refill_at_ms);
       let delta_secs = (now_ms - self.last_refill_at_ms) as f64 / 1000.0;
       let refilled_tokens = (self.tokens + delta_secs * self.plan_refill_rate_per_sec)
           .min(self.plan_burst_capacity);

       if refilled_tokens >= amount {
           self.tokens = refilled_tokens - amount;
           self.last_refill_at_ms = now_ms;
           RateLimitResult { allowed: true, tokens_remaining: self.tokens, ... retry_after_seconds: None }
       } else {
           // Lote 10.8bis P1-1 (R5): guard division-by-zero for refill_rate=0 (canceled tenant; R-009).
           // Without guard: f64::INFINITY.ceil() as u64 saturates to u64::MAX, emitting
           // Retry-After: 18446744073709551615 (HTTP client misinterpretation).
           let retry_after_secs = if self.plan_refill_rate_per_sec <= f64::EPSILON {
               86400 * 7  // 7 days = effectively "never" (canonical canceled-tenant retry)
           } else {
               let needed = amount - refilled_tokens;
               (needed / self.plan_refill_rate_per_sec).ceil() as u64
           };
           // tokens NOT decremented (failure case)
           // last_refill_at_ms updated to compute correct next-tick
           self.last_refill_at_ms = now_ms;
           self.tokens = refilled_tokens;
           RateLimitResult { allowed: false, tokens_remaining: refilled_tokens, ..., retry_after_seconds: Some(retry_after_secs) }
       }
   }
   ```
4. **5-tier refill rate table** (canonical; from sprint contract §16 + slo_catalog §3.1):
   ```rust
   pub fn refill_rate_for_tier(tier: &Tier) -> (f64, f64) {
       match tier {
           Tier::Free       => (10.0, 50.0),       // 10 RPS sustained, 50 burst
           Tier::Solo       => (50.0, 200.0),
           Tier::Team       => (200.0, 1000.0),
           Tier::Business   => (1000.0, 5000.0),
           Tier::Enterprise => (10000.0, 50000.0), // negotiable per contract; admin override S-13
       }
   }
   ```
5. **Plan sync from Neon → D1 → DO** (S-13 forward; staging stub OK):
   - Neon `subscription.plan_id` updates → S-13 admin webhook → per-region D1 cache `tenant_plan(tenant_id, plan_id, refill_rate, burst_capacity, updated_at_ms)` — NEW table; sprint contract DoD §6 plan change propagation ≤ 5min.
   - DO reads from D1 cache on each `check_and_consume` (cached 5min em DO memory; refresh on staleness).
6. **Tower middleware integration** em CAS GET, AC GET, all read paths:
   ```rust
   pub fn rate_limit_layer<S>() -> tower::Layer<S> {
       // wraps service with check_and_consume(1.0); on Err(Exhausted), responds 429 +
       // X-Rate-Limit-Type: tenant_quota + Retry-After + RFC 9331 RateLimit headers (delegate WI-S08-005).
   }
   ```
7. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id from middleware.
8. **Audit fail-closed** (Lote 10.6bis pattern): emit `corelink.rate_limiter.{check, exhausted, plan_updated}` audit events; fail-closed if audit emit fails (rare; D1 outbox pattern).
9. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): use `worker::send_future()` for non-awaited operations; NEVER `tokio::spawn` (no tokio reactor in CF Workers V8 isolate); NEVER `wasm_bindgen_futures::spawn_local`. `async_lock::Semaphore` if any cross-await sync needed (Lote 10.3-tris lesson).
10. **Métricas** (RFC 9331 alignment + Prometheus convention):
    - `corelink.rate_limiter.check_total{tenant_id, result=allowed|exhausted}` (counter; SLI source for SLO-AVAIL-CAS-GET within-quota distinction; WI-S08-005 context).
    - `corelink.rate_limiter.tokens_remaining{tenant_id}` (gauge; updated each check).
    - `corelink.rate_limiter.refill_rate{tenant_id, tier}` (gauge; reflects current plan).
    - `corelink.rate_limiter.plan_sync_lag_ms{tenant_id}` (gauge; alert SEV-2 if > 5min — INV-RATE-LIMIT-PROPORTIONALITY canary).
    - `corelink.rate_limiter.do_cold_start_total{region}` (counter; informational).
    - `corelink.rate_limiter.middleware_duration_us` (histogram; SLO ≤ 3ms p99).
    - `corelink.rate_limiter.cross_tenant_violation_total` (counter; **alert if > 0; SEV-1; INV-AVAIL-ISOLATION canary**).
11. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 lesson absorbed):
    - `prop_token_bucket_proportionality`: 1k random plans × 1k requests; assert `tokens_consumed ≤ refill_rate × elapsed_seconds`.
    - `prop_rate_limit_tenant_isolation`: 1000 tenants × concurrent flood 1k QPS; assert per-tenant token state independent (INV-AVAIL-ISOLATION).
    - `prop_concurrent_check_serialization`: 1000 concurrent check_and_consume on same DO; assert serialization (DO actor model); deterministic outcome.
    - `prop_clock_skew_safety`: inject negative clock delta; assert monotonic clamp; tokens never decrement spuriously.
    - `prop_plan_upgrade_propagation`: simulate plan change; assert ≤ 5min sync.
12. **Chaos suite** (HIGH_RISK ≥ 10):
    - 1. **Tenant flood** (1 tenant 10k QPS sustained 1h) → vizinhos mantêm SLO-AVAIL-CAS-GET 99.9% (sprint contract §6 DoD); INV-AVAIL-ISOLATION verified.
    - 2. **Plan upgrade race**: 100 tenants upgrade simultaneously; each DO independent; sync ≤ 5min each.
    - 3. **DO cold start storm**: 1000 DOs cold-restart simultaneously; latency p99 ≤ 50ms.
    - 4. **Clock skew**: Worker clock skews +1s; tokens over-refill briefly; bounded ≤ 1s drift; clamp catches.
    - 5. **D1 outage**: D1 down 10min; DO continues authoritative; on recovery, sync resumes.
    - 6. **Plan downgrade mid-flight**: tenant downgrades during 1k QPS burst; tokens refill rate decreases; new requests fail-closed correctly.
    - 7. **Cross-tenant injection attempt**: malicious request claims tenant_id=T2 but TenantCtx says T1; middleware uses T1 (Lote 10.4bis); T2's DO untouched.
    - 8. **DO storage limit**: state per-tenant ~100 bytes; 32 MiB DO limit / 100 bytes = 320k tenants per DO instance — far above any realistic scenario; informational alert SEV-3 if approaches.
    - 9. **Refill rate 0 (canceled tenant)**: plan_refill_rate=0; all checks Exhausted; correct fail-closed behavior; tenant cannot bypass via spam.
    - 10. **Burst capacity boundary**: tenant uses 1000 burst tokens em 1s; next 1s requests refill at 200 RPS; assert no over-burst.
    - 11. **TenantCtx tampering**: signed JWT vs request body mismatch; middleware rejects (S-03 inheritance).
    - 12. **Audit emit fail**: audit_outbox INSERT fails; check_and_consume rolls back state? NO — fail-closed bias: state updated in-memory; audit fail emits SEV-1 alert; eventual consistency catches via reconcile.

### 6.2 Out-of-scope (deferred)

- Per-PAT rate limit (delegate WI-S08-003 quota middleware where PAT scope checked).
- Per-IP rate limit (delegate WI-S08-002).
- Global circuit breaker (delegate WI-S08-005 emergency wrapper).
- Bandwidth ingress/egress quota (CAP-QUOTA-002; delegate WI-S08-003).
- Multi-region rate limit federation (S-14 forward; per-region independent em S-08).
- ML-based rate limit prediction (anti-scope sprint contract §10).

## 7. Anti-Scope

- ❌ D1-only rate limit check (race condition; latency budget exceeded).
- ❌ Skip DO actor model (race-prone).
- ❌ Hard-coded refill rates (use `refill_rate_for_tier` 5-tier match).
- ❌ Cross-tenant state via shared DO (INV-AVAIL-ISOLATION violation).
- ❌ TenantCtx bypass.
- ❌ `tokio::spawn` em CF Workers Rust (Lote 10.3-tris lesson).
- ❌ Skip plan_sync_lag alerting (silent INV-RATE-LIMIT-PROPORTIONALITY violation).
- ❌ Skip alarm re-arm (Lote 10.4bis lesson).

## 8. Acceptance Criteria (Gherkin) — 10 scenarios

```gherkin
Feature: DO RateLimiter token bucket per-tenant

  Scenario: Within-plan request allowed
    Given tenant T (team plan; refill 200 RPS, burst 1000) com tokens=500
    When middleware calls check_and_consume(1.0)
    Then DO atomic decrement; tokens=499
    Then RateLimitResult { allowed=true, tokens_remaining=499, retry_after_seconds=None }
    Then SLI metric counted in numerator (within-quota success)

  Scenario: Over-plan burst returns 429 within_quota
    Given tenant T (team) com tokens=0; refill_rate=200 RPS
    When check_and_consume(1.0)
    Then RateLimitResult { allowed=false, retry_after_seconds=1 }  // 1/200 = 5ms ceil → 1s
    Then middleware responds 429 + X-Rate-Limit-Type: tenant_quota + Retry-After: 1
    Then SLI metric: within-quota 429 contado em SLI denominator (sprint contract §10.s08.1)

  Scenario: Tenant flood doesn't degrade neighbor
    Given tenant T1 (free; 10 RPS) flooding 10k QPS sustained 1h
    Given tenant T2 (team; 200 RPS) at 50 QPS sustained
    Then T1 DO state independent of T2 DO state (per-tenant isolation)
    Then T2 SLO-AVAIL-CAS-GET 99.9% maintained
    Then INV-AVAIL-ISOLATION verified em chaos test 30d

  Scenario: Plan upgrade reflects ≤ 5min
    Given tenant T plan_id changes from team→business em Neon at T0
    When S-13 admin webhook propagates to D1 cache em primary_region
    When DO `rate-limiter-<T>` reads D1 cache (cached 5min em DO memory)
    Then within 5min of T0, new check_and_consume uses business refill (1000 RPS)
    Then INV-RATE-LIMIT-PROPORTIONALITY satisfied
    Then plan_sync_lag_ms metric < 300000 (5min threshold)

  Scenario: DO cold start recovers from D1 snapshot
    Given DO `rate-limiter-<T>` killed (Worker restart)
    Given D1 `rate_limiter_state` has snapshot tokens=750, plan=team
    When DO cold start
    Then state recovered from D1 snapshot; tokens=750
    Then alarm re-armed at START (Lote 10.4bis lesson)
    Then next check_and_consume proceeds normally

  Scenario: Clock skew safety (monotonic clamp)
    Given DO last_refill_at_ms = T0
    Given Worker clock skews backward; now_ms = T0 - 100ms
    When check_and_consume(1.0)
    Then now_ms clamped to T0 (monotonic safety)
    Then tokens NOT spuriously decremented (delta_secs = 0)
    Then no INV-RATE-LIMIT-PROPORTIONALITY violation

  Scenario: Cross-tenant injection blocked
    Given attacker JWT for tenant T1
    Given request body claims tenant_id=T2
    When middleware extracts TenantCtx (T1; Lote 10.4bis)
    Then DO `rate-limiter-<T1>` invoked (NOT T2)
    Then T2 DO state untouched
    Then audit emit corelink.rate_limiter.cross_tenant_attempt; SEV-1 alert

  Scenario: Plan downgrade fail-closed
    Given tenant T plan changes business→canceled
    Given refill_rate becomes 0 RPS
    When subsequent check_and_consume
    Then RateLimitResult { allowed=false, retry_after_seconds=∞ }
    Then middleware 429 + X-Rate-Limit-Type: over_quota
    Then customer cannot bypass via spam

  Scenario: Audit fail-closed
    Given check_and_consume succeeds (tokens decremented)
    Given audit_outbox INSERT fails (D1 throttle)
    When middleware completes
    Then state in-memory updated (NOT rolled back; eventual consistency catches)
    Then SEV-1 alert; reconcile job catches drift
    Note: this is fail-closed bias toward over-counting (charge tenant for legitimate use); reconcile detects audit gap

  Scenario: Cross-region tenant routing via primary_region
    Given tenant T primary_region = sam
    Given request hits region iad edge
    When Worker→DO binding routes request
    Then DO `rate-limiter-<T>` instantiated em sam (NOT iad)
    Then cross-region RTT ~50-100ms (acceptable per Lote 10.7bis P0-9)
    Then within-primary-region requests (most) ≤ 3ms p99
```

## 9. Design Decisions

- 9.1: DO actor model (NOT D1 atomic) — sub-ms latency + race-free serialization.
- 9.2: Lazy refill (NOT cron-tick) — O(1) per check; scales to 100k+ tenants.
- 9.3: 5-tier refill rate table (canonical; Lote 10.7bis P0-7 lesson).
- 9.4: DO routing via `tenant.primary_region` (Lote 10.7bis P0-9 lesson).
- 9.5: Plan sync via Neon→S-13→D1→DO cache (≤ 5min INV-RATE-LIMIT-PROPORTIONALITY).
- 9.6: Token bucket math em f64 (precision sufficient ≥ 9 decimal digits).
- 9.7: Monotonic clock clamp (clock skew safety).
- 9.8: TenantCtx-only enforcement (Lote 10.4bis).
- 9.9: Audit fail-closed bias toward over-counting (Lote 10.6bis pattern adapted; eventual consistency catches).
- 9.10: NO new ADR (extends existing patterns; CTRL-RATE-001 already canonical).

## 10. Completeness Criteria SOTA

- [ ] **10.s08.001.1** Module compila + integration tests green.
- [ ] **10.s08.001.2** All 10 Gherkin scenarios green.
- [ ] **10.s08.001.3** Property tests 5 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar; Lote 10.7bis P1-3 lesson).
- [ ] **10.s08.001.4** Chaos suite 12 scenarios green.
- [ ] **10.s08.001.5** **INV-AVAIL-ISOLATION 30d sustained chaos zero violations** (sprint contract §6 DoD).
- [ ] **10.s08.001.6** Rate limit middleware ≤ 3ms p99 criterion benchmark (sprint contract §10.s08.2).
- [ ] **10.s08.001.7** INV-RATE-LIMIT-PROPORTIONALITY: plan sync ≤ 5min sustained.
- [ ] **10.s08.001.8** Métricas (7) emitted; cross_tenant_violation_total alerts SEV-1 if > 0.
- [ ] **10.s08.001.9** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s08.001.10** Cost regression gate per-check ≤ $0.0000005.
- [ ] **10.s08.001.11** **D1 `rate_limiter_state` migration** (NEW table; CHECK constraint inline per Lote 10.5bis lesson).
- [ ] **10.s08.001.12** **D1 `tenant_plan` migration** (NEW table; per-region cache from Neon `subscription`).

## 11. DoD

- [ ] Module compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK upper-bound; framework §33.5.4.3 cap 10-12; Crypto SME advisory consolidated as Architect race-correctness review per ADR-0034 path; Lote 10.8bis P1-2 corrected).

## 12. Invariants Validated

- **INV-RATE-LIMIT-PROPORTIONALITY** (HIGH; registry §3.12): refill × window proportional to plan; 5min sync.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tenant DO; cross-tenant impossible.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): inherited via DO ID + TenantCtx middleware.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| RateLimiter module | `crates/corelink-rate-limiter/` | Rust |
| DO singleton impl | `crates/corelink-rate-limiter/src/do_singleton.rs` | Rust |
| Tower middleware | `crates/corelink-worker/src/middleware/rate_limit.rs` | Rust |
| D1 migrations | `migrations/00X_rate_limiter_state.sql`, `migrations/00X_tenant_plan.sql` | SQL |
| Property tests | `crates/corelink-rate-limiter/tests/prop_rate_limit.rs` | Rust |
| Chaos suite | `tests/chaos_rate_limit.rs` | Rust |
| Wrangler DO binding | `wrangler.toml` (additions) | TOML |

## 14. Quality Standards SOTA

- 14.s08.001.1: Zero unsafe; zero unwrap em production paths.
- 14.s08.001.2: rustdoc 100% public API.
- 14.s08.001.3: Test coverage ≥ 90%.
- 14.s08.001.4: Latência: middleware ≤ 3ms p99; DO check ≤ 1ms p99.
- 14.s08.001.5: SAST clean.
- 14.s08.001.6: Métricas (7 §6.1.10).
- 14.s08.001.7: Memory bounded ≤ 100 bytes DO state per tenant.
- 14.s08.001.8: Cost regression gate per-check ≤ $0.0000005.
- 14.s08.001.9: TenantCtx-only (Lote 10.4bis); DO routing primary_region (Lote 10.7bis); CF Workers Rust API (Lote 10.7bis R5 P0-3); 5-tier canonical (Lote 10.7bis P0-7).
- 14.s08.001.10: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).

## 15. Chaos Experiments (12)

§6.1.12 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) PRR; sprint contract §6 DoD enforces.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + RateLimiter trait | 1.5 |
| ST-002 | DO singleton state + lazy refill | 3.5 |
| ST-003 | 5-tier refill_rate_for_tier + Plan sync via Neon→D1 | 3 |
| ST-004 | Tower middleware integration | 2 |
| ST-005 | D1 migrations (rate_limiter_state + tenant_plan) | 1.5 |
| ST-006 | Métricas (7) emit | 1.5 |
| ST-007 | Property tests (5 × 10k; 100k nightly) | 4 |
| ST-008 | Chaos suite (12) | 3 |
| ST-009 | Crypto SME advisory review (race correctness) | 1 |

**Total**: ~21h. **PERT** O=18h M=20h P=28h: **~21h** (sprint contract estimate 20h).

## 18. Dependencies

- Hard: S-03 SEALED (TenantCtx middleware); ADR-0020 FROZEN (quota boundary; Lote 10.7bis Phase 3 fix).
- Soft: S-09 (PagerDuty integration); S-13 (admin plane plan_id sync; staging stub OK); WI-S08-005 (RFC 9331 headers wrap); WI-S08-006 (DASH-RATE consumes metrics).

## 19. Effort PERT: ~21h. ## 20. Time-boxing: 28h hard limit.

## 21. Observability

7 metrics §6.1.10. Trace span `rate_limiter.{check, refill, plan_sync, cold_start}`.

## 22. Cost Analysis

- Per-check: ~$0.0000005 (DO read).
- TCO 12m: 5 regions × 1k tenants × 1k requests/sec × 86400 × 365 × $0.0000005 = ~$78840/yr; on critical path; cost dominates Workers Unbound CPU.
- DO storage: ~100 bytes × 100k tenants = 10 MB total cluster — trivial.

## 23. API Contract

- Public: `RateLimiter` trait + `RateLimitResult`, `RateLimitError` types; `#[non_exhaustive]`.
- HTTP: 429 + Retry-After + RFC 9331 headers (delegate WI-S08-005).

## 24. Post-mortem Hooks

- INV-AVAIL-ISOLATION violation detected (cross_tenant_violation > 0) → CRITICAL post-mortem.
- Plan sync lag > 5min sustained → SEV-2; INV-RATE-LIMIT-PROPORTIONALITY canary.
- DO cold start storm > 100/s sustained → 5-Why.
- Customer "rate-limited under quota" (within-plan 429 anomaly) → bug investigation.

## 25. Rollback / Recovery

- Rollback: revert Tower middleware mount; rate limit disabled; bulkhead lost (cost overhead but no breakage).
- Recovery: DO state durable; cold start recovers from D1 snapshot.
- RTO ≤ 5min; RPO ≤ 5min (D1 snapshot interval).

## 26. Security & Privacy

**STRIDE**: TenantCtx prevents EoP cross-tenant; DO actor blocks T(ampering) race; sqlx prepared blocks SQL injection; audit fail-closed prevents R(epudiation).

**LINDDUN**: per-tenant DO; no cross-tenant linkability; no PII em rate limit metrics.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-08 Rate Limit: DO Actor + Token Bucket Math + 5-Tier Plan Sync"; doc `docs/dev/rate-limit-architecture.md`; onboarding test 6 questions: DO actor race-correctness, lazy refill rationale, 5-tier canonical, primary_region routing (Lote 10.7bis), monotonic clock clamp, plan sync ≤5min.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | INV-AVAIL-ISOLATION violation | L | M | CRITICAL | L | LOW | Per-tenant DO; chaos test 30d; SEV-1 alert |
| R-002 | DO cold start latency > 50ms | M | L | LOW | L | LOW | Sticky placement; cache D1 cache 5min em DO |
| R-003 | Plan sync lag > 5min | M | L | MEDIUM | L | LOW | S-13 webhook + D1 cache; alert SEV-2 |
| R-004 | Clock skew tokens over-refill | L | L | LOW | L | LOW | Monotonic clamp |
| R-005 | DO storage saturation | L | L | LOW | L | LOW | 100 bytes/tenant; far below 32 MiB DO limit |
| R-006 | Cross-tenant injection via TenantCtx tampering | L | L | CRITICAL | L | LOW | TenantCtx-only middleware (Lote 10.4bis); audit |
| R-007 | Audit fail silently | L | M | MEDIUM | L | LOW | Fail-closed bias; reconcile catches |
| R-008 | f64 precision drift sustained | L | L | LOW | L | LOW | ≥ 9 decimal digits sufficient |
| R-009 | Plan downgrade race | L | L | LOW | L | LOW | refill_rate=0 fail-closed; correct |
| R-010 | Crypto SME advisory race-correctness gap | L | M | MEDIUM | L | LOW | Property test 100k race iterations |
| R-011 | Cost regression > 10% | M | L | MEDIUM | L | LOW | §14.s08.001.8 gate |
| R-012 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3 lesson; worker::send_future |

## 29. Review Checkpoints

D+0 design (Architect; race analysis); D+2 AppSec (TenantCtx + audit); D+4 code review; D+6 chaos validation; D+7 PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2).

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — INV-AVAIL-ISOLATION + TenantCtx_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos + property test 100k race_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory** — LGPD Art. 20 (decisões automatizadas) advisory_ |
| 10 | Privacy | _TBD; **mandatory** — no PII em rate limit metrics_ |
| 11 | Architect | _TBD; **mandatory** — DO routing + 5-tier canonical + race-correctness review (DO actor; consolidates Crypto SME advisory per ADR-0034 path; Lote 10.8bis P1-2 cap reduction)_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — bulkhead correctness + audit fail-closed_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.8) | Criação WI-S08-001; HIGH_RISK; SOTA pós-Lote 10.7bis lessons absorbed: TenantCtx-only; DO routing primary_region (P0-9); 5-tier canonical (P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (P1-3); audit fail-closed (Lote 10.6bis); alarm re-arm AT START (Lote 10.4bis); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis). NEW migrations rate_limiter_state + tenant_plan. |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.8bis) | R4+R5 review remediation: P1-1 INV §3.X → §3.12 (canonical reg line 163); P1-2 sign-off cap 13→12 (framework §33.5.4.3 cap; Crypto SME advisory consolidated em Architect race-correctness review per ADR-0034); R5 P1-1 division-by-zero guard em retry_after_secs for refill_rate=0 (canonical 7-day retry; previous f64::INFINITY.ceil() saturated to u64::MAX = HTTP client misinterpret); CI-2 NEW table `tenant_rate_override` shared schema com WI-S08-004 SilentDowngrade communication mechanism (effective_refill_rate = plan_refill * override_multiplier; 5min DO cache; auto-revert on expiry). Aggregate score post-bis target ≥ 8.5/10 (R4 8.4 + R5 7.5 baselines). |

## 32. Anti-patterns evitados

- ❌ D1-only rate check (race + latency); ❌ Skip DO actor; ❌ Hard-coded refill rates; ❌ Cross-tenant DO; ❌ TenantCtx bypass; ❌ tokio::spawn em CF Workers; ❌ Skip plan sync alert; ❌ Skip alarm re-arm; ❌ Skip monotonic clamp.

---

**Fim WI-S08-001.** Próximo: WI-S08-002 (CF edge per-IP rate rules + CIDR blocklist).
