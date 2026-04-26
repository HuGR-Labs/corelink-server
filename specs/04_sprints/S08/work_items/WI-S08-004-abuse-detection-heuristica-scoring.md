---
id: "WI-S08-004"
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
  - "PRIVACY-MODEL"
tags: ["wi", "s08", "abuse-detection", "scoring", "heuristica", "lgpd-art20", "humane-response", "high-risk"]
---

# WI-S08-004 — Abuse Detection Heurística Multi-Feature + Scoring + Humane Response (`crates/corelink-abuse-detector`; weighted-sum score `corelink_abuse_score{tenant_id}` em [0.0, 1.0]; features = {cpu_wallclock_ratio, egress_bytes_per_min, action_digest_entropy, concurrent_exec_count} sprint contract §5 R-S08-7; observed via S-09 metrics 5min aggregation windows; 4-tier response gradient: noop / silent-downgrade-50% / admin-review-trigger-SEV2 / suspend-candidate-human-review-only; LGPD Art. 20 + GDPR Art. 22 humane response sprint contract §7.10.s08.3 (3 customer self-service endpoints: GET /v1/admin/abuse_score + POST /v1/admin/abuse_appeal + audit log every decision); threshold calibration via 50 synthetic workloads (n=50 benign + n=50 high-intensity abusive; Lote 10.8bis P0-E corrected; 95% CI requirement) ≥ 80% true positive + 0 false positive sprint contract §6 DoD; per-tenant isolation formal property test sprint contract §7.10.s08.4)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-08](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S08-004 |
| Título | Heurística-based abuse detection NÃO-ML (anti-scope sprint contract §10); 4 features observed via S-09 metrics 5min windows: (a) `cpu_wallclock_ratio` (cpu_seconds / wallclock_seconds; abusive ≥ 0.95 sustained = compute farming); (b) `egress_bytes_per_min` (egress / min; abusive ≥ 95th percentile WoW + 5σ); (c) `action_digest_entropy` (Shannon entropy of unique action_digests; abusive ≤ 1 bit = single-action spam); (d) `concurrent_exec_count` (parallel exec sessions; abusive ≥ 100× plan tier baseline); weighted-sum score em [0.0, 1.0]; weights calibrated em staging (50+50 synthetic workloads (Lote 10.8bis P0-E corrected)); 4-tier response gradient with humane LGPD Art. 20 alignment: score < 0.5 noop / 0.5–0.8 silent-downgrade-50%-rate-1h-auto-recover / 0.8–0.95 admin-review-trigger-SEV-2 / ≥ 0.95 suspend-candidate-human-review-NEVER-auto / 3 customer-facing endpoints (S-13 dependency staging stub OK); audit log every decision (LGPD compliance); per-tenant isolation property test 100k nightly (sprint contract §7.10.s08.4) |
| Sprint | S-08 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-RATE-001 + CTRL-AUTH security controls; abuse detection complements bulkhead camadas 1-4), FF-HR-002 (cross-tenant SLO degradation if abuse undetected) |

## 1. Intent

Abuse detection é **the canary layer** — detecta padrões abusive ANTES de SLO breach (proactive vs reactive). Score-based gradient response permits humane handling: most tenants noop; suspicious gets silent downgrade; egregious gets admin review; extreme NEVER auto-suspended (LGPD Art. 20 / GDPR Art. 22 humane response). Heurística simples primeiro (sprint contract §10 anti-scope ML); 4 features carefully chosen para capture real abuse patterns (compute farming, scraping, spam, exec flood) without ML complexity.

```rust
// File: crates/corelink-abuse-detector/src/lib.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait AbuseDetector: Send + Sync {
    /// Compute current score for tenant; observed via S-09 metrics 5min windows;
    /// idempotent (same window → same score; until window advances).
    async fn compute_score(
        &self,
        tenant_ctx: &TenantCtx,
    ) -> Result<AbuseScoreResult, AbuseError>;

    /// Apply gradient response based on score; emits decision event;
    /// NEVER auto-suspends (humane LGPD Art. 20).
    async fn apply_response(
        &self,
        tenant_ctx: &TenantCtx,
        score: AbuseScore,
    ) -> Result<ResponseAction, AbuseError>;

    /// Customer self-service: GET current score + features breakdown (LGPD compliance).
    async fn get_score_with_breakdown(
        &self,
        tenant_ctx: &TenantCtx,
    ) -> Result<AbuseScoreBreakdown, AbuseError>;

    /// Customer appeal endpoint; routes to human reviewer ≤ 24h SLA.
    async fn submit_appeal(
        &self,
        tenant_ctx: &TenantCtx,
        appeal: AbuseAppeal,
    ) -> Result<AppealId, AbuseError>;

    /// Admin: review pending appeals; approve / reject; emit decision audit.
    async fn review_appeal(
        &self,
        admin_ctx: &AdminCtx,
        appeal_id: &AppealId,
        decision: AppealDecision,
        reviewer_notes: String,
    ) -> Result<(), AbuseError>;
}

pub struct AbuseScoreResult {
    pub tenant_id: TenantId,
    pub score: AbuseScore,                          // [0.0, 1.0] f64
    pub computed_at_ms: i64,
    pub window_start_ms: i64,                       // 5min window canonical
    pub window_end_ms: i64,
    pub features: AbuseFeatures,                    // breakdown for transparency
}

pub struct AbuseFeatures {
    pub cpu_wallclock_ratio: f64,                   // [0.0, 1.0+]; clamped 1.0 for score
    pub egress_bytes_per_min: u64,
    pub action_digest_entropy_bits: f64,            // Shannon entropy ≥ 0
    pub concurrent_exec_count: u32,
}

pub struct AbuseScoreBreakdown {
    pub score: AbuseScore,
    pub features: AbuseFeatures,
    pub feature_weights: AbuseFeatureWeights,       // disclosed for transparency LGPD
    pub feature_contributions: Vec<(String, f64)>,  // each feature's % contribution
    pub current_response_tier: ResponseTier,
    pub appeal_url: String,                         // /v1/admin/abuse_appeal
}

pub enum ResponseTier {
    Noop,                                           // score < 0.5
    SilentDowngrade50pct1h,                         // 0.5 ≤ score < 0.8; auto-recover if score drops
    AdminReviewTriggerSev2,                         // 0.8 ≤ score < 0.95
    SuspendCandidateHumanReviewOnly,                // ≥ 0.95; NEVER auto-applied
}

pub struct AbuseAppeal {
    pub explanation: String,                        // customer's narrative
    pub timeline: String,                           // when did this start; what changed
    pub contact_email: String,
}

pub enum AppealDecision {
    Approved { reset_score: bool, allowlist_until_ms: Option<i64> },
    Rejected { reason: String, escalate_to_review: bool },
    NeedsMoreInfo { request_text: String },
}

#[derive(thiserror::Error, Debug)]
pub enum AbuseError {
    #[error("metrics observability lag (S-09 unavailable): {0}")]
    MetricsLag(String),

    #[error("audit emit failed (fail-closed): {0}")]
    AuditEmitFailed(String),

    #[error("appeal not found: {0}")]
    AppealNotFound(String),

    #[error("appeal already reviewed: {0}")]
    AppealAlreadyReviewed(String),

    #[error("auto-suspend forbidden by humane response (LGPD Art. 20): admin must manually review score ≥ 0.95")]
    AutoSuspendForbidden,

    #[error("D1 backend error: {0}")]
    D1BackendError(String),

    #[error("plan unknown for tenant {tenant_id}")]
    PlanUnknown { tenant_id: String },
}
```

**Cripto-driven invariants enforced**:

1. **Heurística-only (NOT ML)** — sprint contract §10 anti-scope absorbed:
   - 4 features: cpu_wallclock_ratio, egress_bytes_per_min, action_digest_entropy_bits, concurrent_exec_count.
   - Weighted-sum scoring; weights calibrated em staging (50+50 synthetic workloads (Lote 10.8bis P0-E corrected) sprint contract §6 DoD).
   - Transparent: customer can inspect features + weights + contributions (LGPD Art. 20 compliance).

2. **Per-tenant isolation** (sprint contract §7.10.s08.4 property test 100k):
   - Per-tenant score computation; cross-tenant features NOT considered (no comparison-based scoring que poderia leak cross-tenant info).
   - INV-TENANT-ISOLATION inheritance.

3. **Humane LGPD Art. 20 / GDPR Art. 22 response** (sprint contract §7.10.s08.3 absorbed):
   - **NEVER auto-suspend** at any score (suspend always requires human review; AutoSuspendForbidden error if attempted programmatically).
   - 3 customer-facing endpoints: (a) `GET /v1/admin/abuse_score` (transparency); (b) `POST /v1/admin/abuse_appeal` (human reviewer ≤ 24h SLA); (c) audit log every decision (compliance trail).
   - Silent downgrade (response tier 2) is automated but *reversible* (auto-recover if score drops) — does not constitute "significant decision" per LGPD interpretation; admin notification SEV-3 ensures visibility.

4. **5min aggregation window canonical** (sprint contract §10.s08.5 spirit absorbed; calendar-aligned reset):
   - Window: `[utc_5min_floor(now), utc_5min_floor(now) + 5min)`.
   - Cron-tick batch: every 5min, compute score for active tenants (DO `AbuseScoreCron` per region; alarm 5min; re-arm AT START Lote 10.4bis).
   - Idempotent: same window → same score; until window advances.

5. **Score formula canonical** (calibrated em staging):
   ```rust
   pub fn compute_abuse_score(features: &AbuseFeatures, weights: &AbuseFeatureWeights, tier: &Tier) -> AbuseScore {
       // Normalize each feature to [0.0, 1.0] via tier-specific baselines:
       let cpu_norm = (features.cpu_wallclock_ratio - 0.5).max(0.0).min(0.5) * 2.0;
       // 0.0 if ratio ≤ 0.5; 1.0 if ratio ≥ 1.0 (compute farming = 100% CPU on wallclock)

       let egress_baseline = egress_baseline_for_tier(tier);   // bytes/min per tier
       let egress_norm = ((features.egress_bytes_per_min as f64 / egress_baseline) - 1.0)
           .max(0.0).min(10.0) / 10.0;
       // 0.0 if ≤ baseline; 1.0 if ≥ 10× baseline

       let entropy_norm = (3.0 - features.action_digest_entropy_bits).max(0.0).min(3.0) / 3.0;
       // 0.0 if entropy ≥ 3 bits (8+ unique actions); 1.0 if entropy ≤ 0 bits (1 unique = spam)

       let exec_baseline = exec_baseline_for_tier(tier);       // concurrent execs per tier
       let exec_norm = ((features.concurrent_exec_count as f64 / exec_baseline) - 1.0)
           .max(0.0).min(100.0) / 100.0;
       // 0.0 if ≤ baseline; 1.0 if ≥ 100× baseline

       let score = weights.cpu * cpu_norm
                 + weights.egress * egress_norm
                 + weights.entropy * entropy_norm
                 + weights.exec * exec_norm;

       AbuseScore::clamp(score)
   }

   pub struct AbuseFeatureWeights {
       pub cpu: f64,        // initial 0.30
       pub egress: f64,     // initial 0.25
       pub entropy: f64,    // initial 0.25
       pub exec: f64,       // initial 0.20
       // sum to 1.0
   }
   ```
   Initial weights from sprint contract benchmarks; tunable via admin S-13 ADR future (post-staging calibration).

6. **Calibration target** (sprint contract §6 DoD):
   - 50 synthetic workloads (n=50 benign + n=50 high-intensity abusive; Lote 10.8bis P0-E) em staging.
   - Benign scenarios: (1) typical Bazel build 200 RPS team plan; (2) parallel CI at 1000 RPS business; (3) ML model training egress 10 GB/min; (4) Docker layer rebuild 50 RPS; (5) academic research 100 RPS team.
   - Abusive scenarios: (6) compute farming cpu_ratio=0.95 sustained 2h; (7) scraping egress 100× baseline; (8) action_digest spam (1 unique digest 10k requests); (9) exec flood 1000× baseline; (10) combined multi-vector attack.
   - **Pass criteria**: 0 false positives benign + ≥ 80% true positives abusive.

7. **Cron-tick aggregation** via DO `AbuseScoreCron-<region>`:
   ```rust
   impl AbuseScoreCron {
       async fn alarm(&mut self) -> Result<(), Error> {
           // Re-arm AT START (Lote 10.4bis lesson)
           self.state.set_alarm(now() + Duration::from_secs(300)).await?;

           // Active tenants em region (D1 query batch ≤ 250)
           let active_tenants = read_active_tenants_d1().await?;
           let chunks: Vec<_> = active_tenants.chunks(250).collect();

           for chunk in chunks {
               for tenant_id in chunk {
                   // Read S-09 metrics last 5min window
                   let features = read_metrics_5min_window(tenant_id).await?;

                   // Compute score
                   let plan_tier = read_tier_d1(tenant_id).await?;
                   let weights = read_calibrated_weights_d1().await?;
                   let score = compute_abuse_score(&features, &weights, &plan_tier);

                   // Persist score em D1 (history)
                   insert_abuse_score_history_d1(tenant_id, score, &features).await?;

                   // Apply response gradient
                   let action = response_for_score(score);
                   apply_response(tenant_id, &action).await?;

                   // Audit emit (fail-closed)
                   audit_emit("corelink.abuse.score_computed", &score, &features, &action)?;
               }
           }
           Ok(())
       }
   }
   ```

8. **Per-tenant DO isolation** (INV-TENANT-ISOLATION inheritance): cron processes tenants serially; no cross-tenant comparison; each tenant's score independent.

9. **TenantCtx-only enforcement** (Lote 10.4bis lesson): tenant_id from middleware; for admin endpoints, AdminCtx (S-13).

10. **Audit fail-closed** (Lote 10.6bis pattern absorbed): all decisions audited; fail-closed if audit emit fails (decision not applied; retry on next 5min cycle).

11. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): `worker::send_future()`; NEVER `tokio::spawn` ou `wasm_bindgen_futures::spawn_local`. `async_lock::Semaphore` if needed (Lote 10.3-tris).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + race-correctness justification)

Abuse detection é **the proactive layer** — bulkhead camadas 1-4 (per-tenant DO + per-IP edge + per-PAT + global circuit) reactively limit current rate; abuse detection identifies *patterns* that precede breach (compute farming, scraping, spam, exec flood). 4-feature heurística simples primeiro (sprint contract §10 anti-scope ML); calibrated em staging; transparent (LGPD Art. 20 compliance).

**Why heurística NOT ML** (sprint contract §10 anti-scope): ML adds (a) opacity (LGPD Art. 20 inverso — customer cannot inspect "decision factors"); (b) operational complexity (model training, retraining, drift detection); (c) cost (inference latency on each request); (d) false-positive risk (ML may overfit staging patterns). Heurística simples 4-feature: transparent, tunable via admin, low false-positive rate (calibrated 95% CI FP rate ≤ 5% on n=50 benign workloads (Lote 10.8bis P0-E)). ML deferred S-14+ if heurística insuficiente.

**Why 4 features specifically**:
- **cpu_wallclock_ratio**: detects compute farming (sustained CPU usage near wallclock indicates botnet/cryptominer using CoreLink as compute substrate).
- **egress_bytes_per_min**: detects scraping (anomalously high egress vs baseline; legitimate Bazel reads ~ 10 MB/min; scraper ~ 1 GB/min).
- **action_digest_entropy_bits**: detects spam (Shannon entropy ≤ 1 bit = single repeated action_digest 10k times = obvious spam; entropy ≥ 3 bits = 8+ distinct actions = legitimate build).
- **concurrent_exec_count**: detects exec flood (parallel exec sessions ≥ 100× tier baseline = abuse).

**Why 4-tier response gradient (NOT binary)** (humane LGPD): binary (allow / suspend) is harsh + irreversible. Gradient permits proportional response: noop most cases, silent downgrade marginal cases (auto-reversible), admin review serious cases, suspend ONLY after human approval. Customer self-service appeal at any tier; transparency via /v1/admin/abuse_score endpoint.

**Why NEVER auto-suspend** (LGPD Art. 20 / GDPR Art. 22): suspend is "significant automated decision affecting individual" — customer loses service access; revenue impact; reputational harm. LGPD Art. 20 + GDPR Art. 22 prohibit such decisions without human review. Suspend always requires admin manual approval; AutoSuspendForbidden error if attempted programmatically. Sprint contract §7.10.s08.3 explicit.

**Why 5min aggregation window**: balance sensitivity vs noise. 1min: false-positives from natural burst; 1h: too slow to react. 5min: industry standard (PagerDuty, Datadog); aligns S-09 metrics 5min canonical.

**Why calibration via 50+50 synthetic workloads (Lote 10.8bis P0-E corrected)**: pre-launch validation; 0 false positive benign + ≥ 80% true positive abusive (sprint contract §6 DoD). Real-world calibration post-launch via shadow mode (compute scores; do NOT apply gradient; collect ground truth via customer feedback for 30d).

**Adversarial scenarios**:
- **Adversary game-theoretic** (knows weights): adversary just-below-threshold attack vector. Mitigation: weights tunable via admin S-13; rotating weights detected via anomaly metric; combined-feature score harder to game than single feature.
- **Score regression race** (score 0.85 at T0; 0.45 at T0+5min): silent downgrade applied at T0; auto-recover at T0+5min; bounded ≤ 5min false-downgrade.
- **Customer appeal wave** (10k appeals at once): admin queue; PriorityQueue por score (highest score reviewed first); SLA ≤ 24h with overflow alert SEV-3.
- **Per-tenant isolation race**: cron processes tenants serially; per-tenant score independent; INV-TENANT-ISOLATION property test 100k.
- **Audit fail mid-decision**: fail-closed: decision NOT applied; retry next 5min cycle; eventual consistency.
- **S-09 metrics outage**: AbuseError::MetricsLag; cron skips this cycle; next cycle resumes; alert SEV-3 (observability gap).

**Risk justification HIGH_RISK**:
- **FF-HR-005**: complementa CTRL-RATE-001 + CTRL-AUTH; abuse detection critical for AVAIL-ISOLATION.
- **FF-HR-002**: undetected abuse leads cross-tenant degradation.
- 12 sign-offs (Compliance + Privacy mandatory emphatic; LGPD Art. 20 alignment) + chaos suite + property test 100k.

## 3. Customer Impact & Journey

**Persona 1 — Customer (typical workload)**: tenant T running normal Bazel build 200 RPS team plan; cpu_ratio=0.3 (typical), egress=10 MB/min, entropy=4 bits (~16 actions), concurrent_exec=20. Score = 0.30×0.0 + 0.25×0.0 + 0.25×0.0 + 0.20×0.0 = 0.0; tier Noop; oblivious to detection.

**Persona 2 — Customer (heavy ML training egress)**: tenant T ML training; egress 5 GB/min × 30min sustained; egress 50× baseline. Score component egress_norm = 1.0 (saturated); other features normal. Score = 0.30×0 + 0.25×1.0 + 0.25×0 + 0.20×0 = 0.25; tier Noop. No false-positive (other features normal).

**Persona 3 — Customer (compute-farming high intensity SilentDowngrade)**: tenant T compute farming pattern: cpu_ratio=0.98 sustained + entropy=0.5 (1-2 unique actions repeated) + exec=50× baseline. Score = 0.30×0.96 + 0.25×0 + 0.25×0.83 + 0.20×0.5 = 0.596; tier SilentDowngrade50pct1h. Tenant gets rate cap reduced 50% for 1h; auto-recover if score drops. Admin notified SEV-3.

**Persona 3.1 — Calibration acknowledged limitation (Lote 10.8bis P0-E absorbed)**: medium-intensity compute-farming patterns (e.g., cpu_ratio=0.85, entropy=2 bits, exec=5× baseline → score ≈ 0.29) DO score Noop tier — heurística calibrated for high-intensity attacks NOT subtle medium-intensity patterns. This is a **known limitation of n=50+50 weighted-sum heurística**: low-medium-intensity abuse falls within natural CPU-heavy workload variance (e.g., legitimate AOT compilation, parallel testing). Mitigation: (a) periodic shadow re-calibration via real production traffic post-launch (shadow mode 30d ground-truth labeling); (b) ML-based scoring deferred S-14+ for finer-grained discrimination; (c) for now, S-08 explicitly accepts that medium-intensity abuse will require manual customer-relations escalation rather than automated detection. Sprint contract §6 DoD calibration target: ≥ 80% TP **on the 50-abusive HIGH-INTENSITY workloads** (NOT medium-intensity); medium-intensity acknowledged as out-of-scope automated detection.

**Persona 4 — Customer (admin review)**: combined-vector attack: cpu=0.99 + egress=20× baseline + entropy=0.3 + exec=80×baseline. Score = 0.30×0.98 + 0.25×1.0 + 0.25×0.9 + 0.20×0.79 = 0.294+0.25+0.225+0.158 = 0.927; tier AdminReviewTriggerSev2. Admin alerted; reviews context (was customer running bona-fide ML training? compute farming?); decides.

**Persona 5 — Customer (suspend candidate)**: extreme attack score 0.97; tier SuspendCandidateHumanReviewOnly. Admin alerted SEV-1; manual review required ≤ 24h; admin decision: temporarily suspend OR contact customer first. NEVER auto-suspend.

**Persona 6 — DevOps reviewing**: DASH-RATE (WI-S08-006) shows score distribution + per-tier counts + appeal queue depth.

**Persona 7 — Customer self-service**: customer suspect they're being downgraded; calls `GET /v1/admin/abuse_score`; sees score=0.62 + breakdown (cpu=high, others normal); appeals via `POST /v1/admin/abuse_appeal {explanation: "running legitimate stress test"}`; admin reviews ≤ 24h; approved → score reset.

**SLA addendum**:
- Score computation latency: ≤ 1s p99 per tenant (cron 5min batch).
- Customer appeal SLA: ≤ 24h human review (humane LGPD).
- Customer self-service /v1/admin/abuse_score: ≤ 200ms p99.
- **Calibration target (Lote 10.8bis P0-E corrected)**: 95% CI FP rate ≤ 5% on n=50 benign workloads (binomial: 0 FP em 50 samples → 95% CI upper 5.7%; ≤ 2 FP em 50 → 95% CI upper 9.6% acceptable initial); 95% CI TP rate ≥ 80% on n=50 high-intensity abusive workloads (≥ 40 TP em 50 → CI lower 67%; ≥ 45 TP → CI lower 78% target). Pre-launch staging validation; post-launch 30d shadow re-calibration via production traffic ground-truth labeling.
- INV-TENANT-ISOLATION: 0 cross-tenant feature leak em chaos test 30d.

## 4. Capability Mapping

- **CAP-ABUSE-001** (Abuse detection heurística multi-feature) — IMPLEMENTA primary.
- **CAP-ABUSE-002** (Automated response: downgrade + admin trigger; suspend NEVER auto) — IMPLEMENTA primary.
- Trace: `security_model.md CTRL-RATE-001 + CTRL-AUTH` + `invariant_registry.md INV-AVAIL-ISOLATION + INV-TENANT-ISOLATION` + `failure_modes.md FM-201 (Config change causa rate-limit drop; canonical FM lookup; Lote 10.8bis P1-3 corrected — FM-251 actually é "Credential stuffing / brute force")` + `privacy_model.md PII redaction` + sprint contract §7.10.s08.3 (LGPD Art. 20) + §7.10.s08.4 (per-tenant isolation property test).

## 5. Tipo

Heurística scoring + cron DO + admin endpoints + Tower middleware response gradient; HIGH_RISK; FF-HR-005 + FF-HR-002.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-abuse-detector/` module** — AbuseDetector trait + cron DO + admin endpoints.

2. **DO `AbuseScoreCron-<region>`** per-region:
   - Alarm 5min cron-tick (re-arm AT START Lote 10.4bis).
   - Read S-09 metrics 5min window for active tenants.
   - Compute scores; apply gradient response; emit audit.
   - D1 batch ≤ 250 (Lote 10.5bis).

3. **Score computation** (4-feature weighted-sum):
   - Features observed via S-09 PromQL queries:
     - `cpu_wallclock_ratio`: `rate(corelink_exec_cpu_seconds_total[5m]) / rate(corelink_exec_wallclock_seconds_total[5m])`.
     - `egress_bytes_per_min`: `rate(corelink_egress_bytes_total[5m]) * 60`.
     - `action_digest_entropy_bits`: Shannon entropy computed over distinct action_digests in window.
     - `concurrent_exec_count`: max(corelink_concurrent_exec_count) over [5m].
   - Weights canonical (initial; tunable via admin S-13 future):
     - `cpu`: 0.30, `egress`: 0.25, `entropy`: 0.25, `exec`: 0.20 (sum 1.0).
   - Tier-aware baselines via `egress_baseline_for_tier` + `exec_baseline_for_tier` (5-tier canonical Lote 10.7bis P0-7 absorbed).

4. **4-tier response gradient**:
   - **Noop** (score < 0.5): no action; emit `corelink.abuse.noop` audit.
   - **SilentDowngrade50pct1h** (0.5 ≤ score < 0.8): rate cap × 0.5 for 1h; auto-recover if score drops; emit `corelink.abuse.silent_downgrade` + customer email notification (S-13 stub OK; defer S-13 sealing).
   - **Cross-WI integration mechanism (Lote 10.8bis CI-2 corrected)**: SilentDowngrade applied via NEW shared D1 table `tenant_rate_override(tenant_id PRIMARY KEY, override_multiplier REAL NOT NULL CHECK (override_multiplier >= 0.0 AND override_multiplier <= 1.0), expires_at INTEGER NOT NULL, applied_by_reason TEXT NOT NULL, applied_at INTEGER NOT NULL)`. WI-S08-001 DO RateLimiter `check_and_consume` reads this override (cached 5min em DO memory; refresh on staleness) e applies `effective_refill_rate = plan_refill_rate * override_multiplier` (e.g., 0.5 = 50% downgrade). On expiry (`now_ms > expires_at`): revert to plan_refill_rate. WI-S08-004 writes (insert/update) on tier=SilentDowngrade transition; deletes on auto-recover. Audit fail-closed em both paths.
   - **AdminReviewTriggerSev2** (0.8 ≤ score < 0.95): SEV-2 PagerDuty admin notification; emit `corelink.abuse.admin_review_triggered`.
   - **SuspendCandidateHumanReviewOnly** (score ≥ 0.95): SEV-1 alert; admin manual review required; emit `corelink.abuse.suspend_candidate` + AutoSuspendForbidden error if any code attempts auto-suspend.

5. **Customer self-service endpoints** (S-13 dependency; staging stub OK):
   - `GET /v1/admin/abuse_score` (TenantCtx; current tenant) → AbuseScoreBreakdown (score + features + weights + contributions + current tier + appeal_url).
   - `POST /v1/admin/abuse_appeal` (TenantCtx) → submit appeal; routes to admin queue.
   - Audit log every access (LGPD compliance).

6. **Admin endpoints** (S-13 dependency; staging stub OK):
   - `GET /v1/admin/abuse/queue` (AdminCtx) → pending appeals (PriorityQueue por score).
   - `POST /v1/admin/abuse/queue/{appeal_id}/approve` (AdminCtx) → AppealDecision::Approved.
   - `POST /v1/admin/abuse/queue/{appeal_id}/reject` (AdminCtx) → AppealDecision::Rejected.
   - `POST /v1/admin/abuse/queue/{appeal_id}/needs_more_info` (AdminCtx).
   - `GET /v1/admin/abuse/scores/{tenant_id}` (AdminCtx) → admin view of any tenant's score history.

7. **D1 migrations** (NEW tables; CHECK constraints inline per Lote 10.5bis):
   ```sql
   CREATE TABLE abuse_score_history (
       tenant_id TEXT NOT NULL,
       computed_at INTEGER NOT NULL,                -- unix ms; canonical no _ms suffix per Lote 10.7bis P0-3
       window_start INTEGER NOT NULL,               -- unix ms; canonical no _ms suffix
       window_end INTEGER NOT NULL,                 -- unix ms; canonical no _ms suffix
       score REAL NOT NULL,                         -- [0.0, 1.0]
       cpu_wallclock_ratio REAL NOT NULL,
       egress_bytes_per_min INTEGER NOT NULL,
       action_digest_entropy_bits REAL NOT NULL,
       concurrent_exec_count INTEGER NOT NULL,
       response_tier TEXT NOT NULL,
       PRIMARY KEY (tenant_id, computed_at),
       CHECK (score >= 0.0 AND score <= 1.0),
       CHECK (cpu_wallclock_ratio >= 0.0),
       CHECK (egress_bytes_per_min >= 0),
       CHECK (action_digest_entropy_bits >= 0.0),
       CHECK (concurrent_exec_count >= 0),
       CHECK (window_end > window_start),
       CHECK (response_tier IN ('Noop', 'SilentDowngrade50pct1h', 'AdminReviewTriggerSev2', 'SuspendCandidateHumanReviewOnly'))
   );

   CREATE INDEX idx_abuse_recent ON abuse_score_history(tenant_id, computed_at DESC);

   CREATE TABLE abuse_appeals (
       appeal_id TEXT PRIMARY KEY,                  -- ULID
       tenant_id TEXT NOT NULL,
       submitted_at INTEGER NOT NULL,               -- unix ms; canonical no _ms suffix
       explanation TEXT NOT NULL,
       timeline TEXT NOT NULL,
       contact_email TEXT NOT NULL,
       score_at_submission REAL NOT NULL,
       reviewed_at INTEGER,                         -- unix ms; canonical no _ms suffix
       reviewer_admin_id TEXT,
       decision TEXT,                               -- approved | rejected | needs_more_info
       reviewer_notes TEXT,
       sla_deadline_at INTEGER NOT NULL,            -- submitted_at + 24h
       CHECK (score_at_submission >= 0.0 AND score_at_submission <= 1.0),
       CHECK (decision IS NULL OR decision IN ('approved', 'rejected', 'needs_more_info')),
       CHECK (reviewed_at IS NULL OR reviewed_at >= submitted_at),
       CHECK (sla_deadline_at > submitted_at)
   );

   CREATE INDEX idx_appeals_pending ON abuse_appeals(submitted_at, reviewed_at)
       WHERE reviewed_at IS NULL;

   CREATE TABLE abuse_response_actions (
       tenant_id TEXT NOT NULL,
       applied_at INTEGER NOT NULL,                 -- unix ms; canonical no _ms suffix
       expires_at INTEGER,                          -- when downgrade lifts; NULL = pending human review (AdminReview/SuspendCandidate tiers; Lote 10.8-tris P1-NEW-3 NOT NULL → NULLABLE corrected)
       action TEXT NOT NULL,                        -- silent_downgrade_50pct_1h | admin_review_triggered | suspend_candidate
       triggered_by_score REAL NOT NULL,
       reverted_at INTEGER,                         -- when reverted (auto-recover OR appeal approved)
       PRIMARY KEY (tenant_id, applied_at),
       CHECK (triggered_by_score >= 0.5 AND triggered_by_score <= 1.0),
       CHECK (expires_at IS NULL OR expires_at > applied_at),
       CHECK (reverted_at IS NULL OR reverted_at >= applied_at)
   );

   CREATE TABLE abuse_calibration_weights (
       version TEXT PRIMARY KEY,                    -- "v1.0.0"
       cpu_weight REAL NOT NULL,
       egress_weight REAL NOT NULL,
       entropy_weight REAL NOT NULL,
       exec_weight REAL NOT NULL,
       updated_at INTEGER NOT NULL,                 -- unix ms; canonical no _ms suffix
       calibrated_via TEXT NOT NULL,                -- "staging_10_workloads_2026-04-25"
       active BOOLEAN NOT NULL DEFAULT FALSE,
       CHECK (cpu_weight >= 0.0 AND cpu_weight <= 1.0),
       CHECK (egress_weight >= 0.0 AND egress_weight <= 1.0),
       CHECK (entropy_weight >= 0.0 AND entropy_weight <= 1.0),
       CHECK (exec_weight >= 0.0 AND exec_weight <= 1.0),
       CHECK (ABS(cpu_weight + egress_weight + entropy_weight + exec_weight - 1.0) < 0.001)
   );
   ```

8. **Audit fail-closed pattern** (Lote 10.6bis absorbed): all abuse decisions emit audit events; fail-closed if audit emit fails (decision NOT applied; retry next 5min).

9. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 absorbed): `worker::send_future()`; NEVER `tokio::spawn`.

10. **Métricas** (Prometheus convention):
    - `corelink.abuse.score{tenant_id, tier}` (gauge; latest score per tenant).
    - `corelink.abuse.tier_count_total{tier=Noop|SilentDowngrade|AdminReview|SuspendCandidate}` (counter).
    - `corelink.abuse.appeal_pending{age_bucket=<1h|1-12h|12-24h|>24h}` (gauge; **alert SEV-3 if any > 24h** — humane SLA breach).
    - `corelink.abuse.appeal_resolution_time_hours` (histogram; SLO ≤ 24h p95).
    - `corelink.abuse.cron_duration_ms` (histogram; SLO ≤ 60s p99).
    - `corelink.abuse.metrics_lag_total` (counter; **alert SEV-3 if > 5/h** — S-09 outage signal).
    - `corelink.abuse.auto_suspend_attempts_total` (counter; **alert SEV-1 if > 0; LGPD violation canary**).
    - `corelink.abuse.calibration_drift_total` (counter; periodic shadow re-calibration).
    - `corelink.abuse.cross_tenant_feature_leak_total` (counter; **alert SEV-1 if > 0; INV-TENANT-ISOLATION canary**).

11. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar** — Lote 10.7bis P1-3 absorbed):
    - `prop_score_clamping`: random feature inputs; assert score ∈ [0.0, 1.0].
    - `prop_score_monotonicity`: increasing each feature → non-decreasing score (held other features constant).
    - `prop_per_tenant_isolation`: 1k tenants × random features; assert per-tenant score independent (sprint contract §7.10.s08.4).
    - `prop_response_gradient`: random scores; assert correct tier mapping; no boundary errors.
    - `prop_auto_suspend_forbidden`: 100k random scores; assert 0 auto-suspend attempts succeed (humane LGPD).
    - `prop_silent_downgrade_auto_recover`: simulate score drop after downgrade; assert recovery within 1h.
    - `prop_appeal_sla_24h`: 1k synthetic appeals; assert SLA deadline = submitted_at + 24h.

12. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 11):
    - 1. **Calibration validation**: 50+50 synthetic workloads; assert 0 FP + ≥ 80% TP (sprint contract §6 DoD).
    - 2. **Adversary just-below-threshold**: simulate adversary score=0.49 sustained; assert no false-positive downgrade.
    - 3. **Score regression race**: tenant score 0.85 at T0; 0.45 at T0+5min; assert downgrade applied at T0; auto-recover at T0+5min; bounded ≤ 5min false-downgrade.
    - 4. **Customer appeal wave**: 10k appeals at once; assert PriorityQueue por score; SLA ≤ 24h sustained; overflow alert SEV-3.
    - 5. **Per-tenant isolation race**: 1000 tenants concurrent score computation; assert no cross-tenant feature leak.
    - 6. **Audit emit fail mid-decision**: decision NOT applied; retry next cycle; eventual consistency.
    - 7. **S-09 metrics outage 30min**: AbuseError::MetricsLag; cron skips cycles; SEV-3 alert; recovery resumes.
    - 8. **Auto-suspend attempt** (programmatic): AutoSuspendForbidden error; SEV-1 alert; LGPD compliance preserved.
    - 9. **Weight rotation** (admin tunes weights via S-13): old version archived; new version active; scores recomputed using new weights; gradient applied consistently.
    - 10. **TenantCtx tampering**: signed JWT vs request body mismatch; middleware uses TenantCtx (S-03 inheritance Lote 10.4bis); other tenant's score untouched.
    - 11. **Cross-region cron coordination**: per-region DO `AbuseScoreCron`; tenant primary_region routing; no double-scoring; per-region serial.

### 6.2 Out-of-scope (deferred)

- ML-based abuse detection (anti-scope sprint contract §10; deferred S-14+).
- Multi-tenant comparative scoring (cross-tenant feature comparison; LINDDUN linkability concern; deferred).
- Real-time abuse scoring (sub-1min latency); 5min canonical OK initial.
- Customer-tunable abuse thresholds (deferred S-13 admin plane).
- Score normalization across plan tiers (Phase 2); initial: tier-aware baselines.
- Federated abuse detection cross-region (per-region independent; deferred S-14).

## 7. Anti-Scope

- ❌ Auto-suspend at any score (LGPD Art. 20 / GDPR Art. 22 violation; sprint contract §7.10.s08.3 humane).
- ❌ ML-based scoring (anti-scope sprint contract §10).
- ❌ Cross-tenant feature comparison (LINDDUN linkability).
- ❌ Opaque score (must disclose features + weights + contributions LGPD Art. 20).
- ❌ Hardcoded weights (use `abuse_calibration_weights` table; tunable).
- ❌ Skip per-tenant isolation property test (sprint contract §7.10.s08.4 explicit).
- ❌ Skip audit log every decision (LGPD compliance trail).
- ❌ TenantCtx bypass.
- ❌ `tokio::spawn` em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ Skip alarm re-arm AT START (Lote 10.4bis).
- ❌ Sliding-window scoring (cron-tick 5min canonical).

## 8. Acceptance Criteria (Gherkin) — 11 scenarios

```gherkin
Feature: Abuse Detection Heurística + Scoring + Humane Response

  Scenario: Typical workload score Noop
    Given tenant T (team plan) running normal Bazel build
    Given features: cpu=0.3, egress=10MB/min, entropy=4 bits, exec=20 (within tier baseline)
    When cron computes score
    Then score = 0.0 (all features below thresholds)
    Then tier=Noop; no action applied
    Then audit emit corelink.abuse.noop

  Scenario: Heavy ML training NOT false-positive
    Given tenant T running ML training; egress=5GB/min sustained 30min
    Given other features normal: cpu=0.4, entropy=3.5, exec=10
    When cron computes
    Then egress_norm=1.0; other features ~0
    Then score = 0.30×0 + 0.25×1.0 + 0.25×0 + 0.20×0 = 0.25
    Then tier=Noop (no false-positive single-feature)

  Scenario: Compute farming pattern silent downgrade
    Given tenant T compute farming; cpu=0.98 sustained, entropy=0.5 (1-2 unique actions), exec=50× baseline
    When cron computes
    Then score ≈ 0.60; tier=SilentDowngrade50pct1h
    Then rate cap × 0.5 applied for 1h
    Then customer email notification (S-13 staging stub OK)
    Then admin SEV-3 notification
    Then audit emit corelink.abuse.silent_downgrade

  Scenario: Combined-vector attack admin review
    Given tenant T multi-vector: cpu=0.99, egress=20× baseline, entropy=0.3, exec=80× baseline
    When cron computes
    Then score ≈ 0.93; tier=AdminReviewTriggerSev2
    Then SEV-2 PagerDuty admin notification
    Then admin reviews context; decides

  Scenario: Suspend candidate NEVER auto-applied
    Given tenant T extreme score=0.97
    When cron computes
    Then tier=SuspendCandidateHumanReviewOnly
    Then SEV-1 alert; admin manual review required
    Then NO programmatic auto-suspend (LGPD Art. 20 compliance)
    Then if any code attempts auto-suspend: AutoSuspendForbidden error + SEV-1 LGPD canary alert

  Scenario: Customer self-service score inspection
    Given tenant T currently downgraded (score=0.65)
    When customer GET /v1/admin/abuse_score (TenantCtx auth)
    Then AbuseScoreBreakdown returned: {score: 0.65, features: {cpu: 0.85, egress: 0.1, entropy: 1.5, exec: 5}, weights: {cpu: 0.30, ...}, contributions: [(cpu, 0.255), (egress, 0.025), (entropy, 0.375), (exec, 0.0)], current_tier: SilentDowngrade50pct1h, appeal_url: "/v1/admin/abuse_appeal"}
    Then audit emit corelink.abuse.score_inspected (LGPD compliance)

  Scenario: Customer appeal SLA ≤ 24h
    Given customer submits POST /v1/admin/abuse_appeal at T0
    When appeal stored
    Then sla_deadline_at = T0 + 24h
    Then admin notification SEV-2 (priority por score)
    Then if not reviewed by T0+24h: SEV-3 alert (humane SLA breach)
    Then reviewer must decide: approved | rejected | needs_more_info

  Scenario: Appeal approved → score reset
    Given appeal_id A pending; tenant T currently downgraded
    When admin POST /v1/admin/abuse/queue/{A}/approve {reset_score: true}
    Then abuse_appeals.decision='approved'; reviewed_at=now
    Then abuse_response_actions: reverted_at=now (downgrade lifted)
    Then audit emit corelink.abuse.appeal_approved
    Then customer notification (S-13 stub OK)

  Scenario: Per-tenant isolation property test
    Given 1000 tenants concurrent; varying features
    When cron computes for each
    Then each tenant's score independent (no cross-tenant feature comparison)
    Then INV-TENANT-ISOLATION verified em property test 100k iter
    Then cross_tenant_feature_leak_total metric = 0

  Scenario: Calibration target 50+50 synthetic workloads (Lote 10.8bis P0-E statistically rigorous)
    Given n=50 benign workloads (10× variations of typical Bazel + parallel CI + ML training + Docker rebuild + academic research; intra-class variance simulating real production diversity)
    Given n=50 high-intensity abusive workloads (10× variations of compute farming + scraping + spam + exec flood + multi-vector; high-intensity defined: cpu_ratio≥0.9, egress≥10× baseline, entropy≤1 bit, exec≥50× baseline)
    When cron computes scores
    Then ≤ 2 FP em 50 benign (95% CI upper-bound 9.6% acceptable; target ideal 0 FP em 50 → CI upper 5.7%)
    Then ≥ 40 TP em 50 high-intensity abusive (95% CI lower-bound 67%; target ≥ 45 TP → CI lower 78%)
    Then medium-intensity abuse (intermediate cases) acknowledged out-of-scope automated detection (Persona 3.1; sprint contract §6 DoD)
    Then statistical methodology validated by Data Scientist advisor sign-off
    Then sprint contract §6 DoD calibration target satisfied

  Scenario: S-09 metrics outage AbuseError::MetricsLag
    Given S-09 metrics endpoint unavailable 30min
    When cron alarm fires
    Then AbuseError::MetricsLag returned for affected tenants
    Then cycle skipped; SEV-3 alert (observability gap; not service breakage)
    Then on S-09 recovery; next cycle resumes; backfill possible
```

## 9. Design Decisions

- 9.1: Heurística NOT ML (sprint contract §10 anti-scope; transparency LGPD Art. 20).
- 9.2: 4 features specifically chosen (cpu_ratio + egress + entropy + exec) — covers compute farming + scraping + spam + flood.
- 9.3: Weighted-sum score [0.0, 1.0]; 4-tier response gradient.
- 9.4: NEVER auto-suspend (LGPD Art. 20 / GDPR Art. 22; sprint contract §7.10.s08.3).
- 9.5: 5min cron-tick aggregation (industry standard PagerDuty/Datadog).
- 9.6: Tier-aware baselines (5-tier canonical Lote 10.7bis P0-7 absorbed).
- 9.7: Customer self-service endpoints transparent (LGPD Art. 20 compliance).
- 9.8: Customer appeal SLA ≤ 24h human review.
- 9.9: Per-tenant isolation property test 100k (sprint contract §7.10.s08.4).
- 9.10: Calibration via 50+50 synthetic workloads (Lote 10.8bis P0-E corrected) (sprint contract §6 DoD).
- 9.11: Weights persistent em D1 `abuse_calibration_weights`; tunable via admin S-13.
- 9.12: Audit fail-closed (Lote 10.6bis absorbed).
- 9.13: NO new ADR (extends CTRL-RATE-001 + CAP-ABUSE-001/002 sprint contract canonical).

## 10. Completeness Criteria SOTA

- [ ] **10.s08.004.1** Module compila + integration tests green.
- [ ] **10.s08.004.2** All 11 Gherkin scenarios green.
- [ ] **10.s08.004.3** Property tests 7 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- [ ] **10.s08.004.4** Chaos suite 11 scenarios green.
- [ ] **10.s08.004.5** **Calibration target met**: 50+50 synthetic workloads (Lote 10.8bis P0-E corrected) em staging; 95% CI FP rate ≤ 5% upper-bound benign + 95% CI TP rate ≥ 80% lower-bound abusive (Lote 10.8bis P0-E) (sprint contract §6 DoD).
- [ ] **10.s08.004.6** Customer appeal SLA ≤ 24h sustained 7d.
- [ ] **10.s08.004.7** **Per-tenant isolation 100k property test** (sprint contract §7.10.s08.4 explicit).
- [ ] **10.s08.004.8** AutoSuspendForbidden enforced (0 attempts succeed em property test 100k).
- [ ] **10.s08.004.9** Customer self-service /v1/admin/abuse_score ≤ 200ms p99.
- [ ] **10.s08.004.10** Métricas (9) emitted; auto_suspend_attempts_total alerts SEV-1 if > 0.
- [ ] **10.s08.004.11** Cargo-audit + cargo-deny + clippy clean.
- [ ] **10.s08.004.12** Cost regression gate per-cycle ≤ $0.001/region/5min.
- [ ] **10.s08.004.13** **D1 migrations** (4 NEW tables; CHECK inline per Lote 10.5bis; D1 batch ≤250).
- [ ] **10.s08.004.14** LGPD audit trail: every decision audit logged (Compliance review).

## 11. DoD

- [ ] Module compila + tests green; all Gherkin/property/chaos green; 12 sign-offs (HIGH_RISK).

## 12. Invariants Validated

- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): per-tenant score; 100k property test (sprint contract §7.10.s08.4).
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): abuse detection complements bulkhead.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+): every decision audited.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| AbuseDetector module | `crates/corelink-abuse-detector/` | Rust |
| Cron DO impl | `crates/corelink-abuse-detector/src/cron_do.rs` | Rust |
| Admin endpoints (S-13 stub) | `crates/corelink-worker/src/admin/abuse.rs` | Rust |
| Tower middleware (apply tier action) | `crates/corelink-worker/src/middleware/abuse_response.rs` | Rust |
| D1 migrations | `migrations/00X_abuse_score_history.sql`, `migrations/00X_abuse_appeals.sql`, `migrations/00X_abuse_response_actions.sql`, `migrations/00X_abuse_calibration_weights.sql` | SQL |
| Property tests | `crates/corelink-abuse-detector/tests/prop_abuse.rs` | Rust |
| Chaos suite | `tests/chaos_abuse.rs` | Rust |
| Synthetic workloads | `tests/calibration_workloads/` (10 fixtures) | Rust |
| Wrangler DO binding | `wrangler.toml` (additions; `AbuseScoreCron-<region>`) | TOML |

## 14. Quality Standards SOTA

- 14.s08.004.1: Zero unsafe; zero unwrap em production paths.
- 14.s08.004.2: rustdoc 100% public API.
- 14.s08.004.3: Test coverage ≥ 90%.
- 14.s08.004.4: Score computation latency ≤ 1s p99 per tenant.
- 14.s08.004.5: Customer self-service endpoint ≤ 200ms p99.
- 14.s08.004.6: Calibration target validated em staging (sprint contract §6 DoD).
- 14.s08.004.7: SAST clean.
- 14.s08.004.8: Métricas (9 §6.1.10).
- 14.s08.004.9: Cost regression gate per-cycle ≤ $0.001/region/5min.
- 14.s08.004.10: TenantCtx-only (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); 5-tier canonical (Lote 10.7bis P0-7); column drift no `_ms` suffix (Lote 10.7bis P0-3); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis); chrono `corelink_time::utc_5min_floor()` for cron-tick alignment (NOT `tomorrow_at_utc_midnight()` — fabricated lineage rejected Lote 10.8bis P0-D).
- 14.s08.004.11: 100k nightly property test (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s08.004.12: Humane LGPD Art. 20 compliance: NEVER auto-suspend; transparent score breakdown; appeal SLA ≤ 24h.

## 15. Chaos Experiments (11)

§6.1.12 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (Compliance + Privacy mandatory emphatic for LGPD); sprint contract §6 DoD enforces.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Module skeleton + AbuseDetector trait | 1 |
| ST-002 | Score computation (4-feature weighted-sum) + tier-aware baselines | 2.5 |
| ST-003 | DO `AbuseScoreCron-<region>` cron-tick + alarm re-arm | 2 |
| ST-004 | 4-tier response gradient apply | 2 |
| ST-005 | Customer self-service endpoints (3) S-13 stubs | 2 |
| ST-006 | Admin endpoints (4) S-13 stubs | 2 |
| ST-007 | D1 migrations (4 new tables) | 1 |
| ST-008 | Métricas (9) emit | 1 |
| ST-009 | 50+50 synthetic workloads (Lote 10.8bis P0-E corrected) calibration (n=50 benign + n=50 high-intensity abusive; 95% CI methodology rigor) | 8 |
| ST-010 | Property tests (7 × 10k; 100k nightly) | 3.5 |
| ST-011 | Chaos suite (11) | 2.5 |
| ST-012 | Compliance review (LGPD Art. 20) | 1 |
| ST-013 | Privacy review (PII redaction em features) | 0.5 |

**Total**: ~24h. **PERT** O=20h M=22h P=32h: **~24h** (sprint contract estimate 20h; revised upward por: 10 calibration workloads + 4 migrations + LGPD compliance review).

## 18. Dependencies

- Hard: S-03 SEALED (TenantCtx middleware); WI-S08-001 SEALED (rate cap formula 5-tier canonical referenced for downgrade); ADR-0034 (PRR staffing waiver Compliance/Privacy mandatory).
- Hard: S-09 SEALED OR em paralelo (PromQL metrics; cron reads metrics S-09 endpoint).
- Soft: S-13 admin plane (customer + admin endpoints staging stubs OK; full sealing post-S-13); WI-S08-005 (RFC 9331 headers); WI-S08-006 (DASH-RATE consumes metrics).

## 19. Effort PERT: ~24h. ## 20. Time-boxing: 32h hard limit.

## 21. Observability

9 metrics §6.1.10. Trace span `abuse.{cron, score_compute, response_apply, appeal_submit, appeal_review, score_inspect}`.

## 22. Cost Analysis

- Per-cycle: ~$0.001 (DO read S-09 metrics + compute + D1 write).
- TCO 12m: 5 regions × 1 DO × 12 cycles/h × 24 × 365 × $0.001 = ~$525/yr — trivial.
- D1 storage: abuse_score_history ~ 100 bytes/row × 12 rows/h × 8760h × 100k tenants = 105 GB/yr; partition by tenant_id quarterly via S-13 retention policy.
- **Cost saved by abuse detection**: prevents extreme abuse scenarios that would cost orders of magnitude more (compute farming = R2 + Workers CPU cost + bandwidth cost).

## 23. API Contract

- Public: `AbuseDetector` trait + `AbuseScoreResult`, `AbuseScoreBreakdown`, `AbuseAppeal`, `AppealDecision`, `AbuseError` types; `#[non_exhaustive]`.
- HTTP customer: `GET /v1/admin/abuse_score`, `POST /v1/admin/abuse_appeal` (TenantCtx); LGPD compliance audit.
- HTTP admin: `GET /v1/admin/abuse/queue`, `POST /v1/admin/abuse/queue/{id}/{approve|reject|needs_more_info}` (AdminCtx).

## 24. Post-mortem Hooks

- Auto-suspend attempt detected (programmatic) → CRITICAL post-mortem; LGPD violation canary.
- INV-TENANT-ISOLATION violation (cross_tenant_feature_leak > 0) → CRITICAL.
- Calibration drift > 5% sustained → SEV-2; re-calibration trigger.
- Appeal SLA breach > 24h sustained → SEV-3; humane response process gap.
- False-positive rate > 0.1% benign workload → SEV-2; calibration review.
- Adversarial just-below-threshold pattern detected → no-blame post-mortem; threshold tuning.
- Customer reports flagged-incorrectly → no-blame post-mortem; calibration signal.

## 25. Rollback / Recovery

- Rollback: revert Tower middleware + cron DO disabled; abuse detection layer disabled (no enforcement; existing bulkhead camadas 1-4 backstop).
- Recovery: D1 score history durable; cron resumes from last cycle; alarm re-armed AT START.
- RTO ≤ 5min; RPO ≤ 5min.

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx middleware S-03; AdminCtx for admin endpoints.
- T(ampering): D1 audit append-only via S-04; weights versioned em abuse_calibration_weights.
- R(epudiation): audit fail-closed every decision.
- I(nformation disclosure): score breakdown transparent (LGPD compliance); features computed from existing observability — no new PII.
- D(enial of Service): abuse detection IS DoS mitigation.
- E(scalation of Privilege): per-tenant isolation; no cross-tenant features.

**LINDDUN** (LGPD/GDPR mandatory emphatic):
- L(inkability): per-tenant scoring; NO cross-tenant comparative scoring (LGPD/GDPR + sprint contract anti-scope).
- I(dentifiability): tenant_id em metrics; redact in logs (privacy_model.md).
- N(on-repudiation): audit append-only; LGPD compliance trail.
- D(etectability): customer self-service /v1/admin/abuse_score (transparency LGPD Art. 20).
- D(isclosure): customer can inspect features + weights + contributions (LGPD).
- U(nawareness): customer notified em downgrade (S-13 email staging stub OK).
- N(on-compliance): **LGPD Art. 20 + GDPR Art. 22 explicit compliance**: NEVER auto-suspend; human review ≤ 24h SLA; appeal mechanism; transparent breakdown.

## 27. Knowledge Transfer

Tech talk (2h): "S-08 Abuse Detection: Heurística Multi-Feature + LGPD Art. 20 Humane Response + Calibration"; doc `docs/dev/abuse-detection-architecture.md`; onboarding test 8 questions: heurística-NOT-ML rationale (sprint contract anti-scope + LGPD transparency), 4-feature choice rationale (compute/scrape/spam/flood coverage), 4-tier response gradient + NEVER auto-suspend (LGPD Art. 20), 5min cron-tick canonical, calibration target 0 FP + ≥80% TP em 10 workloads, customer self-service transparency LGPD, appeal SLA ≤ 24h, per-tenant isolation property test 100k.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Auto-suspend programmatic attempt (LGPD violation) | L | M | CRITICAL | L | LOW | AutoSuspendForbidden enforced; SEV-1 alert; property test 100k |
| R-002 | False-positive benign workload downgrade | M | M | MEDIUM | L | LOW | Calibration 10 workloads (sprint contract §6 DoD); 0 FP target |
| R-003 | False-negative abusive (TP < 80%) | M | M | MEDIUM | L | LOW | Calibration target ≥ 80% TP; periodic shadow re-calibration |
| R-004 | Adversary game weights | M | L | MEDIUM | L | LOW | Tunable weights via admin S-13; rotating + combined-feature score |
| R-005 | Per-tenant isolation leak | L | M | CRITICAL | L | LOW | Per-tenant DO; no cross-tenant scoring; property test 100k |
| R-006 | Customer appeal SLA breach > 24h | M | L | MEDIUM | L | LOW | PriorityQueue por score; alert > 24h; staffing escalation |
| R-007 | S-09 metrics outage | M | L | MEDIUM | L | LOW | AbuseError::MetricsLag; cycle skip; recovery on resume |
| R-008 | Calibration drift over time | H | L | MEDIUM | L | LOW | Periodic re-calibration; shadow mode |
| R-009 | LGPD audit trail gap | L | M | CRITICAL | L | LOW | Audit fail-closed; reconcile catches |
| R-010 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-011 | Cost regression cron-tick | L | L | LOW | L | LOW | §14.s08.004.9 gate |
| R-012 | Customer reputation harm (flagged-incorrectly) | M | M | HIGH | L | LOW | Self-service appeal + transparent breakdown + reset on approval |

## 29. Review Checkpoints

D+0 design (Architect; heurística + 4-feature choice); D+1 Compliance (LGPD Art. 20 alignment); D+2 Privacy (LINDDUN linkability + transparency); D+3 AppSec (TenantCtx + audit + AutoSuspendForbidden); D+4 code review; D+5 calibration validation 10 workloads; D+6 chaos validation; D+7 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — INV-TENANT-ISOLATION + AutoSuspendForbidden_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos + calibration validation 10 workloads + property test 100k_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — LGPD Art. 20 + GDPR Art. 22 humane response (sprint contract §7.10.s08.3); audit trail compliance_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — LINDDUN linkability + transparency (LGPD Art. 20)_ |
| 11 | Architect | _TBD; **mandatory** — heurística-NOT-ML rationale + 4-feature design_ |
| 12 | Data Scientist advisor | _**substitutes Crypto SME** — calibration methodology + weighted-sum scoring + threshold validation; statistical rigor; data-driven thresholds_ |

(Crypto SME N/A — no key material; substituted by Data Scientist advisor for statistical methodology rigor.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.8) | Criação WI-S08-004; HIGH_RISK; SOTA pós-Lote 10.7bis lessons absorbed: 5-tier canonical baselines (P0-7); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly property test + per-tenant isolation explicit (P1-3 + sprint contract §7.10.s08.4); audit fail-closed (Lote 10.6bis); alarm re-arm AT START (Lote 10.4bis); D1 batch ≤250 (Lote 10.5bis); CHECK inline (Lote 10.5bis); column drift no `_ms` suffix (P0-3). Humane LGPD Art. 20 + GDPR Art. 22 fully integrado (sprint contract §7.10.s08.3 alignment): NEVER auto-suspend + 3 customer-facing endpoints (transparency + appeal + audit) + 24h SLA. Heurística NOT ML (anti-scope §10). 4 features specifically chosen (compute/scrape/spam/flood). 4-tier response gradient. Calibration via 50+50 synthetic workloads (Lote 10.8bis P0-E corrected) (sprint contract §6 DoD). NEW migrations abuse_score_history + abuse_appeals + abuse_response_actions + abuse_calibration_weights. Data Scientist advisor substitutes Crypto SME (statistical methodology). |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.8bis) | R4+R5 review remediation: P0-E calibration scaled to n=50 benign + n=50 high-intensity abusive workloads + 95% CI bounded targets (≤2 FP em 50 → CI upper 9.6%; ≥40 TP em 50 → CI lower 67%); Persona 3 reasoning leak removed; Persona 3.1 explicitly acknowledged medium-intensity abuse out-of-scope automated detection (mitigation: 30d shadow re-calibration + ML deferred S-14+); P1-3 FM-251 → FM-201 canonical; CI-2 NEW shared D1 table `tenant_rate_override` for SilentDowngrade communication mechanism (WI-S08-001 reads override_multiplier; effective_refill_rate = plan * override; auto-revert on expiry); ST-009 PERT 3h → 8h (statistical methodology rigor required); aggregate score post-bis target ≥ 8.5/10 (R4 8.1 + R5 6.0 baselines). |

## 32. Anti-patterns evitados

- ❌ Auto-suspend (LGPD violation); ❌ ML-based scoring (anti-scope §10 + LGPD opacity); ❌ Cross-tenant feature comparison (LINDDUN linkability); ❌ Opaque score (LGPD Art. 20 violation); ❌ Hardcoded weights (use abuse_calibration_weights); ❌ Skip per-tenant isolation property test; ❌ Skip audit log every decision (LGPD trail); ❌ TenantCtx bypass; ❌ tokio::spawn em CF Workers; ❌ Skip alarm re-arm AT START; ❌ Sliding-window scoring (cron-tick canonical).

---

**Fim WI-S08-004.** Próximo: WI-S08-005 (Response code types + RFC 9331 RateLimit headers + global circuit breaker camada 4).
