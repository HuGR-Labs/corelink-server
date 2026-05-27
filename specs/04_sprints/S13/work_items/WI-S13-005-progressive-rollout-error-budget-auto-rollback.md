---
id: "WI-S13-005"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.2.0"
created: "2026-04-28"
updated: "2026-05-27"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-13"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s13", "admin-plane", "progressive-rollout", "auto-rollback", "error-budget", "canary", "high-risk"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-rollout-controller` was absorbed into `corelink-adapter-host` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-adapter-host-absorption.md. Canonical consumer path is now `corelink_adapter_host::*`.

# WI-S13-005 — Progressive Rollout Controller 4-Stage (1% → 10% → 50% → 100%) + Auto-Rollback Triggers (Error Rate > Baseline + 3σ OR SLO Burn > 14.4 OR p99 > Baseline + 50% Sustained 5 min) + PAT-PROGRESSIVE-ROLLOUT-001 + Rollback Budget 30% Monthly Cap + Chaos Test Bad Deploy Auto-Rollback ≤ 10 min p99

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-13](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S13-005 |
| Título | Progressive rollout controller implementing PAT-PROGRESSIVE-ROLLOUT-001: 4 stages (1% → 10% → 50% → 100%) com gate criteria per stage based on error budget burn rate; auto-rollback triggers (error rate > baseline + 3σ OR SLO burn > 14.4 1h-window OR p99 latency > baseline + 50% sustained 5 min); bad deploy detection p99 ≤ 10 min; rollback p99 ≤ 5 min; **rollback budget 30% monthly cap** — auto-rollback consumes ≤ 30% mensal error budget; excedeu = freeze deploys + SEV-2 alert (canonical safeguard against false-positive rollback flooding); chaos test bad deploy injection → auto-rollback ≤ 10 min sustained 30d staging |
| Sprint | S-13 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (bad deploy = blast radius all customers; auto-rollback bypass = sustained breakage; budget excedido = false-positive flood freezing legitimate deploys catastrophic) |

## 1. Intent

Implementar progressive rollout controller que mitiga **FM-200** (mau release atinge todos simultaneamente): (1) **4-stage progression** 1% → 10% → 50% → 100% via Cloudflare gradual deploy + monitoring hooks; (2) **gate criteria per stage** — advance only se error budget burn rate < baseline (Google SRE Workbook Ch 16 pattern); (3) **auto-rollback triggers** combinados (any of):
- Error rate > baseline + 3σ.
- SLO burn-rate > 14.4 (1h window per Google SRE Workbook).
- p99 latency > baseline + 50%.

Sustained 5 min → auto-rollback to previous version + SEV-2 alert; (4) **detection p99 ≤ 10 min** (chaos test target); **rollback p99 ≤ 5 min**; (5) **rollback budget cap 30% monthly** — auto-rollback consumes error budget; if cumulative consumption > 30% mês = freeze deploys + SEV-2 alert (PROTECTS against false-positive rollback flood); manual override available with Architect + Security lead approval; (6) chaos test bad deploy injection → auto-rollback ≤ 10 min sustained 30d staging (DoD 10.s13.5).

```rust
// File: crates/corelink-rollout-controller/src/lib.rs

#![forbid(unsafe_code)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RolloutStage {
    Stage1Pct,                     // 1% traffic
    Stage10Pct,                    // 10% traffic
    Stage50Pct,                    // 50% traffic
    Stage100Pct,                   // 100% traffic
}

impl RolloutStage {
    pub fn next(&self) -> Option<RolloutStage> {
        match self {
            Self::Stage1Pct => Some(Self::Stage10Pct),
            Self::Stage10Pct => Some(Self::Stage50Pct),
            Self::Stage50Pct => Some(Self::Stage100Pct),
            Self::Stage100Pct => None,                  // terminal
        }
    }
    pub fn traffic_pct(&self) -> u8 {
        match self {
            Self::Stage1Pct => 1,
            Self::Stage10Pct => 10,
            Self::Stage50Pct => 50,
            Self::Stage100Pct => 100,
        }
    }
    pub fn dwell_minutes_min(&self) -> u32 {
        // minimum dwell time before advancing (hi-fi metric collection)
        match self {
            Self::Stage1Pct => 15,                      // 15 min minimum at 1%
            Self::Stage10Pct => 30,                     // 30 min minimum at 10%
            Self::Stage50Pct => 60,                     // 60 min minimum at 50%
            Self::Stage100Pct => 0,                     // terminal; no dwell
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RolloutStatus {
    Pending,                       // deploy queued
    Active,                        // rolling
    Completed,                     // 100% reached
    AutoRolledBack,                // auto-rollback triggered
    ManuallyAborted,               // admin abort
    BudgetFrozen,                  // monthly budget exceeded
}

#[async_trait]
pub trait RolloutController: Send + Sync {
    /// Initiate new rollout (starts at Stage1Pct).
    async fn start(&self, deploy_artifact: &DeployArtifact) -> Result<RolloutHandle, RolloutError>;

    /// Probe current stage health; advance if gate criteria met.
    async fn probe_and_advance(&self, handle: &RolloutHandle) -> Result<RolloutDecision, RolloutError>;

    /// Trigger auto-rollback (PAT-PROGRESSIVE-ROLLOUT-001 + PAT-ROLL-FORWARD-001 reused).
    async fn auto_rollback(
        &self,
        handle: &RolloutHandle,
        trigger: AutoRollbackTrigger,
    ) -> Result<RolloutHandle, RolloutError>;

    /// Manual abort (admin override).
    async fn abort(&self, handle: &RolloutHandle, actor: &AdminActor) -> Result<RolloutHandle, RolloutError>;

    /// Current monthly error budget consumption ratio (0..=1.0).
    async fn monthly_budget_consumed(&self) -> Result<f64, RolloutError>;
}

#[derive(Debug, Clone)]
pub struct RolloutDecision {
    pub current_stage: RolloutStage,
    pub next_action: NextAction,
    pub gate_metrics: GateMetrics,
}

#[derive(Debug, Clone)]
pub enum NextAction {
    Advance(RolloutStage),
    Hold(String),                  // reason
    AutoRollback(AutoRollbackTrigger),
    Complete,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AutoRollbackTrigger {
    ErrorRateExceedsBaseline3Sigma,
    SloBurnRateExceeds14_4,
    P99LatencyExceedsBaseline50Pct,
}

#[derive(Debug, Clone)]
pub struct GateMetrics {
    pub error_rate: f64,
    pub error_rate_baseline: f64,
    pub error_rate_sigma: f64,
    pub slo_burn_rate_1h: f64,
    pub p99_latency_ms: f64,
    pub p99_baseline_ms: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum RolloutError {
    #[error("deploy artifact missing Cosign signature (S-12 INV-SUPPLY-SIGNED-DEPLOY)")]
    UnsignedDeploy,
    #[error("monthly rollback budget exceeded ({consumed:.2} > 30%); deploys frozen")]
    BudgetExceeded { consumed: f64 },
    #[error("rollout in-flight (handle {0}); concurrent start blocked")]
    RolloutInFlight(uuid::Uuid),
    #[error("Cloudflare gradual deploy API error: {0}")]
    CloudflareApi(String),
    #[error("storage error: {0}")]
    Storage(String),
}
```

D1 schema:
```sql
CREATE TABLE rollout_state (
    handle_id BLOB(16) PRIMARY KEY,
    deploy_artifact_sha256 BLOB(32) NOT NULL,
    cosign_signature_url TEXT NOT NULL,            -- INV-SUPPLY-SIGNED-DEPLOY (S-12)
    rekor_log_index BIGINT NOT NULL,                -- INV-SUPPLY-PROVENANCE-IN-REKOR (S-12)
    current_stage TEXT NOT NULL CHECK (current_stage IN ('stage_1pct', 'stage_10pct', 'stage_50pct', 'stage_100pct')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'active', 'completed', 'auto_rolled_back', 'manually_aborted', 'budget_frozen')),
    started_at_ms BIGINT NOT NULL,
    stage_entered_at_ms BIGINT NOT NULL,
    completed_at_ms BIGINT,
    rollback_trigger TEXT,                          -- if status = auto_rolled_back
    rollback_at_ms BIGINT,
    actor_user_id BLOB(16) NOT NULL,
    UNIQUE (status) WHERE status = 'active'         -- only 1 active rollout at a time per env
);

CREATE TABLE rollout_budget_consumption (
    month_yyyy_mm TEXT PRIMARY KEY,                 -- "2026-04"
    rollback_count_total INTEGER NOT NULL DEFAULT 0,
    error_budget_consumed_ratio REAL NOT NULL DEFAULT 0.0,
    last_updated_at_ms BIGINT NOT NULL
);
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Progressive rollout é boundary cripto-load-bearing operacional: bad deploy without progressive rollout = 100% blast radius (todos os customers afetados simultaneously); cripto-touching changes (auth, audit chain, secret rotation) catastrophic. Reference: Google SRE Workbook Ch 16 (canarying releases) + AWS Cell-based Architecture (cell-by-cell rollout). FM-200 é P1 priority.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Bypass progressive stages** (deploy direct 100%): admin hits API to skip stages. Mitigação: rollout API enforces stage progression; skip = 403; audit emit; auto-rollback if direct 100% detected; this is a destructive admin op gated WI-S13-002 dual-approval.

2. **Auto-rollback false-positive flood**: thresholds too tight; minor variance triggers cascade rollbacks; deploys frozen unnecessarily. Mitigação: 5 min sustained threshold (not instant); 30% monthly budget cap protects against flood; manual override available with Architect + Security lead approval; thresholds tuned via ADR.

3. **Auto-rollback false-negative**: thresholds too loose; bad deploy passes gates; sustained breakage. Mitigação: 3 independent triggers (error rate + SLO burn + p99) — any triggers rollback; chaos test bad deploy injection sustained 30d staging validates thresholds.

4. **Rollback budget cap creates DoS**: legitimate good deploys frozen because previous false-positives consumed budget. Mitigação: monthly cap 30% allows ~3 rollbacks/mês; manual override via Architect + Security lead approval; post-mortem required for false-positive (root cause threshold tuning).

5. **Stage advance race condition**: 2 controllers concurrently advance same rollout. Mitigação: D1 UNIQUE constraint `(status='active')`; controller singleton via DO `idFromName("rollout-controller-{env}")`; concurrent start blocked.

6. **Cloudflare gradual deploy API outage**: rollout controller can't progress; stuck mid-rollout. Mitigação: stage stuck > 1h → SEV-3 alert; manual override via `wrangler deploy` rollback; runbook RB-ROLLOUT-STUCK.md.

7. **Bad deploy at 100% (post-rollout)**: rollout completed at 100%; degradation observed afterward. Mitigação: post-rollout monitoring continues 24h; auto-rollback can trigger even at 100% stage; budget consumption applies.

8. **Cosign signature missing on rollout artifact**: rollout starts on unsigned deploy. Mitigação: `RolloutError::UnsignedDeploy` rejection; INV-SUPPLY-SIGNED-DEPLOY herdada S-12; CF deploy webhook gate (S-12 WI-003) blocks at deploy time.

9. **Chaos test threshold tuning gap**: chaos test thresholds ≠ production; false-confidence. Mitigação: chaos test sustained 30d staging in production-equivalent infrastructure (DoD 10.s13.5 *(GA Evidence Gate D+45)*).

**Atacante adversarial scenarios**:

- **Inject error spike to trigger DoS**: attacker creates synthetic error spike to consume rollback budget. Mitigação: error rate baseline computed from rolling 7d window (attack must sustain large fraction); budget cap 30% prevents complete freeze; SEV-2 alert + investigation.

- **Bypass dual-approval for rollout abort**: attacker tries direct admin abort. Mitigação: `RolloutController::abort` requires AdminOpType `RolloutAbort` via WI-S13-002 dual-approval gate.

- **Race condition in stage advance**: attacker triggers concurrent advance. Mitigação: D1 UNIQUE active constraint + DO singleton.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: bad deploy = 100% blast radius if no progressive rollout; cripto-touching changes catastrophic.
- **Reversibility**: auto-rollback ≤ 5 min; mas downstream side-effects (e.g., schema migration) may require manual reconciliation.

11 sign-offs canonical incl. SRE Lead (canary methodology + chaos test) + Architect (state machine + budget cap) + Security Lead (Cosign signature gate + dual-approval composition).

## 3. Customer Impact & Journey

**Persona 1 — SRE on-call em prospect enterprise (RFP)**:
- Evidence: chaos test runs 30d staging com bad deploy injection → auto-rollback ≤ 10 min p99 verified.
- Diferenciador: BuildBuddy/NativeLink lack progressive rollout; CoreLink S-13 = Google SRE Workbook Ch 16 + AWS Cell-based + 4-stage + budget cap.

**Persona 2 — Auditor SOC 2 Type II + ISO 27001**:
- Evidence: D1 `rollout_state` + `rollout_budget_consumption` audit log; SOC 2 CC8.1 (system change management) + ISO 27001 A.5.18 (access provisioning).
- PRR doc demonstrates threshold tuning via ADR + chaos test methodology.

**Persona 3 — Internal admin executing deploy**:
- Workflow: PR merged + Cosign signed (S-12 WI-003) → admin POST /v1/admin/ops with op_type=`DeployStartRollout` (composed WI-S13-002 dual-approval) → rollout starts at 1% → controller advances based on gate criteria.
- Rollout dashboard: real-time stage gauge + auto-rollback events + budget consumption.

**SLA addendum**:
- Bad deploy detection p99: ≤ 10 min.
- Rollback p99: ≤ 5 min.
- Stage progression cadence: 1% (15 min min) → 10% (30 min) → 50% (60 min) → 100%.
- Monthly rollback budget: 30% error budget cap.
- Chaos test cadence: monthly (sustained 30d staging).
- Auto-rollback budget excedido: freeze deploys + SEV-2.

## 4. Capability Mapping

- **CAP-ADMIN-005** (Progressive rollout orchestrator) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.5` + `resilience_patterns.md §3.7 (PAT-PROGRESSIVE-ROLLOUT-001 + PAT-ROLL-FORWARD-001)` + `failure_modes.md FM-200` + `slo_catalog.md SLO-* (1h burn rate ≤ 14.4 per Google SRE Workbook)`.

## 5. Tipo

Background controller + DO state + Cloudflare gradual deploy integration; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-rollout-controller/`**:
   - `RolloutController` trait + impl `RolloutControllerImpl`.
   - State machine driver D1-backed.
   - Probe interval: 60s during active rollout.
   - Cloudflare gradual deploy API integration (`wrangler` API or REST).

2. **DO `RolloutControllerSingletonDO`** (per environment: staging + prod):
   - Single-instance per environment prevents concurrent rollouts.
   - Storage: current rollout handle + state.
   - Methods: `start`, `probe_and_advance`, `auto_rollback`, `abort`.

3. **Auto-rollback driver**:
   - 3 independent triggers (any triggers rollback):
     - Error rate > baseline + 3σ (rolling 7d baseline).
     - SLO burn-rate > 14.4 (1h window per Google SRE Workbook).
     - p99 latency > baseline + 50% (rolling 7d baseline).
   - Sustained 5 min threshold (filters transient).
   - Rollback action: revert to previous version via `wrangler deploy --version <prev>`.

4. **Monthly budget cap (30%) — measured burn enforcement (Lote 10.13 codex P1 fix)**:
   - D1 `rollout_budget_consumption` table tracks per-month rollback events com **measured burn** (não estimated 5%):
     - Per-rollback row: `rollback_started_ts`, `rollback_completed_ts`, `error_count_consumed` (real count of 5xx/SLO-violation events durante rollback window — extraído via observability_model métrica `corelink_http_errors_5xx_total` + `corelink_slo_burn_rate`), `budget_consumed_bps` (basis points, computed = error_count_consumed / monthly_error_budget_target × 10000).
   - **Continuous enforcement** (não só pre-start): rollout controller subscribes to métrica updates a cada 60s; recompute `consumed_ratio_month_to_date = SUM(budget_consumed_bps) / 3000`. Se `consumed_ratio > 1.0` (i.e., > 30% of monthly budget consumed YTD) durante in-flight rollout → **abort current stage advancement + freeze further rollouts** + SEV-2 alert (não só pre-start gate).
   - Recompute window: 30d sliding (não calendar month — evita "reset gaming" no 1º dia do mês).
   - Manual override available com Architect + Security lead approval (waiver + ADR documenting accepted residual error budget overconsumption).
   - Property test 10k: simula sequência de N auto-rollbacks com error_count varying; assert (a) cumulative consumed_ratio computed correctly via measured (não estimated) bps; (b) freeze trigger fires at exact threshold > 1.0; (c) post-freeze further rollout start = `BudgetExceeded` rejection.

5. **D1 schema migration** (`rollout_state` + `rollout_budget_consumption`):
   - Per-rollout audit log.
   - Per-month budget tracker.
   - UNIQUE active constraint prevents concurrent.

6. **Métricas underscored Prometheus** (per `observability_model.md §3.1 + §4.1`; label `plan` aplicável):
   - `corelink_admin_rollout_stage_gauge{stage,outcome}` (gauge; stage ∈ stage_1pct|stage_10pct|stage_50pct|stage_100pct; outcome ∈ active|complete|rolled_back).
   - `corelink_admin_rollout_auto_rollback_total{trigger,plan}` (counter; trigger ∈ error_rate|slo_burn|p99_latency).
   - `corelink_admin_rollout_budget_consumed_ratio{plan}` (gauge; alert > 0.30).
   - `corelink_admin_rollout_detection_duration_seconds_bucket` (histogram; SLO ≤ 10 min p99).
   - `corelink_admin_rollout_rollback_duration_seconds_bucket` (histogram; SLO ≤ 5 min p99).
   - `corelink_admin_rollout_dwell_seconds{stage}` (histogram; per stage minimum dwell).
   - `corelink_admin_rollout_freeze_total{reason}` (counter; reason ∈ budget_exceeded|manual_override|chaos_test).

7. **Observability** — trace spans `admin.rollout.{start,probe,advance,auto_rollback,complete,abort}` com attributes:
   - `rollout.handle_id` (UUID).
   - `rollout.current_stage` (enum).
   - `rollout.next_action` (enum).
   - `rollout.gate_metrics.error_rate` (f64).
   - `rollout.gate_metrics.slo_burn_rate` (f64).
   - `rollout.gate_metrics.p99_ms` (f64).
   - `result` (enum).

8. **Audit emission** (CloudEvent per state transition):
   - `corelink.admin.rollout.{stage_advance|auto_rollback|complete|abort|budget_freeze}`.
   - Atomic batch with D1 UPDATE.

9. **Adversarial regression tests**:
   - Inject error rate baseline + 3σ sustained 5 min → auto-rollback triggers ≤ 10 min p99.
   - Inject SLO burn-rate > 14.4 sustained → auto-rollback.
   - Inject p99 latency > baseline + 50% sustained → auto-rollback.
   - Synthesize 30% budget consumption + 4th rollback attempt → BudgetExceeded.
   - Bypass progressive stages (deploy direct 100% via API) → 403 + audit.
   - Cosign signature missing → UnsignedDeploy rejection.
   - Concurrent rollout start same env → RolloutInFlight rejection.
   - Cloudflare gradual deploy API outage → SEV-3 alert + manual override path.

10. **Property tests** (10k iter PR + 100k iter nightly):
    - `prop_rollout_stage_progression`: 10k random gate metrics; assert advance only when criteria met; never skip.
    - `prop_rollout_auto_rollback_triggers`: 10k random metric combinations; verify triggers fire correctly per spec contract §5.5.
    - `prop_rollout_budget_cap_enforced`: synthesize budget consumption sequences; assert > 30% = freeze.
    - `prop_rollout_concurrent_blocked`: 1000 concurrent start attempts; exactly 1 succeeds.

11. **Integration test E2E** (chaos test sustained 30d staging — DoD 10.s13.5):
    - Real bad deploy injection: deploy artifact com synthetic 5% error rate increment.
    - Verify rollout starts at 1%; controller probes; auto-rollback triggers at ≥ baseline + 3σ; rollback ≤ 10 min p99 sustained.
    - Repeat monthly during 30d observation window.

12. **`docs/internal/admin-plane.md` extended** (rollout section):
    - 4-stage progression diagram.
    - Auto-rollback decision tree.
    - Budget cap rationale.
    - Manual override procedure.

### 6.2 Out-of-scope (deferred)

- **DO config-singleton**: WI-S13-001.
- **Admin API + dual-approval**: WI-S13-002 (composed for `DeployStartRollout` + `RolloutAbort` op types).
- **Secret rotation worker**: WI-S13-003.
- **Terraform drift detection**: WI-S13-004.
- **Property tests + RB dry-runs + PRR doc**: WI-S13-006.
- **Multi-region rollout coordination**: pós-GA enterprise (current = single env at a time).
- **Custom rollout strategies** (blue-green, A/B): pós-GA; current = canary only.
- **Customer-facing rollout dashboard**: S-16 admin UI.
- **Auto-tune thresholds via ML**: pós-GA; current = ADR-tuned manual.
- **Cell-based architecture full implementation**: pós-GA enterprise.

## 7. Anti-Scope

- Skip 4-stage progression (bypass = 100% blast radius).
- Skip auto-rollback (sustained breakage).
- Skip rollback budget cap (false-positive flood freezing legitimate deploys).
- Skip Cosign signature gate (INV-SUPPLY-SIGNED-DEPLOY herdada S-12 mandatory).
- Concurrent rollouts same env (DO singleton + D1 UNIQUE).
- Override budget cap sem Architect + Security lead approval + ADR.
- Bypass progressive stages via direct API.
- Skip chaos test sustained 30d staging.
- Auto-tune thresholds em production sem ADR.
- Direct DO write bypass (Worker binding only).
- Skip post-rollout monitoring 24h at 100%.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Progressive rollout controller + auto-rollback + budget cap

  Background:
    Given DO RolloutControllerSingletonDO per env operational
    Given Cloudflare gradual deploy API wired
    Given metrics + audit chain pipeline operational
    Given Cosign signature gate (S-12 WI-003) operational

  Scenario: Successful 4-stage progression
    Given valid Cosign-signed deploy artifact
    When admin starts rollout (composed dual-approval)
    Then controller transitions Stage1Pct (15 min dwell) → Stage10Pct (30 min) → Stage50Pct (60 min) → Stage100Pct
    And gate metrics within baseline at each stage
    And metric corelink_admin_rollout_stage_gauge{stage="stage_100pct",outcome="complete"} = 1
    And audit "admin.rollout.complete" emitted
    And total wall time ≈ 105 min minimum

  Scenario: Auto-rollback on error rate exceeding baseline + 3σ
    Given rollout active at Stage10Pct
    When error rate > baseline + 3σ sustained 5 min
    Then auto_rollback triggers within 5 min after threshold met
    And rollback completes ≤ 5 min p99
    And total detection-to-rollback ≤ 10 min p99
    And metric corelink_admin_rollout_auto_rollback_total{trigger="error_rate"} incremented
    And SEV-2 alert fires
    And audit "admin.rollout.auto_rollback" emitted with trigger=ErrorRateExceedsBaseline3Sigma

  Scenario: Auto-rollback on SLO burn-rate > 14.4
    Given rollout active at Stage50Pct
    When SLO burn-rate > 14.4 (1h window) sustained 5 min
    Then auto_rollback triggers
    And metric corelink_admin_rollout_auto_rollback_total{trigger="slo_burn"} incremented

  Scenario: Auto-rollback on p99 latency > baseline + 50%
    Given rollout active at Stage50Pct
    When p99 > baseline + 50% sustained 5 min
    Then auto_rollback triggers
    And metric corelink_admin_rollout_auto_rollback_total{trigger="p99_latency"} incremented

  Scenario: Budget cap exceeded → freeze deploys
    Given monthly rollback budget consumed = 0.32 (32%)
    When admin attempts new rollout start
    Then RolloutError::BudgetExceeded { consumed: 0.32 } returned
    And response 403
    And metric corelink_admin_rollout_freeze_total{reason="budget_exceeded"} incremented
    And SEV-2 alert fires
    And admin must wait for next month OR Architect + Security lead override (waiver + ADR)

  Scenario: Bypass progressive stages rejected
    When admin attempts deploy direct 100% via API
    Then 403 (rollout API enforces stage progression)
    And audit "admin.rollout.bypass_attempted" emitted

  Scenario: Cosign signature missing rejected
    Given deploy artifact without Cosign signature
    When admin attempts rollout start
    Then RolloutError::UnsignedDeploy returned
    And response 403

  Scenario: Concurrent rollout same env blocked
    Given rollout in-flight (handle H1, env=staging)
    When second rollout attempt env=staging
    Then RolloutError::RolloutInFlight(H1) returned
    And D1 UNIQUE constraint enforces

  Scenario: Manual abort via dual-approval
    Given rollout active at Stage10Pct
    When admin POST /v1/admin/ops body={op_type: "RolloutAbort", handle_id: H1}
      And X-Dual-Approver: B + valid HMAC (composed WI-S13-002)
    Then dual-approval verified
    And rollout aborted; previous version restored
    And metric corelink_admin_rollout_freeze_total{reason="manual_override"} incremented
    And audit "admin.rollout.abort" emitted

  Scenario: Cloudflare gradual deploy API outage
    Given Cloudflare API returns 503 sustained 1h during rollout
    When controller tries to advance stage
    Then SEV-3 alert fires (stage stuck)
    And manual override path available via wrangler deploy rollback
    And runbook RB-ROLLOUT-STUCK.md invoked

  Scenario: Post-rollout monitoring 24h at 100%
    Given rollout completed at Stage100Pct
    When metrics monitored sustained 24h post-completion
    Then auto-rollback can trigger at any time during 24h
    And budget consumption applies

  Scenario: Chaos test bad deploy auto-rollback ≤ 10 min sustained 30d staging
    Given chaos test injects bad deploy weekly during 30d staging window
    When test runs
    Then auto-rollback triggers ≤ 10 min p99 each iteration
    And report committed em specs/_audits/2026-XX-XX-rollout-chaos-30d.md
```

## 9. Design Decisions

### 9.1 Why 4 stages 1%/10%/50%/100% (NÃO 3 stages)

- 1%: lowest blast radius; surfaces obvious bugs.
- 10%: validates at 10× scale; surfaces ratio-based bugs.
- 50%: validates at majority scale; surfaces capacity bugs.
- 100%: terminal; full traffic.
- 3-stage (skip 50%) ratio jump 10× → 10× (10%→100%) too coarse; 4-stage tested em Google SRE Workbook + AWS Cell-based.
- ADR override available with explicit risk acceptance (per spec contract waiver §19).

### 9.2 Why dwell minimum per stage (15/30/60 min)

- Lower stages = less data; longer dwell needed for hi-fi metric collection.
- Higher stages = more data; shorter dwell sufficient.
- Bounded total ~105 min minimum; balances safety vs deploy velocity.

### 9.3 Why 3 independent auto-rollback triggers

- Error rate alone misses latency degradation (e.g., timeout → no error).
- SLO burn alone misses error spike at sub-1h window.
- p99 alone misses reliability.
- 3 independent triggers (any) covers orthogonal failure modes.
- Sustained 5 min filter mitigates false-positive (transient).

### 9.4 Why SLO burn-rate threshold 14.4 (1h window)

- Google SRE Workbook Ch 16 canonical: 14.4× burn rate em 1h window = depleting 1 mês error budget em ~70 min.
- Industry standard; not invented number.

### 9.5 Why 30% monthly budget cap

- Each auto-rollback consumes ~5% error budget (estimated; tuned via ADR).
- 30% cap allows ~6 rollbacks/mês; protects against false-positive flood.
- Manual override path available for legitimate edge cases.

### 9.6 Why DO singleton per environment (NÃO global)

- Concurrent rollouts same env = race condition on traffic shifting.
- Per-env scope allows staging + prod parallel rollouts.
- DO `idFromName("rollout-controller-{env}")` ensures singleton.

### 9.7 Why Cloudflare gradual deploy API (NÃO custom traffic shifting)

- Cloudflare platform-native; integrated with Workers + Versions.
- No custom load balancer required.
- Traffic shifting atomic per-cell (Cloudflare edge).

### 9.8 Why audit emission per state transition

- INV-AUDIT-APPEND-ONLY herdada; rollout = audit-grade state machine.
- Compliance evidence: SOC 2 CC8.1 (system change management).

### 9.9 Why ADR potencial?

- Sim — **ADR-XXXX**: "Progressive rollout controller 4-stage + auto-rollback PAT-PROGRESSIVE-ROLLOUT-001 + monthly budget cap 30%". Decisão arquitetural foundational; reuse pattern em S-17 chaos engineering forward.
- Whitelist em `validate_references.py` até materializar.

## 10. Completeness Criteria SOTA

- [ ] **10.s13.005.1** Property test 10k iter (PR) + 100k iter (nightly) `prop_rollout_*` (4 props) → 0 violations (EVT-002).
- [ ] **10.s13.005.2** Adversarial test 8 scenarios (3 triggers + budget exceeded + bypass + Cosign + concurrent + Cloudflare outage) → 100% mitigation (EVT-040).
- [ ] **10.s13.005.3** E2E chaos test bad deploy injection sustained 30d staging → auto-rollback ≤ 10 min p99 (DoD 10.s13.5 *(GA Evidence Gate D+45)*).
- [ ] **10.s13.005.4** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) em rollout controller crate (EVT-002).
- [ ] **10.s13.005.5** SLO-ADMIN-ROLLBACK-RECOVERY: detection ≤ 10 min p99 + rollback ≤ 5 min p99 sustained 30d staging *(GA Evidence Gate D+45)*.
- [ ] **10.s13.005.6** OWASP ASVS V14 (configuration) 100% checklist pass (EVT-002).
- [ ] **10.s13.005.7** Cost regression gate: rollout controller infra ≤ $20/mês.
- [ ] **10.s13.005.8** Monthly budget consumption tracked accurately D1; freeze enforced (EVT-022).
- [ ] **10.s13.005.9** Cosign signature gate composed S-12 WI-003 (UnsignedDeploy rejection).
- [ ] **10.s13.005.10** Google SRE Workbook Ch 16 + AWS Cell-based attestation em PRR doc (EVT-031).

## 11. DoD

- [ ] Crate `corelink-rollout-controller` compila.
- [ ] DO `RolloutControllerSingletonDO` per environment operational.
- [ ] All 12 Gherkin scenarios green.
- [ ] Property tests 4 props × 10k iter green em PR; 100k iter green em nightly.
- [ ] Adversarial regression tests 8 scenarios green.
- [ ] E2E chaos test sustained 30d staging green (incremental during observation window).
- [ ] D1 schema migration applied (`rollout_state` + `rollout_budget_consumption`).
- [ ] CloudEvent audit emission per state transition verified.
- [ ] Métricas emitidas (7 listadas §6.1.6).
- [ ] Trace spans em OTel pipeline.
- [ ] rustdoc + 3 examples (start rollout, manual abort, budget exceeded handling).
- [ ] `docs/internal/admin-plane.md` extended (rollout section).
- [ ] ADR-XXXX (rollout controller + budget cap) escrito + ratificado.
- [ ] Code review (Architect + Security Lead + SRE Lead).
- [ ] PRR Architect mini-sign-off (ship gate é WI-S13-006).
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- **INV-SUPPLY-SIGNED-DEPLOY** (HIGH — registry §3.12 herdada S-12): rollout starts only on Cosign-signed artifacts.
- **INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH — registry §3.12 herdada S-12): rollout artifact has Rekor inclusion proof.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL — registry §3.6 herdada S-09): rollout state transitions append-only.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — herdada S-03): D1 atomic batch.

### Novas

Nenhuma (rollout controller = operational pattern; sem novel runtime invariant).

TLA+ alignment: não-aplicável (state machine D1-backed; integrates with `audit_immutability.tla` herdada).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| RolloutController trait + impl | `crates/corelink-rollout-controller/src/lib.rs` | Rust |
| State machine + DO singleton | `crates/corelink-rollout-controller/src/state_machine.rs` | Rust |
| Auto-rollback driver | `crates/corelink-rollout-controller/src/auto_rollback.rs` | Rust |
| Budget cap enforcer | `crates/corelink-rollout-controller/src/budget.rs` | Rust |
| Cloudflare gradual deploy adapter | `crates/corelink-rollout-controller/src/cf_gradual_deploy.rs` | Rust |
| D1 migration | `migrations/0XX_rollout_state_and_budget.sql` | SQL |
| Property tests | `crates/corelink-rollout-controller/tests/prop_rollout.rs` | Rust |
| Adversarial regression tests | `crates/corelink-rollout-controller/tests/adversarial.rs` | Rust |
| E2E chaos test (30d sustained) | `tests/e2e_rollout_chaos_30d.rs` | Rust |
| ADR-XXXX (rollout controller + budget cap) | `specs/03_architecture/adrs/ADR-XXXX-progressive-rollout-budget-cap.md` | Markdown |
| RB-ROLLOUT-STUCK runbook | `specs/05_runbooks/RB-ROLLOUT-STUCK.md` | Markdown |
| Examples | `crates/corelink-rollout-controller/examples/` (start_rollout.rs, manual_abort.rs, budget_exceeded.rs) | Rust |

## 14. Quality Standards SOTA

- **14.s13.005.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s13.005.2** rustdoc 100% public API + 3 examples.
- **14.s13.005.3** Test coverage ≥ 90%.
- **14.s13.005.4** Latência: detection ≤ 10 min p99; rollback ≤ 5 min p99.
- **14.s13.005.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings` clean.
- **14.s13.005.6** Métricas RED + per-trigger breakdown + budget gauge.
- **14.s13.005.7** Runbook RB-ROLLOUT-STUCK committed.
- **14.s13.005.8** Breaking changes em `RolloutStage` enum = bump major + ADR.
- **14.s13.005.9** Memory bounded em controller loop.
- **14.s13.005.10** Cost regression gate em CI.
- **14.s13.005.11** SLO ≤ 10 min p99 detection + ≤ 5 min p99 rollback enforced em CI benchmark.
- **14.s13.005.12** Google SRE Workbook Ch 16 + AWS Cell-based attestation.

## 15. Chaos Experiments

1. **Bad deploy auto-rollback (PRIMARY chaos test, sustained 30d staging)**: weekly bad deploy injection; verify auto-rollback triggers ≤ 10 min p99; report committed.

2. **Error rate trigger**: synthesize error rate baseline + 3σ sustained 5 min; verify auto_rollback triggers via `ErrorRateExceedsBaseline3Sigma`.

3. **SLO burn trigger**: synthesize burn rate > 14.4 (1h window) sustained; verify trigger.

4. **p99 latency trigger**: synthesize p99 > baseline + 50% sustained; verify trigger.

5. **Budget cap freeze**: synthesize 30% consumption sequence; verify freeze + manual override path.

6. **Bypass progressive stages**: red team tries direct 100% deploy via API; verify 403 + audit.

7. **Concurrent rollout**: 2 simultaneous start same env; D1 UNIQUE rejects.

8. **Cosign signature gate**: deploy artifact without signature; verify UnsignedDeploy.

9. **Cloudflare gradual deploy API outage**: simulate API 503 sustained; verify SEV-3 alert + manual override path.

10. **Post-rollout 24h monitoring**: bad deploy at 100%; verify auto-rollback can trigger up to 24h post-completion.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-13 ship gate é WI-S13-006; este WI passa por mini-PRR Architect + SRE Lead + Security Lead review):

- [ ] All 12 Gherkin scenarios green.
- [ ] Property tests + adversarial tests green.
- [ ] E2E chaos test 30d sustained green (incremental).
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados em DASH-ADMIN.
- [ ] ADR-XXXX (rollout controller) published.
- [ ] SRE Lead review (canary methodology + chaos test sustained).
- [ ] Architect approval (state machine + budget cap + DO singleton).
- [ ] Security Lead review (Cosign signature gate + dual-approval composition).
- [ ] OWASP ASVS V14 100% pass.
- [ ] Google SRE Workbook Ch 16 + AWS Cell-based attestation.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate `corelink-rollout-controller/` scaffold + types | 1.5h |
| ST-002 | RolloutController trait + state machine | 2.5h |
| ST-003 | DO `RolloutControllerSingletonDO` per env | 2h |
| ST-004 | Auto-rollback driver (3 triggers) | 3h |
| ST-005 | Budget cap enforcer + monthly tracker | 2h |
| ST-006 | Cloudflare gradual deploy adapter | 2.5h |
| ST-007 | D1 migration `rollout_state` + `rollout_budget_consumption` | 1h |
| ST-008 | CloudEvent audit emission per transition | 1.5h |
| ST-009 | Métricas emit (7 metrics) + trace spans | 1.5h |
| ST-010 | Property tests 4 props × 10k iter | 3h |
| ST-011 | Adversarial regression tests 8 scenarios | 3h |
| ST-012 | E2E chaos test framework (30d sustained) | 3h |
| ST-013 | rustdoc + 3 examples | 1.5h |
| ST-014 | RB-ROLLOUT-STUCK runbook | 1h |
| ST-015 | ADR-XXXX redação | 1.5h |
| ST-016 | Code review (Architect + SRE Lead + Security Lead) iteration | 2h |

**Total Optimistic**: ~32h. **PERT** (O=14h, M=22h, P=36h, per spec contract §12): **23.0h**. Sub-tasks soma é detail-grain; PERT spec contract é consolidated.

## 18. Dependencies

### Hard blockers

- **WI-S13-002 SEALED** (admin API + dual-approval gate; `DeployStartRollout` + `RolloutAbort` op types composed).
- **S-09 SEALED** (audit chain hash + atomic batch + SLO burn-rate metric pipeline).
- **S-12 SEALED recomendado** (Cosign signature gate at deploy webhook; INV-SUPPLY-SIGNED-DEPLOY herdada).

### Soft blockers

- Cloudflare gradual deploy API operational em platform.

### Outbound

- **WI-S13-006** PRR ship gate gates S-13 close.
- **S-17** chaos engineering uses progressive rollout for deploy chaos.
- **S-20** GA exige chaos test 30d sustained green.

## 19. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12).

## 20. Time-boxing

**28h hard limit**. If exceeded → escalation: split em sub-WI (controller + state machine vs auto-rollback + budget cap).

## 21. Observability

7 métricas listadas §6.1.6. Trace spans em §6.1.7.

Dashboard widget DASH-ADMIN:
- Rollout stage gauge (per env).
- Auto-rollback events (per trigger breakdown).
- Budget consumption (gauge, alert > 30%).
- Detection-to-rollback latency p99.
- Stage dwell time per stage.
- Freeze events (budget vs manual).

## 22. Cost Analysis

- DO storage: ~10 KiB per env × 2 (staging + prod) = 20 KiB; negligible.
- D1 writes: ~10 rollouts/mês × 4 stages = 40 rows/mês × 200 bytes = 8 KB; negligible.
- Cloudflare gradual deploy API: free (within Workers plan).
- Métricas pipeline: shared with S-09.
- **Total custo direto WI-S13-005**: ~$5/mês = $60/yr.

## 23. API Contract

`RolloutController` é internal Rust trait. Admin API:
- `POST /v1/admin/ops` body=`{op_type: "DeployStartRollout", deploy_artifact_sha256, ...}` → 202.
- `POST /v1/admin/ops` body=`{op_type: "RolloutAbort", handle_id}` → 200.
- `GET /v1/admin/rollout/state?env=X` → 200 `RolloutHandle`.
- `GET /v1/admin/rollout/budget?month=YYYY-MM` → 200 `{rollback_count_total, error_budget_consumed_ratio}`.

API semver stable post v1.0; breaking changes em `RolloutStage` enum = bump major + ADR.

## 24. Post-mortem Hooks

- Auto-rollback false-positive (rollback de deploy bom) > 1× mês → review thresholds + post-mortem (per spec contract §18).
- Progressive rollout bypass (deploy 100% direto sem stages) → post-mortem + privilege review.
- Budget cap exceeded (freeze) > 2× ano → review thresholds + ADR retrospective.
- Detection > 10 min p99 violated > 1h sustained → SEV-2 + post-mortem.
- Rollback > 5 min p99 violated > 1h sustained → SEV-2 + post-mortem.

## 25. Rollback / Recovery

- Code rollback: revert PR + redeploy controller (rollout abort all in-flight).
- Rollout rollback: PAT-PROGRESSIVE-ROLLOUT-001 auto + manual override.
- Budget reset: monthly automatic at 1st of month UTC.
- RTO: ≤ 5 min (auto-rollback); ≤ 30 min (manual override).
- RPO: 0 (audit chain unbroken).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: deploy artifact Cosign signature gate (S-12 WI-003); admin role + dual-approval (WI-S13-002).
- **Tampering**: state machine atomic D1 + DO singleton.
- **Repudiation**: audit emission per transition + 7y retention.
- **Information disclosure**: rollout state pseudo-public (deploy SHA, stage, status); no secrets.
- **DoS**: budget cap protects against false-positive flood; SEV-2 alerts.
- **Elevation of privilege**: stage progression enforced; bypass = 403 + audit.

**LINDDUN delta**:
- **Linkability**: rollout actor em audit (compliance accountability).
- **Identifiability**: admin user_id (intentional CTRL-AUDIT-002).
- **Non-repudiation**: cripto property intentional.
- **Detectability**: rollout events publicly tracked em audit (internal team).
- **Disclosure**: no secrets em rollout state.
- **Unawareness**: admin onboarding training documents 4-stage flow + budget cap.
- **Non-compliance**: SOC 2 CC8.1 + ISO 27001 A.5.18 satisfied.

## 27. Knowledge Transfer

- `crates/corelink-rollout-controller/README.md` — overview + integration pattern.
- ADR-XXXX — rollout controller + budget cap ratification.
- Doc `docs/internal/admin-plane.md` (rollout section) — 4-stage diagram + auto-rollback decision tree + budget rationale.
- Workshop interno (1.5h) com Architect + SRE Lead pós-merge.
- Onboarding test (10 questions): 4 stages, dwell minimum, 3 triggers, SLO burn-rate 14.4, budget cap 30%, manual override path.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Auto-rollback false-positive flood | M | L | MEDIUM (operational) | M | LOW | 30% monthly budget cap + SEV-2 alert + manual override |
| R-002 | Auto-rollback false-negative (bad deploy passes) | L | H | HIGH | M | LOW | 3 independent triggers + chaos test 30d sustained |
| R-003 | Budget cap creates DoS for legitimate deploys | L | M | HIGH | L | LOW | Manual override via Architect + Security lead approval + ADR |
| R-004 | Stage advance race condition | L | H | HIGH | L | LOW | DO singleton + D1 UNIQUE active constraint |
| R-005 | Cloudflare gradual deploy API outage | L | M | HIGH (operational) | L | LOW | SEV-3 alert + manual override + RB-ROLLOUT-STUCK |
| R-006 | Bad deploy at 100% post-rollout | L | M | HIGH | L | LOW | Post-rollout 24h monitoring + auto-rollback |
| R-007 | Cosign signature gate bypass | L | H | CRITICAL | L | LOW | INV-SUPPLY-SIGNED-DEPLOY herdada S-12 + chain verify |
| R-008 | Bypass progressive stages via API | L | H | CRITICAL | L | LOW | Stage progression enforced + 403 + audit |
| R-009 | Threshold tuning gap (chaos test ≠ prod) | M | M | HIGH | M | LOW | Chaos test sustained 30d staging in prod-equivalent infra |
| R-010 | Concurrent rollout same env | L | L | LOW | L | LOW | DO singleton + D1 UNIQUE |
| R-011 | Property test flakiness | M | L | LOW | L | LOW | Deterministic seeds + retry policy |
| R-012 | Cost regression em rollout infra | L | L | LOW | L | LOW | Cost gate + benchmark |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + SRE Lead review state machine + 4-stage progression + auto-rollback triggers + budget cap.
2. **Code (D+1)**: peer review.
3. **Security (D+1)**: Security Lead review Cosign signature gate + dual-approval composition.
4. **Property test (pre-merge D+2)**: 10k iter green em PR.
5. **Adversarial (pre-merge D+3)**: red team session — bypass attempts + Cloudflare API outage simulation.
6. **Chaos test setup (D+3)**: 30d sustained framework operational; first injection executed.
7. **PRR mini (D+4)**: Architect + SRE Lead + Security Lead sign-off (gates inclusion em WI-S13-006 ship gate).

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — state machine + DO singleton + budget cap + composition with WI-S13-001/002/003_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — Cosign signature gate + bypass enforcement + dual-approval composition_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — canary methodology + 30d sustained chaos test + Google SRE Workbook Ch 16 alignment_ | _pending_ | _pending_ |
| 6 | Engineer (S-13 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; SOC 2 CC8.1 evidence pack_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; LGPD Art. 38 review_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — bypass attempts threat model + budget cap DoS analysis_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (per framework §33.5.4.3 + ADR-0034). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S13-005 (cycle 12.S13.0). |

## 32. Anti-patterns evitados

- Skip 4-stage progression.
- Skip auto-rollback.
- Skip rollback budget cap (false-positive flood freezing legitimate deploys).
- Skip Cosign signature gate.
- Concurrent rollouts same env.
- Override budget cap sem Architect + Security lead.
- Bypass progressive stages via direct API.
- Skip chaos test sustained 30d staging.
- Auto-tune thresholds em production sem ADR.
- Direct DO write bypass.
- Skip post-rollout monitoring 24h at 100%.

---

**Fim WI-S13-005.** Próximo: WI-S13-006 (property tests 10k + RB dry-runs + PRR ship gate 11 sign-offs canonical).
