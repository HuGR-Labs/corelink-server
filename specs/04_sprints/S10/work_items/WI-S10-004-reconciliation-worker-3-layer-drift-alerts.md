---
id: "WI-S10-004"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.4.0"
created: "2026-04-26"
updated: "2026-05-03"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-009"]
parent: "S-10"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DATA-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
tags: ["wi", "s10", "reconciliation", "billing", "3-layer", "drift-detection", "fail-closed", "high-risk"]
---

# WI-S10-004 — Reconciliation Worker Daily 3-Layer + Drift Alerts SEV-1/2 + Reconciliation Reports R2 7y (`crates/corelink-billing-reconcile/`; daily cron 02:00 UTC; **3-layer enforcement**: Layer 1 [Σ(R2 events) ↔ Σ(D1 usage_counter) per tenant/sku/hour drift > 0.1% = SEV-2] + Layer 2 [Σ(usage_counter) ↔ Σ(invoice_line_item) per tenant/billing_period drift > 0.1% = SEV-2] + Layer 3 [Σ(invoice_line_item) ↔ Σ(Stripe invoice fetched via WI-S10-003 fetch_invoice) drift > 0.1% = SEV-1]; INV-BILLING-RECONCILE-3-LAYER NEW invariant introduced sprint contract §8; reconciliation report R2 `reconciliation-reports/<region>/YYYY-MM-DD.json` Object Lock 7y per-tenant breakdown; **invoice freeze on drift** > 0.1% até resolution sprint contract §14.s10.1; hash chain re-verification cooperation com WI-S10-002; Stripe API pull via WI-S10-003 fetch_invoice cooperation; statistical drift bounds via Lote 10.8bis P0-E calibration n=50+50 + 95% CI; SEV-1 escalation Layer 3 com PagerDuty corelink-finance + corelink-sre 3 services canonical inheritance from WI-S09-006; FAIL-CLOSED canonical Lote 10.6bis split-tier — reconciliation integrity > availability; Layer 3 Stripe API outage triggers RB-FM-151 cooperation + RB-FM-302 [billing drift] dry-run prerequisite sprint contract §6 DoD; corelink_time::next_month_first_utc_midnight() boundary canonical Lote 10.8bis P0-D)

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-10](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S10-004 |
| Título | Daily reconciliation worker 3-layer (sprint contract §5.4 R-S10-8/9 + §8 NEW INV-BILLING-RECONCILE-3-LAYER); cron 02:00 UTC daily per region; **Layer 1**: Σ(R2 events) ↔ Σ(D1 usage_counter) per (tenant, region, sku, hour) — drift > 0.1% = SEV-2; **Layer 2**: Σ(usage_counter) ↔ Σ(invoice_line_item) per (tenant, billing_period) — drift > 0.1% = SEV-2; **Layer 3**: Σ(invoice_line_item) ↔ Σ(Stripe invoice fetched) — drift > 0.1% = SEV-1; reconciliation report R2 `reconciliation-reports/<region>/YYYY-MM-DD.json` Object Lock Governance Mode 7y retention (SOC 2 CC1.4 + GAAP ASC 606 evidence trail); **invoice freeze automatic** on Layer 1/2/3 drift > 0.1% — close-of-month bloqueado até resolution (sprint contract §8 INV + §14.s10.1 zero tolerance); hash chain re-verification cooperation WI-S10-002 (chain head re-derived from R2 raw events; mismatch = SEV-1 tampering signal); Stripe API pull via WI-S10-003 fetch_invoice with PAT-BACKOFF-001 retry + PAT-QUEUE-EVENTS-001 fallback (FM-151 cooperation); statistical drift threshold 0.1% canonical (sprint contract §14.s10.1 zero tolerance); calibration n=50+50 + 95% CI bounded (Lote 10.8bis P0-E inheritance); per-tenant drift breakdown em report (Finance triage + RB-FM-302 runbook); SEV-1 escalation PagerDuty `corelink-finance` + `corelink-sre` services (3 canonical inheritance from WI-S09-006); corelink_time::next_month_first_utc_midnight() boundary canonical (Lote 10.8bis P0-D); FAIL-CLOSED at reconciliation canonical Lote 10.6bis split-tier; 12 sign-offs HIGH_RISK (Finance + Legal + Privacy + Architect emphatic + Compliance Officer mandatory); RB-FM-302 (billing drift) + RB-FM-151 (Stripe outage) dry-run prerequisite sprint contract §6 DoD; typed `ReconciliationReport` payload (NOT serde_json::Value — Lote 10.9-quinquies NEW-P0-2) |
| Sprint | S-10 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-BILLING-001 financial integrity 3-layer enforcement; bypass = silent drift = audit failure), FF-HR-009 (drift > 0.1% = customer dispute or revenue leak; legal exposure) |

## 1. Intent

Reconciliation worker é **the financial integrity verification layer** que enforces NEW invariant `INV-BILLING-RECONCILE-3-LAYER` (sprint contract §8). Sem reconciliation 3-layer daily, drift entre events R2, counter D1, e Stripe invoice é silently undetected — single-layer reconciliation pode mascarar bug entre layers. 3 níveis de defesa: Layer 1 detects emit/aggregator bugs (events ↔ counter); Layer 2 detects invoice generation bugs (counter ↔ invoice line items); Layer 3 detects Stripe API discrepancy (invoice ↔ Stripe). SOC 2 CC1.4 + GAAP ASC 606 require automated reconciliation + auditor-grade evidence trail.

```rust
// File: crates/corelink-billing-reconcile/src/worker.rs

#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};

#[async_trait]
pub trait ReconciliationWorker: Send + Sync {
    /// Daily reconciliation cron entry: 3-layer execution.
    /// FAIL-CLOSED: any layer error halts subsequent layers; SEV-1 alert.
    async fn run_daily_reconciliation(
        &self,
        region: Region,                                 // 5 canonical regions inheritance
        target_date: DateTime<Utc>,                     // UTC date-aligned (yesterday canonical)
    ) -> Result<DailyReconciliationReport, ReconcileError>;

    /// Layer 1: R2 events ↔ D1 usage_counter per (tenant, region, sku, hour).
    async fn reconcile_layer_1(
        &self,
        region: Region,
        target_date: DateTime<Utc>,
    ) -> Result<Layer1Report, ReconcileError>;

    /// Layer 2: D1 usage_counter ↔ Neon invoice_line_item per (tenant, billing_period).
    async fn reconcile_layer_2(
        &self,
        region: Region,
        billing_period: BillingPeriod,                  // YYYY-MM canonical
    ) -> Result<Layer2Report, ReconcileError>;

    /// Layer 3: Neon invoice_line_item ↔ Stripe invoice (fetched via WI-S10-003).
    async fn reconcile_layer_3(
        &self,
        region: Region,
        billing_period: BillingPeriod,
    ) -> Result<Layer3Report, ReconcileError>;

    /// Hash chain re-verification (cooperation WI-S10-002): re-derive chain head from R2 raw events.
    async fn verify_hash_chain_integrity(
        &self,
        region: Region,
        target_date: DateTime<Utc>,
    ) -> Result<HashChainVerification, ReconcileError>;

    /// Invoice freeze trigger: drift > 0.1% on any layer halts close-of-month.
    async fn freeze_invoice_generation(
        &self,
        region: Region,
        billing_period: BillingPeriod,
        reason: FreezeReason,                           // typed enum NOT String
    ) -> Result<FreezeAck, ReconcileError>;
}

#[derive(serde::Serialize)]
pub struct DailyReconciliationReport {
    pub region: Region,
    pub target_date: DateTime<Utc>,
    pub layer_1: Layer1Report,
    pub layer_2: Option<Layer2Report>,                  // None if not month-end
    pub layer_3: Option<Layer3Report>,                  // None if not month-end
    pub hash_chain_verification: HashChainVerification,
    pub overall_status: OverallStatus,                  // green | drift_detected | halted
    pub generated_at: DateTime<Utc>,
}

#[derive(serde::Serialize)]
pub struct Layer1Report {
    pub region: Region,
    pub target_date: DateTime<Utc>,
    pub r2_events_total_per_sku: BTreeMap<Sku, EventAggregate>,
    pub counter_d1_total_per_sku: BTreeMap<Sku, CounterAggregate>,
    pub drift_per_sku: BTreeMap<Sku, DriftPercent>,
    pub max_drift_pct: f64,                             // canonical metric
    pub drift_threshold_breached: bool,                 // > 0.1%
    pub per_tenant_breakdown: Vec<TenantDrift>,         // Finance triage
    pub late_event_total: u64,                          // routed to usage_counter_late
}

#[derive(serde::Serialize)]
pub struct Layer2Report {
    pub region: Region,
    pub billing_period: BillingPeriod,
    pub counter_total_per_sku: BTreeMap<Sku, CounterAggregate>,
    pub invoice_line_item_total_per_sku: BTreeMap<Sku, LineItemAggregate>,
    pub drift_per_sku: BTreeMap<Sku, DriftPercent>,
    pub max_drift_pct: f64,
    pub drift_threshold_breached: bool,
    pub per_tenant_breakdown: Vec<TenantDrift>,
}

#[derive(serde::Serialize)]
pub struct Layer3Report {
    pub region: Region,
    pub billing_period: BillingPeriod,
    pub invoice_line_item_total_per_sku: BTreeMap<Sku, LineItemAggregate>,
    pub stripe_invoice_total_per_sku: BTreeMap<Sku, StripeInvoiceAggregate>,
    pub drift_per_sku: BTreeMap<Sku, DriftPercent>,
    pub max_drift_pct: f64,
    pub drift_threshold_breached: bool,
    pub per_tenant_breakdown: Vec<TenantDrift>,
    pub stripe_api_latency_p99_ms: u32,                 // FM-151 monitoring
}

#[derive(serde::Serialize)]
pub struct HashChainVerification {
    pub region: Region,
    pub target_date: DateTime<Utc>,
    pub stored_chain_head: String,                      // BLAKE3-256 hex from hash_chain_head table
    pub recomputed_chain_head: String,                  // re-derived from R2 raw events
    pub chain_intact: bool,                             // stored == recomputed
    pub events_processed: u64,
    pub anomalies_detected: Vec<HashChainAnomaly>,
}

#[derive(serde::Serialize)]
pub enum OverallStatus {
    Green,                                              // all layers under threshold
    DriftDetected { sev: Severity, layer: u8, max_drift_pct: f64 },
    Halted { reason: HaltReason },                      // fail-CLOSED triggered
}

#[derive(serde::Serialize)]
pub enum FreezeReason {
    Layer1DriftExceeded { drift_pct: f64 },
    Layer2DriftExceeded { drift_pct: f64 },
    Layer3DriftExceeded { drift_pct: f64 },             // SEV-1
    HashChainViolation,                                 // CRITICAL
}

#[derive(thiserror::Error, Debug)]
pub enum ReconcileError {
    #[error("Layer 1 query failed (R2/D1; fail-CLOSED; SEV-1): {0}")]
    Layer1Failed(String),

    #[error("Layer 2 query failed (D1/Neon; fail-CLOSED; SEV-1): {0}")]
    Layer2Failed(String),

    #[error("Layer 3 Stripe API failed (FM-151 cooperation; queue fallback): {0}")]
    Layer3StripeFailed(String),

    #[error("Hash chain integrity violation (CRITICAL; tampering signal): {0}")]
    HashChainViolation(String),

    #[error("Drift threshold exceeded {layer}: drift={drift_pct}%; threshold=0.1%")]
    DriftThresholdExceeded { layer: u8, drift_pct: f64 },

    #[error("Invoice freeze triggered: {reason}")]
    InvoiceFreezeTriggered { reason: String },

    #[error("R2 reconciliation report write failed: {0}")]
    ReportWriteFailed(String),

    #[error("Statistical bound violation (calibration n=50+50 95% CI breach): {0}")]
    CalibrationBoundBreach(String),
}
```

**Cripto-driven invariants enforced**:

1. **INV-BILLING-RECONCILE-3-LAYER** (HIGH; sprint contract §8 NEW; registry §3.12 line 166 — verify via grep before commit per Lote 10.8bis P1-13):
   - Daily 3-layer reconciliation runs em production canonical 30 dias clean (sprint contract §6 DoD).
   - Layer 1 ↔ Layer 2 ↔ Layer 3 drift bounds enforced.
   - Drift > 0.1% any layer = invoice generation freeze (close-of-month blocked).
   - 30d sustained zero drift required for sprint promotion (sprint contract §14).

2. **INV-BILLING-NO-LOSS** (HIGH; registry §3.9 line 136):
   - Layer 1 enforces: Σ(R2 events) = Σ(usage_counter qty) + Σ(usage_counter_late qty) per (tenant, region, sku, hour).
   - Cooperation WI-S10-002 (counter aggregator producer); Layer 1 consumer.
   - Drift > 0.1% triggers SEV-2 + investigation.

3. **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137):
   - Reconciliation idempotent: re-running for same (region, target_date) produces same drift values.
   - Reports R2 Object Lock 7y immutable (Stripe API state may evolve; reports snapshot point-in-time).

4. **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance):
   - Reports R2 Object Lock Governance Mode 7y.
   - Hash chain re-verification cooperation WI-S10-002 detects tampering.

5. **CTRL-BILLING-001** (security_model.md): financial integrity 3-layer enforcement.

6. **Lote 10.9-quinquies NEW-P0-2 lesson absorbed**: typed reports (ReconciliationReport, Layer1/2/3Report, HashChainVerification) NOT serde_json::Value; per-tenant breakdown contains tenant_id (UUID pseudonym; no raw PII); JSON output em R2 Object Lock 7y archive uses typed serialization.

7. **Fail-CLOSED at reconciliation** (Lote 10.6bis split-tier canonical inheritance):
   - Reconciliation integrity > availability.
   - Any layer failure halts subsequent layers.
   - Drift > 0.1% triggers invoice freeze (close-of-month blocked).
   - SEV-1 alert if Layer 3 drift OR hash chain violation OR Layer 1/2 query failure.

8. **5 canonical regions** (sprint contract §17 inheritance; matches WI-S10-002 DO routing canonical Lote 10.7bis P0-9): iad, fra, nrt, syd, gru.

9. **5 SKUs canonical** (sprint contract §10 anti-scope; inheritance WI-S10-001/002): cas_storage_gb_month, cas_egress_gb, cas_put_op_count, cas_get_op_count, ac_lookup_op_count.

10. **corelink_time::next_month_first_utc_midnight() canonical primitive** (Lote 10.8bis P0-D):
    - billing_period boundary canonical.
    - GAAP cutoff time alignment with WI-S10-003.
    - target_date canonical = previous UTC date midnight.

11. **Statistical drift bounds via calibration n=50+50 + 95% CI** (Lote 10.8bis P0-E inheritance):
    - Drift threshold 0.1% canonical (sprint contract §14.s10.1).
    - Calibration: 50 sample dates + 50 sample tenants → empirical drift distribution → 95% CI → upper bound = drift threshold.
    - Statistical bound breach (drift > 95% CI even if < 0.1%) → SEV-3 monitoring signal.

12. **PagerDuty 3 canonical services** (sprint contract §17 inheritance from WI-S09-006):
    - `corelink-sre` (Layer 1/2 SEV-2; Layer 3 SEV-1).
    - `corelink-finance` (Layer 1/2/3 drift > 0.1% triage).
    - `corelink-security` (hash chain violation SEV-1; tampering signal).

13. **TenantCtx propagation** (Lote 10.4bis): tenant_id em reports authenticated upstream WI-S10-001/002/003; reconciliation aggregates per (region, tenant) bounded.

14. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future`; D1/Neon HTTP via wasm-bindgen; NEVER `tokio::spawn`.

15. **5-tier canonical Plan reference** (per `data_model.md §1` line 68; Lote 10.7bis P0-7 inheritance): per-tier reconciliation breakdown available em report (free/solo/team/business/enterprise).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + 3-layer reconciliation discipline justification)

Reconciliation worker é **the audit-grade integrity primitive** que torna CoreLink billing pipeline financial-grade SOC 2 + GAAP compliant. Stripe Billing Architecture Guide (sprint contract §17): "automated reconciliation between source-of-truth events e billed amount é mandatory financial-grade requirement; manual quarterly reconciliation pode mascarar drift acumulado meses." CoreLink S-10 implementa **3 níveis de defesa** que excedem competitor practice (Stripe Billing manual, Mux daily 1-layer, Lago daily 1-layer):

- **Layer 1** (R2 events ↔ D1 counter): detecta emit bugs (WI-S10-001) ou aggregator bugs (WI-S10-002). Drift > 0.1% = SEV-2; investigation 24h.
- **Layer 2** (counter ↔ invoice line items): detecta invoice generation bugs (WI-S10-003 monthly cron). Drift > 0.1% = SEV-2.
- **Layer 3** (invoice line items ↔ Stripe invoice): detecta Stripe API discrepancy (rare but must catch). Drift > 0.1% = SEV-1 imediato — Finance + SRE + Security paged.

**Why 3 layers (not 1 or 2)**: single-layer reconciliation can mask bug between layers. Example: emit bug under-counts events → counter low → invoice low → Stripe charges low. Σ(R2) vs Σ(Stripe) shows match (both wrong by same factor). Layer 1 catches: Σ(R2) ≠ Σ(counter); Layer 2 catches: Σ(counter) ≠ Σ(invoice); Layer 3 catches: Σ(invoice) ≠ Σ(Stripe). 3-layer is **mathematically necessary** for full coverage.

**Why fail-CLOSED at reconciliation** (Lote 10.6bis split-tier canonical): reconciliation integrity > availability. Partial reconciliation result = false confidence. If Layer 1 query fails, halt; do NOT proceed to Layer 2/3 with stale data. Manual replay after root-cause fix; SEV-1 alert. Distinct from WI-S10-001 hot path fail-OPEN: reconciliation é background cron — NO customer SLA at risk.

**Why drift threshold 0.1%** (sprint contract §14.s10.1 zero tolerance): empirical calibration n=50+50 + 95% CI (Lote 10.8bis P0-E inheritance). 50 sample dates × 50 tenants observed drift distribution → 95% CI upper bound ≈ 0.05%. 0.1% canonical = 2× upper bound for safety margin; absolute zero (0%) impractical due to floating-point + clock skew + staging-to-R2 lag race conditions.

**Why hash chain re-verification cooperation WI-S10-002**: tampering detection. Admin INSERT directly to usage_counter bypassing aggregator → chain head stored ≠ chain head re-derived from R2 raw events → HashChainViolation → SEV-1 imediato; CTRL-BILLING-001 audit; CRITICAL post-mortem.

**Why invoice freeze on drift**: GAAP ASC 606 forbids invoice issuance when revenue recognition uncertain. Drift > 0.1% Layer 3 = customer-facing invoice may be wrong → freeze close-of-month → Finance triage → resolve drift → release freeze. Customer protection > revenue velocity.

**Why R2 reports Object Lock 7y**: SOC 2 CC1.4 audit-grade evidence trail. Auditor years later reconstructs reconciliation history → all reports retained → audit-defensible.

**Adversarial scenarios**:
- **Layer 1 drift 0.5% detected**: SEV-2; investigation reveals emit bug em WI-S10-001 fixed in commit X; backfill replay; re-reconcile → green.
- **Layer 3 Stripe API outage**: PAT-BACKOFF-001 retry; PAT-QUEUE-EVENTS-001 fallback; queue retains pending Layer 3; on Stripe recovery, retry; INV-BILLING-NO-LOSS preserved.
- **Hash chain violation detected**: SEV-1; admin INSERT signal; CRITICAL post-mortem; CTRL-BILLING-001 audit; D1 audit log review.
- **Drift acumulado days mascarado**: 3-layer daily catches ≤ 24h; vs manual quarterly competitor (90 days exposure).
- **Statistical bound breach** (drift < 0.1% but outside 95% CI): SEV-3 monitor; signal early degradation; 3 days before threshold breach detected.
- **Reconciliation worker outage 2d**: catch-up replay on resume; INV-BILLING-RECONCILE-3-LAYER not violated (eventual consistency 30d clean).

**Risk justification HIGH_RISK**:
- **FF-HR-005**: 3-layer reconciliation = financial-grade integrity; bypass = silent drift = audit failure.
- **FF-HR-009**: drift > 0.1% = customer dispute or revenue leak; legal exposure.
- 12 sign-offs (Finance + Compliance Officer + Privacy emphatic) + chaos suite + RB-FM-302 dry-run + RB-FM-151 cooperation + 30d clean prerequisite.

## 3. Customer Impact & Journey

**Persona 1 — Finance auditor (SOC 2)**: queries R2 `reconciliation-reports/<region>/2026-09-15.json` archive 7y; verifies daily 3-layer green for entire SOC 2 audit period; auditor reconstructs invoice trail for ANY customer dispute em < 30min (sprint contract §6 DoD).

**Persona 2 — Customer disputing invoice $X**: customer files dispute (Stripe webhook charge.dispute.created); Finance triggers WI-S10-006 replay forensic; reconciliation report shows green Layer 1/2/3 for that period → dispute resolved (CoreLink data correct); OR drift detected → escalate to refund + post-mortem.

**Persona 3 — SRE responding to Layer 3 SEV-1**: PagerDuty fires for drift > 0.1% Layer 3 (invoice ↔ Stripe); investigates; root-causes Stripe webhook missed dispatch; manually re-fetches Stripe invoice; updates D1 invoice_line_item; re-reconcile → green.

**Persona 4 — Compliance Officer reviewing 30d clean**: queries `corelink_billing_reconcile_drift_pct{layer, region}` over 30d; verifies sustained < 0.1%; signs-off SOC 2 control evidence.

**Persona 5 — Customer Finance team querying their own usage**: opens admin UI (S-13 inheritance); per-tenant breakdown from `reconciliation_report.per_tenant_breakdown` field → transparent visibility; trust signal.

**Persona 6 — Adversary attempting hash chain tamper**: admin DB access INSERT directly to usage_counter; reconciliation hash chain verification detects mismatch; CRITICAL alert; investigation; CTRL-BILLING-001 audit chain integrity preserved.

**SLA addendum**:
- Reconciliation latency: ≤ 30min p99 daily (sprint contract §15 R-004 mitigation).
- INV-BILLING-RECONCILE-3-LAYER: 30d production sustained sin drift > 0.1% (sprint contract §6 DoD prerequisite for sprint promotion).
- Drift threshold: 0.1% canonical (zero tolerance §14.s10.1).
- SEV-1 escalation: Layer 3 drift OR hash chain violation OR Layer 1/2 query fail.
- SEV-2 escalation: Layer 1 OR Layer 2 drift > 0.1%.
- Reports R2 retention: 7y Object Lock (SOC 2 CC1.4 + GAAP).

## 4. Capability Mapping

- **CAP-BILLING-004** (Daily reconciliation worker) — IMPLEMENTA primary.
- Trace: `data_model.md` (reconciliation_report schema; usage_counter + invoice_line_item) + `security_model.md CTRL-BILLING-001` + `invariant_registry.md INV-BILLING-RECONCILE-3-LAYER + INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP + INV-AUDIT-APPEND-ONLY` + sprint contract §5.4 (R-S10-8/9) + §8 NEW INV + Stripe Billing Architecture Guide + GAAP ASC 606.

## 5. Tipo

Reconciliation worker Rust crate + Cloudflare Worker cron-trigger 02:00 UTC daily + R2 Object Lock IaC for reports + Neon Postgres queries + Stripe API pull cooperation + RB-FM-302 + RB-FM-151 dry-run; HIGH_RISK; FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-billing-reconcile/` module** — `ReconciliationWorker` trait + impls + tests.

2. **Daily cron 02:00 UTC** (sprint contract §5.4 R-S10-8):
   - 5 regions canonical: iad, fra, nrt, syd, gru.
   - Cron expression: `0 2 * * *` per region (CF Workers cron trigger).
   - target_date = previous UTC midnight (yesterday).
   - Layer 1 daily; Layer 2/3 monthly (1st of month).

3. **Layer 1 reconciliation logic** (sprint contract §5.4 R-S10-8.1):
   ```rust
   async fn reconcile_layer_1(&self, region: Region, target_date: DateTime<Utc>) -> Result<Layer1Report, ReconcileError> {
       // Step 1: read R2 events for target_date hour buckets (24 buckets per region)
       let r2_events_total: BTreeMap<Sku, EventAggregate> = self.r2_aggregate_by_sku(region, target_date).await?;

       // Step 2: read D1 usage_counter for target_date hours
       let counter_d1_total: BTreeMap<Sku, CounterAggregate> = self.d1_counter_aggregate_by_sku(region, target_date).await?;

       // Step 3: include usage_counter_late aggregates (events that arrived late but for target_date originally)
       let counter_late_total: BTreeMap<Sku, CounterAggregate> = self.d1_counter_late_aggregate_by_sku(region, target_date).await?;

       // Step 4: compute drift per SKU
       let mut drift_per_sku = BTreeMap::new();
       for sku in Sku::iter() {
           let events_total = r2_events_total.get(&sku).copied().unwrap_or_default();
           let counter_total_combined = counter_d1_total.get(&sku).copied().unwrap_or_default()
               + counter_late_total.get(&sku).copied().unwrap_or_default();
           let drift_pct = compute_drift_pct(events_total.qty, counter_total_combined.qty);
           drift_per_sku.insert(sku, DriftPercent(drift_pct));
       }

       // Step 5: per-tenant breakdown (Finance triage)
       let per_tenant_breakdown = self.tenant_drift_breakdown_layer_1(region, target_date).await?;

       // Step 6: report assembly
       let max_drift_pct = drift_per_sku.values().map(|d| d.0).fold(0.0_f64, f64::max);
       let drift_threshold_breached = max_drift_pct > 0.001;  // 0.1%

       Ok(Layer1Report {
           region,
           target_date,
           r2_events_total_per_sku: r2_events_total,
           counter_d1_total_per_sku: counter_d1_total,
           drift_per_sku,
           max_drift_pct,
           drift_threshold_breached,
           per_tenant_breakdown,
           late_event_total: counter_late_total.values().map(|a| a.qty).sum(),
       })
   }
   ```

4. **Layer 2 reconciliation logic** (sprint contract §5.4 R-S10-8.2):
   - Reads D1 `usage_counter` for billing_period (entire month) — counter PK é `(tenant_id, region, sku, hour)` 4-tuple.
   - Reads Neon `invoice_line_item` for billing_period (per-tenant, NOT region-tagged).
   - **Aggregation**: SUM(qty) GROUP BY (tenant, sku) collapsing across regions BEFORE comparison (Stripe invoice é per-tenant — sem region dimension); per-region drift signal preservada via Layer 1 (region-scoped) que precede Layer 2 (R4 NEW-P1-6 fix).
   - Compares per (tenant, sku, billing_period) → drift > 0.1% = SEV-2.
   - Runs monthly on 1st (after WI-S10-003 monthly cron completes).

5. **Layer 3 reconciliation logic** (sprint contract §5.4 R-S10-8.3):
   - Reads Neon `invoice_line_item` for billing_period (per-tenant).
   - Pulls Stripe invoice via WI-S10-003 fetch_invoice (cooperation; Stripe Customer-scoped, sem region dimension).
   - Compares per (tenant, sku, billing_period) → drift; aggregation matches Layer 2 (per-tenant) por construção.
   - SEV-1 escalation if drift > 0.1%.
   - PAT-BACKOFF-001 retry on Stripe API failure; PAT-QUEUE-EVENTS-001 fallback.

6. **Hash chain re-verification** (cooperation WI-S10-002):
   - Re-derives chain head from R2 raw events for target_date.
   - Compares to stored hash_chain_head em D1.
   - Mismatch = HashChainViolation = SEV-1 + CRITICAL post-mortem.

7. **Reconciliation report R2 Object Lock 7y** (R4 P0-6 fix: schema CF provider verificado pre-merge — ver WI-S10-001 §6.1.3 nota; nested config dentro de `cloudflare_r2_bucket`):
   ```hcl
   # Validar resource type names contra `cloudflare/cloudflare` provider pinned em infra/cloudflare/versions.tf.
   resource "cloudflare_r2_bucket" "reconciliation_reports" {
       account_id = var.cloudflare_account_id
       name       = "reconciliation-reports-${var.region}"
       location   = var.region

       object_lock_configuration {
           enabled                = true
           default_retention_mode = "GOVERNANCE"
           default_retention_days = 2557           # 7 anos (365*7 + 2 leap)
       }
   }
   ```

8. **Report path canonical**: `reconciliation-reports/<region>/YYYY-MM-DD.json` per Lote 10.5bis path conventions; per-tenant breakdown nested.

9. **Invoice freeze automatic** (sprint contract §8 INV + §14.s10.1):
   ```rust
   if drift_threshold_breached {
       self.freeze_invoice_generation(region, billing_period, FreezeReason::Layer1DriftExceeded { drift_pct: max_drift_pct }).await?;
       // Sets Neon `invoice_generation_state(region, billing_period, frozen_at, reason)` row;
       // Monthly cron WI-S10-003 reads this; halts invoice generation; SEV-2 alert.
   }
   ```

10. **Statistical drift bounds calibration** (Lote 10.8bis P0-E inheritance):
    - Calibration: n=50 sample dates + n=50 sample tenants → empirical drift distribution.
    - 95% CI computed → upper bound published em runbook drift calibration doc.
    - Drift > 0.1% but < 95% CI upper = SEV-3 monitor; > 0.1% = SEV-2/1.

11. **PagerDuty 3 services canonical** (sprint contract §17 inheritance WI-S09-006):
    - `corelink-sre`: Layer 1/2 SEV-2; Layer 3 SEV-1; query failure SEV-1.
    - `corelink-finance`: drift > 0.1% any layer (triage).
    - `corelink-security`: hash chain violation SEV-1.

12. **corelink_time::next_month_first_utc_midnight() canonical primitive** (Lote 10.8bis P0-D):
    - billing_period boundary alignment com WI-S10-003.
    - GAAP cutoff time canonical.

13. **TenantCtx propagation** (Lote 10.4bis): per-tenant breakdown uses tenant_id from upstream layers; no request body re-authentication.

14. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3): `worker::send_future`; D1/Neon HTTP via wasm-bindgen; NEVER `tokio::spawn`.

15. **5 SKUs canonical** (sprint contract §10 anti-scope; CHECK constraints inheritance from invoice_line_item).

16. **5 PlanTier canonical** (Lote 10.7bis P0-7): per-tier breakdown available.

17. **RB-FM-302 + RB-FM-151 dry-run prerequisite** (sprint contract §6 DoD):
    - RB-FM-302 (billing drift): drift detection runbook executed em staging.
    - RB-FM-151 (Stripe outage cooperation Layer 3): Stripe API failure runbook executed em staging.

18. **Métricas operacionais** (via WI-S09-001 emit lib; cardinality budget; underscored canonical Lote 10.9bis P0-E):
    - `corelink_billing_reconcile_runs_total{region, layer, status}` (counter; 5×3×2 = 30 séries; status ∈ green/drift_detected/halted).
    - `corelink_billing_reconcile_duration_seconds{region, layer}` (histogram; 5×3×11 buckets + sum/count = 165).
    - `corelink_billing_reconcile_drift_pct{region, layer, sku, tenant_tier}` (gauge; **alert SEV-2 if Layer 1/2 > 0.1%**, **SEV-1 if Layer 3 > 0.1%**; 5×3×5×5 = 375 séries).
    - `corelink_billing_reconcile_max_drift_pct{region, layer}` (gauge; 5×3 = 15 séries).
    - `corelink_billing_reconcile_hash_chain_violations_total{region}` (counter; **alert SEV-1 if > 0**; 5 séries).
    - `corelink_billing_reconcile_invoice_freeze_count_total{region, layer, reason}` (counter; **alert SEV-1 if > 0**).
    - `corelink_billing_reconcile_stripe_api_failures_total{region, reason}` (counter; **alert SEV-2 if Layer 3 retry exhausted**).
    - `corelink_billing_reconcile_late_event_proportion{region}` (gauge; sprint contract §18 post-mortem trigger if > 5% mensal).
    - `corelink_billing_reconcile_clean_streak_days{region}` (gauge; 30d clean prerequisite tracking; sprint contract §6 DoD).
    - `corelink_billing_reconcile_calibration_bound_breaches_total{region, layer}` (counter; **alert SEV-3** statistical signal Lote 10.8bis P0-E).

19. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_layer_1_idempotent`: 100k random target_dates; reconcile twice → same drift values.
    - `prop_drift_threshold_consistent`: 10k drift values; assert > 0.1% triggers freeze; < 0.1% green.
    - `prop_hash_chain_verification_correct`: 10k synthetic event sets; recompute matches stored.
    - `prop_3_layer_full_coverage`: 1k synthetic bug injection (one of 3 layers wrong); assert detected at correct layer.
    - `prop_typed_payload_no_serde_json_value`: 10k report serializations; typed enum; serde_json::Value rejected at compile time.
    - `prop_calibration_bounds`: 50+50 sample → 95% CI within expected.
    - `prop_invoice_freeze_idempotent`: 1k freeze attempts same period; idempotent state.

20. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 12):
    1. **Layer 1 drift 0.5% synthetic injection**: SEV-2 alert; corelink-finance paged; invoice freeze for affected billing_period.
    2. **Layer 2 drift 0.3% synthetic**: SEV-2; freeze; counter ↔ invoice_line_item investigation.
    3. **Layer 3 drift 0.2% synthetic** (Stripe vs Neon): SEV-1; corelink-finance + corelink-sre paged; close-of-month bloqueado.
    4. **Hash chain violation synthetic** (admin INSERT bypass): SEV-1; corelink-security paged; CRITICAL post-mortem.
    5. **Stripe API outage 1h Layer 3** (FM-151 cooperation RB-FM-151): PAT-BACKOFF-001 retry; queue fallback; on recovery 0 lost Layer 3 reconciliation.
    6. **Cron worker kill mid-execution**: D1 transaction rollback; manual replay via target_date; reconciliation idempotent.
    7. **R2 Object Lock retention violation attempt** (admin DELETE report): rejected; SEV-1 alert.
    8. **Drift threshold 0.099% (just under)**: green; statistical bound breach SEV-3 monitor (Lote 10.8bis P0-E).
    9. **30d clean streak chaos** (sprint contract §6 DoD): track corelink_billing_reconcile_clean_streak_days; prerequisite for sprint promotion.
    10. **Late event > 5% mensal** (sprint contract §18 post-mortem trigger): post-mortem auto-opened; Finance review.
    11. **RB-FM-302 dry-run**: billing drift runbook executed em staging; verified.
    12. **RB-FM-151 dry-run**: Stripe outage cooperation runbook executed em staging.

### 6.2 Out-of-scope (deferred)

- Counter aggregation (delegate WI-S10-002 — Layer 1 input source).
- Stripe integration (delegate WI-S10-003 — Layer 3 input source).
- Quota state machine (delegate WI-S10-005).
- Replay forensic endpoint (delegate WI-S10-006 — separate from reconciliation).
- TLA+ billing_atomicity (delegate WI-S10-007).
- Customer-facing reconciliation dashboard (delegate S-13).
- Multi-currency reconciliation (anti-scope sprint contract §10).

## 7. Anti-Scope

- ❌ serde_json::Value em ReconciliationReport (Lote 10.9-quinquies NEW-P0-2; typed canonical).
- ❌ Single-layer reconciliation (3-layer canonical sprint contract §5.4 R-S10-8).
- ❌ Fail-OPEN at reconciliation (Layer 1/2/3 partial result = false confidence; fail-CLOSED canonical Lote 10.6bis).
- ❌ Manual quarterly reconciliation (sprint contract §16 SOTA bar; daily canonical).
- ❌ Drift threshold > 0.1% (sprint contract §14.s10.1 zero tolerance).
- ❌ Skip hash chain re-verification (CRITICAL audit signal).
- ❌ Skip RB-FM-302/151 dry-run (sprint contract §6 DoD prerequisite).
- ❌ ad-hoc `now() + Duration::days(30)` (corelink_time::next_month_first_utc_midnight canonical Lote 10.8bis P0-D).
- ❌ Statistical bound calibration skipped (Lote 10.8bis P0-E inheritance).
- ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ TenantCtx bypass.

## 8. Acceptance Criteria (Gherkin) — 12 scenarios

```gherkin
Feature: Reconciliation Worker Daily 3-Layer + Drift Alerts

  Scenario: Layer 1 daily reconciliation green
    Given target_date 2026-09-15 region=iad
    Given Σ(R2 events) per (tenant, region, sku, hour) = Σ(D1 usage_counter qty + usage_counter_late qty)
    When reconcile_layer_1() runs
    Then drift_per_sku all values ≤ 0.001 (0.1%)
    Then drift_threshold_breached = false
    Then OverallStatus::Green
    Then report written to R2 reconciliation-reports/iad/2026-09-15.json
    Then corelink_billing_reconcile_runs_total{region=iad, layer=1, status=green} increments

  Scenario: Layer 1 drift 0.5% triggers SEV-2 + invoice freeze
    Given Σ(R2 events) - Σ(D1 counter) drift 0.5% for sku=cas_put_op_count
    When reconcile_layer_1() runs
    Then drift_per_sku[cas_put_op_count] = 0.005
    Then drift_threshold_breached = true
    Then DriftThresholdExceeded { layer: 1, drift_pct: 0.5 } error
    Then SEV-2 alert: PagerDuty corelink-sre + corelink-finance
    Then freeze_invoice_generation(region=iad, billing_period=2026-09, FreezeReason::Layer1DriftExceeded)
    Then corelink_billing_reconcile_drift_pct{region=iad, layer=1, sku=cas_put_op_count} = 0.005
    Then corelink_billing_reconcile_invoice_freeze_count_total increments

  Scenario: Layer 3 drift 0.2% triggers SEV-1
    Given Σ(invoice_line_item) - Σ(Stripe fetched) drift 0.2% for billing_period=2026-09
    When reconcile_layer_3() runs
    Then drift_threshold_breached = true
    Then DriftThresholdExceeded { layer: 3, drift_pct: 0.2 } error
    Then SEV-1 alert: PagerDuty corelink-finance + corelink-sre
    Then close-of-month bloqueado for 2026-09
    Then corelink_billing_reconcile_drift_pct{region=iad, layer=3} = 0.002

  Scenario: Hash chain violation detected SEV-1
    Given admin INSERT directly to usage_counter bypassing hash chain advance
    When verify_hash_chain_integrity() runs
    Then stored_chain_head ≠ recomputed_chain_head
    Then chain_intact = false
    Then HashChainViolation error
    Then SEV-1 alert: PagerDuty corelink-security
    Then corelink_billing_reconcile_hash_chain_violations_total increments
    Then CRITICAL post-mortem auto-opened
    Then CTRL-BILLING-001 audit chain integrity investigation

  Scenario: Layer 3 Stripe API outage cooperation FM-151
    Given Stripe API returns 5xx for 1h during Layer 3 reconciliation
    When reconcile_layer_3() invokes fetch_invoice via WI-S10-003
    Then PAT-BACKOFF-001 retries 5× with exponential backoff
    Then queue fallback PAT-QUEUE-EVENTS-001 stripe-pending
    Then SEV-2 alert: Layer 3 stripe_api_failures
    Then on Stripe recovery: queue drained; reconciliation completes
    Then INV-BILLING-NO-LOSS preserved
    Then RB-FM-151 dry-run executed em staging (sprint contract §6 DoD)

  Scenario: 3-layer full coverage detection
    Given synthetic bug injected at Layer 1 (emit under-counts 0.3%)
    When daily reconciliation runs
    Then Layer 1 detects: drift = 0.3% > 0.1%; SEV-2; freeze
    Then Layer 2 may show consequent drift (counter low → invoice low → both wrong same factor)
    Then Layer 3 may show drift if Stripe charged correctly per invoice
    Then Layer 1 root-cause identified first (canonical detection layer)

  Scenario: Reconciliation report R2 7y retention
    Given report written to R2 reconciliation-reports/iad/2026-09-15.json
    When 7y elapse
    Then R2 Lifecycle Expiration deletes report
    Then SOC 2 CC1.4 + GAAP retention satisfied
    Given admin attempts DELETE within 7y
    Then R2 Object Lock Governance rejects
    Then SEV-1 alert; investigation

  Scenario: Statistical bound breach SEV-3 monitor (calibration Lote 10.8bis P0-E)
    Given drift = 0.08% (under 0.1% threshold)
    Given calibration n=50+50 95% CI upper bound = 0.05%
    When reconcile_layer_1() runs
    Then drift_threshold_breached = false (under 0.1%)
    Then statistical_bound_breached = true (drift > 95% CI upper)
    Then SEV-3 monitor: corelink_billing_reconcile_calibration_bound_breaches_total increments
    Then early degradation signal; investigation pre-emptive

  Scenario: 30d clean streak prerequisite
    Given 30 consecutive days without drift > 0.1% any layer
    When reconcile completes day 30
    Then corelink_billing_reconcile_clean_streak_days = 30
    Then sprint contract §6 DoD prerequisite met
    Then sprint promotion eligible

  Scenario: Reconciliation idempotent re-run
    Given reconciliation completed for target_date 2026-09-15
    When reconcile_layer_1() re-invoked same target_date
    Then identical drift values produced (deterministic queries)
    Then existing R2 report overwritten OR same content idempotent

  Scenario: Typed ReconciliationReport (Lote 10.9-quinquies NEW-P0-2)
    Given developer attempts use serde_json::Value em report
    When cargo build runs
    Then compilation fails: typed ReconciliationReport canonical
    Then merge bloqueado

  Scenario: Late event > 5% mensal post-mortem trigger
    Given monthly aggregate corelink_billing_reconcile_late_event_proportion{region=iad} = 6%
    When reconcile_layer_1() runs at month-end
    Then SEV-3 alert
    Then post-mortem auto-opened (sprint contract §18 hook)
    Then Finance + SRE review root-cause (system delay analysis)
```

## 9. Design Decisions

- 9.1: 3-layer reconciliation canonical (NOT 1 or 2; mathematically necessary for full coverage).
- 9.2: Daily cron 02:00 UTC per region (5 canonical regions).
- 9.3: Layer 1 daily; Layer 2/3 monthly (1st of month after WI-S10-003 invoice cron).
- 9.4: Drift threshold 0.1% canonical (sprint contract §14.s10.1 zero tolerance).
- 9.5: Statistical drift bounds calibration n=50+50 + 95% CI (Lote 10.8bis P0-E inheritance).
- 9.6: Fail-CLOSED at reconciliation (Lote 10.6bis split-tier canonical inheritance).
- 9.7: Invoice freeze automatic on drift > 0.1% (sprint contract §8 INV).
- 9.8: Hash chain re-verification cooperation WI-S10-002 (tampering detection).
- 9.9: Stripe API pull cooperation WI-S10-003 fetch_invoice (Layer 3 input).
- 9.10: PAT-BACKOFF-001 + PAT-QUEUE-EVENTS-001 (FM-151 cooperation Layer 3).
- 9.11: PagerDuty 3 services canonical (sprint contract §17 inheritance WI-S09-006).
- 9.12: Reports R2 Object Lock Governance Mode 7y (SOC 2 CC1.4 + GAAP).
- 9.13: corelink_time::next_month_first_utc_midnight() canonical (Lote 10.8bis P0-D).
- 9.14: Typed reports (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 inheritance).
- 9.15: 5 SKUs + 5 PlanTier + 5 regions canonical (cardinality bounded).
- 9.16: TenantCtx propagation (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3).
- 9.17: RB-FM-302 + RB-FM-151 dry-run prerequisite (sprint contract §6 DoD).
- 9.18: 30d clean streak prerequisite (sprint contract §6 DoD).
- 9.19: NEW INV-BILLING-RECONCILE-3-LAYER em invariant_registry — verify canonical position before commit (Lote 10.8bis P1-13).

## 10. Completeness Criteria SOTA

- [ ] **10.s10.004.1** Crate compila + integration tests green.
- [ ] **10.s10.004.2** All 12 Gherkin scenarios green.
- [ ] **10.s10.004.3** Property tests 7 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s10.004.4** Chaos suite 12 scenarios green.
- [ ] **10.s10.004.5** **INV-BILLING-RECONCILE-3-LAYER**: 30d production sustained sin drift > 0.1% (sprint contract §6 DoD prerequisite).
- [ ] **10.s10.004.6** Statistical drift bounds calibration n=50+50 95% CI documented em drift calibration doc runbook.
- [ ] **10.s10.004.7** R2 Object Lock 7y configured; admin DELETE rejected; reports retained.
- [ ] **10.s10.004.8** Invoice freeze automatic on drift > 0.1% verified em chaos test.
- [ ] **10.s10.004.9** Hash chain re-verification cooperation com WI-S10-002 verified.
- [ ] **10.s10.004.10** Stripe API pull cooperation com WI-S10-003 fetch_invoice verified.
- [ ] **10.s10.004.11** RB-FM-302 (billing drift) dry-run executed em staging (sprint contract §6 DoD).
- [ ] **10.s10.004.12** RB-FM-151 (Stripe outage cooperation) dry-run executed em staging.
- [ ] **10.s10.004.13** Métricas (10) emitted; cardinality budget respected (~600 séries baseline).
- [ ] **10.s10.004.14** Cargo-audit + cargo-deny + clippy clean.

## 11. DoD

- [ ] Crate compila + tests green; all Gherkin/property/chaos green; 30d clean streak production; RB-FM-302 + RB-FM-151 dry-run executed staging; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Finance + Compliance Officer + Legal + Privacy emphatic).

## 12. Invariants Validated

- **INV-BILLING-RECONCILE-3-LAYER** (HIGH; sprint contract §8 NEW; registry §3.12 line 166; verify via grep before commit per Lote 10.8bis P1-13): 3-layer daily 30d clean.
- **INV-BILLING-NO-LOSS** (HIGH; registry §3.9 line 136): Layer 1 enforcement; cooperation WI-S10-002.
- **INV-BILLING-NO-DUP** (HIGH; registry §3.9 line 137): reconciliation idempotent; reports R2 immutable.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+ proven Lote 6.2; S-09 inheritance): R2 reports Object Lock 7y; hash chain verification.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-region reconciliation; cross-region race eliminated.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): per-tenant breakdown; aggregate-only reconciliation.
- **CTRL-BILLING-001** (security_model.md): financial integrity 3-layer enforcement.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Reconciliation worker module | `crates/corelink-billing-reconcile/` | Rust |
| Daily cron Worker | `crates/corelink-worker/src/crons/reconcile_daily.rs` | Rust |
| Layer 1/2/3 logic | `crates/corelink-billing-reconcile/src/layers.rs` | Rust |
| Hash chain verification | `crates/corelink-billing-reconcile/src/hash_chain_verify.rs` | Rust |
| R2 reports IaC | `infra/cloudflare/r2/reconciliation_reports.tf` | Terraform |
| Calibration runbook | `runbooks/rb-billing-calibration.md` | Markdown |
| RB-FM-302 dry-run script | `runbooks/rb-fm-302-billing-drift-dry-run.sh` | Bash |
| RB-FM-151 cooperation script | `runbooks/rb-fm-151-stripe-outage-layer-3.sh` | Bash |
| Property tests | `crates/corelink-billing-reconcile/tests/prop_reconcile.rs` | Rust |
| Chaos suite | `tests/chaos_billing_reconcile.rs` | Rust |
| Schema invoice_generation_state migration | `migrations/00X_invoice_generation_state.sql` | SQL |

## 14. Quality Standards SOTA

- 14.s10.004.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s10.004.2: rustdoc 100% public API.
- 14.s10.004.3: Test coverage ≥ 90%.
- 14.s10.004.4: Reconciliation latency ≤ 30min p99 daily (sprint contract §15 R-004 mitigation).
- 14.s10.004.5: SAST clean.
- 14.s10.004.6: Métricas (10 §6.1.18).
- 14.s10.004.7: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s10.004.8: TenantCtx propagation (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical Plan ref (Lote 10.7bis P0-7); column drift no `_ms` suffix (Lote 10.7bis P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV positions §3.9 L136/137 (NO-LOSS/NO-DUP) + §3.12 L166/167 (RECONCILE-3-LAYER/REPLAYABLE) verified Lote 10.10bis (Lote 10.8bis P1-13 lesson absorbed).
- 14.s10.004.9: corelink_time::next_month_first_utc_midnight() canonical (Lote 10.8bis P0-D); HourWindow::from_aligned() rejects non-aligned (inheritance WI-S10-002).
- 14.s10.004.10: D1 batch ≤ 250 (Lote 10.5bis); CHECK constraints inline (Lote 10.5bis).
- 14.s10.004.11: Reconciliation fail-CLOSED canonical (Lote 10.6bis split-tier; vs WI-S10-001 hot path fail-OPEN).
- 14.s10.004.12: BLAKE3-256 hash chain consistency com WI-S09-004 + WI-S10-002 hash patterns.
- 14.s10.004.13: Typed ReconciliationReport (NOT serde_json::Value; Lote 10.9-quinquies NEW-P0-2 inheritance).
- 14.s10.004.14: Statistical drift bounds calibration n=50+50 + 95% CI (Lote 10.8bis P0-E inheritance).
- 14.s10.004.15: Prom metric names underscored canonical (Lote 10.9bis P0-E inheritance).
- 14.s10.004.16: 30d clean streak tracked via corelink_billing_reconcile_clean_streak_days gauge (sprint contract §6 DoD).

## 15. Chaos Experiments (12)

§6.1.20 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap; Finance + Compliance Officer + Legal + Privacy + Architect emphatic).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + ReconciliationWorker trait + typed report structs | 1.5 |
| ST-002 | Layer 1 logic (R2 events ↔ D1 usage_counter) + drift compute + per-tenant breakdown | 3 |
| ST-003 | Layer 2 logic (D1 counter ↔ Neon invoice_line_item) | 2 |
| ST-004 | Layer 3 logic (Neon invoice_line_item ↔ Stripe fetched) + WI-S10-003 cooperation | 2.5 |
| ST-005 | Hash chain re-verification + cooperation WI-S10-002 | 2 |
| ST-006 | R2 reports Object Lock 7y IaC + report path canonical | 1 |
| ST-007 | Invoice freeze logic + Neon invoice_generation_state schema | 1.5 |
| ST-008 | Statistical drift bounds calibration n=50+50 + 95% CI + drift calibration doc runbook | 1.5 |
| ST-009 | Daily cron Worker 02:00 UTC + 5 region routing | 1 |
| ST-010 | Métricas (10) emit | 1 |
| ST-011 | Property tests (7 × 10k; 100k nightly) | 2 |
| ST-012 | Chaos suite (12) | 2 |
| ST-013 | RB-FM-302 + RB-FM-151 cooperation runbook + staging dry-run | 1.5 |
| ST-014 | 30d clean streak production rollout | 0.5 |
| ST-015 | Compliance walkthrough + Finance walkthrough | 0.5 |

**Total**: ~23.5h. **PERT** O=14h M=20h P=32h: **20.7h** (matches sprint contract §12 estimate).

## 18. Dependencies

- Hard: WI-S10-001 SEALED (R2 events Layer 1 input); WI-S10-002 SEALED (D1 usage_counter Layer 1 input + hash chain head); WI-S10-003 SEALED (Stripe fetch_invoice Layer 3 input + invoice_line_item Layer 2 input); WI-S09-001 SEALED (cardinality emit lib); WI-S09-006 SEALED (PagerDuty 3 services canonical); WI-S09-004 SEALED (R2 Object Lock IaC reference + hash chain pattern).
- Soft: WI-S10-005 (quota state machine para overage signaling); WI-S10-007 (TLA+ billing_atomicity).
- Hard infra: Cloudflare R2 Object Lock per region; D1 + Neon Postgres available; Stripe API access (cooperation); Cloudflare Queue for FM-151 fallback.

## 19. Effort PERT: ~20.7h. ## 20. Time-boxing: 32h hard limit.

## 21. Observability

10 metrics §6.1.18. Trace span `billing.reconcile.{layer_1, layer_2, layer_3, hash_chain_verify, freeze_invoice, write_report}`.

## 22. Cost Analysis

- R2 Object Lock storage reports: 7y × ~2 KB/report × 365 reports/yr × 5 regions = 25.6 MB/region/yr cumulative; storage cost negligible.
- R2 PUT operations: 365×5 = 1825/yr; $4.50/1M × 1825 = ~$0.01/yr (negligible).
- D1 reads (Layer 1 queries): ~50M reads/mo × 5 regions × 12mo = 3B; $0.001/1k = ~$3000/yr.
- Neon Postgres queries (Layer 2/3): ~10M queries/mo × 12mo = 120M; ~$50/yr (Neon free tier likely sufficient).
- Stripe API fetch_invoice (Layer 3): ~5M fetches/mo × 12mo = 60M; Stripe rate-limit budget; minimal cost ($0).
- Worker cron invocations: 365 × 5 regions × 30min avg = ~$50/yr.
- TCO 12m: ~$3100/yr reconciliation infrastructure.
- **Cost saved by INV-BILLING-RECONCILE-3-LAYER**: prevents silent drift; competitor manual quarterly = 90 days exposure × millions revenue at risk; daily 3-layer = ≤ 24h exposure.

## 23. API Contract

- Public Rust: `ReconciliationWorker` trait + `DailyReconciliationReport`, `Layer1Report`, `Layer2Report`, `Layer3Report`, `HashChainVerification`, `OverallStatus`, `FreezeReason`, `ReconcileError` types; `#[non_exhaustive]`.
- Wire: R2 reports JSON canonical (typed serialization).
- Storage: R2 Object Lock 7y reports per region; Neon `invoice_generation_state` for freeze tracking.
- Cron-trigger: `0 2 * * *` per region.

## 24. Post-mortem Hooks

- INV-BILLING-NO-LOSS Layer 1 violation detected (Σ(events) ≠ Σ(counter)) → HIGH-severity post-mortem (HIGH severity per registry §3.9 L136; SEV-2 + Finance review canonical mapping; > 1% drift escalates to SEV-1 per §14.s10.1).
- INV-BILLING-NO-DUP Layer 1 violation (duplicate counter row enforcement gap) → HIGH-severity post-mortem (registry §3.9 L137; SEV-2 alert).
- INV-BILLING-RECONCILE-3-LAYER violation (drift > 0.1% any layer) → 5-Why post-mortem (sprint contract §18; HIGH severity per registry §3.12 L166).
- Hash chain violation → CRITICAL post-mortem (CTRL-BILLING-001 audit; INV-AUDIT-APPEND-ONLY canonical CRITICAL §3.6 L116).
- Layer 3 drift > 0.1% → CRITICAL post-mortem (Stripe API discrepancy = legal exposure; SEV-1 escalation gate per §14.s10.1 zero tolerance).
- Late event > 5% mensal → post-mortem (sprint contract §18 hook).
- Statistical bound breach SEV-3 sustained 7d → post-mortem (early degradation analysis).
- 30d clean streak interrupted → post-mortem + sprint promotion delayed.

## 25. Rollback / Recovery

- Rollback: revert reconciliation worker + cron; daily reconciliation halts; manual reconciliation possible via WI-S10-006 replay endpoint cooperation; R2 reports retained.
- Recovery: worker re-deployed; catch-up replay for missed dates; INV-BILLING-RECONCILE-3-LAYER re-verified after catch-up.
- RTO ≤ 1h; RPO ≤ 1d (daily granularity; events retained upstream).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): Reports authenticated via R2 access; Stripe API key per env (test|live).
- T(ampering): R2 Object Lock + hash chain re-verification detects post-hoc modification.
- R(epudiation): Reports immutable evidence trail 7y.
- I(nformation disclosure): Per-tenant breakdown in reports → tenant_id pseudonym (UUID); no raw PII em reports.
- D(enial of Service): Fail-CLOSED at reconciliation halts on failure; recovery via catch-up replay.
- E(scalation of Privilege): R2 reports Object Lock prevents admin DELETE.

**LINDDUN** (LGPD/GDPR):
- L(inkability): Reports linkable per tenant_id (financial-grade audit).
- I(dentifiability): Tenant_id pseudonym; no raw PII em reports.
- N(on-repudiation): R2 Object Lock 7y immutable.
- D(etectability): Customer access via WI-S10-006 replay endpoint role-protected.
- D(isclosure): 7y retention vs erasure right (LGPD Art. 16); pseudonymization preserves chain integrity (CTRL-PRIV-002 + S-11 cooperation).
- U(nawareness): Customer notified via S-13 admin plane.
- N(on-compliance): **CTRL-BILLING-001 + SOC 2 CC1.4 + GAAP ASC 606 + LGPD Art. 32 + GDPR Art. 32** via 3-layer reconciliation + 7y retention + hash chain audit chain integrity.

## 27. Knowledge Transfer

Tech talk (1.5h): "S-10 Reconciliation: 3-Layer Daily + Hash Chain + Calibration Bounds"; doc `docs/dev/billing-reconcile-architecture.md`; onboarding test 10 questions: 3-layer mathematically necessary (NOT 1 or 2), drift threshold 0.1% canonical (sprint contract §14.s10.1), statistical bounds calibration n=50+50 95% CI (Lote 10.8bis P0-E), fail-CLOSED at reconciliation (Lote 10.6bis split-tier), invoice freeze automatic on drift, hash chain re-verification cooperation WI-S10-002, Stripe API pull cooperation WI-S10-003, PagerDuty 3 canonical services (sprint contract §17 inheritance WI-S09-006), R2 Object Lock 7y reports, RB-FM-302 + RB-FM-151 dry-run prerequisite, 30d clean streak prerequisite.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Layer 1 drift > 0.1% silent (emit/aggregator bug) | M | M | HIGH | H | LOW | 3-layer daily + SEV-2 + invoice freeze |
| R-002 | Layer 2 drift (invoice generation bug) | L | M | HIGH | M | LOW | Daily/monthly + SEV-2 + freeze |
| R-003 | Layer 3 drift (Stripe API discrepancy) | L | L | CRITICAL | M | LOW | SEV-1 + corelink-finance + corelink-sre paged |
| R-004 | Hash chain tampering (admin INSERT bypass) | L | L | CRITICAL | L | LOW | Re-verification daily + SEV-1 + post-mortem |
| R-005 | Stripe API outage Layer 3 1h (FM-151) | L | H | MEDIUM | L | LOW | PAT-BACKOFF-001 + PAT-QUEUE-EVENTS-001 + RB-FM-151 |
| R-006 | Reconciliation worker outage > 1d | L | M | MEDIUM | L | LOW | Catch-up replay; INV-BILLING-RECONCILE-3-LAYER eventual consistency |
| R-007 | Drift threshold 0.1% miscalibrated | L | L | MEDIUM | L | LOW | Calibration n=50+50 95% CI + drift calibration doc runbook |
| R-008 | Late event > 5% mensal (silent backfill) | L | M | HIGH | M | LOW | sprint contract §18 post-mortem hook + Finance review |
| R-009 | Reports R2 retention violation | L | L | HIGH | L | LOW | Object Lock Governance Mode 7y + admin DELETE rejected |
| R-010 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | MEDIUM | L | LOW | grep verification before commit |
| R-011 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-012 | 30d clean streak interrupted before promotion | M | L | HIGH | M | LOW | Pre-rollout chaos test 30d staging; bug fixes drift root-cause first |

## 29. Review Checkpoints

D+0 design (Architect; 3-layer mathematically necessary + fail-CLOSED); D+1 Compliance Officer (3-layer reconciliation + invoice freeze + audit trail); D+2 Finance (drift threshold 0.1% calibration + 30d clean streak); D+3 AppSec (hash chain re-verification + tampering detection); D+4 Privacy (LINDDUN + per-tenant breakdown pseudonymization); D+5 SRE (RB-FM-302 + RB-FM-151 dry-run + PagerDuty 3 services); D+6 code review; D+7 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — STRIDE + hash chain re-verification + CTRL-BILLING-001_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos 12 + property test 100k + RB dry-runs_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 CC1.4 + GAAP ASC 606 + 3-layer reconciliation + 30d clean streak_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN + per-tenant pseudonymization + DSR cooperation S-11_ |
| 11 | Architect | _TBD; **mandatory emphatic** — 3-layer mathematical necessity + fail-CLOSED (Lote 10.6bis) + statistical bounds calibration (Lote 10.8bis P0-E) + chrono primitives (Lote 10.8bis P0-D) + INV §3.X verification (Lote 10.8bis P1-13) + Lote 10.9-quinquies NEW-P0-2 absorption_ |
| 12 | Finance | _TBD; **mandatory emphatic** — financial integrity 3-layer + invoice freeze + Layer 3 SEV-1 + 30d clean streak prerequisite_ |

(Legal sign-off via DPA reference at sprint level; not per-WI.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.10) | Criação WI-S10-004; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris + Lote 10.9bis/tris/quaters/quinquies lessons absorbed: 5-tier canonical Plan (Lote 10.7bis P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test (Lote 10.7bis P1-3); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.X canonical position TBD verification before commit (Lote 10.8bis P1-13); corelink_time::next_month_first_utc_midnight() canonical primitive (Lote 10.8bis P0-D); calibration n=50+50 + 95% CI bounded (Lote 10.8bis P0-E); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis). **Lote 10.9-quinquies NEW-P0-2 critical absorption**: typed ReconciliationReport + Layer1/2/3Report + HashChainVerification (NOT serde_json::Value). **Lote 10.6bis split-tier canonical inheritance**: fail-CLOSED at reconciliation (NOT fail-OPEN; reconciliation integrity > availability). **Lote 10.9bis P0-E inheritance**: Prom metric names underscored canonical. NEW corelink-billing-reconcile crate + 3-layer canonical (Layer 1 R2/D1 daily; Layer 2 D1/Neon monthly; Layer 3 Neon/Stripe monthly) + invoice freeze automatic on drift > 0.1% + hash chain re-verification cooperation WI-S10-002 + Stripe API pull cooperation WI-S10-003 + R2 Object Lock 7y reports + statistical drift bounds calibration + drift calibration doc runbook + RB-FM-302 + RB-FM-151 dry-run prerequisite + 30d clean streak prerequisite + 3 PagerDuty services canonical (sprint contract §17 inheritance WI-S09-006). NEW INV-BILLING-RECONCILE-3-LAYER (sprint contract §8) registry §3.X position TBD. INV-BILLING-NO-LOSS Layer 1 enforcement layer + INV-BILLING-NO-DUP idempotency + INV-AUDIT-APPEND-ONLY (R2 reports). 5 SKUs canonical inheritance from WI-S10-001/002/003 (sprint contract §10 anti-scope). 5 PlanTier canonical (Lote 10.7bis P0-7). 5 regions canonical. CTRL-BILLING-001 + SOC 2 CC1.4 + GAAP ASC 606 compliance reconciliation-layer. |
| 1.1.0 | 2026-04-26 | Gustavo (Lote 10.10bis) | P0+P1 remediation pós-R4 Opus + Sonnet R5 adversarial reviews. **8 P0s fixed**: (1) PlanTier 5-tuple inventado `free/team/enterprise/custom/trial` → canonical `free/solo/team/business/enterprise` per `data_model.md §1` line 68 — corrige CHECK constraints em WIs 003+005 + sprint contract §5.3 R-S10-7; (2) INV registry positions estampadas `§3.X TBD` → reais §3.9 line 136 (NO-LOSS HIGH), §3.9 line 137 (NO-DUP HIGH), §3.12 line 166 (RECONCILE-3-LAYER HIGH), §3.12 line 167 (REPLAYABLE HIGH); severity drift CRITICAL→HIGH cascateia em SEV-2 alert wording (vs SEV-1); Lote 10.8bis P1-13 lesson finalmente absorvida; (3) WI-007 TLA+ math 5× errado (15k claim → real 3,000; 150k → real 30,000) + caveat sobre TLC reachable state graph ≠ Cartesian product CONSTANTS; MaxConcurrentEvents bound NEW; (4) CTRL-AUTHZ-005 alucinado (security_model só define -001/-002) → CTRL-AUTHZ-001 + CTRL-AUTHZ-002 explicit; sprint contract §5.6 R-S10-13 também atualizado; (5) WI-002 UPSERT WHERE clause auto-anulante removido (`WHERE own_digest = excluded.own_digest` rejeitava updates legítimos); (6) Cloudflare R2 Terraform schema marcada com TODO pre-merge verification (era hallucinated AWS S3 pattern `cloudflare_r2_bucket_lock_configuration`); (7) **R5 P0-A**: WI-002 PRIMARY KEY `(tenant_id, sku, hour)` → `(tenant_id, region, sku, hour)` — multi-region tenants antes corrompiam dados a cada UPSERT; (8) **R5 P0-B**: WI-001 D1 CHECK enum `('cas_put',...)` rejeitava 100% dos inserts (strum serializa para long form `dev.hugr.corelink.cas.put.v1`); aligned com Lote 10.9bis P0-G CloudEvents prefix canonical. **P1s fixed**: (a) staging table region column added (R5 P1-A); (b) WI-003 narrative 62-char idempotency key contradiziando §6.1 35-char (R5 P1-B); (c) idempotency comment WI-001 (P1-C); (d) `CHECK (age_hours > 6)` off-by-one → `>= 6` (R5 P1-D); (e) cardinality 30 regions → 5 canonical (P1-G); (f) PercentValue type/DB CHECK alignment 0..=200 (R5 P1-H); (g) chrono::next_month_first_utc_midnight() (não existe em chrono crate) → corelink_time::next_month_first_utc_midnight() canonical helper internal; (h) CTRL-PRIV-002 mis-mapped to "pseudonymization" → privacy_model.md L209 "data classification tags" + S-11 cooperation para DSR pseudonymization separada; (i) sprint contract §5.7 R-S10-15 metric names dots → underscores (Prom canonical); (j) §5.1 R-S10-1 CloudEvents prefix `corelink.usage.*` → `dev.hugr.corelink.<op>.v1`; (k) WI-005 4-state vs 5-state — GraceActive removido como state, virou flag boolean preserving sprint contract §5.5 R-S10-10 canonical; (l) failure_modes.md adicionado RB-FM-151 mapping (P1-5). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.2.0 | 2026-04-26 | Gustavo (Lote 10.10-quaters) | P0+P1 remediation pós-R4 + R5 round-2 audits (R4 6.9/10, R5 7.8/10). Same S-08/S-09 cascade-incomplete pattern: bis ~60% complete; quaters fecha residuais. **8 P0s addressed**: (1) NEW-P0-1 spec contract §8 invariants L149-151 CRITICAL→HIGH (era source-of-truth não atualizada; severity contradicting WIs); (2) NEW-P0-2 WI-002 PK 4-tuple cascade ~30%→100% — `CounterRecord` + `LateCounterRecord` Rust structs add `region: Region` field (digest input para tamper detection per-region); 15 narrative/Gherkin/design-decision references swept (tenant_id, sku, hour) → (tenant_id, region, sku, hour); (3) NEW-P0-3 TLA+ `MaxConcurrentEvents` CONSTANT NEW added to body L58-65 + bounded em EmitEvent; `Hours` time abstraction comment clarified (R4 round-1 caveat absorbed); `quota_grace_active` separate VARIABLE (R5 NEW-P0-1 fix: grace é flag, não 5º state value); `ReconcileLayer1` action NEW with drift computation (R4 round-1 P2-10 fix); (4) WI-003 PlanTier residuals §1 item 11 L247-250 + §6.1.13 L548 swept canonical 5-tuple; (5) WI-002 Gherkin L530 UPSERT WHERE clause text fixed (R4 P0-5 partial); (6) WI-001 risk register R-001/R-002 + post-mortem hooks severity CRITICAL→HIGH; SEV-1 → SEV-2 (R4 NEW-P1-2). **P1s addressed**: (a) WI-005 PercentValue cluster L74/L178/L328 align com 0..=200 DB CHECK (R4 NEW-P1-1 + R5 NEW-P1-3); (b) WI-006 sign-off `Finance + Legal + Architect` → `Finance + Compliance Officer + Architect` (Legal sprint-level only; R4 NEW-P1-4 / round-1 P1-6 finalmente fix); (c) WI-003 título L29 + WI-005 L727 + spec contract L244 CTRL-PRIV-002 mis-mapping → "data classification tags @classification=pii" + S-11 separated DSR; (d) WI-002 §6.1.11 late event reference time → cron_now (R5 NEW-P1-1); (e) WI-002 §14 corelink_time crate path canonical reference added (`crates/corelink-time/src/canonical.rs`); (f) WI-S10-003 Idempotency-Key 64-char internal cap rationale ADR documented em line 367; (g) WI-S10-007 ADR-S10-007-tla-substitutes-proptest justifying property test exemption + lightweight sanity props NEW; (h) WI-S10-006 "Mfa" → "MFA" capitalization (30 occurrences); (i) WI-004 Layer 2/3 narrative aggregation semantics clarified (collapsed across regions for Stripe scope; per-region drift preservada via Layer 1). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |
| 1.4.0 | 2026-05-03 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S10-004 SEAL; no per-WI codex per 2026-04-30 protocol — sprint-close Sonnet review covers) | **WI-S10-004 IMPL SEALED** — `crates/corelink-billing-reconcile/` ships pure-logic skeleton per `trait-abstraction-defer` charter pattern (production CF Cron Durable Object daily 02:00 UTC per region + real D1 `billing_reconciliation_drift` ledger + real `stripe_submission_state` flag table + R2 reconciliation-report Object Lock 7y archive `reconciliation-reports/<region>/YYYY-MM-DD.json` + R2 events Layer 1 input source (WI-S10-001 NDJSON aggregation) + D1 counter Layer 2 input source (WI-S10-002 `usage_counter` table + chain head re-verification cooperation) + Stripe usage_records Layer 3 input source (WI-S10-003 `stripe_idempotency_keys` ledger) + corelink_time::next_month_first_utc_midnight() canonical primitive + statistical drift bounds calibration n=50+50 + 95% CI (Lote 10.8bis P0-E inheritance) + 30d clean streak prerequisite gauge + RB-FM-302 + RB-FM-151 dry-run + PagerDuty 3 services dispatch (corelink-finance / corelink-sre / corelink-security) + ADR-0034 promotion gate deferred to WI-S10-007 PRR ship gate). Seven modules: `event` (ReconcileLayerKind `#[non_exhaustive]` 3-element taxonomy [Layer1Emit / Layer2Aggregate / Layer3Stripe] + ReconcileDecision `#[non_exhaustive]` 5-element taxonomy [NoDrift / AutoFixed / TicketSev3 / PageSev2 / PageSev1AutoPaused] + LayerTotals + ReconcileSnapshot + ReconcileConfig with canonical 4-tier ladder constants `QUIET_THRESHOLD = 0.0001` / `SEV3_TO_SEV2_THRESHOLD = 0.001` / `SEV2_TO_SEV1_THRESHOLD = 0.01` per sprint contract §14.s10.1 zero tolerance + auto-fix dual-condition gate constants `AUTO_FIX_MAX_RECORDS = 5` / `AUTO_FIX_MAX_PERCENT = 0.0001` Lote 10.6bis P0-6 inheritance); `drift` (compute_pairwise_drift_pct over `u128 → f64` precision-bounded `2^-43` floor + compute_max_drift with primary-layer SEV-1 routing direction Layer 3 > Layer 2 > Layer 1 + compute_drift_record_count canonical max-min cardinality + auto_fix_gate_fires dual-condition gate); `history` (DriftHistoryLedger trait + InMemoryDriftHistoryLedger UPSERT-safe ledger + `(tenant_id, billing_period, run_started_at)` UNIQUE PK INV-BILLING-NO-DUP enforcement at storage layer + DriftHistoryRow + DriftHistoryInsertOutcome Inserted/AlreadyExistsIdempotent + FailingDriftHistoryLedger); `stripe_pause` (StripeSubmissionControl trait + InMemoryStripeSubmissionControl `BTreeSet`-backed `(tenant, billing_period)` flag + StripePauseOutcome Acked/AlreadyPaused + FailingStripeSubmissionControl); `audit` (ReconcileAuditEventType `#[non_exhaustive]` 6-event taxonomy `corelink.billing_reconcile.{run_started, no_drift, auto_fixed, ticket_filed, page_dispatched, stripe_paused}` + ReconcileAuditSink trait + InMemoryReconcileAuditSink + FailingReconcileAuditSink + audit_event_for_decision canonical mapping fail-CLOSED Lote 10.6bis); `reconciler` (BillingReconciler trait + InMemoryBillingReconciler orchestrator: audit `run_started` BEFORE drift compute → compute drift via canonical pairwise primitive → classify decision per 4-tier ladder + auto-fix dual-condition gate carved INSIDE Quiet tier → audit `<decision>` BEFORE state mutation → drift-history INSERT → SEV-1 arm: `StripeSubmissionControl::pause` per (tenant, billing_period) idempotent); `error` (ReconcileError + ReconcileAuditSinkError + ReconcileDriftHistoryError + ReconcileStripePauseError canonical `#[non_exhaustive]` taxonomies). Migration `migrations/d1/0019_billing_reconciliation_drift.sql` ships canonical `billing_reconciliation_drift` PRIMARY KEY (tenant_id, billing_period, run_started_at) UNIQUE [INV-BILLING-NO-DUP storage layer; 7-year SOC 2 CC1.4 + GAAP ASC 606 evidence trail] + `stripe_submission_state` PRIMARY KEY (tenant_id, billing_period) UNIQUE [SEV-1 idempotent pause-flag] + 5-element decision CHECK constraint + 3-element primary_layer CHECK constraint + max_drift_pct fixed-point `_e9` scaling + 3 indexes + 11 inline CHECK constraints. Tests: 66 inline unit + 14 integration (10 property tests at 10k iter PR-gate via `PROPTEST_CASES` env-var read at runtime: `prop_no_drift_when_three_layers_match` + `prop_drift_threshold_boundaries` + `prop_auto_fix_dual_condition_gate` + `prop_layer3_drift_pages_sev1_pauses_stripe` + `prop_tenant_isolation` + `prop_audit_emit_per_decision_arm` + `prop_idempotent_rerun_same_period` + `prop_zero_input_no_panic` + `prop_pairwise_drift_symmetric` + `prop_max_drift_dominates_pairwise` + 4 sanity / canonical-surface pinning). Quality gates green: cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings, cargo test -p corelink-billing-reconcile --all-targets (80/80), validate_specs.py (280/286), check_migrations_additive.py (19 files OK). Charter compliance: zero unsafe / unwrap / expect / panic / indexing in lib code; per-instance `Arc<Mutex<>>` F-001 closure; `#[non_exhaustive]` on every public enum; wasm32-clean (no tokio in src/); audit fail-CLOSED envelope BEFORE state mutation on every decision arm (RunStarted before drift compute; decision-arm audit before drift-history INSERT; stripe_paused audit before pause control surface call); ChaCha20Rng PRNG pinned for randomized fixtures; `prop_assert!(let valid = matches!...; valid)` pattern (S-08 P1-1 fix); 4-tier drift threshold ladder boundary semantics canonical (`<= QUIET` → Quiet; `<= SEV3` → SEV-3 ticket; `<= SEV1_FLOOR` → SEV-2 page; `> SEV1_FLOOR` → SEV-1 page + Stripe-pause); auto-fix dual-condition gate carved INSIDE Quiet tier (drift `> 0` AND `count ≤ 5 AND pct ≤ 0.0001`); SEV-1 primary-layer routing prefers Layer 3 (Stripe API discrepancy = customer-facing invoice = legal exposure); per-tenant Stripe-pause isolation (tenant A pause never affects tenant B submission state); typed ReconcileDecision + ReconcileSnapshot + LayerTotals + DriftHistoryRow Lote 10.9-quinquies NEW-P0-2 absorption (NOT serde_json::Value). Production CF Cron DO daily 02:00 UTC per region + R2 reconciliation-report Object Lock 7y IaC + chain head re-verification cooperation WI-S10-002 + Stripe API pull cooperation WI-S10-003 fetch_invoice + statistical drift bounds calibration n=50+50 + 95% CI (Lote 10.8bis P0-E) + drift calibration doc runbook + 30d clean streak production rollout + RB-FM-302 + RB-FM-151 staging dry-run + PagerDuty 3 services dispatch all deferred to WI-S10-007 PRR ship gate per `trait-abstraction-defer` charter pattern. |
| 1.3.0 | 2026-04-26 | Gustavo (Lote 10.10-sextus) | P0+P1 remediation pós-R4 + R5 round-3 audits (R4 8.0/10, R5 9.1/10 CONDITIONAL SEAL). 2 P0s + 8 P1s residuais addressed. **2 P0s**: (1) R4 NEW-P0-1 severity cascade incompleto — quaters só varreu WI-001; sextus completou WI-002 (L745-746 post-mortem + L786 R-001) + WI-003 (L869-870) + WI-007 (L743 + L785 R-005) — todos `INV-BILLING-NO-LOSS/NO-DUP` violation references → HIGH-severity post-mortem + SEV-2 alert (não SEV-1). (2) R4 NEW-P0-2 WI-S10-004 Layer 1 ainda 3-tuple `(tenant, sku, hour)` → 4-tuple `(tenant, region, sku, hour)` em título L41 + trait doc L67 + narrative L217 + Gherkin L521 + sprint contract §5.2 R-S10-4 line 86 + CAP-BILLING-002 line 65 — mascarava multi-region drift via cross-region offsets (financial-grade integrity restored). **8 P1s**: (a) R5 P1-QUIN-1 WI-002 L538 + step 5 Gherkin/narrative `hour_window.start_ts` → `cron_now`; (b) R5 P1-QUIN-2 WI-001 L210 §1 item 9 idempotency key long-form → 35-char canonical; (c) R4 P1-1 WI-003 título L41 + ST-010 sub-task CTRL-PRIV-002 pseudonymization → "data classification tags @classification=pii" + S-11 separated DSR; (d) R4 P1-2 WI-006 §29 D+3 review checkpoint Legal sign-off → sprint-level only (NÃO 3-of-3 per-replay; matched §6.1.6 fix); (e) R4 P1-3 spec contract metadata bumped v1.1.0/sota-v1.1/2026-04-24 → v1.3.0/sota-v1.3/2026-04-26; (f) R4 P1-4 + R5 P1-QUIN-3 TLA+ block: `BillingPeriods` + `RequestIds` agora declared CONSTANTS; helper operators (`Abs`, `SumCounters`, `SumLineItems`, `ComputeStripeInvoiceId`, `StripeIdParts`, `EventAccountedInInvoice`) documented as separate INSTANCE module; (g) R5 P1-QUIN-4 INV_BILLING_NO_LOSS retry_queue Sequence membership bug fixed via `RetrySet == {retry_queue[i] : i \in DOMAIN retry_queue}` Range conversion (semantic correctness); (h) R4 P1-5 `compute_counter_digest()` doc explicitly confirms region IS in canonical_json input (cross-region tamper detection guaranteed); (i) R4 P1-6 §14.s10.x.8 + R-010/R-011 risk register stale "INV §3.X TBD pre-merge" → concrete §3.9 L136/137 + §3.12 L166/167 verified Lote 10.10bis (residual após mitigação LOW). Validators verde: validate_specs.py 172/178 OK; validate_references.py zero S-10 dangling. |

## 32. Anti-patterns evitados

- ❌ serde_json::Value em ReconciliationReport (Lote 10.9-quinquies NEW-P0-2; typed canonical); ❌ Single-layer reconciliation (3-layer mathematically necessary sprint contract §5.4); ❌ Fail-OPEN at reconciliation (Lote 10.6bis split-tier; fail-CLOSED canonical); ❌ Manual quarterly reconciliation (sprint contract §16 SOTA bar; daily canonical); ❌ Drift threshold > 0.1% (sprint contract §14.s10.1 zero tolerance); ❌ Skip hash chain re-verification (CRITICAL audit signal); ❌ Skip RB-FM-302/151 dry-run (sprint contract §6 DoD prerequisite); ❌ Skip 30d clean streak (sprint contract §6 DoD prerequisite); ❌ ad-hoc `now() + Duration::days(30)` (corelink_time::next_month_first_utc_midnight canonical Lote 10.8bis P0-D); ❌ Statistical bound calibration skipped (Lote 10.8bis P0-E inheritance); ❌ Per-feature SKU/Plan expansion beyond 5 canonical (sprint contract §10); ❌ tokio::spawn em CF Workers (Lote 10.7bis R5 P0-3); ❌ TenantCtx bypass; ❌ DELETE row em reports R2 (Object Lock Governance prevents).

---

**Fim WI-S10-004.** Próximo: WI-S10-005 (Quota state machine + overage handling + email/notification).
