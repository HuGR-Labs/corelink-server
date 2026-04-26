---
id: "WI-S09-007"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-09"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s09", "synthetic-canary", "runbook-dry-run", "rb-fm-153", "rb-obs-cardinality-001", "ga-readiness", "high-risk"]
---

# WI-S09-007 — Synthetic Canary 3 Regiões (us-east, eu-west, ap-south) 24/7 + Dashboard Health-Check + Runbook Dry-Run RB-FM-153 (Grafana Cloud Outage) + RB-OBS-CARDINALITY-001 (Cardinality Explosion) (`crates/corelink-canary` + `tests/runbooks/`; CF Workers cron-trigger 24/7 sustained 72h sem gap per sprint contract §6 DoD; canary tests CAS PUT/GET + AC lookup end-to-end; observability stack health verification em paralelo; 2 runbooks dry-run executed em staging environment per sprint contract §6 DoD EVT-017)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-09](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S09-007 |
| Título | Synthetic canary 3 regiões (us-east IAD + eu-west LHR + ap-south BOM) 24/7 sustained 72h sem gap (sprint contract §6 DoD); CF Workers cron-trigger executando canary tests CAS PUT/GET + AC lookup end-to-end via OTLP-instrumented HTTP client; assertions: latency p99 ≤ 100ms, error rate ≤ 0.1%, response correctness validation (BLAKE3 digest match); dashboard health-check em paralelo verifying Mimir/Loki/Tempo tenant ingest functioning + dashboard refresh latency; RB-FM-153 (Grafana Cloud outage) runbook dry-run em staging — simulated outage durante 30min, validate (a) graceful "no data" em dashboards, (b) SEV-3 alert fires, (c) Twilio backup notification path, (d) recovery on simulated resume; RB-OBS-CARDINALITY-001 (cardinality explosion) runbook dry-run — synthetic injection 25k séries via test-only metric, validate cardinality_check.py CI gate rejects, Mimir tenant tier limit secondary defense rejects ingest, SEV-2 alert fires; sprint contract §6 DoD EVT-017 mandatory |
| Sprint | S-09 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (canary é foundation operability validation; sem canary = blind production confidence; sprint contract §6 DoD explicit: 24/7 + 72h sustained antes ship gate) |

## 1. Intent

Synthetic canary é **the operational confidence primitive** — sem canary 24/7 multi-region, ship gate has no objective evidence que CoreLink runs em produção. CF Workers cron canonical (DOs scheduled triggers); canary executes real CAS PUT + GET + AC lookup; assertions on latency + correctness + observability stack health. Runbook dry-runs validate operational response procedures sem real incident; canonical pre-GA exercise.

```rust
// File: crates/corelink-canary/src/lib.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait Canary: Send + Sync {
    /// Execute full canary loop em region; emits métricas + alerts on failure.
    async fn execute_canary_loop(
        &self,
        region: Region,
    ) -> Result<CanaryResult, CanaryError>;

    /// Dashboard health verification (separate from canary loop).
    async fn verify_observability_stack_health(
        &self,
        region: Region,
    ) -> Result<HealthReport, CanaryError>;
}

pub struct CanaryResult {
    pub region: Region,
    pub timestamp_ms: i64,
    pub cas_put_latency_p99_ms: u32,
    pub cas_get_latency_p99_ms: u32,
    pub ac_lookup_latency_p99_ms: u32,
    pub digest_match_correctness: bool,             // BLAKE3 verify
    pub overall_pass: bool,                         // all assertions passed
}

pub struct HealthReport {
    pub region: Region,
    pub mimir_tenant_ingest_lag_ms: u32,
    pub loki_query_latency_p99_ms: u32,
    pub tempo_trace_visibility_lag_ms: u32,
    pub dashboard_refresh_latency_ms: u32,
    pub all_components_healthy: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum CanaryError {
    #[error("canary assertion failed em region {region}: {assertion}")]
    AssertionFailed { region: String, assertion: String },

    #[error("BLAKE3 digest mismatch: written={written}, read={read}")]
    DigestMismatch { written: String, read: String },

    #[error("observability stack component unhealthy: {component}")]
    ObservabilityUnhealthy { component: String },

    #[error("CF Workers cron failed dispatch: {0}")]
    CronDispatchFailed(String),
}
```

**Cripto-driven invariants enforced**:

1. **24/7 multi-region canary** (sprint contract §6 DoD):
   - 3 regions: us-east IAD, eu-west LHR, ap-south BOM (canonical CF region codes).
   - CF Workers cron-trigger interval: 60s per region (1 canary loop/min/region = 4320 loops/dia/region = 12960/dia total).
   - 72h sustained sem gap (sprint contract §6 DoD; total 12960 × 3 = 38880 successful canary loops).

2. **Canary loop assertions**:
   - CAS PUT 1 KB blob: latency p99 ≤ 100ms, success.
   - CAS GET same blob: latency p99 ≤ 50ms, BLAKE3 digest match.
   - AC LOOKUP synthetic action: latency p99 ≤ 30ms, hit/miss as expected.
   - All 3 assertions pass = `overall_pass=true`; otherwise SEV-2 alert.

3. **Observability stack health verification** (sprint contract §6 DoD):
   - Mimir ingest lag: ≤ 30s p99 (Logpush → Mimir SLA).
   - Loki query latency: ≤ 5s p99 hot tier (sprint contract §10.s09 SLO).
   - Tempo trace visibility lag: ≤ 30s p99 (BatchSpanProcessor flush).
   - Dashboard refresh latency: ≤ 3s p99.
   - All 4 components healthy = `all_components_healthy=true`.

4. **RB-FM-153 (Grafana Cloud outage) runbook dry-run** (sprint contract §6 DoD EVT-017):
   - Simulated outage em staging: block Mimir/Loki/Tempo endpoints 30min via firewall rule.
   - Expected behavior:
     - Dashboards show "no data" graceful (NOT crash).
     - SEV-3 alert fires (`corelink.dashboard.refresh_failures_total`).
     - Twilio backup SMS dispatched (PagerDuty primary degraded).
     - Synthetic canary continues from 3 regions (independent of Grafana stack).
   - Recovery: simulated resume; validate dashboards re-flow data within 30s; alerts auto-resolve.

5. **RB-OBS-CARDINALITY-001 (cardinality explosion) runbook dry-run** (sprint contract §6 DoD EVT-017):
   - Synthetic injection: test-only metric `corelink.test.cardinality_explosion{label_a, label_b, ..., label_z}` adds 25k unique series em staging.
   - Expected behavior:
     - cardinality_check.py CI gate (WI-S09-001) would catch em PR (verify offline).
     - Runtime: Mimir tenant tier limit rejects ingest beyond 20k per-metric (WI-S09-001 secondary defense).
     - SEV-2 alert fires (`corelink.metrics.cardinality_budget_violation_total`).
     - Degraded observability documented (some series dropped; not all queryable).
   - Recovery: rollback test metric; runtime alerts auto-resolve; investigation per RB-OBS-CARDINALITY-001 step-by-step.

6. **Canary tenant_id** (synthetic): canary uses dedicated `tenant_id=00000000-0000-0000-0000-CANARY00000` (canonical UUID); excluded from SLI computation via WI-S09-006 recording rule filter (analogous to ManualOverride exclusion).

7. **TenantCtx-only enforcement** (Lote 10.4bis lesson): canary uses dedicated synthetic TenantCtx; isolated from real tenant traffic.

8. **Audit fail-OPEN para canary emit** (matches WI-S09-001/002/003 fail-OPEN pattern):
   - Canary failure em individual loop: log + métrica; continues next loop.
   - 3+ consecutive failures em region: SEV-2 alert.
   - 10+ consecutive failures em region: SEV-1 alert (sustained outage signal).

9. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): canary cron via Worker scheduled trigger; HTTP client via `worker::Fetch`; NEVER `tokio::spawn`. Cron interval canonical 60s.

10. **5-tier canonical** (Lote 10.7bis P0-7): canary tests use synthetic team-tier limits (canonical mid-tier); validates aggregate.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + canary discipline justification)

Synthetic canary é **the operational confidence primitive** — distinguishes "ship-ready service" from "service that ran tests once em CI". CF Workers cron-trigger canary 24/7 from 3 geo-distributed regions exercises full CAS write/read + AC lookup paths every 60s; 72h sustained = 38880 successful loops = high-confidence operability evidence. Sprint contract §6 DoD: este número é gate para GA readiness (S-20 ship).

**Why 3 regions specific** (us-east IAD + eu-west LHR + ap-south BOM): each tests independent CF region + R2 storage + DO instance + D1 database; cross-region failure modes detected (e.g., DO routing primary_region misconfigured, R2 cross-region replication lag). Single-region canary insufficient for multi-region production confidence.

**Why 60s interval**: balances canary cost (CF Workers cron + R2 ops + observability ingest) vs detection latency. 60s = 1440 loops/dia/region; sufficient sample size for SLO computation (1440 = 95% CI ±0.5% on success rate). Shorter intervals (10s) cost prohibitive; longer (5min) detection latency too high.

**Why runbook dry-runs sprint contract §6 DoD EVT-017**: runbook validation = canonical pre-GA exercise. Dry-run em staging (NOT production) simulates incident; oncall responders execute step-by-step; validates (a) runbook documentation accurate, (b) tooling responsive (PagerDuty/Twilio), (c) recovery path functional. Without dry-run, first incident em produção would expose gaps em runbook.

**Why RB-FM-153 (Grafana Cloud outage) priority**: Grafana Cloud is single-vendor dependency; outage = full observability loss; canary independence (CF Workers cron from 3 regions independent of Grafana) is the SOTA mitigation; dry-run validates.

**Why RB-OBS-CARDINALITY-001 priority**: cardinality explosion is #1 root cause of Prom blowup (NetflixOSS 2018 precedent; cited em WI-S09-001 narrative). Sprint contract §15 R-Cardinality-explosion identifies as M-Probability/H-Detection/HIGH-Impact risk. Dry-run validates CI gate + Mimir tier limit defense-in-depth.

**Adversarial scenarios**:
- **Region partial outage** (R2 down em IAD only): canary IAD fails; SEV-2 after 3 consecutive; cross-region canary continues; isolation verified.
- **Canary cron drift** (CF Workers cron lag > 60s): SEV-3 alert; investigation; CF infra status check.
- **Synthetic tenant accidentally counted em SLI**: WI-S09-006 recording rule filter excludes; verify via property test.
- **Canary digest correctness break**: BLAKE3 mismatch alert SEV-1; investigation indicates CAS data integrity issue (rare; validate via WI-S01 inheritance).
- **Observability stack outage detected**: canary independent confirms outage; SEV-3 alert dispatched via Twilio backup.
- **Runbook dry-run reveals documentation gap**: PR updates runbook; re-dry-run.
- **GDPR Art. 17 erasure of canary tenant**: canary tenant explicitly excluded (synthetic; not real customer); legal review confirms exemption.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: canary = foundation GA confidence; sem canary = blind production.
- 12 sign-offs + chaos suite + runbook dry-runs + 72h sustained validation.

## 3. Customer Impact & Journey

**Persona 1 — SRE pre-GA review**: opens canary dashboard; sees 38880/38880 successful loops over 72h; ship gate criterion met.

**Persona 2 — Oncall responder**: receives SEV-2 alert "canary IAD failed 3 consecutive"; opens runbook; investigates; root-causes (e.g., R2 IAD transient outage); ack within 1h.

**Persona 3 — DevOps reviewing**: opens canary dashboard weekly; tracks success rate trend per region; identifies degradation early.

**Persona 4 — Platform engineer reviewing dry-runs**: completes RB-FM-153 dry-run; documents gaps em runbook; PR improvements.

**Persona 5 — Customer (indirect)**: benefits from improved operability; SLA backed by canary evidence; status page (deferred S-13) reflects canary results.

**SLA addendum**:
- Canary loop interval: 60s per region (1 loop/min/region).
- 72h sustained sem gap: 38880 successful loops total target (3 regions × 12960 loops/72h).
- Detection latency: ≤ 5min (3 consecutive failures × 60s intervals).
- Runbook dry-run cadence: quarterly (RB-FM-153 + RB-OBS-CARDINALITY-001 + others as added).

## 4. Capability Mapping

- Trace: `observability_model.md §12 synthetic monitoring canonical` + sprint contract §6 DoD (synthetic 72h + RB dry-runs) + §10.s09 + RB-FM-153 + RB-OBS-CARDINALITY-001 (NEW runbook em este WI).

## 5. Tipo

CF Workers cron-trigger canary + Rust crate + runbook documentation + dry-run scripts em staging; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-canary/` module** — Canary trait + CF Worker impl + assertions + observability emit.

2. **CF Workers cron canary**:
   ```toml
   # wrangler.toml (additions)
   [triggers]
   crons = [
       "*/1 * * * *"  # every 60s; 3 regions independent CF Workers
   ]
   ```
   - Per-region Worker deployment (3 regions); each cron trigger executes canary loop.
   - Cron drift detection: compare actual interval vs expected; SEV-3 if > 90s.

3. **Canary loop implementation**:
   ```rust
   pub async fn execute_canary_loop(&self, region: Region) -> Result<CanaryResult, CanaryError> {
       let canary_tenant_id = "00000000-0000-0000-0000-CANARY00000".parse().unwrap();
       let canary_ctx = TenantCtx::synthetic_canary(canary_tenant_id, region);

       // Test 1: CAS PUT 1 KB blob
       let blob_data: Vec<u8> = generate_canary_blob_1kb(region, now_ms);
       let expected_digest = blake3::hash(&blob_data);
       let put_start = now_ms();
       let put_result = self.client.put_blob(&canary_ctx, &blob_data).await?;
       let put_latency_ms = now_ms() - put_start;
       if put_latency_ms > 100 {
           return Err(CanaryError::AssertionFailed { region: region.into(), assertion: "cas_put_latency_p99_ms <= 100".into() });
       }

       // Test 2: CAS GET + BLAKE3 verify
       let get_start = now_ms();
       let read_data = self.client.get_blob(&canary_ctx, &put_result.digest).await?;
       let get_latency_ms = now_ms() - get_start;
       let actual_digest = blake3::hash(&read_data);
       if actual_digest != expected_digest {
           return Err(CanaryError::DigestMismatch {
               written: hex::encode(expected_digest.as_bytes()),
               read: hex::encode(actual_digest.as_bytes()),
           });
       }
       if get_latency_ms > 50 {
           return Err(CanaryError::AssertionFailed { region: region.into(), assertion: "cas_get_latency_p99_ms <= 50".into() });
       }

       // Test 3: AC lookup synthetic action
       let ac_start = now_ms();
       let ac_result = self.client.ac_lookup(&canary_ctx, &synthetic_action_digest()).await?;
       let ac_latency_ms = now_ms() - ac_start;
       if ac_latency_ms > 30 {
           return Err(CanaryError::AssertionFailed { region: region.into(), assertion: "ac_lookup_latency_p99_ms <= 30".into() });
       }

       Ok(CanaryResult {
           region, timestamp_ms: now_ms(),
           cas_put_latency_p99_ms: put_latency_ms as u32,
           cas_get_latency_p99_ms: get_latency_ms as u32,
           ac_lookup_latency_p99_ms: ac_latency_ms as u32,
           digest_match_correctness: true,
           overall_pass: true,
       })
   }
   ```

4. **Observability stack health verification**:
   - Mimir ingest lag: query `up{job="mimir"}` + comparison vs canonical SLO 30s.
   - Loki query latency: synthetic LogQL query benchmark.
   - Tempo trace visibility lag: emit synthetic trace + query Tempo for retrieval lag.
   - Dashboard refresh latency: synthetic Grafana API render benchmark.

5. **RB-FM-153 runbook + dry-run script** (`docs/runbooks/RB-FM-153-grafana-outage.md` + `tests/runbooks/rb-fm-153-dryrun.sh`):
   ```markdown
   # RB-FM-153: Grafana Cloud Outage

   ## Detection
   - SEV-3 alert: corelink.dashboard.refresh_failures_total > 0 sustained 5min
   - Synthetic canary independent confirms (canary remains up)

   ## Triage
   1. Check Grafana Cloud status page
   2. Verify Mimir/Loki/Tempo tenant ingest health
   3. Determine outage duration estimate

   ## Mitigation
   1. Twilio backup SMS notifications activated automatically
   2. Synthetic canary results redirected to alternate ingestion (deferred)
   3. Status page customer comm (S-13 future)

   ## Recovery
   1. Wait for Grafana Cloud recovery
   2. Validate dashboards re-flow data within 30s
   3. Alerts auto-resolve

   ## Dry-run validation (sprint contract §6 DoD EVT-017)
   - Block Mimir/Loki/Tempo endpoints 30min via firewall rule em staging
   - Validate (a) graceful "no data" + (b) SEV-3 fires + (c) Twilio backup + (d) recovery within 30s of resume
   ```

6. **RB-OBS-CARDINALITY-001 runbook + dry-run script** (`docs/runbooks/RB-OBS-CARDINALITY-001.md` + `tests/runbooks/rb-obs-cardinality-001-dryrun.sh`):
   ```markdown
   # RB-OBS-CARDINALITY-001: Cardinality Explosion

   ## Detection
   - SEV-2 alert: corelink.metrics.cardinality_budget_violation_total > 0
   - Mimir tenant tier limit rejects ingest

   ## Triage
   1. Identify offending metric: query Mimir cardinality endpoint
   2. Identify recent PR adding label cartesian
   3. Estimate cost impact

   ## Mitigation
   1. Emergency: drop offending metric via Mimir tenant config
   2. Or: rollback PR adding label
   3. Reset cardinality_check.py CI gate

   ## Recovery
   1. PR fix label cartesian (canonical aggregation, not raw)
   2. cardinality_check.py CI gate updated
   3. Mimir tier limit re-validated

   ## Dry-run validation (sprint contract §6 DoD EVT-017)
   - Inject test-only metric corelink.test.cardinality_explosion{label_a..z} 25k séries em staging
   - Validate (a) cardinality_check.py rejects offline + (b) Mimir tier limit rejects ingest + (c) SEV-2 fires
   ```

7. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): canary HTTP client via `worker::Fetch`; cron via Worker scheduled trigger; NEVER `tokio::spawn`.

8. **Synthetic tenant SLI exclusion** (Lote 10.8bis P1-NEW-3 inheritance from WI-S09-006):
   - Canary tenant_id excluded em recording rules: `tenant_id != "00000000-0000-0000-0000-CANARY00000"`.
   - Property test validates canary doesn't inflate SLI false-positive.

9. **Métricas operacionais**:
    - `corelink.canary.loops_total{region, result=pass|fail}` (counter; expected ~38880/72h target).
    - `corelink.canary.assertion_failures_total{region, assertion}` (counter; **alert SEV-2 if 3+ consecutive em region**).
    - `corelink.canary.observability_health_failures_total{component}` (counter; **alert SEV-3 if any > 0**).
    - `corelink.canary.dispatch_lag_ms{region}` (histogram; cron drift detection; **alert SEV-3 if > 90s**).
    - `corelink.canary.digest_mismatch_total{region}` (counter; **alert SEV-1 if > 0** — data integrity issue).

10. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar**):
    - `prop_canary_assertion_correctness`: 10k synthetic canary results; assert overall_pass logic correct.
    - `prop_synthetic_tenant_excluded_from_sli`: 10k traffic patterns including canary loops; assert SLI computation excludes canary.
    - `prop_runbook_dryrun_idempotent`: 1k dry-run executions; assert no production impact (staging-only).
    - `prop_72h_sustained_simulation`: simulated 72h × 60s × 3 regions; assert 100% success rate em ideal conditions.
    - `prop_cardinality_dryrun_safe`: 1k injection scenarios; assert containment via Mimir tier limit.
    - `prop_grafana_outage_dryrun_safe`: 1k simulations; assert no production data loss em staging.

11. **Chaos suite** (HIGH_RISK ≥ 10):
    - 1. **Region partial outage** (R2 IAD down): canary IAD fails; SEV-2 after 3; cross-region continues.
    - 2. **Canary cron drift**: synthetic CF Workers cron lag; SEV-3 alert.
    - 3. **Synthetic tenant SLI inflation**: assert canary excluded.
    - 4. **BLAKE3 digest mismatch**: synthetic CAS corruption; SEV-1 alert.
    - 5. **Observability stack outage** (Mimir unavailable): canary continues; SEV-3 alert.
    - 6. **RB-FM-153 dry-run em staging**: full execution; validate behavior; documentation update.
    - 7. **RB-OBS-CARDINALITY-001 dry-run em staging**: full execution; validate containment.
    - 8. **3 regions simultaneous outage**: SEV-1 (catastrophic); page on-call multi-tier.
    - 9. **72h sustained simulation**: 38880 loops; assert 100% success em ideal conditions.
    - 10. **Canary tenant erasure (synthetic)**: legal review confirms exemption; not real customer.

### 6.2 Out-of-scope (deferred)

- Customer-facing status page (deferred S-13; canary feeds future).
- Real User Monitoring (RUM) frontend (anti-scope per sprint contract §10).
- Synthetic monitoring extra-regional (>3 regions; sprint contract §10 anti-scope).
- AI-driven anomaly detection (anti-scope §10).
- Multi-tenant canary (synthetic dedicated tenant canonical).

## 7. Anti-Scope

- ❌ Synthetic monitoring beyond 3 regions (anti-scope §10).
- ❌ Real customer tenant em canary (synthetic dedicated canonical).
- ❌ Canary inflating SLI (Lote 10.8bis P1-NEW-3 lesson; exclusion canonical).
- ❌ Skip runbook dry-runs (sprint contract §6 DoD EVT-017).
- ❌ Skip 72h sustained validation (ship gate criterion).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ Customer-facing status page inicial (S-13 deferred).

## 8. Acceptance Criteria (Gherkin) — 10 scenarios

```gherkin
Feature: Synthetic Canary 3 Regions + Runbook Dry-Run

  Scenario: Canary loop succeeds em all 3 regions
    Given canary deployed em IAD + LHR + BOM
    When CF Workers cron triggers each minute
    Then canary loop executes: CAS PUT + GET + AC LOOKUP
    Then assertions pass: cas_put_latency_p99_ms ≤ 100, cas_get ≤ 50, ac_lookup ≤ 30
    Then BLAKE3 digest matches (data integrity)
    Then corelink.canary.loops_total{result=pass} increments

  Scenario: 72h sustained validation (sprint contract §6 DoD ship gate)
    Given canary running 24/7 em 3 regions
    When 72h elapsed
    Then total loops 38880 (3 × 12960 expected)
    Then success rate 100% (or documented exceptions)
    Then ship gate criterion met

  Scenario: Region partial outage detection
    Given R2 IAD experiencing transient outage 5min
    When canary IAD fails 3 consecutive loops
    Then SEV-2 alert: corelink.canary.assertion_failures_total{region=iad}
    Then cross-region canary (LHR + BOM) continues
    Then isolation verified

  Scenario: BLAKE3 digest mismatch SEV-1
    Given canary CAS PUT writes blob with digest D
    Given canary CAS GET reads blob with digest D' != D (synthetic corruption)
    When canary loop validates
    Then CanaryError::DigestMismatch returned
    Then SEV-1 alert: corelink.canary.digest_mismatch_total{region}
    Then data integrity investigation per RB-CAS-DIGEST-001

  Scenario: Observability stack outage detected
    Given Mimir tenant unavailable 30min
    When canary observability health check runs
    Then HealthReport: all_components_healthy=false
    Then SEV-3 alert: corelink.canary.observability_health_failures_total{component=mimir}
    Then service continues (canary independent)

  Scenario: RB-FM-153 dry-run validates Grafana outage handling
    Given staging environment em quarantine
    When dry-run script blocks Mimir/Loki/Tempo endpoints 30min
    Then dashboards show "no data" graceful (NOT crash)
    Then SEV-3 alert fires: corelink.dashboard.refresh_failures_total
    Then Twilio backup SMS dispatched
    Then on simulated resume: dashboards re-flow within 30s
    Then runbook documentation updated based on findings
    Then sprint contract §6 DoD EVT-017 satisfied

  Scenario: RB-OBS-CARDINALITY-001 dry-run validates cardinality explosion handling
    Given staging environment em quarantine
    When dry-run script injects test metric corelink.test.cardinality_explosion 25k séries
    Then cardinality_check.py rejects offline (CI gate; PR would fail)
    Then Mimir tier limit rejects ingest beyond 20k per-metric
    Then SEV-2 alert fires: corelink.metrics.cardinality_budget_violation_total
    Then degraded observability documented (some series dropped)
    Then on rollback: alerts auto-resolve
    Then sprint contract §6 DoD EVT-017 satisfied

  Scenario: Synthetic tenant excluded from SLI
    Given canary tenant_id 00000000-0000-0000-0000-CANARY00000
    Given 38880 canary loops over 72h
    When SLI burn rate computation runs
    Then canary loops NOT counted em SLI denominator
    Then SLI computation reflects only real traffic
    Then property test prop_synthetic_tenant_excluded_from_sli green (10k iter)

  Scenario: Canary cron drift detection
    Given CF Workers cron expected interval 60s
    Given actual interval > 90s sustained 5min
    When canary dispatch_lag_ms metric exceeds threshold
    Then SEV-3 alert: corelink.canary.dispatch_lag_ms > 90000
    Then investigation; CF infra status check

  Scenario: 3 regions simultaneous outage SEV-1
    Given canary fails em all 3 regions simultaneously sustained 3 loops
    When alert evaluation
    Then SEV-1 alert: catastrophic outage signal
    Then on-call paged via PagerDuty primary + Twilio backup
    Then incident response per RB-MULTI-REGION-OUTAGE
```

## 9. Design Decisions

- 9.1: 3 regions specific (us-east IAD + eu-west LHR + ap-south BOM); sprint contract §6 DoD canonical.
- 9.2: 60s cron interval; balances cost vs detection latency.
- 9.3: 72h sustained (sprint contract §6 DoD ship gate criterion).
- 9.4: CAS PUT + GET + AC LOOKUP synthetic flow.
- 9.5: BLAKE3 digest verify (data integrity correctness).
- 9.6: Synthetic tenant_id canonical UUID; SLI exclusion via WI-S09-006 inheritance.
- 9.7: 2 runbook dry-runs (RB-FM-153 + RB-OBS-CARDINALITY-001) sprint contract §6 DoD EVT-017.
- 9.8: TenantCtx-only enforcement (Lote 10.4bis).
- 9.9: CF Workers Rust API worker::Fetch (Lote 10.7bis R5 P0-3).
- 9.10: NEW 2 runbooks artifacts em `docs/runbooks/`.
- 9.11: NO new ADR (extends observability_model.md §12 + sprint contract §6 DoD canonical).

## 10. Completeness Criteria SOTA

- [ ] **10.s09.007.1** Crate compila + integration tests green.
- [ ] **10.s09.007.2** All 10 Gherkin scenarios green.
- [ ] **10.s09.007.3** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s09.007.4** Chaos suite 10 scenarios green.
- [ ] **10.s09.007.5** **Synthetic canary 24/7 sustained 72h sem gap** (sprint contract §6 DoD; 38880 loops total).
- [ ] **10.s09.007.6** **RB-FM-153 dry-run executed em staging** (sprint contract §6 DoD EVT-017).
- [ ] **10.s09.007.7** **RB-OBS-CARDINALITY-001 dry-run executed em staging** (sprint contract §6 DoD EVT-017).
- [ ] **10.s09.007.8** Observability stack health: Mimir/Loki/Tempo/dashboards all 4 components SLO compliance.
- [ ] **10.s09.007.9** Synthetic tenant SLI exclusion validated (Lote 10.8bis P1-NEW-3 inheritance).
- [ ] **10.s09.007.10** Métricas (5 §6.1.9) emitted; digest_mismatch alerts SEV-1.
- [ ] **10.s09.007.11** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s09.007.12** 3 regions deployment configured per environment.

## 11. DoD

- [ ] All Gherkin/property/chaos green; 72h sustained validated; both runbook dry-runs passed; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Lote 10.8bis P1-2).

## 12. Invariants Validated

- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): canary excluded from SLI; doesn't affect tenant isolation.
- **INV-OBS-CARDINALITY-BUDGET** (HIGH; registry §3.13): RB-OBS-CARDINALITY-001 dry-run validates secondary defense.
- **INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH; registry §3.14): canary execution audit emit via WI-S09-004.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): synthetic canary tenant isolated; no cross-contamination.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Canary module | `crates/corelink-canary/` | Rust |
| CF Worker cron config | `wrangler.toml` (additions per region) | TOML |
| RB-FM-153 runbook | `docs/runbooks/RB-FM-153-grafana-outage.md` | Markdown |
| RB-OBS-CARDINALITY-001 runbook | `docs/runbooks/RB-OBS-CARDINALITY-001.md` | Markdown |
| RB-FM-153 dry-run script | `tests/runbooks/rb-fm-153-dryrun.sh` | Bash |
| RB-OBS-CARDINALITY-001 dry-run script | `tests/runbooks/rb-obs-cardinality-001-dryrun.sh` | Bash |
| Property tests | `crates/corelink-canary/tests/prop_canary.rs` | Rust |
| Chaos suite | `tests/chaos_canary.rs` | Rust |
| Canary dashboard panels | `infra/grafana/dashboards/dash-global-health.json` (additions) | JSON |

## 14. Quality Standards SOTA

- 14.s09.007.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s09.007.2: rustdoc 100% public API.
- 14.s09.007.3: Test coverage ≥ 90%.
- 14.s09.007.4: Canary loop overhead ≤ 10ms p99.
- 14.s09.007.5: SAST clean.
- 14.s09.007.6: Métricas (5 §6.1.9).
- 14.s09.007.7: 100k nightly property test (HIGH_RISK SOTA bar).
- 14.s09.007.8: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::Fetch (Lote 10.7bis R5 P0-3); 5-tier canonical (Lote 10.7bis P0-7); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2).
- 14.s09.007.9: Synthetic tenant SLI exclusion (Lote 10.8bis P1-NEW-3 inheritance from WI-S08-005 ManualOverride pattern).
- 14.s09.007.10: 72h sustained validation (sprint contract §6 DoD ship gate).
- 14.s09.007.11: 2 runbook dry-runs canonical (RB-FM-153 + RB-OBS-CARDINALITY-001; sprint contract §6 DoD EVT-017).

## 15. Chaos Experiments (10)

§6.1.11 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Canary trait + CF Worker cron config + 3 regions deployment | 2 |
| ST-002 | CAS PUT + GET + AC LOOKUP canary loop + BLAKE3 verify | 2 |
| ST-003 | Observability stack health verification | 1.5 |
| ST-004 | Synthetic tenant SLI exclusion (WI-S09-006 inheritance) | 0.5 |
| ST-005 | RB-FM-153 runbook + dry-run script + execution em staging | 1.5 |
| ST-006 | RB-OBS-CARDINALITY-001 runbook + dry-run script + execution em staging | 1.5 |
| ST-007 | Métricas (5) emit | 0.5 |
| ST-008 | Property tests (6 × 10k; 100k nightly) | 1.5 |
| ST-009 | Chaos suite (10) | 1.5 |
| ST-010 | 72h sustained validation execution | 0.5 (passive monitoring) |

**Total**: ~13h. **PERT** O=8h M=12h P=18h: **~12.3h** (matches sprint contract §12 estimate).

## 18. Dependencies

- Hard: WI-S09-001 SEALED (métricas emit + cardinality budget); WI-S09-002 SEALED (logs); WI-S09-003 SEALED (tracing); WI-S09-004 SEALED (audit em canary execution); WI-S09-005 SEALED (dashboards consume canary métricas); WI-S09-006 SEALED (alerts trigger on canary failures + ManualOverride/synthetic exclusion pattern).
- Soft: S-01 + S-02 SEALED (CAS write/read paths real); S-04 SEALED (AC); S-08 SEALED (rate limit aware exclusion).
- Hard infra: 3 CF regions (IAD/LHR/BOM) provisioned; staging environment for runbook dry-runs; PagerDuty + Twilio backup configured.

## 19. Effort PERT: ~12.3h. ## 20. Time-boxing: 18h hard limit.

## 21. Observability

5 metrics §6.1.9 (visibility-on-visibility for canary). Trace span `canary.{loop, assert, health_check, runbook_dryrun}`.

## 22. Cost Analysis

- CF Workers cron: included em CF Workers Unbound plan (cron-trigger free tier).
- R2 ops: 38880 PUT + 38880 GET / 72h × 365/3 / 1M ≈ trivial; ~$0.05/yr.
- Mimir/Loki/Tempo ingest: included em existing tenant cost (WI-S09-001/002/003).
- TCO 12m: ~$1/yr canary infrastructure.
- **Cost saved by canary discipline**: prevents catastrophic GA failure (SEV-1 first-week + customer SLA breach + reputation cost ${significant}).

## 23. API Contract

- Public Rust: `Canary` trait + `CanaryResult`, `HealthReport`, `CanaryError` types; `#[non_exhaustive]`.
- Internal: synthetic tenant TenantCtx; isolated from real traffic.

## 24. Post-mortem Hooks

- 72h validation gap (sustained failure) → SEV-1 + post-mortem mandatório (sprint contract §18 trigger; ship gate blocked).
- Runbook dry-run reveals gap em documentation → PR update + re-dry-run.
- Canary digest mismatch → SEV-1 + post-mortem (data integrity issue investigation).
- Synthetic tenant SLI inflation → SEV-2 (Lote 10.8bis P1-NEW-3 regression).
- 3 regions simultaneous outage detected → CRITICAL post-mortem.

## 25. Rollback / Recovery

- Rollback: revert CF Workers cron config; canary disabled; visibility lost (NOT enforcement).
- Recovery: PR + Worker deploy; idempotent.
- RTO ≤ 5min; RPO ≤ 0min (next cron picks up).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): synthetic TenantCtx isolated; canary tenant_id canonical.
- T(ampering): CF Workers cron immutable; deployment via Terraform.
- R(epudiation): canary results audit emit via WI-S09-004 inheritance.
- I(nformation disclosure): canary uses synthetic data only; no real customer data.
- D(enial of Service): canary loop overhead bounded; no production impact.
- E(scalation of Privilege): CF Workers ACL per region.

**LINDDUN**:
- L(inkability): synthetic tenant_id; no linkability to real customers.
- I(dentifiability): canary blob data synthetic; no PII.
- N(on-repudiation): audit trail via WI-S09-004 inheritance.
- D(etectability): canary results dashboard public-internal.
- D(isclosure): no real customer data em canary path.
- U(nawareness): N/A (synthetic; no customer awareness needed).
- N(on-compliance): N/A (canary excluded from SLI; no automated decisions affecting customers).

## 27. Knowledge Transfer

Tech talk (1.5h): "S-09 Canary: 3 Regions + Runbook Dry-Run + Synthetic Tenant"; doc `docs/dev/canary-architecture.md`; onboarding test 6 questions: 3 regions specific (us-east + eu-west + ap-south), 60s cron interval, 72h sustained ship gate, RB-FM-153 + RB-OBS-CARDINALITY-001 dry-runs, synthetic tenant SLI exclusion (Lote 10.8bis P1-NEW-3 inheritance), BLAKE3 digest correctness.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | 72h sustained gap detected | M | L | HIGH (ship gate blocked) | M | LOW | Multi-region redundancy; SEV-2 alert 3 consec; investigation |
| R-002 | Canary digest mismatch (data integrity) | L | L | CRITICAL | L | LOW | BLAKE3 verify; SEV-1 alert |
| R-003 | Synthetic tenant SLI inflation | L | M | MEDIUM | L | LOW | Lote 10.8bis P1-NEW-3 inheritance; recording rule filter |
| R-004 | RB-FM-153 dry-run reveals gap | M | M | LOW (docs improvement) | L | LOW | Post-dry-run PR documentation update |
| R-005 | RB-OBS-CARDINALITY-001 dry-run impact production | L | L | LOW | L | LOW | Staging-only execution; isolation enforced |
| R-006 | Canary cron drift > 90s | M | L | MEDIUM | L | LOW | SEV-3 alert; CF Workers infra status check |
| R-007 | Region partial outage cascade | L | L | MEDIUM | L | LOW | Cross-region independence; isolation verified |
| R-008 | 3 regions simultaneous outage | L | L | CRITICAL | L | LOW | SEV-1 catastrophic; multi-tier on-call |
| R-009 | Observability stack outage | M | L | MEDIUM | L | LOW | Canary independence; SEV-3; Twilio backup |
| R-010 | Canary loop latency tax > 10ms | L | L | LOW | L | LOW | Bounded; criterion benchmark |
| R-011 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::Fetch |
| R-012 | Cost regression > 10% | L | L | LOW | L | LOW | §14.s09.7 gate (canary cost trivial) |

## 29. Review Checkpoints

D+0 design (Architect; 3 regions + 72h target); D+1 SRE (CF Workers cron + multi-region deployment); D+2 AppSec (TenantCtx synthetic isolation); D+3 code review; D+4 staging deployment + 72h sustained validation start; D+5 RB-FM-153 + RB-OBS-CARDINALITY-001 dry-runs; D+6 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap; Lote 10.8bis P1-2)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation; **mandatory emphatic** — 3 regions deployment + cron + 72h sustained validation_ |
| 4 | Security Lead | _TBD; **mandatory** — synthetic tenant isolation_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos + property test 100k + 2 runbook dry-runs_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; mandatory — runbook documentation + audit trail_ |
| 10 | Privacy | _TBD; mandatory — synthetic data only; no PII em canary_ |
| 11 | Architect | _TBD; **mandatory** — 3 regions canonical + 72h ship gate criterion + 2 runbook dry-runs absorption; consolidates Crypto SME advisory race-correctness via ADR-0034_ |
| 12 | AppSec | _TBD; **mandatory** — synthetic tenant ACL + isolation_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.9) | Criação WI-S09-007; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris lessons absorbed: 5-tier canonical Tier (P0-7); CF Workers Rust API worker::Fetch (R5 P0-3); 100k nightly property test (P1-3); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.13 + §3.14 (Lote 10.8bis P1-13). NEW 2 runbooks (RB-FM-153 Grafana outage + RB-OBS-CARDINALITY-001 cardinality explosion) sprint contract §6 DoD EVT-017. **Lote 10.8bis P1-NEW-3 inheritance**: synthetic tenant SLI exclusion (analogous a ManualOverride exclusion em WI-S08-005). 3 regions specific (IAD + LHR + BOM); 60s cron canonical; 72h sustained ship gate. BLAKE3 digest correctness (consistent com CAS digests primary). Audit trail inheritance from WI-S09-004. |

## 32. Anti-patterns evitados

- ❌ Synthetic monitoring beyond 3 regions (anti-scope §10); ❌ Real customer tenant em canary; ❌ Canary inflating SLI (Lote 10.8bis P1-NEW-3 lesson); ❌ Skip runbook dry-runs (sprint contract §6 DoD EVT-017); ❌ Skip 72h sustained validation (ship gate criterion); ❌ tokio::spawn em CF Workers; ❌ Customer-facing status page inicial (S-13 deferred).

---

**Fim WI-S09-007.** **Fim S-09 7 WIs criados.** Próximo: validators (validate_specs + validate_references) green; commit Lote 10.9; opcional dispatch adversarial reviews (Agent R4 Opus + Sonnet R5).
