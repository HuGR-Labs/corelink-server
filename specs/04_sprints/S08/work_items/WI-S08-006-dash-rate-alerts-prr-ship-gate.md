---
id: "WI-S08-006"
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
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "SLO-CATALOG"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
tags: ["wi", "s08", "dashboard", "alerts", "sli-distinction", "prr-ship-gate", "rb-fm-250", "ddos-runbook", "high-risk"]
---

# WI-S08-006 — DASH-RATE Operational Dashboard + Alerts + SLI Distinction Validation + RB-FM-250 DDoS Volumetric Runbook Dry-Run + S-08 PRR Ship Gate (`infra/grafana/dashboards/dash-rate.json`; aggregates 41 métricas from WI-S08-001/002/003/004/005; 15 alert rules canonical (Lote 10.8-tris P1-NEW-1 corrected; YAML actual count: 4 SEV-1 + 6 SEV-2 + 5 SEV-3 = 15; bis Phase 6 miscounted SEV-2 as 5); SEV taxonomy SEV-1/2/3 conforme observability_model.md §3.1; SLI distinction panel within-quota-429 vs over-quota-429 sprint contract §7.10.s08.1; RB-FM-250 DDoS volumetric runbook dry-run sprint contract §6 DoD EVT-017; cost regression cron-tick gate; consolidated PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate; SLO-AVAIL-CAS-GET denominator correctness validation; 100k property test acceptance harness for cross-WI integration)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-08](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S08-006 |
| Título | DASH-RATE Grafana dashboard agregando 41 métricas de WI-S08-001 (7) + WI-S08-002 (7) + WI-S08-003 (9) + WI-S08-004 (9) + WI-S08-005 (9); 14 panels organized por camada bulkhead (per-tenant + per-IP + per-PAT + global) + SLI distinction panel within-quota vs over-quota (sprint contract §7.10.s08.1 critical) + abuse detection panel + manual override + customer appeal queue panel; 15 alert rules canonical (Lote 10.8-tris P1-NEW-1 corrected; YAML actual count: 4 SEV-1 + 6 SEV-2 + 5 SEV-3 = 15; bis Phase 6 miscounted SEV-2 as 5): SEV-1 global circuit trip + cross-tenant violation + auto-suspend attempt; SEV-2 admin review trigger + PAT misuse + manual override + reconcile drift + appeals queue overflow; SEV-3 quota 95% + single-signal alarm + appeal SLA breach + abuse calibration drift + suggest block pending; RB-FM-250 DDoS volumetric runbook dry-run validation sprint contract §6 DoD EVT-017 (sustained 100k QPS distributed flood test → CF DDoS managed engages + camada 2 edge per-IP + camada 4 global circuit shed gracefully; SLO maintained for non-attacker tenants; recovery ≤ 15min); cost regression gate cron-tick ≤ $0.001/region/cycle; consolidated PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate enforces sprint contract §6 DoD complete; ADR-0034 staffing waiver acknowledged; 100k property test cross-WI acceptance harness em CI nightly |
| Sprint | S-08 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-RATE-001 + CTRL-QUOTA-001 + CTRL-AUTH operational visibility é security control completeness; sem dashboard = silent regression risk), FF-HR-002 (cross-tenant SLO regression sem visibility) |

## 1. Intent

DASH-RATE consolidates operational visibility para todas 4 camadas do bulkhead (sprint contract §4 PAT-RATE-LIMIT-001) + abuse detection + SLI distinction + global circuit. Sem dashboard: silent regressions, missed false-positives, undetected cross-tenant degradation, customer appeal SLA breach, calibration drift. RB-FM-250 dry-run validates DDoS volumetric runbook (sprint contract §6 DoD EVT-017). PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate enforces consolidated S-08 readiness antes de ship.

```yaml
# File: infra/grafana/dashboards/dash-rate.json
# (JSON serialized; pseudo-yaml here for clarity)

dashboard:
  id: "dash-rate"
  title: "DASH-RATE: S-08 Rate Limit + Quota + Abuse Operational Visibility"
  refresh: "30s"
  time_range: { from: "now-24h", to: "now" }

panels:
  # Row 1: SLI distinction (CRITICAL sprint contract §7.10.s08.1)
  - id: "sli-distinction"
    type: "graph"
    title: "SLI: Within-Quota 429 (BUG) vs Over-Quota 429 (LEGITIMATE)"
    queries:
      - expr: 'sum(rate(corelink_rate_limited_within_quota_total[5m]))'
        legend: "within-quota (counts in SLO denominator)"
      - expr: 'sum(rate(corelink_rate_limited_over_quota_total[5m]))'
        legend: "over-quota (excluded from SLO)"
    alerts:
      - rule: "sli-distinction-regression"
        condition: "delta > 10% over 5min"
        severity: "SEV-1"
        runbook: "RB-SLI-DISTINCTION-001"

  - id: "slo-avail-cas-get"
    type: "stat"
    title: "SLO-AVAIL-CAS-GET (within-quota only; sprint contract §7.10.s08.1)"
    queries:
      - expr: |
          sum(rate(corelink_cas_get_total{result="success"}[5m])) /
          (sum(rate(corelink_cas_get_total{result="success"}[5m])) + sum(rate(corelink_rate_limited_within_quota_total[5m])))
        legend: "SLO % within-quota"
    targets:
      - threshold: 0.999
      - error_budget_window: "30d"

  # Row 2: Camada 1 (per-tenant DO RateLimiter)
  - id: "camada-1-per-tenant"
    type: "heatmap"
    title: "Camada 1: Per-Tenant DO RateLimiter (Top 100 Tenants by Activity)"
    queries:
      - expr: 'corelink_rate_limiter_tokens_remaining'
      - expr: 'rate(corelink_rate_limiter_check_total[5m])'

  - id: "camada-1-isolation-canary"
    type: "stat"
    title: "INV-AVAIL-ISOLATION Canary (cross_tenant_violation_total)"
    queries:
      - expr: 'sum(corelink_rate_limiter_cross_tenant_violation_total)'
    alerts:
      - rule: "isolation-violation"
        condition: "value > 0"
        severity: "SEV-1"
        runbook: "RB-ISOLATION-001"

  # Row 3: Camada 2 (CF edge per-IP + CIDR blocklist)
  - id: "camada-2-edge-blocks"
    type: "graph"
    title: "Camada 2: CF Edge Per-IP Blocks (Anonymous + Authenticated)"
    queries:
      - expr: 'rate(corelink_edge_rate_limit_block_total{tier="anonymous"}[5m])'
      - expr: 'rate(corelink_edge_rate_limit_block_total{tier="authenticated"}[5m])'

  - id: "camada-2-cidr-blocklist-drift"
    type: "stat"
    title: "CIDR Blocklist D1 ↔ CF List Drift"
    queries:
      - expr: 'corelink_edge_cidr_blocklist_drift_total'
    alerts:
      - rule: "blocklist-drift"
        condition: "value > 10"
        severity: "SEV-2"
        runbook: "RB-EDGE-BLOCKLIST-001"

  - id: "camada-2-suggest-block-pending"
    type: "graph"
    title: "Suggested Blocks Pending Admin Review (LGPD ≤ 24h SLA)"
    queries:
      - expr: 'corelink_edge_suggest_block_pending'
    alerts:
      - rule: "suggest-block-sla-breach"
        condition: "any age_bucket=>24h > 0"
        severity: "SEV-3"
        runbook: "RB-EDGE-BLOCKLIST-002"

  # Row 4: Camada 3 (Quota + Per-PAT)
  - id: "camada-3-storage-utilization"
    type: "graph"
    title: "Storage Utilization % (Top 100 Tenants; Boundary 95% S-07 trigger; 100% S-08 hard-block)"
    queries:
      - expr: 'corelink_quota_storage_utilization_pct'
    alerts:
      - rule: "storage-95pct"
        condition: "value >= 0.95"
        severity: "SEV-3"
        runbook: "RB-QUOTA-001"
      - rule: "storage-100pct"
        condition: "value >= 1.0"
        severity: "SEV-2"
        runbook: "RB-QUOTA-002"

  - id: "camada-3-bandwidth-consumed"
    type: "graph"
    title: "Bandwidth Consumed % (Egress + Ingress Monthly)"
    queries:
      - expr: 'corelink_quota_bandwidth_consumed_pct{direction="egress"}'
      - expr: 'corelink_quota_bandwidth_consumed_pct{direction="ingress"}'

  - id: "camada-3-pat-misuse"
    type: "graph"
    title: "PAT Misuse Detected (Cap = 10× Tenant Refill Rate)"
    queries:
      - expr: 'rate(corelink_quota_pat_misuse_detected_total[5m])'
    alerts:
      - rule: "pat-misuse"
        condition: "value > 0"
        severity: "SEV-2"
        runbook: "RB-QUOTA-003"

  # Row 5: Camada 4 (Global Circuit Breaker)
  - id: "camada-4-circuit-state"
    type: "stat"
    title: "Global Circuit State per Region (0=Closed, 1=HalfOpen, 2=Open)"
    queries:
      - expr: 'corelink_global_circuit_state'
    alerts:
      - rule: "global-circuit-trip"
        condition: "value == 2"
        severity: "SEV-1"
        runbook: "RB-GLOBAL-CIRCUIT-001"

  - id: "camada-4-trips-history"
    type: "table"
    title: "Recent Global Circuit Trips (Last 30d)"
    queries:
      - expr: 'corelink_global_circuit_trips_total'

  # Row 6: Abuse Detection (CAP-ABUSE)
  - id: "abuse-score-distribution"
    type: "heatmap"
    title: "Abuse Score Distribution (All Tenants; Tier Boundaries 0.5/0.8/0.95)"
    queries:
      - expr: 'corelink_abuse_score'

  - id: "abuse-tier-counts"
    type: "graph"
    title: "Tier Counts (Noop / SilentDowngrade / AdminReview / SuspendCandidate)"
    queries:
      - expr: 'rate(corelink_abuse_tier_count_total{tier="Noop"}[5m])'
      - expr: 'rate(corelink_abuse_tier_count_total{tier="SilentDowngrade50pct1h"}[5m])'
      - expr: 'rate(corelink_abuse_tier_count_total{tier="AdminReviewTriggerSev2"}[5m])'
      - expr: 'rate(corelink_abuse_tier_count_total{tier="SuspendCandidateHumanReviewOnly"}[5m])'
    alerts:
      - rule: "abuse-admin-review"
        condition: "AdminReviewTriggerSev2 rate > 0"
        severity: "SEV-2"
        runbook: "RB-ABUSE-001"
      - rule: "auto-suspend-attempt-LGPD-canary"
        condition: "corelink_abuse_auto_suspend_attempts_total > 0"
        severity: "SEV-1"
        runbook: "RB-ABUSE-LGPD-001"

  - id: "abuse-appeals-queue"
    type: "graph"
    title: "Customer Appeals Queue (LGPD ≤ 24h SLA)"
    queries:
      - expr: 'corelink_abuse_appeal_pending'
    alerts:
      - rule: "appeal-sla-breach"
        condition: "any age_bucket=>24h > 0"
        severity: "SEV-3"
        runbook: "RB-ABUSE-002"

  # Row 7: Cost + RFC 9331 Compliance
  - id: "cost-per-cycle"
    type: "stat"
    title: "Cron-Tick Cost per Region (≤ $0.001/cycle gate)"
    queries:
      - expr: 'sum by (region) (rate(corelink_abuse_cron_duration_ms[5m]))'

  - id: "rfc9331-compliance"
    type: "stat"
    title: "RFC 9331 Headers Compliance (Customer SDK Acceptance)"
    queries:
      - expr: 'rate(corelink_rate_limit_headers_rfc9331_compliance_total[5m])'

  # Row 8: PRR Ship Gate Status
  - id: "prr-ship-gate"
    type: "table"
    title: "S-08 PRR Ship Gate Status (12 sign-offs HIGH_RISK; framework §33.5.4.3 cap)"
    queries:
      - expr: 'corelink_prr_signoff_status{sprint="S-08"}'
```

```yaml
# File: infra/alerts/dash-rate.alerts.yaml

alerts:
  # SEV-1 (oncall pager; 5min response)
  - id: "global-circuit-trip"
    expr: "corelink_global_circuit_state == 2"
    duration: "1m"
    severity: SEV-1
    pager: oncall
    runbook: RB-GLOBAL-CIRCUIT-001
    description: "Global circuit breaker tripped; all tenants in region affected; immediate response required"

  - id: "isolation-violation"
    expr: "corelink_rate_limiter_cross_tenant_violation_total > 0"
    duration: "0s"
    severity: SEV-1
    pager: oncall
    runbook: RB-ISOLATION-001
    description: "INV-AVAIL-ISOLATION violated; cross-tenant state contamination; CRITICAL"

  - id: "auto-suspend-attempt-LGPD-canary"
    expr: "corelink_abuse_auto_suspend_attempts_total > 0"
    duration: "0s"
    severity: SEV-1
    pager: oncall
    runbook: RB-ABUSE-LGPD-001
    description: "Auto-suspend attempted programmatically; LGPD Art. 20 violation canary; investigation required"

  - id: "sli-distinction-regression"
    expr: "abs(rate(corelink_rate_limited_within_quota_total[5m]) - rate(corelink_rate_limited_within_quota_total[5m] offset 24h)) > 0.1 * rate(corelink_rate_limited_within_quota_total[5m] offset 24h)"
    duration: "5m"
    severity: SEV-1
    pager: oncall
    runbook: RB-SLI-DISTINCTION-001
    description: "SLI within-quota vs over-quota counter regression detected; SLO accuracy compromised"

  # SEV-2 (admin notification; 1h response)
  - id: "abuse-admin-review"
    expr: "rate(corelink_abuse_tier_count_total{tier='AdminReviewTriggerSev2'}[5m]) > 0"
    duration: "1m"
    severity: SEV-2
    pager: admin
    runbook: RB-ABUSE-001
    description: "Tenant abuse score 0.8-0.95 triggered admin review; manual investigation required"

  - id: "pat-misuse"
    expr: "rate(corelink_quota_pat_misuse_detected_total[5m]) > 0"
    duration: "1m"
    severity: SEV-2
    pager: admin
    runbook: RB-QUOTA-003
    description: "PAT consumed > 10× tenant refill rate; possible compromise"

  - id: "manual-override"
    expr: "rate(corelink_global_circuit_manual_override_total[5m]) > 0"
    duration: "0s"
    severity: SEV-2
    pager: admin
    runbook: RB-GLOBAL-CIRCUIT-002
    description: "Global circuit manual override triggered; investigate if planned drill or emergency"

  - id: "blocklist-drift"
    expr: "corelink_edge_cidr_blocklist_drift_total > 10"
    duration: "5m"
    severity: SEV-2
    pager: admin
    runbook: RB-EDGE-BLOCKLIST-001
    description: "D1 ↔ CF List drift > 10 entries; reconcile failing"

  - id: "appeals-queue-overflow"
    expr: "sum(corelink_abuse_appeal_pending) > 100"
    duration: "1h"
    severity: SEV-2
    pager: admin
    runbook: RB-ABUSE-003
    description: "Abuse appeal queue depth > 100; staffing escalation required"

  - id: "storage-100pct"
    expr: "corelink_quota_storage_utilization_pct >= 1.0"
    duration: "5m"
    severity: SEV-2
    pager: admin
    runbook: RB-QUOTA-002
    description: "Tenant storage 100% (S-08 hard-block engaged); customer notification + S-07 eviction lag investigation"

  # SEV-3 (visibility; 24h response)
  - id: "storage-95pct"
    expr: "corelink_quota_storage_utilization_pct >= 0.95"
    duration: "30m"
    severity: SEV-3
    pager: visibility
    runbook: RB-QUOTA-001
    description: "Tenant storage 95% (S-07 eviction trigger boundary); customer notification (sprint contract §6 DoD)"

  - id: "single-signal-alarm"
    expr: "rate(corelink_global_circuit_single_signal_alarm_total[5m]) > 0"
    duration: "10m"
    severity: SEV-3
    pager: visibility
    runbook: RB-GLOBAL-CIRCUIT-003
    description: "Single signal observed but circuit NOT tripped (multi-signal canonical); investigate trending"

  - id: "appeal-sla-breach"
    expr: "corelink_abuse_appeal_pending{age_bucket='>24h'} > 0"
    duration: "0s"
    severity: SEV-3
    pager: visibility
    runbook: RB-ABUSE-002
    description: "Customer abuse appeal pending > 24h; humane LGPD SLA breach (sprint contract §7.10.s08.3)"

  - id: "suggest-block-sla-breach"
    expr: "corelink_edge_suggest_block_pending{age_bucket='>24h'} > 0"
    duration: "0s"
    severity: SEV-3
    pager: visibility
    runbook: RB-EDGE-BLOCKLIST-002
    description: "CIDR suggested block pending > 24h; LGPD SLA breach"

  - id: "abuse-calibration-drift"
    expr: "rate(corelink_abuse_calibration_drift_total[1h]) > 0"
    duration: "1h"
    severity: SEV-3
    pager: visibility
    runbook: RB-ABUSE-004
    description: "Abuse score calibration drifting; re-calibration trigger"
```

**Cripto-driven invariants enforced**:

1. **SLI distinction validation** (sprint contract §7.10.s08.1 critical absorbed):
   - Panel separates within-quota-429 (counts em SLO denominator) vs over-quota-429 (excluded).
   - Alert SEV-1 if SLI counter regression detected (within-quota counter ≠ expected under known traffic).
   - Property test 100k cross-WI integration validates correctness.

2. **15 alert rules canonical (Lote 10.8-tris P1-NEW-1 corrected; YAML actual count: 4 SEV-1 + 6 SEV-2 + 5 SEV-3 = 15; bis Phase 6 miscounted SEV-2 as 5)** (consolidated from sprint contract §6 DoD):
   - SEV-1 (4): global circuit trip + isolation violation + auto-suspend LGPD canary + SLI regression.
   - SEV-2 (6): abuse admin review + PAT misuse + manual override + blocklist drift + appeals queue overflow + storage 100% (Lote 10.8-tris P1-NEW-1 count corrected).
   - SEV-3 (5): storage 95% (S-07 boundary; sprint contract DoD) + single-signal alarm + appeal SLA breach + suggest block SLA breach + abuse calibration drift.

3. **RB-FM-250 DDoS volumetric runbook dry-run** (sprint contract §6 DoD EVT-017 absorbed):
   - Test scenario: simulated 100k QPS distributed flood across 1000 IPs sustained 30min.
   - Expected: CF DDoS-managed engages (account-level); CF edge per-IP (camada 2) blocks per-IP excess; per-tenant DO RateLimiter (camada 1) backstop; global circuit breaker (camada 4) trips if signals breach.
   - Pass criteria: SLO-AVAIL-CAS-GET maintained for non-attacker tenants ≥ 99.0% (degraded from 99.9% baseline; attack-window only); recovery ≤ 15min after attack ceases.

4. **PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate** (sprint contract §6 DoD enforced):
   - Lane-aware per `00_framework.md §33.5.4.3`.
   - Ship blocked unless: 12 sign-offs collected (framework §33.5.4.3 cap; Lote 10.8bis P1-2); ADR-0034 staffing waiver acknowledged for unstaffed roles; chaos test 30d sustained; calibration validated; alerts armed.
   - Validates ALL 6 WIs SEALED.

5. **Cost regression cron-tick gate** (sprint contract §14.s08.7 absorbed):
   - Per-region cron cycle ≤ $0.001 (sprint contract baseline).
   - Alert SEV-3 if regression > 10%.

6. **TenantCtx-only enforcement** (Lote 10.4bis lesson): metrics labels include tenant_id from middleware; admin endpoints AdminCtx (S-13).

7. **Audit fail-closed** (Lote 10.6bis pattern absorbed): dashboard configuration changes audited; manual alert silences audited; fail-closed if audit emit fails.

8. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 lesson absorbed): metrics emission via `worker::send_future()`; NEVER `tokio::spawn`.

9. **41 metrics aggregation** from 5 prior WIs:
   - WI-S08-001: 7 (rate_limiter check_total / tokens / refill / plan_sync_lag / cold_start / duration / cross_tenant_violation).
   - WI-S08-002: 7 (edge block_total / cidr_blocklist_size / drift / suggest_pending / reconcile_duration / cf_api_error / false_positive_appeal_rate).
   - WI-S08-003: 9 (storage_check / utilization / reservations_active / bandwidth_consumed / period_reset / pat_check / pat_misuse / middleware_duration / cross_tenant_violation).
   - WI-S08-004: 9 (abuse_score / tier_count / appeal_pending / appeal_resolution_time / cron_duration / metrics_lag / auto_suspend_attempts / calibration_drift / cross_tenant_feature_leak).
   - WI-S08-005: 9 (within_quota / over_quota / circuit_state / trips_total / recoveries / single_signal_alarm / half_open_duration / manual_override / rfc9331_compliance).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + race-correctness justification)

DASH-RATE consolidates operational visibility for all 4 camadas + abuse + global circuit + SLI distinction. Without dashboard: silent regressions, missed false-positives (abuse calibration drift, NAT customer false-positive blocks, PAT misuse detection lag), undetected cross-tenant degradation, customer appeal SLA breaches (LGPD compliance), single-signal alarm trends invisible. Visibility é canonical part of HIGH_RISK lane PRR ship gate (sprint contract §6 DoD).

**Why dashboard NOT just metrics**: raw metrics não actionable; dashboard provides: (a) cross-WI correlation (SLI distinction shows holistic SLO impact); (b) historical trending (calibration drift over weeks); (c) cross-camada visibility (camadas 1-4 status simultaneously); (d) customer appeals queue depth; (e) PRR ship gate status table. Grafana JSON canonical (IaC; reproducible).

**Why 15 alert rules canonical (Lote 10.8-tris P1-NEW-1 corrected; YAML actual count: 4 SEV-1 + 6 SEV-2 + 5 SEV-3 = 15; bis Phase 6 miscounted SEV-2 as 5)** (sprint contract §6 DoD consolidates): each prior WI emits relevant alerts (cross_tenant_violation, auto_suspend_attempts, pat_misuse, etc.); WI-S08-006 ties them into single coordinated alert config + runbook routing. SEV taxonomy (`observability_model.md §3.1`):
- SEV-1: oncall pager 5min response (catastrophic; user-facing outage OR LGPD violation OR cross-tenant breach).
- SEV-2: admin notification 1h response (degraded but bounded; manual review needed).
- SEV-3: visibility 24h response (trending OR SLA breach but not user-facing).

**Why RB-FM-250 dry-run** (sprint contract §6 DoD EVT-017 absorbed): runbook validation = canonical practice antes de ship. Dry-run simulated DDoS flood (100k QPS distributed); validates: (a) CF DDoS-managed engages account-level; (b) CF edge per-IP (camada 2) drops adversarial; (c) per-tenant DO (camada 1) backstops; (d) global circuit (camada 4) trips IF signals breach; (e) non-attacker tenants SLO maintained ≥ 99.0%; (f) recovery ≤ 15min. Pass criteria documented em RB-FM-250 runbook artifact.

**Why PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate** (sprint contract §6 DoD + framework §33.5.4.3): lane-aware sign-off discipline; HIGH_RISK requires 10-12 sign-offs (lower bound); S-08 = 12 max (framework cap; Crypto SME advisory consolidated as Architect race-correctness review per ADR-0034 path). ADR-0034 staffing waiver formally acknowledged for unstaffed roles.

**Why 100k property test acceptance harness**: validates cross-WI integration (e.g., SLI distinction across all 5 type discriminators; circuit breaker trip triggers correct 429 type; PAT misuse triggers per_pat header). Single-WI property tests cover individual modules; this harness covers integration.

**Adversarial scenarios** (dashboard + alert dependencies):
- **Alert flapping** (signal oscillates around threshold): hysteresis 30min duration prevents flap; deduplication via `for: 30m` Grafana clause.
- **Alert silencing race** (admin silences SEV-1 inadvertently): silence audit emit fail-closed; re-alert if conditions persist after silence expires.
- **Dashboard dependency on metrics endpoint** (S-09 outage): dashboard shows "no data" rather than wrong data; SEV-3 alert observability gap.
- **Cross-tenant metrics leak via dashboard view** (admin views one tenant's metrics): AdminCtx-gated dashboard access; per-tenant metric labels redacted via Grafana datasource permissions.
- **PRR sign-off race** (multiple reviewers signing concurrently): admin_endpoint serializes; deterministic outcome.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: operational visibility é security control completeness (CTRL-RATE-001 + CTRL-QUOTA-001).
- **FF-HR-002**: cross-tenant SLO regression sem visibility.
- 12 sign-offs (HIGH_RISK upper-bound consolidated PRR ship gate; Lote 10.8bis P1-2) + chaos via RB-FM-250 dry-run + property test 100k cross-WI integration.

## 3. Customer Impact & Journey

**Persona 1 — DevOps reviewing**: opens DASH-RATE; sees SLI distinction panel: 99.9% within-quota success + 5% over-quota traffic excluded (legitimate); SLO baseline 99.9% maintained; no false-positive on legitimate over-quota.

**Persona 2 — Oncall responder (SEV-1 page)**: receives "global circuit trip; region iad" page; opens DASH-RATE; sees Camada 4 panel: state=Open; trip reason=MultiSignalCombined (5xx + p99); recent observations show 15min sustained breach; runbook RB-GLOBAL-CIRCUIT-001 recovery procedure followed.

**Persona 3 — Admin reviewing abuse appeals**: opens DASH-RATE Abuse panel; appeals_queue_pending=12 (10 < 24h, 2 close to SLA); reviews highest-score appeal first (PriorityQueue); decides approve/reject ≤ 24h.

**Persona 4 — SRE evaluating PRR ship gate**: opens DASH-RATE PRR panel; sees 12 sign-offs status (framework §33.5.4.3 cap; Lote 10.8-tris P0-NEW-2); 10 collected + 2 staffing-waivered (ADR-0034); chaos test 30d clean; calibration validated; alerts armed; ship gate APPROVED.

**Persona 5 — Compliance auditor (LGPD review)**: opens DASH-RATE LGPD section; sees auto_suspend_attempts_total = 0 (humane response preserved); appeal SLA % within 24h = 100% (last 30d); audit trail accessible.

**Persona 6 — Product reviewing customer impact**: opens DASH-RATE Customer Impact panel; sees per-tier abuse score distribution; identifies 2 tenants in SilentDowngrade50pct1h tier (low impact; auto-recover); 0 in SuspendCandidate (correct — humane).

**Persona 7 — Security reviewing**: opens DASH-RATE Security panel; sees cross_tenant_violation_total = 0 (INV-AVAIL-ISOLATION clean); blocklist_drift_total < 5; no anomalous patterns.

**SLA addendum**:
- Dashboard refresh: ≤ 30s (Grafana refresh interval).
- Alert rule evaluation: ≤ 1min (Grafana standard).
- SEV-1 oncall response: ≤ 5min (PagerDuty escalation).
- SEV-2 admin response: ≤ 1h.
- SEV-3 visibility response: ≤ 24h.
- RB-FM-250 dry-run: pass ≥ 99.0% SLO + recovery ≤ 15min.

## 4. Capability Mapping

- **CAP-RATE-001..004 + CAP-QUOTA-001..002 + CAP-ABUSE-001..002**: ALL operational visibility — IMPLEMENTA dashboard.
- Trace: `observability_model.md §3.1 SEV taxonomy + §3.2 Grafana stack` + `slo_catalog.md SLI-AVAIL-CAS-GET (within-quota distinction)` + `failure_modes.md FM-250 RB-FM-250 + FM-401 thundering herd` + sprint contract §6 DoD ALL items + §7.10.s08.1 SLI distinction + §15 R-S08-004 single-signal false-positive + §14.s08.7 cost regression gate.

## 5. Tipo

Grafana dashboard JSON IaC + alert rules YAML + RB-FM-250 runbook dry-run + PRR consolidated 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8-tris P0-NEW-2); HIGH_RISK; FF-HR-005 + FF-HR-002.

## 6. Escopo

### 6.1 In-scope

1. **`infra/grafana/dashboards/dash-rate.json`** — Grafana dashboard 14 panels organized 8 rows:
   - Row 1: SLI distinction (CRITICAL sprint contract §7.10.s08.1).
   - Row 2: Camada 1 (per-tenant DO RateLimiter; isolation canary).
   - Row 3: Camada 2 (CF edge; CIDR blocklist drift; suggest block pending).
   - Row 4: Camada 3 (storage utilization; bandwidth; PAT misuse).
   - Row 5: Camada 4 (global circuit state; trips history).
   - Row 6: Abuse detection (score distribution; tier counts; appeals queue).
   - Row 7: Cost + RFC 9331 compliance.
   - Row 8: PRR ship gate status.

2. **`infra/alerts/dash-rate.alerts.yaml`** — 15 alert rules canonical (Lote 10.8-tris P1-NEW-1 corrected; YAML actual count: 4 SEV-1 + 6 SEV-2 + 5 SEV-3 = 15; bis Phase 6 miscounted SEV-2 as 5) organized SEV taxonomy:
   - SEV-1 (4): global circuit trip + isolation violation + auto-suspend LGPD canary + SLI regression.
   - SEV-2 (6): abuse admin review + PAT misuse + manual override + blocklist drift + appeals queue overflow + storage 100% (Lote 10.8-tris P1-NEW-1 count corrected).
   - SEV-3 (5): storage 95% + single-signal alarm + appeal SLA breach + suggest block SLA breach + abuse calibration drift.

3. **`docs/runbooks/RB-FM-250-ddos-volumetric.md`** — DDoS runbook + dry-run validation:
   - Detection signals: 5xx_rate + p99_latency + global_circuit state.
   - Triage: identify attacker IP/ASN; CIDR blocklist add (camada 2); admin manual override global circuit if necessary.
   - Mitigation: rely on CF DDoS-managed + camadas 1-4 defense-in-depth.
   - Recovery: hysteresis recovery; reconcile blocklist; post-mortem.
   - **Dry-run validation** (sprint contract §6 DoD EVT-017): simulated 100k QPS distributed flood; pass criteria: SLO ≥ 99.0% non-attacker + recovery ≤ 15min.

4. **PRR consolidated S-08 ship gate** (sprint contract §6 DoD enforcement):
   - 6 WIs SEALED individual PRRs (WI-S08-001..005 + this WI).
   - 12 sign-offs HIGH_RISK consolidated (framework §33.5.4.3 cap; Lote 10.8-tris P0-NEW-2) (Compliance + Privacy emphatic for LGPD).
   - Chaos test 30d sustained zero violations.
   - Calibration validated (10 synthetic workloads).
   - Alerts armed (8 canonical).
   - RB-FM-250 dry-run pass.
   - 100k property test cross-WI integration green.

5. **100k property test cross-WI integration harness**:
   - `tests/integration_s08.rs` — runs all 5 prior WIs property tests + cross-WI scenarios:
     - SLI distinction validation across 5 X-Rate-Limit-Type discriminators.
     - Circuit breaker trip triggers correct 429 type (global_circuit_open).
     - PAT misuse triggers correct 429 type (per_pat).
     - Abuse silent-downgrade reduces rate cap by 50% (cross-WI WI-S08-001 + WI-S08-004 integration).
     - Cross-tenant isolation validated end-to-end (no cross-tenant feature leak in any WI).

6. **Cost regression gate** (sprint contract §14.s08.7 absorbed):
   - CI cron job: compute $/million_ops baseline pre-merge.
   - Alert SEV-3 if regression > 10% versus 30d baseline.
   - Block merge requiring ADR if > 20%.

7. **Dashboard provisioning** via Terraform (`infra/grafana/dash-rate.tf`):
   - Idempotent IaC; reproducible deploy.
   - Datasource permissions: AdminCtx scope (RBAC); per-tenant metrics labels redacted from non-admin viewers.

8. **CF Workers Rust runtime APIs** (Lote 10.7bis R5 P0-3 absorbed): metrics emission via `worker::send_future()`; NEVER `tokio::spawn`.

9. **Métricas operacionais** (this WI emits about visibility):
   - `corelink.dashboard.refresh_total{panel}` (counter).
   - `corelink.alert.fire_total{rule, severity}` (counter; alerts on alerts trending).
   - `corelink.alert.silence_total{rule, admin_id}` (counter; **alert SEV-2 if SEV-1 silenced**).
   - `corelink.prr.signoff_status{sprint, role}` (gauge 0/1; signoff progress).
   - `corelink.prr.ship_gate_status{sprint}` (gauge 0/1; ship gate aprovado).

10. **Property tests** (10k iter PR; **100k nightly cross-WI harness**):
    - `prop_sli_distinction_cross_wi`: 100k random scenarios with 5 X-Rate-Limit-Type discriminators; assert correct SLI counter.
    - `prop_alert_rule_evaluation`: 10k synthetic metric values; assert correct alert firing per rule.
    - `prop_alert_dedup`: 1k oscillation patterns; assert no flapping (30min duration clause).
    - `prop_admin_alert_silence_audit`: 1k admin silences; assert all audited.

11. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 10 cross-WI integration):
    - 1. **RB-FM-250 dry-run**: simulated 100k QPS distributed flood; pass criteria SLO ≥ 99.0% + recovery ≤ 15min.
    - 2. **Multi-WI cascade**: trigger camadas 1+2+3+4 simultaneously; assert correct alert routing.
    - 3. **Dashboard datasource outage**: S-09 unavailable; dashboard shows "no data"; SEV-3 alert.
    - 4. **Alert rule false-positive**: synthetic metric just above threshold; assert duration clause prevents premature firing.
    - 5. **Admin silence race**: 2 admins concurrently silence same rule; DO actor serializes; audit captures both.
    - 6. **Cross-region alert correlation**: trip in iad propagates per-region; multi-region cascade requires ≥ 2 (rare).
    - 7. **PRR ship gate enforce**: synthetic 11 sign-offs (missing 1 mandatory; framework cap 12); assert ship gate REJECTED. Lote 10.8-tris P0-NEW-2 logic inverted from previous "missing 13".
    - 8. **Property test integration**: 100k cross-WI scenarios; assert all SLI distinction correct.
    - 9. **LGPD canary** (auto_suspend_attempts simulated): assert SEV-1 fires immediately.
    - 10. **Cost regression gate**: synthetic 15% cost increase; assert SEV-3 alert + merge gate triggers.

### 6.2 Out-of-scope (deferred)

- Customer-facing dashboard (S-13 admin plane; deferred customer self-service).
- Multi-region federated dashboard (per-region independent; deferred S-14).
- ML-based anomaly detection on metrics (anti-scope; heurística NOT ML).
- Auto-remediation on alerts (humane response; admin manual review canonical).
- Dashboard versioning + rollback UI (Grafana JSON git history canonical).

## 7. Anti-Scope

- ❌ Auto-remediation on alerts (humane response canonical).
- ❌ Customer-facing dashboard initial (S-13 deferred).
- ❌ Multi-region federation initial.
- ❌ ML-based anomaly detection (anti-scope sprint contract §10).
- ❌ Skip RB-FM-250 dry-run (sprint contract §6 DoD EVT-017).
- ❌ Skip SLI distinction panel (sprint contract §7.10.s08.1 critical).
- ❌ Skip PRR ship gate enforcement.
- ❌ TenantCtx bypass on per-tenant metrics view.
- ❌ AdminCtx bypass on dashboard configuration.
- ❌ `tokio::spawn` em CF Workers (Lote 10.7bis R5 P0-3).
- ❌ Skip cost regression gate (sprint contract §14.s08.7).

## 8. Acceptance Criteria (Gherkin) — 10 scenarios

```gherkin
Feature: DASH-RATE Operational Dashboard + Alerts + RB-FM-250 + PRR Ship Gate

  Scenario: SLI distinction panel canonical
    Given dashboard DASH-RATE loaded
    When viewer (AdminCtx) inspects "SLI: Within-Quota 429 (BUG) vs Over-Quota 429 (LEGITIMATE)" panel
    Then 2 separate time-series displayed
    Then within-quota series counts 429 type=tenant_quota + global_circuit_open
    Then over-quota series counts 429 type=per_ip + per_pat + over_quota
    Then alert SEV-1 fires if regression > 10% over 5min

  Scenario: SLO-AVAIL-CAS-GET denominator excludes over-quota
    Given 100 requests; 50 over-quota-429; 50 success
    When metrics query computes SLO
    Then denominator = 50 success + 0 over-quota = 50
    Then numerator = 50 success
    Then SLO = 100% (legitimate over-plan; sprint contract §7.10.s08.1)

  Scenario: 15 alert rules canonical (Lote 10.8-tris P1-NEW-1 corrected; YAML actual count: 4 SEV-1 + 6 SEV-2 + 5 SEV-3 = 15; bis Phase 6 miscounted SEV-2 as 5) fire correctly
    Given dashboard alerting rules deployed
    When synthetic events match each rule
    Then all 15 fires correctly: 4 SEV-1 + 6 SEV-2 + 5 SEV-3 (Lote 10.8-tris P1-NEW-1 corrected)
    Then runbook routing per SEV taxonomy
    Then audit emit corelink.alert.fire_total per rule

  Scenario: RB-FM-250 dry-run pass criteria
    Given simulated 100k QPS distributed across 1000 IPs sustained 30min
    When attack engages
    Then CF DDoS-managed (account-level) absorbs primary
    Then CF edge per-IP (camada 2) blocks adversarial IPs at threshold
    Then per-tenant DO RateLimiter (camada 1) backstops residual
    Then if signals breach: global circuit (camada 4) trips
    Then non-attacker tenants SLO ≥ 99.0% maintained (degraded but bounded)
    Then recovery ≤ 15min after attack ceases (hysteresis)
    Then post-mortem documented

  Scenario: PRR ship gate 12 sign-offs collected (framework §33.5.4.3 cap; Lote 10.8bis P1-2)
    Given S-08 6 WIs SEALED individual PRRs
    Given 12 sign-offs collected (framework §33.5.4.3 cap; Lote 10.8bis P1-2) (or staffing-waivered ADR-0034)
    Given chaos 30d clean; calibration validated; alerts armed; RB-FM-250 dry-run pass
    Given 100k property test cross-WI integration green
    When PRR ship gate evaluator runs
    Then status = APPROVED
    Then audit emit corelink.prr.ship_gate_status{sprint=S-08} = 1
    Then S-08 ship-ready

  Scenario: PRR ship gate REJECTED if 12 sign-offs only
    Given 12 sign-offs collected (1 missing; not staffing-waivered)
    When ship gate evaluator runs
    Then status = REJECTED
    Then ship blocked
    Then audit emit corelink.prr.ship_gate_status{sprint=S-08} = 0

  Scenario: Auto-suspend attempt LGPD canary alerts immediately
    Given AbuseError::AutoSuspendForbidden raised
    Given metric corelink_abuse_auto_suspend_attempts_total +=1
    When alert rule evaluates
    Then SEV-1 fires immediately (duration: 0s)
    Then oncall paged via PagerDuty
    Then runbook RB-ABUSE-LGPD-001 followed

  Scenario: 100k cross-WI property test SLI distinction
    Given 100k synthetic scenarios with 5 X-Rate-Limit-Type discriminators
    When property test runs nightly
    Then 100% scenarios assert correct SLI counter (counts_against_sli() correct mapping)
    Then 0 false counter increments

  Scenario: Cost regression gate enforce
    Given baseline 30d $0.001/region/cycle
    Given new build cost $0.0015/region/cycle (50% regression)
    When CI cron job evaluates
    Then SEV-3 alert + merge gate triggers requiring ADR
    Then merge blocked unless ADR approved

  Scenario: Dashboard admin view RBAC
    Given customer (TenantCtx) attempts dashboard access
    When request reaches Grafana
    Then 403 (AdminCtx required)
    Then audit emit corelink.dashboard.access_denied
```

## 9. Design Decisions

- 9.1: Grafana JSON IaC (NOT manual config); reproducible via Terraform.
- 9.2: 15 alert rules canonical (Lote 10.8-tris P1-NEW-1 corrected; YAML actual count: 4 SEV-1 + 6 SEV-2 + 5 SEV-3 = 15; bis Phase 6 miscounted SEV-2 as 5) organized SEV taxonomy `observability_model.md §3.1`.
- 9.3: SLI distinction panel critical (sprint contract §7.10.s08.1).
- 9.4: RB-FM-250 dry-run mandatory (sprint contract §6 DoD EVT-017).
- 9.5: PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate (sprint contract §6 DoD).
- 9.6: 100k property test cross-WI integration harness.
- 9.7: Cost regression gate (sprint contract §14.s08.7).
- 9.8: AdminCtx-gated dashboard (RBAC).
- 9.9: Dashboard refresh 30s; alert evaluation 1min (Grafana standard).
- 9.10: Hysteresis 30min duration clause prevents flapping.
- 9.11: Audit fail-closed on dashboard config + alert silence (Lote 10.6bis absorbed).
- 9.12: NO new ADR (extends observability_model.md + slo_catalog.md canonical).

## 10. Completeness Criteria SOTA

- [ ] **10.s08.006.1** Grafana dashboard `dash-rate.json` deployed via Terraform; 14 panels operational.
- [ ] **10.s08.006.2** Alert rules `dash-rate.alerts.yaml` deployed; 8 rules + runbook routing.
- [ ] **10.s08.006.3** All 10 Gherkin scenarios green.
- [ ] **10.s08.006.4** Property tests 4 × 10k green; **100k nightly cross-WI integration sustained 7d** (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- [ ] **10.s08.006.5** Chaos suite 10 scenarios green.
- [ ] **10.s08.006.6** **RB-FM-250 DDoS dry-run pass** (sprint contract §6 DoD EVT-017): SLO ≥ 99.0% non-attacker + recovery ≤ 15min.
- [ ] **10.s08.006.7** **PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate** validated for S-08; ship-ready.
- [ ] **10.s08.006.8** **SLI distinction implemented + validated**: within-quota vs over-quota separate counters + SLO denominator excludes over-quota (sprint contract §7.10.s08.1).
- [ ] **10.s08.006.9** Cost regression gate: $/million_ops baseline established + CI gate.
- [ ] **10.s08.006.10** AdminCtx-gated dashboard RBAC; per-tenant labels redacted from non-admin viewers.
- [ ] **10.s08.006.11** **All 6 WIs SEALED**: WI-S08-001 + WI-S08-002 + WI-S08-003 + WI-S08-004 + WI-S08-005 + this WI individual PRRs.
- [ ] **10.s08.006.12** **Chaos test 30d sustained**: zero INV-AVAIL-ISOLATION + zero LGPD violations + zero SLI denominator drift.
- [ ] **10.s08.006.13** Cargo-audit + cargo-deny + clippy + Terraform validate clean.
- [ ] **10.s08.006.14** Calibration validated (10 synthetic workloads from WI-S08-004 sprint contract §6 DoD).
- [ ] **10.s08.006.15** **All 15 alert rules (Lote 10.8-tris P1-NEW-1) armed** in production-equivalent staging environment.

## 11. DoD

- [ ] Dashboard + alerts + RB-FM-250 + PRR ship gate; all Gherkin/property/chaos green; **12 sign-offs (HIGH_RISK consolidated S-08 ship gate; framework §33.5.4.3 cap; Lote 10.8-tris P0-NEW-2)**.

## 12. Invariants Validated

- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): operational visibility canonical part.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, TLA+): all dashboard config + alert silence audited.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): AdminCtx-gated; per-tenant labels RBAC.
- **INV-RATE-LIMIT-PROPORTIONALITY** (HIGH; registry §3.12): visibility validates plan tier proportionality.
- **INV-QUOTA-ENFORCEMENT** (HIGH; registry §3.11): visibility validates atomic enforcement.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Grafana dashboard JSON | `infra/grafana/dashboards/dash-rate.json` | JSON |
| Terraform dashboard provisioner | `infra/grafana/dash-rate.tf` | Terraform |
| Alert rules YAML | `infra/alerts/dash-rate.alerts.yaml` | YAML |
| RB-FM-250 runbook | `docs/runbooks/RB-FM-250-ddos-volumetric.md` | Markdown |
| RB-FM-250 dry-run script | `tests/runbooks/rb-fm-250-dryrun.sh` | Bash |
| Cost regression gate CI | `.github/workflows/cost-regression.yaml` | YAML |
| Cross-WI integration test harness | `tests/integration_s08.rs` | Rust |
| Property tests | `crates/corelink-worker/tests/prop_dash_rate.rs` | Rust |
| Chaos suite | `tests/chaos_dash_rate.rs` | Rust |
| PRR ship gate evaluator | `scripts/prr_ship_gate_s08.py` | Python |

## 14. Quality Standards SOTA

- 14.s08.006.1: Zero unsafe Rust; zero unwrap em production paths.
- 14.s08.006.2: Dashboard JSON IaC reproducible via Terraform.
- 14.s08.006.3: Alert rules YAML IaC.
- 14.s08.006.4: Test coverage ≥ 90%.
- 14.s08.006.5: Dashboard refresh ≤ 30s.
- 14.s08.006.6: Alert evaluation ≤ 1min.
- 14.s08.006.7: SAST clean; Terraform validate clean.
- 14.s08.006.8: Métricas (5 §6.1.9; visibility-on-visibility).
- 14.s08.006.9: Cost regression gate per-cycle ≤ $0.001/region.
- 14.s08.006.10: TenantCtx + AdminCtx (Lote 10.4bis); CF Workers Rust API worker::send_future (Lote 10.7bis R5 P0-3); RBAC enforcement.
- 14.s08.006.11: 100k nightly property test cross-WI (HIGH_RISK SOTA bar; Lote 10.7bis P1-3).
- 14.s08.006.12: SLI distinction implemented (sprint contract §7.10.s08.1 critical).
- 14.s08.006.13: PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate (framework §33.5.4.3).
- 14.s08.006.14: RB-FM-250 dry-run pass (sprint contract §6 DoD EVT-017).
- 14.s08.006.15: 15 alert rules (Lote 10.8-tris P1-NEW-1) armed production-equivalent staging.

## 15. Chaos Experiments (10)

§6.1.11 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) PRR; **consolidated S-08 ship gate**; sprint contract §6 DoD enforces.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Grafana dashboard JSON 14 panels | 3 |
| ST-002 | Terraform dashboard provisioner | 1 |
| ST-003 | Alert rules YAML 8 rules + runbook routing | 1.5 |
| ST-004 | RB-FM-250 runbook artifact | 2 |
| ST-005 | RB-FM-250 dry-run script + validation | 2 |
| ST-006 | Cost regression gate CI workflow | 1 |
| ST-007 | Cross-WI integration test harness | 2 |
| ST-008 | PRR ship gate evaluator script | 1 |
| ST-009 | Property tests (4 × 10k; 100k nightly) | 2 |
| ST-010 | Chaos suite (10) | 2 |
| ST-011 | RBAC datasource permissions | 0.5 |
| ST-012 | Documentation + onboarding | 1 |

**Total**: ~19h. **PERT** O=14h M=16h P=22h: **~17h** (sprint contract estimate 12h; revised upward por: cross-WI integration harness + RB-FM-250 dry-run + PRR ship gate evaluator + cost regression gate scope).

## 18. Dependencies

- Hard: WI-S08-001/002/003/004/005 SEALED (this WI consolidates their métricas + alerts + property tests); ADR-0034 (PRR staffing waiver formal acknowledgment).
- Hard: S-09 SEALED OR em paralelo (Grafana/Prometheus/PagerDuty stack required).
- Soft: S-13 admin plane (RBAC scope; staging stub OK; full sealing post-S-13).

## 19. Effort PERT: ~17h. ## 20. Time-boxing: 22h hard limit.

## 21. Observability

5 metrics §6.1.9 (visibility-on-visibility). Trace span `dashboard.{render, alert_eval, prr_signoff_check}`.

## 22. Cost Analysis

- Dashboard refresh: ~$0.0001/refresh × 30s/refresh × 24h × 30d = ~$8/month — trivial.
- Alert evaluation: included em Grafana plan.
- RB-FM-250 dry-run: one-time cost ~$50 (synthetic load 100k QPS sustained 30min via Bazel client harness).
- TCO 12m: ~$200/yr (dashboard + alerts + RB-FM-250 dry-run quarterly).
- **Cost saved by visibility**: prevents catastrophic regression (silent cross-tenant violation, calibration drift, LGPD violation auto-suspend) — orders of magnitude more.

## 23. API Contract

- Public: dashboard JSON schema + alert rules YAML schema; reproducible via Terraform.
- HTTP admin: dashboard endpoints (Grafana standard); RBAC AdminCtx required.

## 24. Post-mortem Hooks

- RB-FM-250 dry-run fails (SLO < 99.0% non-attacker OR recovery > 15min) → CRITICAL post-mortem; defense-in-depth review.
- PRR ship gate REJECTED → 5-Why; staffing OR sign-off gap.
- Alert flap detected (rule firing >10×/h) → SEV-2 post-mortem; threshold tuning.
- Auto-suspend LGPD canary alarm → CRITICAL; LGPD violation investigation.
- SLI distinction regression → SEV-1; SLI correctness investigation; sprint contract §7.10.s08.1 alignment.
- Dashboard datasource outage > 1h → SEV-2; observability gap; S-09 dependency.
- Cost regression > 20% → SEV-3; ADR required; merge blocked.

## 25. Rollback / Recovery

- Rollback: revert Terraform → Grafana dashboard removed; alerts disabled; visibility lost (NOT enforcement).
- Recovery: dashboard JSON git history; alert rules YAML idempotent re-apply.
- RTO ≤ 5min (Terraform apply); RPO ≤ 0min (dashboard config no state).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): TenantCtx + AdminCtx S-03; RBAC.
- T(ampering): dashboard JSON git-tracked; immutable history.
- R(epudiation): audit fail-closed em config changes + alert silences.
- I(nformation disclosure): per-tenant labels RBAC-redacted from non-admin viewers.
- D(enial of Service): dashboard refresh rate-limited (Grafana standard).
- E(scalation of Privilege): AdminCtx required for sensitive panels.

**LINDDUN**:
- L(inkability): per-tenant labels redacted; no cross-tenant linkability.
- I(dentifiability): tenant_id em metrics; redact in non-admin views.
- N(on-repudiation): audit append-only.
- D(etectability): dashboard expose system state em RBAC-controlled view.
- D(isclosure): system metrics RBAC-controlled.
- U(nawareness): customer self-service via S-13 (deferred).
- N(on-compliance): LGPD canary alert ensures compliance visibility.

## 27. Knowledge Transfer

Tech talk (2h): "S-08 DASH-RATE: Operational Visibility + 15 Alert Rules (Lote 10.8-tris P1-NEW-1) + RB-FM-250 + PRR Ship Gate"; doc `docs/dev/dash-rate-architecture.md`; onboarding test 8 questions: SLI distinction (sprint contract §7.10.s08.1), 15 alert rules SEV taxonomy (Lote 10.8-tris P1-NEW-1), RB-FM-250 dry-run pass criteria, PRR ship gate 12 sign-offs (lane-aware §33.5.4.3 cap; Lote 10.8bis P1-2), 100k cross-WI property test, cost regression gate (§14.s08.7), AdminCtx RBAC, hysteresis duration clauses prevent flapping.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | RB-FM-250 dry-run fails defense-in-depth gap | M | M | CRITICAL | L | LOW | 6 WIs cumulatively + multi-camada; chaos 30d |
| R-002 | PRR ship gate fails staffing | M | L | HIGH | L | LOW | ADR-0034 staffing waiver; lane-aware §33.5.4.3 |
| R-003 | SLI distinction regression undetected | L | M | CRITICAL | L | LOW | Property test 100k cross-WI; SEV-1 alert; sprint contract §7.10.s08.1 |
| R-004 | Alert flapping false-positive | M | L | MEDIUM | L | LOW | Hysteresis 30min duration; property test |
| R-005 | Dashboard datasource outage S-09 | M | L | MEDIUM | L | LOW | "no data" graceful; SEV-3 alert |
| R-006 | RBAC bypass per-tenant data leak | L | M | CRITICAL | L | LOW | AdminCtx + RBAC datasource permissions |
| R-007 | Auto-suspend LGPD canary missed | L | M | CRITICAL | L | LOW | SEV-1 immediate; oncall paged; runbook RB-ABUSE-LGPD-001 |
| R-008 | Cost regression gate false-positive | M | L | LOW | L | LOW | 30d baseline; ADR escalation > 20% |
| R-009 | tokio::spawn em CF Workers (compile fail) | L | L | LOW | L | LOW | Lote 10.7bis R5 P0-3; worker::send_future |
| R-010 | Audit fail silently em silence | L | M | MEDIUM | L | LOW | Fail-closed bias; reconcile catches |
| R-011 | Multi-region cascade missed | L | L | HIGH | L | LOW | Per-region scope; multi-region requires ≥ 2 (rare) |
| R-012 | Calibration drift undetected | M | L | MEDIUM | L | LOW | abuse_calibration_drift metric SEV-3; periodic re-calibration |

## 29. Review Checkpoints

D+0 design (Architect; dashboard layout); D+1 SRE (alert rules + runbook routing); D+2 AppSec (TenantCtx + AdminCtx + RBAC); D+3 Compliance (LGPD canary alert); D+4 code review; D+5 RB-FM-250 dry-run; D+6 chaos validation + 100k integration; D+7 PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate.

## 30. Sign-off (HIGH_RISK 12 — CONSOLIDATED S-08 SHIP GATE; framework §33.5.4.3 cap; Lote 10.8bis P1-2)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation; **emphatic on alert rules + runbook routing**_ |
| 4 | Security Lead | _TBD; **mandatory** — INV-AVAIL-ISOLATION + RBAC_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos + 100k cross-WI integration + RB-FM-250 dry-run validation_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — LGPD canary alert + appeal SLA validation + audit trail completeness (consolidates LGPD review across 6 WIs)_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — RBAC + per-tenant labels redacted_ |
| 11 | Architect | _TBD; **mandatory emphatic** — multi-camada visibility + SLI distinction (sprint contract §7.10.s08.1) + PRR ship gate enforcement; consolidates Data Engineering advisor for dashboard metrics correctness (Crypto SME N/A — observability is NOT cripto-load-bearing)_ |
| 12 | AppSec | _TBD; **mandatory** — RBAC datasource permissions + alert silence audit_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.8bis) | R4+R5 review remediation: P1-1 alert count 8 vs 14 reconciled (canonical 14 alert rules; 4 SEV-1 + 5 SEV-2 + 5 SEV-3); P1-2 sign-off cap 13→12 (framework §33.5.4.3 cap; Data Engineering advisor consolidated em Architect per ADR-0034); P1-13 INV §3.X → §3.12; aggregate score post-bis target ≥ 8.5/10 (R4 7.7 + R5 7.5 baselines). |
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.8) | Criação WI-S08-006; HIGH_RISK; **S-08 PRR consolidated ship gate**; SOTA pós-Lote 10.7bis lessons absorbed: SLI distinction (sprint contract §7.10.s08.1 critical absorbed); CF Workers Rust API worker::send_future (R5 P0-3); 100k nightly cross-WI integration property test (P1-3); audit fail-closed (Lote 10.6bis); 5-tier canonical (P0-7); column drift no `_ms` suffix (P0-3). 14 dashboard panels consolidating 41 métricas de WI-S08-001/002/003/004/005. 15 alert rules canonical (Lote 10.8-tris P1-NEW-1 corrected; YAML actual count: 4 SEV-1 + 6 SEV-2 + 5 SEV-3 = 15; bis Phase 6 miscounted SEV-2 as 5) SEV taxonomy. RB-FM-250 DDoS volumetric runbook dry-run validation (sprint contract §6 DoD EVT-017). PRR HIGH_RISK 12 sign-offs (framework §33.5.4.3 cap; Lote 10.8bis P1-2) ship gate (framework §33.5.4.3 lane-aware; ADR-0034 staffing waiver). Cost regression gate cron-tick (sprint contract §14.s08.7). 100k cross-WI integration property test acceptance harness. AdminCtx RBAC datasource permissions. Sprint contract estimate 12h revised upward to ~17h porque scope expansion include cross-WI integration harness + RB-FM-250 dry-run + PRR evaluator + cost regression gate. |

## 32. Anti-patterns evitados

- ❌ Auto-remediation on alerts (humane response canonical); ❌ Customer-facing dashboard initial; ❌ Multi-region federation initial; ❌ ML-based anomaly detection (anti-scope §10); ❌ Skip RB-FM-250 dry-run (DoD); ❌ Skip SLI distinction panel (§7.10.s08.1 critical); ❌ Skip PRR ship gate enforcement; ❌ TenantCtx bypass on per-tenant metrics view; ❌ AdminCtx bypass on dashboard configuration; ❌ tokio::spawn em CF Workers; ❌ Skip cost regression gate (§14.s08.7); ❌ Manual dashboard config (Terraform IaC canonical); ❌ Single-rule alerting sem hysteresis (flapping vulnerability).

---

**Fim WI-S08-006.** **Fim S-08 6 WIs criados.** Próximo: validators (validate_references + validate_specs + validate_inv_promotion); commit Lote 10.8; dispatch adversarial reviews (Agent R4 Opus + Sonnet R5); Lote 10.8bis P0 fixes cycle.
