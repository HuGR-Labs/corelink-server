---
id: "WI-S09-001"
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
  - "DATA-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
tags: ["wi", "s09", "metrics", "red-use", "cardinality-budget", "analytics-engine", "prometheus", "high-risk"]
---

# WI-S09-001 — Worker Analytics Engine Bindings + RED Métricas + USE Métricas Cloudflare Infra + Cardinality Budget Validator (`crates/corelink-metrics`; CF Workers Analytics Engine binding emitindo 9 métricas RED canonical conforme `observability_model.md §4.2`; USE métricas Cloudflare runtime — cf.cpu_time, R2 ops/sec, D1 row scans, KV read/write, DO storage; cardinality budget validator `scripts/cardinality_check.py` CI gate enforcement INV-OBS-CARDINALITY-BUDGET ≤ 20k séries por métrica + ≤ 100k global; Prom remote write → Grafana Mimir tenant limit secondary defense; trace_id NUNCA em label — exemplar field separate Lote 10.8bis P0-9 race-aware applied to cardinality)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-09](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S09-001 |
| Título | CF Workers Analytics Engine binding emit lib (`crates/corelink-metrics`) emitindo 9 métricas RED canonical (`corelink.cas.put.requests_total{tenant_tier, region, result}`, `corelink.cas.put.duration_seconds{tenant_tier, region}` histogram p50/p95/p99, `corelink.cas.get.bytes_total{tenant_tier, region}`, `corelink.ac.lookup.requests_total{tenant_tier, region, hit/miss}`, `corelink.gc.runs_total{phase, status}`, `corelink.dedup.ratio{tenant_tier, region}` from S-07, `corelink.rate_limit.rejects_total{layer, tenant_tier, reason}` from S-08, `corelink.privacy.dsr_active_total{type}` from S-11, `corelink.billing.events_emitted_total{type, region}` from S-10) + USE métricas Cloudflare runtime (cf.cpu_time gauge, r2.ops counter por bucket, d1.row_scans counter por database, kv.read_quota_used / kv.write_quota_used gauge, do.storage_size gauge); cardinality budget enforcement INV-OBS-CARDINALITY-BUDGET via Python validator CI gate (`scripts/cardinality_check.py`) que estimates séries em PR diff antes de merge; Grafana Mimir tenant limit secondary enforcement; per-metric explicit budget table em `observability_model.md §11.2`; **trace_id NUNCA em label** (exemplar field separate per OpenMetrics 1.0 — Lote 10.8bis lessons absorbed regarding cardinality discipline) |
| Sprint | S-09 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-AUDIT-001 audit chain integrity correlated; observability foundation = blind production se falha = SEV-1 inevitável; sprint contract §2 explicit) |

## 1. Intent

CF Workers Analytics Engine é o **emit primitive** do CoreLink observability stack — sem isso, todos demais sprints emitem métricas para o vazio. Cardinality budget validator é o **cost discipline primitive** — sem isso, single bad-PR adicionando label `trace_id` em métrica explode séries × N requests/dia = OOM em Prometheus + 100× cost spike (Grafana Mimir / DataDog precedent: NetflixOSS 2018 incident). Este WI estabelece foundation para toda emissão de métrica em S-09 + sprints futuros.

```rust
// File: crates/corelink-metrics/src/lib.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait MetricsEmitter: Send + Sync {
    /// Emit counter increment; tenant_tier + region required dimensions; cardinality budget enforced.
    async fn inc_counter(
        &self,
        metric: CanonicalMetric,
        labels: MetricLabels,
        increment: u64,
    ) -> Result<(), MetricsError>;

    /// Emit histogram observation (com optional exemplar trace_id em separate field; NUNCA label).
    async fn observe_histogram(
        &self,
        metric: CanonicalMetric,
        labels: MetricLabels,
        value: f64,
        exemplar: Option<Exemplar>,                    // Lote 10.8bis lesson: trace_id em exemplar NOT label
    ) -> Result<(), MetricsError>;

    /// Emit gauge value (overwrite); typically USE métricas (cf.cpu_time etc.).
    async fn set_gauge(
        &self,
        metric: CanonicalMetric,
        labels: MetricLabels,
        value: f64,
    ) -> Result<(), MetricsError>;

    /// Cardinality estimate API (used by validator + monitoring; budgets enforced em CI).
    async fn estimate_cardinality(
        &self,
        metric: CanonicalMetric,
    ) -> Result<u64, MetricsError>;
}

/// Canonical métrica enum — NEW métrica requires PR amending this enum + observability_model.md §4.2.
/// Lote 10.8bis discipline: enum exhaustive; no string-typed metric names em hot path.
#[derive(strum::Display, strum::EnumIter)]
pub enum CanonicalMetric {
    // RED métricas (sprint contract §5.1 R-S09-1)
    #[strum(serialize = "corelink.cas.put.requests_total")]
    CasPutRequestsTotal,
    #[strum(serialize = "corelink.cas.put.duration_seconds")]
    CasPutDurationSeconds,
    #[strum(serialize = "corelink.cas.get.bytes_total")]
    CasGetBytesTotal,
    #[strum(serialize = "corelink.ac.lookup.requests_total")]
    AcLookupRequestsTotal,
    #[strum(serialize = "corelink.gc.runs_total")]
    GcRunsTotal,
    #[strum(serialize = "corelink.dedup.ratio")]
    DedupRatio,
    #[strum(serialize = "corelink.rate_limit.rejects_total")]
    RateLimitRejectsTotal,
    #[strum(serialize = "corelink.privacy.dsr_active_total")]
    PrivacyDsrActiveTotal,
    #[strum(serialize = "corelink.billing.events_emitted_total")]
    BillingEventsEmittedTotal,

    // USE métricas Cloudflare runtime
    #[strum(serialize = "corelink.cf.cpu_time_us")]
    CfCpuTimeUs,
    #[strum(serialize = "corelink.r2.ops_total")]
    R2OpsTotal,
    #[strum(serialize = "corelink.d1.row_scans_total")]
    D1RowScansTotal,
    #[strum(serialize = "corelink.kv.read_quota_used")]
    KvReadQuotaUsed,
    #[strum(serialize = "corelink.kv.write_quota_used")]
    KvWriteQuotaUsed,
    #[strum(serialize = "corelink.do.storage_size_bytes")]
    DoStorageSizeBytes,
}

/// Allowlist labels (cartesian budget capped); validator rejects PR adding labels fora desta enum.
pub struct MetricLabels {
    pub tenant_tier: Tier,                            // 5-tier canonical (Lote 10.7bis P0-7)
    pub region: Region,                               // ~30 valid CF regions
    pub result: Option<ResultLabel>,                  // Success | ClientError4xx | ServerError5xx
    pub layer: Option<RateLimitLayer>,                // for rate_limit.rejects_total
    pub reason: Option<RejectReason>,                 // for rate_limit.rejects_total
    pub phase: Option<GcPhase>,                       // for gc.runs_total
    pub hit_miss: Option<HitMissLabel>,               // for ac.lookup.requests_total
    // INV-OBS-CARDINALITY-BUDGET: trace_id, request_id, blob_digest NUNCA em label.
    // tenant_id NUNCA em label (use tenant_tier aggregation).
}

/// Exemplar field separate from labels (OpenMetrics 1.0; CAP-OBS-009).
pub struct Exemplar {
    pub trace_id: TraceId,                            // Tempo deep link; NÃO conta em cardinality
    pub span_id: SpanId,
    pub timestamp_ms: i64,
    pub value: f64,
}

#[derive(thiserror::Error, Debug)]
pub enum MetricsError {
    #[error("cardinality budget exceeded: metric={metric}, current={current}, budget={budget}")]
    CardinalityBudgetExceeded { metric: String, current: u64, budget: u64 },

    #[error("Analytics Engine emit failed: {0}")]
    AnalyticsEngineFailed(String),

    #[error("forbidden label detected (Lote 10.8bis discipline): {label} em {metric}")]
    ForbiddenLabelDetected { metric: String, label: String },

    #[error("trace_id em label slot detected (use Exemplar field instead): {metric}")]
    TraceIdEmLabelDetected { metric: String },
}
```

**Cripto-driven invariants enforced**:

1. **INV-OBS-CARDINALITY-BUDGET** (HIGH; registry §3.13 [Lote 10.8bis lesson INV §3.X → §3.12+; verified canonical position via grep]):
   - **Per-metric budget**: máximo **20k séries únicas** (cartesian de tenant_tier × region × result × ...).
   - **Global budget**: máximo **100k séries totais** across all métricas.
   - **Enforcement primary**: Python validator `scripts/cardinality_check.py` em CI; estimates cartesian product de PR diff antes de merge.
   - **Enforcement secondary**: Grafana Mimir tenant limit (`max_series_per_user`); rejects beyond budget at ingest.
   - **5-tier canonical** (Lote 10.7bis P0-7 absorbed): tenant_tier values = free/solo/team/business/enterprise. NO label takes raw `tenant_id` (cardinality explosion: 100k tenants × 30 regions × 3 result = 9M séries).

2. **trace_id NUNCA em label** (Lote 10.8bis cardinality discipline absorbed): `Exemplar` field separate per OpenMetrics 1.0 spec. Without this discipline, `trace_id` em label produces unbounded cardinality (every request = unique trace_id; series count = request count). CI lint check rejects PR with `trace_id` em `MetricLabels` enum.

3. **tenant_id NUNCA em label** (cardinality discipline; tenant_tier aggregation canonical): per-tenant attribution via Logpush/Tempo (NOT Prometheus); 100k tenants × cartesian = blow up.

4. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_tier extracted from `TenantCtx` middleware (S-03 WI-S03-003); NEVER request body (forbidden label detected error).

5. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): emit via `worker::send_future()` (Analytics Engine binding fire-and-forget); NEVER `tokio::spawn`.

6. **Async fail-open** (observability sprint nuance — different from audit fail-closed):
   - Audit emit fail-closed (S-04/S-06 pattern; data integrity).
   - Metrics emit **fail-open** (best-effort; observability degradation acceptable; missing metric NOT compromise security/integrity).
   - Rationale: blocking request on metrics emit failure would create reverse-priority outage (metrics infra outage → all requests blocked).
   - Enforce: emit errors logged em SEV-3 + counter `corelink.metrics.emit_failures_total{reason}` (alerting if > 1% of emits fail).

7. **5-tier canonical Tier label** (Lote 10.7bis P0-7 absorbed): `enum Tier { Free, Solo, Team, Business, Enterprise }`. Cartesian em métrica `cas.put.requests_total{tenant_tier, region, result}` = 5 × 30 × 3 = **450 séries** (well under 20k budget); 9 RED métricas × 450 avg = 4050 séries baseline.

8. **Cardinality validator algorithm** (`scripts/cardinality_check.py`):
   ```python
   # Pseudo:
   # 1. Parse PR diff for `MetricLabels` struct additions OR `CanonicalMetric` enum additions.
   # 2. For each metric: compute cartesian product of label cardinalities.
   #    tenant_tier: 5 (canonical fixed)
   #    region: 30 (canonical max — current CF regions)
   #    result: 3 (Success / ClientError4xx / ServerError5xx)
   #    layer: 4 (camadas 1-4 PAT-RATE-LIMIT-001)
   #    reason: ~6 (RejectReason enum size)
   #    ...
   # 3. Reject PR if estimated > 20k per-metric OR > 100k global.
   # 4. Reject PR if forbidden label detected (trace_id, tenant_id, request_id, blob_digest).
   ```

9. **Audit fail-closed para cardinality budget violations** (Lote 10.6bis pattern adapted): emit `corelink.metrics.cardinality_budget_violation_total` SEV-2 alert; CI gate fails PR; commit blocked.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + cardinality discipline justification)

Worker Analytics Engine bindings constituem a **emit primitive** do CoreLink observability stack. Sem disciplina rigorosa de cardinality budget, **single bad-PR pode catastrofically explode prometheus storage** — NetflixOSS 2018 incident: `request_id` adicionado em label causou 5B séries em 24h, $50k/mo Prometheus blowup, 14h debug. CoreLink absorbeu lessons via INV-OBS-CARDINALITY-BUDGET (registry §3.13) + validator CI gate.

**Why Analytics Engine (vs Worker emit direto via fetch)**: CF Analytics Engine bindings são purpose-built para Workers — `event.waitUntil()` fire-and-forget; aggregation no edge antes de Prom remote write; sub-µs emit overhead. Direct fetch emit per-request blocks request hot path (50-200ms latency tax inaceitável em SLA p99 ≤ 3ms).

**Why cardinality budget 20k per-metric / 100k global**: Prometheus single-series storage ≈ 3 KB/sample × 10k samples/day ≈ 30 MB/series/day. 100k séries × 30 MB = 3 TB/day. Grafana Mimir tier 1 limit é 1M séries/tenant; CoreLink target 100k = 10% headroom para variance. Per-metric 20k = 5% sub-budget; allows 5-10 NEW métricas em S-10/S-11/etc sem replanning.

**Why trace_id em Exemplar (NOT label)**: trace_id é unique-per-request (≈ 10k unique values/sec sustained); em label slot = unbounded cardinality. OpenMetrics 1.0 spec defines `# EXAMPLAR` line per histogram bucket — Grafana Tempo deep links via `traceID` field separate. Click no exemplar em Grafana → opens Tempo trace view → 10× faster debug than searching by trace_id.

**Why tenant_id NÃO em label** (cardinality discipline + privacy): 100k tenants × cartesian = 9M+ séries baseline. Per-tenant attribution via Logpush logs (cardinality bounded by retention) + Tempo traces (sampled). Tenant_tier aggregation (5 values) suffices for SLO compliance + privacy redaction.

**Why fail-open metrics** (vs fail-closed audit): observability degradation é acceptable — alarme via SEV-3 + degraded operability; blocking request on metrics fail é reverse-priority outage. Audit é fail-closed (data integrity). Metrics fail-open + audit fail-closed canonical Lote 10.6bis lesson distinção.

**Adversarial scenarios**:
- **Bad-PR adicionando trace_id em label**: validator CI gate rejects (lint check); local clippy lint catches early; integration test asserts emit returns ForbiddenLabelDetected.
- **Cardinality estimate divergence**: validator overestimates conservatively (cartesian = max); Mimir tenant limit secondary defense at runtime.
- **NEW region added (e.g., 31st CF region)**: PR adds `Region` enum variant; cardinality validator recomputes; if exceeds 20k for any metric → ADR required (e.g., aggregate at metro-level).
- **NEW métrica added beyond 9 canonical**: PR amends `CanonicalMetric` enum + `observability_model.md §4.2`; cardinality validator estimates new métrica + adjusts global budget.
- **Analytics Engine outage** (Cloudflare control plane SEV-1): metrics emit returns AnalyticsEngineFailed; counter increments; alert fires; service continues (fail-open).
- **Per-tenant cardinality explosion via crafted user input**: NÃO possível — labels are enum-typed (Tier/Region/Result enums; no string-typed labels accept user input).

**Risk justification HIGH_RISK**:
- **FF-HR-005**: observability é foundation security control (CTRL-AUDIT-001 chain integrity verifiable via metrics emit health).
- 12 sign-offs + chaos 30d sustained + cardinality validator CI gate + property test 100k.

## 3. Customer Impact & Journey

**Persona 1 — DevOps reviewing**: opens Grafana DASH-GLOBAL-HEALTH; sees per-tier success rate, latency p99, error rate. SLO computation uses canonical 9 RED métricas. No raw tenant_id labels (privacy compliance LGPD/GDPR).

**Persona 2 — Developer adding NEW métrica**: PR adds `CanonicalMetric::S11ConsentEventTotal`; CI cardinality_check.py estimates: 5 tier × 30 region × 6 consent_type = 900 séries. Within budget; PR approved.

**Persona 3 — Developer adding bad-PR (trace_id label)**: PR adds `trace_id: String` to `MetricLabels` struct; CI lint rejects with `ForbiddenLabelDetected{label: "trace_id"}`; PR blocked; developer educated on Exemplar pattern.

**Persona 4 — SRE responder**: SEV-3 alert fires `corelink.metrics.cardinality_budget_violation_total > 0`; opens dashboard; identifies offending metric; investigates recent PRs; rolls back if necessary.

**Persona 5 — Platform engineer reviewing cost**: Grafana Mimir tenant cost projection $USD/month em DASH-COST; cardinality count vs budget tracked; per-PR cost regression gate (sprint contract §14.s09.7) prevents > 10% increases.

**SLA addendum**:
- Metrics emit overhead: ≤ 50µs p99 per call (sub-µs typical).
- Analytics Engine remote write lag: ≤ 30s (CF Analytics Engine SLA).
- Cardinality budget: 20k per-metric / 100k global; CI enforcement ≤ 5min PR feedback.
- Validator false-positive rate: 0 (cartesian estimate is conservative upper bound).

## 4. Capability Mapping

- **CAP-OBS-001** (Metrics emission RED + USE) — IMPLEMENTA primary.
- Trace: `observability_model.md §4.2 9 RED metrics canonical + §11.2 cardinality budget` + `invariant_registry.md INV-OBS-CARDINALITY-BUDGET` + `failure_modes.md FM-NNN observability outages` + sprint contract §5.1 (R-S09-1/2/3) + §14.s09.1 cardinality discipline + §14.s09.7 cost regression gate + OpenMetrics 1.0 spec (exemplars).

## 5. Tipo

CF Workers Analytics Engine binding emit lib + Python cardinality validator + Prom remote write integration; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-metrics/` module** — MetricsEmitter trait + Analytics Engine impl + tests.

2. **9 RED métricas canonical emitting** (sprint contract §5.1 R-S09-1):
   - `corelink.cas.put.requests_total{tenant_tier, region, result}` (counter; 5×30×3 = 450 séries)
   - `corelink.cas.put.duration_seconds{tenant_tier, region}` (histogram; 5×30 = 150 séries × buckets)
   - `corelink.cas.get.bytes_total{tenant_tier, region}` (counter; 150 séries)
   - `corelink.ac.lookup.requests_total{tenant_tier, region, hit/miss}` (counter; 300 séries)
   - `corelink.gc.runs_total{phase, status}` (counter; 4 phase × 3 status = 12 séries)
   - `corelink.dedup.ratio{tenant_tier, region}` (gauge; 150 séries; from S-07)
   - `corelink.rate_limit.rejects_total{layer, tenant_tier, reason}` (counter; 4×5×6 = 120 séries; from S-08)
   - `corelink.privacy.dsr_active_total{type}` (gauge; ~5 dsr_type = 5 séries; from S-11)
   - `corelink.billing.events_emitted_total{type, region}` (counter; ~10 type × 30 region = 300 séries; from S-10)
   - **Total RED**: ~1700 séries (well under 20k per-metric; ~17% of 100k global budget allocated to RED).

3. **6 USE métricas Cloudflare runtime** (sprint contract §5.1 R-S09-3):
   - `corelink.cf.cpu_time_us{region}` (gauge per Worker invocation; 30 séries).
   - `corelink.r2.ops_total{bucket, op_type}` (counter; ~5 buckets × 4 op_type = 20 séries).
   - `corelink.d1.row_scans_total{database}` (counter; ~10 databases = 10 séries).
   - `corelink.kv.read_quota_used{namespace}` (gauge; ~5 KV namespaces = 5 séries).
   - `corelink.kv.write_quota_used{namespace}` (gauge; 5 séries).
   - `corelink.do.storage_size_bytes{do_class}` (gauge; ~10 DO classes = 10 séries).
   - **Total USE**: ~80 séries (negligible budget impact).

4. **Cardinality budget validator** `scripts/cardinality_check.py`:
   ```python
   #!/usr/bin/env python3
   # Lote 10.9 sprint contract §14.s09.1 + §14.s09.7 enforcement.
   # CI hook: runs em PR diff; rejects if cartesian > 20k per-metric OR > 100k global.

   import sys, json
   from pathlib import Path

   FORBIDDEN_LABELS = {"trace_id", "tenant_id", "request_id", "blob_digest", "user_email", "ip_address"}
   PER_METRIC_BUDGET = 20_000
   GLOBAL_BUDGET = 100_000

   # Parse Rust source for CanonicalMetric + MetricLabels enum (use syn-style parsing).
   def estimate_cardinality(metric_name: str, labels: dict[str, int]) -> int:
       """Cartesian product of label cardinalities."""
       result = 1
       for label, card in labels.items():
           result *= card
       return result

   def check_pr_diff(diff_path: Path) -> int:
       """Returns exit code: 0 = pass; 1 = budget exceeded; 2 = forbidden label."""
       # Parse diff for label additions
       # For each metric: estimate cartesian
       # Check global budget
       # Emit JSON report
       pass

   if __name__ == "__main__":
       sys.exit(check_pr_diff(Path(sys.argv[1])))
   ```
   Reference implementation lives em `scripts/cardinality_check.py` (~150 LOC); CI hook em `.github/workflows/cardinality-gate.yml`.

5. **Forbidden label CI lint** (Lote 10.8bis discipline absorbed):
   - clippy lint: rejects `MetricLabels` struct addition with field name in `FORBIDDEN_LABELS`.
   - cardinality_check.py secondary: rejects PR text matching `trace_id|tenant_id|request_id` em label slots.
   - Rust type system primary: `MetricLabels` is closed enum-typed struct; no `String` fields accept arbitrary user input.

6. **Exemplar emission** (CAP-OBS-009 + sprint contract §5.3 R-S09-9):
   - `observe_histogram(metric, labels, value, exemplar=Some(Exemplar{trace_id, span_id, timestamp_ms, value}))`.
   - OpenMetrics 1.0 format: `cas_put_duration_seconds_bucket{le="0.005",tenant_tier="team",region="iad"} 12345 # {trace_id="abc123"} 0.004 1700000000`.
   - Grafana Mimir + Tempo deep link integration: click exemplar dot → opens Tempo trace.

7. **Analytics Engine binding integration**:
   ```rust
   // wrangler.toml (additions):
   // [[analytics_engine_datasets]]
   // binding = "ANALYTICS"
   // dataset = "corelink_metrics"

   pub struct AnalyticsEngineMetricsEmitter {
       binding: AnalyticsEngineBinding,
       cardinality_tracker: Arc<RwLock<HashMap<CanonicalMetric, HashSet<LabelTuple>>>>,
   }

   impl MetricsEmitter for AnalyticsEngineMetricsEmitter {
       async fn inc_counter(&self, metric, labels, increment) -> Result<(), MetricsError> {
           // Lote 10.7bis R5 P0-3: worker::send_future for fire-and-forget
           let event = AnalyticsEngineEvent::counter(metric, &labels, increment);
           worker::send_future(async move {
               match self.binding.write_data_point(event).await {
                   Ok(_) => {}
                   Err(e) => {
                       // Fail-open: log + counter; do NOT propagate to caller
                       worker::console_warn!("metrics emit failed: {}", e);
                       emit_metric("corelink.metrics.emit_failures_total", 1.0, &[("reason", "ae_write_failed")]);
                   }
               }
           });
           // Update cardinality tracker (non-blocking)
           self.cardinality_tracker.write().await
               .entry(metric).or_default()
               .insert(LabelTuple::from(&labels));
           Ok(())
       }
       // ... observe_histogram, set_gauge, estimate_cardinality similar pattern
   }
   ```

8. **Prom remote write integration** (Grafana Mimir backend):
   - Analytics Engine → Cloudflare Logpush → Prometheus remote write endpoint (Grafana Mimir `https://prometheus-prod-xxx.grafana.net/api/prom/push`).
   - Authentication: API key per region em CF Worker secret.
   - Aggregation: AE pre-aggregates 1min windows; downsampled to 5min em Mimir for long-term storage.
   - Retention: Mimir 30d hot + 13mo cold (Mimir tier configuration).

9. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): all emits via `worker::send_future()`; NEVER `tokio::spawn`. `async_lock::RwLock` for cardinality tracker (Lote 10.3-tris).

10. **Audit fail-closed para cardinality budget violations** (Lote 10.6bis adapted):
    - Runtime detection: `inc_counter` em metric near budget triggers `corelink.metrics.cardinality_approaching_budget_total` SEV-3.
    - CI gate: PR adding label exceeding budget → fail; emit `corelink.metrics.cardinality_budget_violation_total` audit event.

11. **Métricas operacionais** (this WI emits about itself):
    - `corelink.metrics.emit_total{metric_canonical, result}` (counter; meta-emit; ≤ 16 metrics × 2 result = 32 séries).
    - `corelink.metrics.emit_failures_total{reason}` (counter; **alert SEV-3 if > 1% of emits fail sustained 5min**).
    - `corelink.metrics.cardinality_approaching_budget_total{metric}` (counter; **alert SEV-3 if > 0** — proactive warning before hard limit).
    - `corelink.metrics.cardinality_budget_violation_total{metric}` (counter; **alert SEV-2 if > 0** — hard limit hit; degraded observability).
    - `corelink.metrics.emit_duration_us` (histogram; SLO ≤ 50µs p99).

12. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_cardinality_cartesian_correct`: 10k random label combinations; assert estimate matches actual cartesian.
    - `prop_no_trace_id_em_label`: 100k random emit calls with random labels; assert ForbiddenLabelDetected if trace_id present.
    - `prop_emit_idempotent_under_retry`: 1k retries on failed emit; assert counter increments correct (no double-count via dedup_key).
    - `prop_fail_open_on_ae_outage`: simulate AE outage; assert emit returns Ok (fail-open); error counter increments.
    - `prop_cardinality_budget_validator_conservative`: 1k synthetic label sets; assert validator never under-estimates (always upper bound).
    - `prop_exemplar_separate_from_label`: 10k histogram observations with exemplar; assert exemplar trace_id NEVER appears in label slot.

13. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 10):
    - 1. **AE outage 30min**: emit fail-open; SEV-3 alert; recovery on resume.
    - 2. **Cardinality budget hit** (synthetic 21k séries): SEV-2 alert; metric ingest rejected by Mimir tier limit; CI gate would have caught earlier.
    - 3. **Bad-PR adding trace_id label**: CI cardinality_check.py rejects; PR blocked; integration test asserts.
    - 4. **NEW region added (31st CF region)**: validator recomputes cartesian; alerts if exceeds budget.
    - 5. **Mimir tenant outage**: AE retains aggregation 5min; recovery resumes; minimal data loss.
    - 6. **Logpush latency 60s**: data lag acceptable per sprint contract §15 R-Logpush; SLO documented.
    - 7. **Concurrent emit + cardinality estimate race**: async_lock::RwLock; deterministic; property test 100k.
    - 8. **Forbidden label injection attempt**: type system rejects at compile time; integration test asserts.
    - 9. **Exemplar volume spike**: 100k requests/sec × exemplar = 100k exemplars; sample 10% to bound storage.
    - 10. **Validator false-positive**: synthetic edge case; manual override via ADR; precedent: Lote 10.7bis P0-9 cross-region routing.

### 6.2 Out-of-scope (deferred)

- Custom métricas via dynamic API (anti-scope §10; enum canonical); NEW métrica requires PR amending enum.
- User-facing métrics (customer self-service); deferred S-13.
- ML-based cardinality forecasting (anti-scope §10).
- Tail-based sampling (delegated WI-S09-003 tracing).
- Audit chain integrity verification (delegated WI-S09-004).
- Cross-region metric federation (deferred S-14; per-region Mimir tenant initial).

## 7. Anti-Scope

- ❌ Dynamic metric registration (string-typed name); enum canonical.
- ❌ trace_id em label slot (Lote 10.8bis discipline; cardinality explosion).
- ❌ tenant_id em label slot (cardinality + privacy LGPD/GDPR).
- ❌ String-typed user-input labels (label injection cardinality attack).
- ❌ Fail-closed em emit (reverse-priority outage; metrics fail-open canonical).
- ❌ Synchronous emit blocking request hot path (fire-and-forget canonical).
- ❌ `tokio::spawn` em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ Skip cardinality validator CI gate (silent budget violation).

## 8. Acceptance Criteria (Gherkin) — 10 scenarios

```gherkin
Feature: Worker Analytics Engine Metrics Emit + Cardinality Budget

  Scenario: Within-budget metric emit succeeds
    Given canonical metric corelink.cas.put.requests_total
    Given labels {tenant_tier=team, region=iad, result=Success}
    When inc_counter(metric, labels, 1) called
    Then AE write_data_point dispatched via worker::send_future (fail-open)
    Then cardinality tracker updated: {team×iad×Success} added
    Then no error returned

  Scenario: Cardinality budget approaching alert
    Given canonical metric near 16k séries (80% of 20k per-metric budget)
    When inc_counter adds 17000th unique label tuple
    Then SEV-3 alert: corelink.metrics.cardinality_approaching_budget_total{metric}
    Then proactive warning to platform team via PagerDuty SEV-3 (sprint contract §6 DoD)

  Scenario: Cardinality budget violation
    Given canonical metric exceeds 20k séries
    When inc_counter adds 20001st tuple
    Then Mimir tier limit rejects ingest
    Then SEV-2 alert: corelink.metrics.cardinality_budget_violation_total{metric}
    Then degraded observability documented (some series dropped)
    Then CI gate would have caught earlier in PR flow

  Scenario: Forbidden label rejected at compile time
    Given developer adds field `trace_id: String` to MetricLabels struct em PR
    When clippy lint runs
    Then PR fails compilation
    Then integration test asserts MetricsError::ForbiddenLabelDetected returned
    Then developer educated on Exemplar pattern

  Scenario: Forbidden label rejected at validator
    Given developer adds CanonicalMetric variant referencing tenant_id em label set
    When scripts/cardinality_check.py CI hook runs
    Then PR fails: "FORBIDDEN_LABELS detected: tenant_id in metric X"
    Then PR blocked from merge

  Scenario: Exemplar emission with trace_id (separate from label)
    Given OTLP middleware (WI-S09-003) propagated trace_id to context
    When observe_histogram(corelink.cas.put.duration_seconds, labels, 0.004, exemplar=Some(trace_id))
    Then OpenMetrics line emitted: cas_put_duration_seconds_bucket{...} 12345 # {trace_id="abc"} 0.004 timestamp
    Then Grafana Tempo deep link works: click exemplar → opens trace
    Then trace_id NEVER appears in label slot (cardinality preserved)

  Scenario: Analytics Engine outage fail-open
    Given AE binding write_data_point returns error 30min sustained
    When inc_counter called for any metric
    Then emit returns Ok (fail-open canonical; observability degradation OK)
    Then corelink.metrics.emit_failures_total{reason=ae_write_failed} increments
    Then SEV-3 alert (NOT SEV-1 — request not blocked)
    Then on AE recovery: emits resume; no retry buffer (best-effort)

  Scenario: NEW region added (31st CF region)
    Given Region enum extended from 30 to 31 variants
    When PR runs cardinality_check.py
    Then cartesian recomputed: cas.put.requests_total = 5×31×3 = 465 séries (previously 450)
    Then within 20k budget; PR approved

  Scenario: Concurrent emit serialization (cardinality tracker race)
    Given 1000 concurrent emit calls (different metrics + labels)
    When async_lock::RwLock acquired
    Then deterministic outcome; tracker state correct
    Then property test prop_emit_idempotent_under_retry green (100k iter)

  Scenario: Cost regression gate per sprint contract §14.s09.7
    Given baseline 30d cardinality count 50k séries
    Given new PR estimates 60k séries (20% increase)
    When CI cost regression gate evaluates
    Then SEV-3 alert; ADR required to merge (sprint contract §14.s09.7)
    Then merge blocked unless ADR approved
```

## 9. Design Decisions

- 9.1: CF Analytics Engine binding (NOT direct fetch emit); sub-µs hot path overhead.
- 9.2: Enum-typed CanonicalMetric (NOT string-typed dynamic registration); compile-time discipline.
- 9.3: Enum-typed MetricLabels (NOT String-typed); cardinality cartesian bounded.
- 9.4: Cardinality budget 20k per-metric / 100k global (sprint contract §5.1 R-S09-2).
- 9.5: trace_id em Exemplar field (NOT label); OpenMetrics 1.0 canonical.
- 9.6: tenant_tier aggregation (NOT tenant_id); 5-tier canonical Lote 10.7bis P0-7.
- 9.7: Fail-open emit (vs audit fail-closed); observability degradation acceptable.
- 9.8: Python validator CI gate primary; Mimir tier limit secondary defense.
- 9.9: TenantCtx-only enforcement (Lote 10.4bis).
- 9.10: CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3).
- 9.11: NEW INV-OBS-CARDINALITY-BUDGET registered em invariant_registry §3.13.
- 9.12: NO new ADR (extends observability_model.md §11.2 budget canonical).

## 10. Completeness Criteria SOTA

- [ ] **10.s09.001.1** Module compila + integration tests green.
- [ ] **10.s09.001.2** All 10 Gherkin scenarios green.
- [ ] **10.s09.001.3** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- [ ] **10.s09.001.4** Chaos suite 10 scenarios green.
- [ ] **10.s09.001.5** **Cardinality budget enforced em CI**: `cardinality_check.py` green em 9 RED + 6 USE = 15 métricas baseline; total ~1780 séries (well under 100k global budget).
- [ ] **10.s09.001.6** Mimir tier limit secondary defense configured per region.
- [ ] **10.s09.001.7** Exemplar deep link Grafana Tempo testado (click → trace open) for 3 fluxos: cas.put, cas.get, ac.lookup.
- [ ] **10.s09.001.8** Forbidden label CI lint validated (PR with trace_id em label rejected).
- [ ] **10.s09.001.9** Métricas (5 §6.1.11) emitted; cardinality_approaching alerts SEV-3 if > 0; budget_violation alerts SEV-2 if > 0.
- [ ] **10.s09.001.10** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s09.001.11** Cost regression gate sprint contract §14.s09.7 enforced (>10% cardinality estimate increase requires ADR).
- [ ] **10.s09.001.12** Analytics Engine binding configured em `wrangler.toml` per environment.
- [ ] **10.s09.001.13** Prom remote write → Grafana Mimir end-to-end validated em staging 24h.

## 11. DoD

- [ ] Module compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Crypto SME advisory consolidated em Architect per ADR-0034 — Lote 10.8bis P1-2 lesson absorbed).

## 12. Invariants Validated

- **INV-OBS-CARDINALITY-BUDGET** (HIGH; registry §3.13): per-metric ≤ 20k + global ≤ 100k; CI gate + Mimir tier limit defense-in-depth.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): tenant_tier aggregation (NOT tenant_id) preserves cross-tenant isolation em métricas.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): no per-tenant labels; aggregated metrics privacy-preserving.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| MetricsEmitter module | `crates/corelink-metrics/` | Rust |
| AE binding impl | `crates/corelink-metrics/src/ae_emitter.rs` | Rust |
| CanonicalMetric enum | `crates/corelink-metrics/src/canonical.rs` | Rust |
| MetricLabels enum | `crates/corelink-metrics/src/labels.rs` | Rust |
| Cardinality validator | `scripts/cardinality_check.py` | Python |
| CI workflow | `.github/workflows/cardinality-gate.yml` | YAML |
| Wrangler AE binding | `wrangler.toml` (additions per environment) | TOML |
| Property tests | `crates/corelink-metrics/tests/prop_metrics.rs` | Rust |
| Chaos suite | `tests/chaos_metrics.rs` | Rust |
| Mimir tenant config | `infra/grafana/mimir-tenant-limits.yaml` | YAML |

## 14. Quality Standards SOTA

- 14.s09.001.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s09.001.2: rustdoc 100% public API.
- 14.s09.001.3: Test coverage ≥ 90%.
- 14.s09.001.4: Emit overhead ≤ 50µs p99 (sub-µs typical via fire-and-forget).
- 14.s09.001.5: SAST clean; cardinality_check.py mypy clean.
- 14.s09.001.6: Métricas (5 §6.1.11).
- 14.s09.001.7: Cardinality budget enforced (sprint contract §5.1 R-S09-2 + §14.s09.1).
- 14.s09.001.8: Cost regression gate per-PR (sprint contract §14.s09.7); >10% cardinality estimate increase requires ADR.
- 14.s09.001.9: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical Tier (Lote 10.7bis P0-7); column drift no `_ms` suffix (Lote 10.7bis P0-3).
- 14.s09.001.10: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s09.001.11: Forbidden label discipline: trace_id, tenant_id, request_id, blob_digest NUNCA em label (Lote 10.8bis lesson absorbed).
- 14.s09.001.12: OpenMetrics 1.0 exemplar canonical (CAP-OBS-009 Tempo deep link).

## 15. Chaos Experiments (10)

§6.1.13 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap; Lote 10.8bis P1-2); sprint contract §6 DoD enforces.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + MetricsEmitter trait + CanonicalMetric/MetricLabels enums | 2 |
| ST-002 | AE binding impl + worker::send_future fire-and-forget | 2 |
| ST-003 | 9 RED métricas emit integration (CAS/AC/GC/Dedup/RateLimit/Privacy/Billing) | 3 |
| ST-004 | 6 USE métricas Cloudflare runtime emit | 1.5 |
| ST-005 | Cardinality validator scripts/cardinality_check.py + CI workflow | 3 |
| ST-006 | Forbidden label clippy lint + integration test | 1 |
| ST-007 | Exemplar emission + Grafana Tempo integration | 1.5 |
| ST-008 | Mimir tenant tier limit configuration + Prom remote write | 1.5 |
| ST-009 | Property tests (6 × 10k; 100k nightly) | 2.5 |
| ST-010 | Chaos suite (10) | 2 |

**Total**: ~20h. **PERT** O=12h M=18h P=28h: **~18.7h** (matches sprint contract §12 estimate exactly).

## 18. Dependencies

- Hard: S-01 SEALED (CAS write métricas); S-02 SEALED (CAS read); S-03 SEALED (TenantCtx middleware com tenant_tier).
- Soft: S-04 SEALED (AC métricas); S-06 SEALED (GC métricas); S-07 SEALED (dedup); S-08 SEALED (rate limit); S-10/S-11 (billing/privacy métricas — staging stubs OK).
- Hard infra: Grafana Mimir tenant provisioned + API key per region; Cloudflare Analytics Engine binding enabled.

## 19. Effort PERT: ~18.7h. ## 20. Time-boxing: 28h hard limit.

## 21. Observability

5 metrics §6.1.11 (visibility-on-visibility). Trace span `metrics.{emit, cardinality_check, ae_write}`.

## 22. Cost Analysis

- Per-emit: ~$0.0000001 (Analytics Engine ingest is included em CF Workers Unbound plan up to 10M events/day per dataset).
- TCO 12m: 5 regions × 100M events/day × 365 = 182.5B events/yr; well within free tier of CF AE (10M events/day = 3.65B/yr included; surplus ~$0.50 per 10M events).
- Mimir storage: 100k séries × 30 MB/yr = 3 TB/yr; Mimir cost ~$300/mo per TB; ~$10800/yr.
- Logpush: ~1 TB/mo logs to Mimir → ~$50/mo Logpush egress.
- TCO total: ~$11000/yr observability infra (90% Mimir storage).
- **Cost saved by cardinality discipline**: prevents NetflixOSS-style 100× cost spike (would be $1.1M/yr if cardinality unconstrained).

## 23. API Contract

- Public: `MetricsEmitter` trait + `CanonicalMetric`, `MetricLabels`, `Exemplar`, `MetricsError` types; `#[non_exhaustive]`.
- Internal: AnalyticsEngineMetricsEmitter struct; AE binding bridge.

## 24. Post-mortem Hooks

- Cardinality explosion (any metric > 80% budget) → 5-Why mandatório (sprint contract §18 trigger).
- AE outage > 30min sustained → SEV-3 5-Why (observability lag).
- Mimir tenant rejection > 1% emits → SEV-2 (degraded ingest).
- Forbidden label leak (PR merged without CI catching) → CRITICAL post-mortem; CI gate review.
- Cost regression > 20% sustained → SEV-3; ADR required.

## 25. Rollback / Recovery

- Rollback: revert AE binding `wrangler.toml`; metrics emit disabled; DASH-* show "no data"; observability lost (NOT enforcement).
- Recovery: AE binding re-applied; emits resume; ~30s data lag; no historical loss (Mimir retains).
- RTO ≤ 5min; RPO ≤ 30s.

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx middleware (S-03); tenant_tier authoritative source.
- T(ampering): AE writes append-only via CF; no in-place modification.
- R(epudiation): emit fail-open + counter; eventual consistency catches.
- I(nformation disclosure): NO tenant_id em labels (privacy LGPD/GDPR); aggregated métricas.
- D(enial of Service): cardinality budget prevents intentional explosion attack.
- E(scalation of Privilege): emit is unauthenticated within Worker; AE binding ACL'd by binding name.

**LINDDUN**:
- L(inkability): aggregated tenant_tier (NOT tenant_id); no cross-tenant linkability.
- I(dentifiability): tenant_tier (5 values) too coarse for re-identification.
- N(on-repudiation): N/A em métricas (audit log separate WI-S09-004).
- D(etectability): cardinality discipline = privacy-preserving.
- D(isclosure): aggregated métricas non-sensitive.
- U(nawareness): customer self-service deferred S-13.
- N(on-compliance): N/A (métricas não constituem "automated decision affecting individual" per LGPD/GDPR).

## 27. Knowledge Transfer

Tech talk (1.5h): "S-09 Métricas: Cardinality Discipline + Exemplars + Fail-Open"; doc `docs/dev/metrics-architecture.md`; onboarding test 8 questions: cardinality budget rationale (NetflixOSS 2018), trace_id em Exemplar (NOT label), tenant_tier aggregation (privacy + cardinality), fail-open vs fail-closed (audit comparison), Analytics Engine vs direct fetch, 5-tier canonical, OpenMetrics 1.0 exemplar spec, cost regression gate.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cardinality explosion via bad-PR | M | H | HIGH | M | LOW | Validator CI gate + clippy lint + Mimir tier limit |
| R-002 | trace_id leaked em label slot | L | H | HIGH | L | LOW | Type system (enum) + lint + property test |
| R-003 | tenant_id leaked em label (privacy) | L | M | CRITICAL | L | LOW | Type system + privacy review + LGPD/GDPR canary |
| R-004 | AE outage observability gap | M | L | MEDIUM | L | LOW | Fail-open + SEV-3 alert; recovery on resume |
| R-005 | Mimir tier limit reject ingest | L | M | MEDIUM | L | LOW | Validator catches earlier; degraded observability documented |
| R-006 | Cost regression > 10% | M | L | MEDIUM | L | LOW | §14.s09.7 gate; ADR required |
| R-007 | NEW region cardinality breach | L | M | MEDIUM | L | LOW | Validator recompute on enum extension |
| R-008 | Logpush latency > 30s | M | L | LOW | L | LOW | Documented em SLO; canary independente |
| R-009 | Exemplar volume spike (cost) | M | L | LOW | L | LOW | Sample 10% exemplars; bounded storage |
| R-010 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-011 | Validator false-positive (overestimate) | M | L | LOW | L | LOW | Conservative cartesian; ADR override path |
| R-012 | INV-OBS-CARDINALITY-BUDGET violation undetected | L | M | HIGH | L | LOW | CI primary + Mimir secondary defense; chaos test 30d |

## 29. Review Checkpoints

D+0 design (Architect; cardinality discipline); D+1 SRE (Mimir integration); D+2 AppSec (TenantCtx + privacy); D+3 Privacy (LINDDUN + LGPD/GDPR); D+4 code review; D+5 chaos validation; D+6 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap; Lote 10.8bis P1-2)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation; **mandatory emphatic** — Mimir + cardinality budget enforcement_ |
| 4 | Security Lead | _TBD; **mandatory** — STRIDE + AE binding ACL_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos + property test 100k_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory** — LGPD/GDPR aggregation discipline (no tenant_id em labels)_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + tenant_tier aggregation_ |
| 11 | Architect | _TBD; **mandatory** — cardinality discipline + 9 RED canonical + Exemplar pattern; consolidates Crypto SME advisory race-correctness review per ADR-0034 path_ |
| 12 | AppSec | _TBD; **mandatory** — type system discipline + forbidden label lint_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.9) | Criação WI-S09-001; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris lessons absorbed: 5-tier canonical Tier (P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (P1-3); audit fail-closed para cardinality violations (Lote 10.6bis adapted; metrics emit fail-OPEN distinct case); D1 N/A (Mimir backend); CHECK inline N/A (enum-typed); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2 framework §33.5.4.3); INV §3.X → §3.13 (Lote 10.8bis P1-13 lesson). NEW INV-OBS-CARDINALITY-BUDGET (HIGH; registry §3.13). NEW Python validator scripts/cardinality_check.py CI gate. trace_id em Exemplar field NOT label (Lote 10.8bis cardinality discipline absorbed). |

## 32. Anti-patterns evitados

- ❌ Dynamic métrica registration (string-typed); ❌ trace_id em label slot (cardinality explosion); ❌ tenant_id em label (privacy + cardinality); ❌ String-typed user-input labels (cardinality attack); ❌ Fail-closed em emit (reverse-priority outage); ❌ Synchronous emit blocking hot path; ❌ tokio::spawn em CF Workers; ❌ Skip cardinality validator CI gate; ❌ Skip Mimir tier limit secondary defense.

---

**Fim WI-S09-001.** Próximo: WI-S09-002 (Logpush + R2 + Loki + log schema + PII redaction lib).
