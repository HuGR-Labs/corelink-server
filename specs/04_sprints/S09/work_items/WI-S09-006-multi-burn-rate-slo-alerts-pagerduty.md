---
id: "WI-S09-006"
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
  - "SLO-CATALOG"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s09", "alerts", "multi-burn-rate", "sloth", "pagerduty", "slo-driven", "high-risk"]
---

# WI-S09-006 — Multi-Burn-Rate SLO Alerts (Sloth-style 4-window per SLI; sprint contract §5.5 R-S09-13) + Promtool Validation + PagerDuty Integration End-to-End (`infra/alerts/*.yaml`; Prometheus Alertmanager rules conforme Google SRE Workbook Ch 5; per SLI no SLO-CATALOG: page if burn_rate(1h) > 14.4 AND burn_rate(5min) > 14.4 OR ticket if burn_rate(6h) > 6 AND burn_rate(30min) > 6; PagerDuty service per environment {staging, prod-us, prod-eu, prod-sam, prod-iad}; runbook URL no alert payload (deep link); auto-quarantine flapping alerts > 3×/week per sprint contract §14.s09.2; sprint contract §6 DoD MTTA < 5min synthetic 7d)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-09](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S09-006 |
| Título | Multi-burn-rate SLO alerts (Sloth-style 4-window per SLI conforme Google SRE Workbook Ch 5 — multi-burn-rate-alerting; alternativa a threshold-based clássicos = false positives reduzidos 100x); para cada SLI em `slo_catalog.md` (SLI-AVAIL-CAS-GET, SLI-AVAIL-CAS-PUT, SLI-AVAIL-AC-LOOKUP, SLO-DEDUP-RATIO inheritance from S-07, SLO-LATENCY-P99-CAS-PUT, etc), gerar 4 alerts: **page if burn_rate(1h) > 14.4 AND burn_rate(5min) > 14.4** (fast burn; 2% budget burned em 1h = page imediato) OU **ticket if burn_rate(6h) > 6 AND burn_rate(30min) > 6** (slow burn; 3% budget em 6h = next-business-day ticket); Prometheus Alertmanager rules em `infra/alerts/multi-burn-rate.yaml`; promtool test rules verde 100% rules em CI; PagerDuty service per environment (staging, prod-us, prod-eu, prod-sam, prod-iad — 5 services configurable per region); SEV-1 → on-call page imediato; SEV-2 → ticket business hours; runbook URL embedded no alert payload via `runbook_url` annotation (deep link to `docs/runbooks/RB-*.md`); **auto-quarantine flapping alerts** > 3×/week (sprint contract §14.s09.2 SRE Workbook Ch 8 alert discipline) via PagerDuty API + post-mortem mandatório; sprint contract §6 DoD MTTA < 5min synthetic 7d sustained |
| Sprint | S-09 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (alert reliability é foundation operability; alert flapping = oncall fatigue = SEV-1 missed = SLA breach) |

## 1. Intent

Multi-burn-rate alerts são **the alert discipline primitive** do CoreLink. Threshold-based alerts (e.g., `error_rate > 1%`) produce false positives + missed real errors at scale (Honeycomb/Google internal data: 30% noise, 20% missed). Sloth-style multi-burn-rate alerting consume error budget directly: page if budget burning fast (page-worthy); ticket if budget burning slow (review). Google SRE Workbook Ch 5 canonical pattern; reduces false-positive 100x at same true-positive recall.

```yaml
# File: infra/alerts/multi-burn-rate.yaml
# Generated via Sloth (sloth.dev) OR manual canonical Sloth-style; Prometheus Alertmanager format.
# Lote 10.8-tris P1-NEW-1 lesson absorbed: alert YAML rule count MUST match narrative claim.

groups:
  # SLO: SLI-AVAIL-CAS-GET = 99.9% (sprint contract §5.5 R-S09-13; reference slo_catalog.md §4.2)
  - name: slo-avail-cas-get-multi-burn-rate
    interval: 30s
    rules:
      # Recording rules: error budget consumption rate
      - record: slo:cas_get_error_rate:5m
        expr: |
          sum(rate(corelink_cas_get_total{result="ServerError5xx"}[5m]))
          /
          sum(rate(corelink_cas_get_total{result!="over_quota"}[5m]))
      - record: slo:cas_get_error_rate:30m
        expr: sum(rate(corelink_cas_get_total{result="ServerError5xx"}[30m])) / sum(rate(corelink_cas_get_total{result!="over_quota"}[30m]))
      - record: slo:cas_get_error_rate:1h
        expr: sum(rate(corelink_cas_get_total{result="ServerError5xx"}[1h])) / sum(rate(corelink_cas_get_total{result!="over_quota"}[1h]))
      - record: slo:cas_get_error_rate:6h
        expr: sum(rate(corelink_cas_get_total{result="ServerError5xx"}[6h])) / sum(rate(corelink_cas_get_total{result!="over_quota"}[6h]))

      # FAST BURN: page immediately (SEV-1)
      - alert: SLI_AVAIL_CAS_GET_FastBurn
        expr: |
          slo:cas_get_error_rate:5m  > (14.4 * (1 - 0.999))   # 1.44% error rate × 14.4x = 0.0144
          AND
          slo:cas_get_error_rate:1h  > (14.4 * (1 - 0.999))
        for: 2m  # require sustained 2min to debounce
        labels:
          severity: SEV-1
          slo: SLI-AVAIL-CAS-GET
          alert_type: fast_burn
        annotations:
          summary: "SLI-AVAIL-CAS-GET fast burn: 14.4x rate (page imediato)"
          description: "Error budget burning at {{ $value | humanizePercentage }} over 1h sustained 5min. 2% budget consumed/hour = exhaustion in 50h sem mitigation."
          runbook_url: "https://corelink.io/runbooks/RB-SLO-AVAIL-CAS-GET-FAST-BURN.md"
          impact: "Customer-facing CAS GET requests failing at 1.44%+ rate"

      # SLOW BURN: ticket business hours (SEV-2)
      - alert: SLI_AVAIL_CAS_GET_SlowBurn
        expr: |
          slo:cas_get_error_rate:30m > (6 * (1 - 0.999))      # 0.6% error rate × 6x = 0.006
          AND
          slo:cas_get_error_rate:6h  > (6 * (1 - 0.999))
        for: 15m
        labels:
          severity: SEV-2
          slo: SLI-AVAIL-CAS-GET
          alert_type: slow_burn
        annotations:
          summary: "SLI-AVAIL-CAS-GET slow burn: 6x rate (ticket; review)"
          description: "Error budget burning slow but sustained. 3% budget over 6h = exhaustion in 200h."
          runbook_url: "https://corelink.io/runbooks/RB-SLO-AVAIL-CAS-GET-SLOW-BURN.md"
          impact: "Quality degradation; investigate before customer impact"

  # Repeat groups for: SLI-AVAIL-CAS-PUT, SLI-AVAIL-AC-LOOKUP, SLO-LATENCY-P99-CAS-PUT, SLO-DEDUP-RATIO (S-07), SLI-AVAIL-AUTH (S-03), etc.
  # Total: ~10 SLIs × 4 rules each (fast burn + slow burn + 2 recording) = ~40 rules total
  # Lote 10.8-tris discipline: rigorous count verification YAML rule count == narrative claim
```

```yaml
# File: infra/pagerduty/services.tf
provider "pagerduty" {
    token = var.pagerduty_api_token
}

resource "pagerduty_service" "corelink_staging" {
    name = "corelink-staging"
    auto_resolve_timeout = 14400  # 4h
    acknowledgement_timeout = 600  # 10min MTTA target sprint contract §10.s09.4
    escalation_policy = pagerduty_escalation_policy.staging.id
    alert_creation = "create_alerts_and_incidents"
}

resource "pagerduty_service" "corelink_prod_us" { ... }
resource "pagerduty_service" "corelink_prod_eu" { ... }
resource "pagerduty_service" "corelink_prod_sam" { ... }
resource "pagerduty_service" "corelink_prod_iad" { ... }

# 5 services per sprint contract §5.5 R-S09-14
```

**Cripto-driven invariants enforced**:

1. **Multi-burn-rate alerting per Google SRE Workbook Ch 5** (sprint contract §5.5 R-S09-13):
   - Per SLI 4 rules: 2 recording (5m/30m/1h/6h windows) + 2 alerting (fast burn / slow burn).
   - **Fast burn**: `burn_rate(5m) > 14.4 AND burn_rate(1h) > 14.4` → page (SEV-1).
   - **Slow burn**: `burn_rate(30m) > 6 AND burn_rate(6h) > 6` → ticket (SEV-2).
   - **Why 14.4 + 6 multipliers**: SRE Workbook canonical (5% budget burned em 1h = "fast"; 10% em 6h = "slow"; reduces false positives 100x vs threshold-based).

2. **Lote 10.8-tris P1-NEW-1 lesson absorbed**: alert YAML rule count MUST match narrative claim. CI verification: `yq '.groups[].rules | length' infra/alerts/multi-burn-rate.yaml` == canonical claimed total.

3. **PagerDuty 5 services per environment** (sprint contract §5.5 R-S09-14):
   - staging, prod-us, prod-eu, prod-sam, prod-iad.
   - Per-service escalation policy: oncall rotation primary + secondary.
   - SEV-1 → page imediato (acknowledgement_timeout 10min target MTTA).
   - SEV-2 → ticket business hours.

4. **Runbook URL no alert payload** (sprint contract §14.s09.3):
   - Each alert annotations.`runbook_url` deep link.
   - CI hook em `.github/workflows/runbook-coverage.yml`: PR adicionando alert sem `runbook_url` → fails.
   - PR adicionando alert sem corresponding `docs/runbooks/RB-*.md` → fails.

5. **Auto-quarantine flapping alerts** (sprint contract §14.s09.2):
   - Alert flapping > 3×/week (synthetic + production combined) → PagerDuty API silence policy applied automatically.
   - Sloth-style flapping detection: `count_over_time(ALERTS{alertstate="firing"}[7d]) > 3` recording rule.
   - 5-Why post-mortem mandatory before un-quarantine.

6. **promtool test rules** (sprint contract §6 DoD):
   - 100% rules pass `promtool test rules infra/alerts/*.yaml`.
   - Synthetic input fixtures: deliberate burn rate breach injection → expected fire.
   - CI gate em `.github/workflows/alert-rules-validate.yml`.

7. **Manual override exclusion from SLI** (Lote 10.8bis P1-NEW-3 inheritance from WI-S08-005):
   - ManualOverride circuit breaker trips (planned drill / load shedding) excluded from SLI burn rate computation.
   - Expression: `result="ServerError5xx" AND trip_reason != "ManualOverride"` em recording rules.

8. **TenantCtx-aware alert routing**: tenant_tier dimension em alerts; per-tier escalation différentiated (enterprise tier higher priority). Tipo aggregation via 5-tier canonical (Lote 10.7bis P0-7).

9. **CF Workers Rust runtime APIs**: N/A (alerts are Prometheus YAML + Terraform; no Rust code).

10. **Audit fail-CLOSED em alert config changes** (Lote 10.6bis pattern adapted):
    - Alert silence em PagerDuty API → audit emit via WI-S09-004 emitter.
    - Alert config PR merge → audit emit (Compliance Officer review trail).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + alert discipline justification)

Multi-burn-rate alerting é **the alert discipline primitive** do CoreLink — operational distinction entre "ship-ready service" e "broken service em silêncio". Threshold-based alerts (e.g., `error_rate > 1%`) catastrophic precedent: AWS S3 2017 outage 4h before alarm fired (latency rose 100ms threshold; threshold based em static 99.9% causing trail; SLO burn-rate would have alerted 30min earlier).

**Why multi-burn-rate vs threshold-based** (Google SRE Workbook Ch 5): threshold-based assumes static error rate threshold — but reality is error budget consumption. SLI 99.9% = 0.1% error budget; if real error rate is 0.6% sustained 30min, that consumes 30min × 0.6% / 0.1% = 1.8% of monthly budget. Multi-burn-rate flags this; threshold-based misses it. Plus: 14.4x + 6x multipliers calibrated against false-positive minimization (Google internal data: < 0.1% false positive rate vs threshold-based 30%+ false positive).

**Why 4 windows per SLI**: short window (5m/30m) catches rapid burn; long window (1h/6h) confirms sustained. AND combination prevents transient spike false alerts (single 5m spike NÃO é page-worthy unless sustained 1h).

**Why PagerDuty 5 services per environment**: per-region oncall rotation; SEV-1 page → imediate response on-call engineer; SEV-2 ticket → business-hours review; SEV-3 alerta → dashboard visibility only (não page). Sprint contract §10.s09.4 MTTA < 5min synthetic 7d sustained.

**Why runbook URL no alert payload** (sprint contract §14.s09.3): incident responder during SEV-1 has 5min MTTA budget; opening alert → click runbook URL = direct context. Without runbook URL, responder must manually search `docs/runbooks/`; 2-5min wasted.

**Why auto-quarantine flapping alerts** (sprint contract §14.s09.2; SRE Workbook Ch 8): alert fatigue = oncall burnout = real alerts missed. Alert firing > 3×/week without root cause = noise; auto-quarantine via PagerDuty API + 5-Why mandatório before un-quarantine. SOTA precedent: Google SRE on-call discipline.

**Why ManualOverride exclusion from SLI** (Lote 10.8bis P1-NEW-3 lesson absorbed from WI-S08-005): planned drill / chaos test should not consume error budget (intentional shedding). Recording rule expression filters via `trip_reason != "ManualOverride"`.

**Why Lote 10.8-tris P1-NEW-1 discipline absorbed**: bis fix incorrectly claimed "14 alert rules" when YAML had 15. Tris cycle catches discrepancy. CI hook verifies count == narrative; rigorous absorption from S-08 lesson.

**Adversarial scenarios**:
- **Alert flapping** (network blip causes 5x firing/week): auto-quarantine after 3rd; 5-Why post-mortem; root-cause threshold tuning OR query optimization.
- **Multi-region simultaneous SEV-1**: PagerDuty incident merge; oncall handles consolidated.
- **PagerDuty outage during SEV-1**: secondary fallback (email + SMS via Twilio backup).
- **Synthetic page failed dispatch** (cron weekly): SEV-2 alert; investigation; Twilio backup.
- **Manual override PagerDuty silence drift**: audit emit catches; SEV-2 review.
- **NEW SLI added without alert config**: CI hook em `.github/workflows/slo-alert-coverage.yml` checks `slo_catalog.md` SLIs vs `infra/alerts/*.yaml` rules; PR fails se mismatch.
- **Burn rate computation false-positive** (denominator small): rate() em short windows; require minimum sample (e.g., > 100 reqs em window via `_AND_ rate(... [5m]) > 0.0001`).
- **Cardinality drift em recording rules** (e.g., per-tenant burn rate): inheritance from WI-S09-001 cardinality budget; aggregated tenant_tier canonical.

**Risk justification HIGH_RISK**:
- **FF-HR-005**: alert reliability = operability foundation.
- 12 sign-offs + chaos suite + property test 100k.

## 3. Customer Impact & Journey

**Persona 1 — Oncall responder (SEV-1 page)**: page received with `runbook_url` annotation; opens runbook; follows mitigation; ack PagerDuty within 5min MTTA target.

**Persona 2 — DevOps reviewing weekly**: opens DASH-SLO; sees error budget burn rate per SLI; identifies trends; addresses slow-burn issues during business hours.

**Persona 3 — SRE planning**: opens auto-quarantined alerts list; runs 5-Why per quarantined alert; tunes thresholds OR queries; un-quarantines after PR.

**Persona 4 — Customer-facing impact (when SEV-1)**: customer sees CAS GET failures; auto-incident comm via S-13 status page (deferred S-13).

**Persona 5 — Compliance officer**: reviews PagerDuty alert audit trail (via WI-S09-004 inheritance); SOC 2 evidence for incident response capability.

**SLA addendum**:
- MTTA SEV-1: ≤ 5min synthetic test 7d sustained (sprint contract §10.s09.4).
- MTTA SEV-2: ≤ 1h business hours.
- Alert dispatch reliability: ≥ 99.9% successful dispatch (PagerDuty SLA).
- Runbook URL coverage: 100% SEV-1/SEV-2 alerts.
- Flapping rate: ≤ 3 firings/week sustained (auto-quarantine threshold).

## 4. Capability Mapping

- **CAP-OBS-006** (Multi-burn-rate SLO alerts) — IMPLEMENTA primary.
- **CAP-OBS-007** (PagerDuty integration) — IMPLEMENTA primary.
- Trace: `slo_catalog.md SLIs canonical` + `observability_model.md §8 alerts canonical` + sprint contract §5.5 (R-S09-13/14) + §6 DoD + §14.s09.2/3 + Google SRE Workbook Ch 5 + Sloth project canonical.

## 5. Tipo

Prometheus Alertmanager YAML rules + PagerDuty Terraform IaC + promtool CI gate + auto-quarantine API integration; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Alert YAML files** em `infra/alerts/`:
   - `multi-burn-rate-cas.yaml` — 2 SLIs × 4 rules each = 8 rules (CAS PUT + CAS GET).
   - `multi-burn-rate-ac.yaml` — SLI-AVAIL-AC-LOOKUP × 4 rules = 4 rules.
   - `multi-burn-rate-auth.yaml` — SLI-AVAIL-AUTH × 4 rules = 4 rules.
   - `multi-burn-rate-dedup.yaml` — SLO-DEDUP-RATIO (S-07 inheritance) × 4 rules = 4 rules.
   - `multi-burn-rate-rate-limit.yaml` — SLI-RATE-LIMIT-WITHIN-QUOTA (S-08 inheritance) × 4 rules = 4 rules.
   - `multi-burn-rate-latency.yaml` — SLO-LATENCY-P99-CAS-PUT × 4 rules = 4 rules.
   - **Total**: ~7 SLIs × 4 rules + 7 SLIs × 4 recording rules = ~56 rules total (verify via `yq '.groups[].rules | length' | awk '{s+=$1} END {print s}'`).

2. **PagerDuty 5 services** em `infra/pagerduty/services.tf`:
   - corelink-staging, corelink-prod-us, corelink-prod-eu, corelink-prod-sam, corelink-prod-iad.
   - Per-service escalation policy: oncall rotation 4-week per region.
   - acknowledgement_timeout 600s (10min target MTTA buffer).

3. **Auto-quarantine logic** (`scripts/alert-quarantine.py`):
   - CI cron: weekly `count_over_time(ALERTS{alertstate="firing"}[7d]) > 3` query.
   - For each flapping alert: PagerDuty API silence policy applied (24h silence).
   - 5-Why post-mortem mandatory; un-quarantine via PR + manual approval.

4. **Runbook coverage CI hook** (`.github/workflows/runbook-coverage.yml`):
   ```yaml
   name: Alert Runbook Coverage
   on: [pull_request]
   jobs:
       coverage:
           runs-on: ubuntu-latest
           steps:
               - uses: actions/checkout@v4
               - name: Check runbook_url annotation
                 run: |
                     for alert_yaml in infra/alerts/*.yaml; do
                         yq -r '.groups[].rules[] | select(.alert) | .annotations.runbook_url // empty' "$alert_yaml" | while read url; do
                             # Extract path from URL: https://corelink.io/runbooks/RB-X.md → docs/runbooks/RB-X.md
                             path="docs/runbooks/$(basename "$url")"
                             [ -f "$path" ] || { echo "ERROR: alert references missing runbook: $url"; exit 1; }
                         done
                     done
               - name: Check SLO coverage
                 run: |
                     # Each SLI em slo_catalog.md must have multi-burn-rate alert
                     # Diff slo_catalog SLIs vs alert rule labels.slo
                     # Fail PR if SLI added without alert
   ```

5. **promtool test rules** (`tests/alerts/`):
   - Synthetic test fixtures: deliberate burn rate breach injection → expected fire.
   - CI gate: `promtool test rules tests/alerts/*.yaml` em `.github/workflows/alert-rules-validate.yml`.

6. **PagerDuty integration testing**:
   - Synthetic page weekly via PagerDuty API; assert dispatch within 30s; ack within 10min target.
   - Synthetic test em staging environment; documented em `tests/synthetic_pagerduty.sh`.

7. **ManualOverride exclusion from SLI** (Lote 10.8bis P1-NEW-3 inheritance from WI-S08-005):
   - Recording rule expression filters: `result="ServerError5xx" AND trip_reason != "ManualOverride"`.
   - Trip_reason label sourced from WI-S08-005 GlobalCircuitBreaker via labeled métrica.

8. **5-tier canonical** (Lote 10.7bis P0-7): per-tier alert thresholds OK (e.g., enterprise tier 99.99% SLO; team tier 99.9%). Tier-aware burn rate computation.

9. **Cardinality discipline** (WI-S09-001 inheritance): alert rules use aggregated tenant_tier (NOT raw tenant_id). Recording rules cardinality bounded.

10. **Métricas operacionais**:
    - `corelink_alerts_fired_total{alertname, severity}` (counter; informational).
    - `corelink_alerts_dispatched_total{pagerduty_service, severity}` (counter; **alert SEV-3 if dispatch failures > 0.1%** — PagerDuty SLA).
    - `corelink_alerts_flapping_quarantined_total{alertname}` (counter; **alert SEV-3 if > 0** — alert hygiene).
    - `corelink_alerts_runbook_coverage_pct` (gauge; **alert SEV-2 if < 100% SEV-1/2 alerts**).
    - `corelink_alerts_mtta_seconds_p99` (histogram; **alert SEV-2 if > 300s sustained 7d** — sprint contract §10.s09.4).
    - `corelink_alerts_synthetic_dispatch_failures_total` (counter; **alert SEV-1 if > 0** — synthetic test failure).

11. **Property tests** (10k iter PR; **100k nightly per HIGH_RISK SOTA bar**):
    - `prop_burn_rate_math_correct`: 10k synthetic error rates; assert burn_rate computation matches Google SRE formula.
    - `prop_alert_yaml_rule_count_canonical`: assert YAML count matches narrative (Lote 10.8-tris P1-NEW-1 discipline).
    - `prop_runbook_url_coverage`: 100% SEV-1/SEV-2 alerts have runbook_url + corresponding `docs/runbooks/RB-*.md`.
    - `prop_no_flapping_under_normal_conditions`: synthetic stable error rate; assert 0 alerts fire over 7d simulated.
    - `prop_manual_override_excluded`: synthetic ManualOverride trips; assert NOT counted in burn rate.
    - `prop_promtool_test_rules_green`: 100% rules pass promtool test em CI fixture suite.

12. **Chaos suite** (HIGH_RISK ≥ 10; this WI = 10):
    - 1. **SEV-1 fast burn synthetic**: inject burn_rate(1h) > 14.4 sustained 5min; assert alert fires within 2min.
    - 2. **SEV-2 slow burn synthetic**: inject burn_rate(6h) > 6 sustained 30min; assert alert fires within 30min.
    - 3. **Alert flapping (3+ firings/week)**: synthetic; auto-quarantine triggers.
    - 4. **PagerDuty outage**: secondary fallback (email + SMS); SEV-2 alert quanto PagerDuty SLA breach.
    - 5. **Synthetic page weekly cron**: assert dispatch + ack < 10min.
    - 6. **Multi-region simultaneous SEV-1**: PagerDuty incident merge; oncall consolidated.
    - 7. **NEW SLI added without alert**: CI hook fails PR.
    - 8. **Runbook URL drift** (alert references missing runbook file): CI hook fails PR.
    - 9. **promtool test rules failure**: synthetic fixture; CI gate rejects.
    - 10. **ManualOverride drift** (SLI inflation by drill): assert excluded from burn rate computation.

### 6.2 Out-of-scope (deferred)

- Custom Sloth Operator deployment (manual YAML canonical; Sloth tooling deferred).
- Multi-region alert federation (per-region canonical; deferred S-14).
- ML-based anomaly alerting (anti-scope §10).
- Customer-facing alert subscription (deferred S-13).
- Slack integration (PagerDuty primary; Slack via PagerDuty webhook integration future).

## 7. Anti-Scope

- ❌ Threshold-based alerts (multi-burn-rate canonical).
- ❌ Alerts sem runbook_url annotation (sprint contract §14.s09.3).
- ❌ Alerts sem corresponding `docs/runbooks/RB-*.md`.
- ❌ Skip auto-quarantine flapping (oncall fatigue).
- ❌ Skip ManualOverride SLI exclusion (Lote 10.8bis P1-NEW-3 lesson).
- ❌ Cardinality unbound em recording rules (WI-S09-001 inheritance).
- ❌ Skip promtool test rules CI gate.
- ❌ Skip synthetic PagerDuty test cron weekly.

## 8. Acceptance Criteria (Gherkin) — 10 scenarios

```gherkin
Feature: Multi-Burn-Rate SLO Alerts + PagerDuty Integration

  Scenario: Fast burn SEV-1 page within 5min MTTA
    Given SLI-AVAIL-CAS-GET = 99.9% (slo_catalog §4.2)
    Given burn_rate(1h) AND burn_rate(5m) > 14.4 sustained 5min
    When Alertmanager evaluates rules
    Then SLI_AVAIL_CAS_GET_FastBurn fires (after 2min for: clause)
    Then PagerDuty dispatch within 30s
    Then on-call ack within 5min target MTTA (sprint contract §10.s09.4)

  Scenario: Slow burn SEV-2 ticket
    Given burn_rate(30m) AND burn_rate(6h) > 6 sustained 15min
    When Alertmanager evaluates
    Then SLI_AVAIL_CAS_GET_SlowBurn fires (after 15min for: clause)
    Then PagerDuty SEV-2 ticket created
    Then business-hours review queue

  Scenario: Auto-quarantine flapping alert
    Given alert SLI_AVAIL_CAS_GET_FastBurn fires 4x em 7d
    When weekly quarantine cron runs
    Then PagerDuty API silence policy applied 24h
    Then 5-Why post-mortem mandatory created
    Then un-quarantine requires PR + manual approval

  Scenario: Runbook URL coverage CI gate
    Given PR adds NEW alert rule without annotations.runbook_url
    When CI hook runbook-coverage runs
    Then PR fails: "alert references missing runbook"
    Then developer adds runbook_url + creates docs/runbooks/RB-*.md
    Then PR passes

  Scenario: SLO coverage CI gate
    Given PR adds NEW SLI to slo_catalog.md
    Given PR does NOT add multi-burn-rate alert for new SLI
    When CI hook slo-alert-coverage runs
    Then PR fails: "SLI X added without alert"
    Then developer creates alert rule
    Then PR passes

  Scenario: ManualOverride excluded from SLI
    Given GlobalCircuitBreaker manual_override(target=open, reason=drill)
    Given 1000 requests fail with global_circuit_open during drill 30min
    When burn rate computation runs
    Then drill 429s NOT counted (trip_reason != "ManualOverride" filter)
    Then SLI burn rate stable; no spurious page

  Scenario: PagerDuty outage fallback
    Given PagerDuty API returns 503 sustained 30min
    When SEV-1 alert fires
    Then secondary fallback dispatch (email + SMS via Twilio backup)
    Then SEV-2 alert: corelink_alerts_dispatched_total{pagerduty_service=*, dispatch=fail}
    Then on-call notified despite PagerDuty outage

  Scenario: Synthetic page weekly cron
    Given synthetic page test scheduled weekly Mon UTC 12:00
    When cron triggers
    Then PagerDuty dispatch + ack < 10min
    Then dispatch failures alert SEV-1
    Then sprint contract §6 DoD MTTA test sustained 7d clean

  Scenario: promtool test rules CI gate
    Given alert YAML em infra/alerts/
    When .github/workflows/alert-rules-validate.yml runs
    Then promtool test rules infra/alerts/*.yaml passes 100%
    Then synthetic burn rate fixtures fire expected rules

  Scenario: Alert YAML count canonical (Lote 10.8-tris P1-NEW-1 discipline)
    Given infra/alerts/*.yaml total rule count claimed em sprint contract
    When CI verification runs (yq count rules)
    Then count matches narrative claim exactly
    Then PR fails se mismatch (rigorous absorption from S-08 tris lesson)
```

## 9. Design Decisions

- 9.1: Multi-burn-rate Sloth-style (Google SRE Workbook Ch 5 canonical).
- 9.2: 4 windows per SLI: 5m + 30m + 1h + 6h.
- 9.3: Multipliers 14.4x fast burn + 6x slow burn (SRE canonical).
- 9.4: PagerDuty 5 services per environment.
- 9.5: Runbook URL annotation mandatory (sprint contract §14.s09.3).
- 9.6: Auto-quarantine flapping > 3×/week (sprint contract §14.s09.2).
- 9.7: ManualOverride excluded from SLI (Lote 10.8bis P1-NEW-3 inheritance).
- 9.8: 5-tier canonical Tier dimension (Lote 10.7bis P0-7).
- 9.9: Cardinality bounded em recording rules (WI-S09-001 inheritance).
- 9.10: Lote 10.8-tris P1-NEW-1 discipline: rigorous YAML count == narrative.
- 9.11: NO new ADR (extends slo_catalog.md + observability_model.md §8 canonical).

## 10. Completeness Criteria SOTA

- [ ] **10.s09.006.1** All alert YAML files em `infra/alerts/` covering 100% SLIs em slo_catalog.md.
- [ ] **10.s09.006.2** All 10 Gherkin scenarios green.
- [ ] **10.s09.006.3** Property tests 6 × 10k green; **100k nightly sustained 7d** (HIGH_RISK SOTA bar).
- [ ] **10.s09.006.4** Chaos suite 10 scenarios green.
- [ ] **10.s09.006.5** **`promtool test rules` green em 100% rules** (sprint contract §6 DoD).
- [ ] **10.s09.006.6** **PagerDuty MTTA < 5min synthetic 7d** (sprint contract §10.s09.4).
- [ ] **10.s09.006.7** Runbook coverage 100% SEV-1/SEV-2 alerts (CI gate).
- [ ] **10.s09.006.8** SLO coverage 100% SLIs em slo_catalog.md (CI gate).
- [ ] **10.s09.006.9** Auto-quarantine logic deployed; 5-Why post-mortem hook.
- [ ] **10.s09.006.10** PagerDuty 5 services configurados; escalation policies live.
- [ ] **10.s09.006.11** ManualOverride SLI exclusion validated (Lote 10.8bis P1-NEW-3).
- [ ] **10.s09.006.12** Lote 10.8-tris P1-NEW-1 lesson absorbed: YAML count == narrative claim CI verification.
- [ ] **10.s09.006.13** Cost regression gate per sprint contract §14.s09.7 (PagerDuty seat count tracking).

## 11. DoD

- [ ] All Gherkin/property/chaos green; promtool 100%; 12 sign-offs (HIGH_RISK; framework §33.5.4.3 cap; Lote 10.8bis P1-2).

## 12. Invariants Validated

- **INV-OBS-CARDINALITY-BUDGET** (HIGH; registry §3.12): aggregated recording rules; cardinality bounded.
- **INV-AVAIL-ISOLATION** (HIGH; registry §3.8): per-tier alerts respect tenant isolation.
- **INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH; registry §3.12): alert silence emits audit via WI-S09-004.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Alert YAML rules | `infra/alerts/multi-burn-rate-*.yaml` | YAML |
| PagerDuty IaC | `infra/pagerduty/services.tf` + `escalation.tf` | Terraform |
| Auto-quarantine script | `scripts/alert-quarantine.py` | Python |
| Runbook coverage CI | `.github/workflows/runbook-coverage.yml` | YAML |
| SLO coverage CI | `.github/workflows/slo-alert-coverage.yml` | YAML |
| promtool test fixtures | `tests/alerts/*.yaml` | YAML |
| Synthetic PagerDuty test | `tests/synthetic_pagerduty.sh` | Bash |
| Property tests | `tests/alert_props.rs` | Rust |
| Chaos suite | `tests/chaos_alerts.rs` | Rust |
| Runbooks per SLI | `docs/runbooks/RB-SLO-*.md` | Markdown |

## 14. Quality Standards SOTA

- 14.s09.006.1: Zero ambiguity em alert thresholds; canonical Sloth-style.
- 14.s09.006.2: 100% rules pass promtool test.
- 14.s09.006.3: Test coverage ≥ 90%.
- 14.s09.006.4: MTTA SEV-1 ≤ 5min p99 sustained 7d.
- 14.s09.006.5: SAST clean; YAML lint clean.
- 14.s09.006.6: Métricas (6 §6.1.10).
- 14.s09.006.7: Cost regression gate (§14.s09.7); PagerDuty seat tracking em DASH-COST.
- 14.s09.006.8: 100k nightly property test (HIGH_RISK SOTA bar).
- 14.s09.006.9: Lote 10.8-tris P1-NEW-1 discipline: YAML count == narrative.
- 14.s09.006.10: Lote 10.8bis P1-NEW-3 inheritance: ManualOverride SLI exclusion.
- 14.s09.006.11: WI-S09-001 inheritance: cardinality bounded em recording rules.
- 14.s09.006.12: WI-S09-004 inheritance: alert silence audit via CloudEvents.

## 15. Chaos Experiments (10)

§6.1.12 enumerated.

## 16. PRR

HIGH_RISK 12 sign-offs PRR (framework §33.5.4.3 cap).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Alert YAML design 7 SLIs × 4 rules + 4 recording = ~56 rules | 4 |
| ST-002 | PagerDuty 5 services Terraform + escalation policies | 2 |
| ST-003 | Auto-quarantine script + 5-Why post-mortem hook | 1.5 |
| ST-004 | Runbook coverage CI hook + SLO coverage CI hook | 1.5 |
| ST-005 | promtool test fixtures + CI gate | 1.5 |
| ST-006 | Synthetic PagerDuty weekly test cron | 1 |
| ST-007 | ManualOverride SLI exclusion (Lote 10.8bis inheritance) | 0.5 |
| ST-008 | Métricas (6) emit | 0.5 |
| ST-009 | Property tests (6 × 10k; 100k nightly) | 1.5 |
| ST-010 | Chaos suite (10) | 1.5 |
| ST-011 | Runbooks RB-SLO-* per SLI (~7 runbooks) | 1.5 |

**Total**: ~16h. **PERT** O=10h M=14h P=22h: **~14.7h** (matches sprint contract §12 estimate).

## 18. Dependencies

- Hard: WI-S09-001 SEALED (métricas emit; cardinality budget); WI-S09-005 SEALED (DASH-SLO consume alerts state); slo_catalog.md current SLIs canonical; S-08 SEALED (rate-limit SLIs from WI-S08-006 inheritance + ManualOverride exclusion lesson).
- Soft: WI-S09-007 (synthetic canary inheritance for MTTA test); S-17 (chaos test integration).
- Hard infra: Prometheus Alertmanager + PagerDuty Pro plan + Twilio fallback; Sloth-style ToolingY (manual YAML OK).

## 19. Effort PERT: ~14.7h. ## 20. Time-boxing: 22h hard limit.

## 21. Observability

6 metrics §6.1.10 (visibility-on-visibility). Trace span `alerts.{evaluate, dispatch, quarantine, ack}`.

## 22. Cost Analysis

- PagerDuty Pro: $19/user/mo × 10 users × 12mo = $2280/yr.
- Alertmanager + Prometheus query: included em Mimir tenant cost (WI-S09-001).
- Twilio backup SMS: $0.0075/SMS × 100/yr = ~$1/yr (rare fallback).
- TCO 12m: ~$2280/yr PagerDuty + included infra.
- **Cost saved by alert discipline**: prevents 30+ min MTTA = customer SLA breach + reputation cost ${significant}.

## 23. API Contract

- Public: Prometheus Alertmanager YAML format (canonical); PagerDuty Events API v2.
- Wire: PagerDuty webhook receives Alertmanager fires; runbook_url propagated.
- HTTP admin: PagerDuty API for silence + un-silence; auto-quarantine integration.

## 24. Post-mortem Hooks

- SEV-1 page false-positive (no real impact) → 5-Why; threshold tuning.
- Alert flapping > 3×/week sustained → auto-quarantine + 5-Why mandatório (sprint contract §18 trigger).
- MTTA SEV-1 > 5min sustained 7d → SEV-2 + post-mortem (process gap).
- PagerDuty dispatch failure rate > 0.1% → SEV-2; PagerDuty SLA review.
- Runbook URL drift → CI gate review.

## 25. Rollback / Recovery

- Rollback: revert alert YAML; alerts disabled; oncall blind (NOT enforcement).
- Recovery: PR + Alertmanager reload; idempotent.
- RTO ≤ 5min; RPO ≤ 0min (alerts em git).

## 26. Security & Privacy

**STRIDE**:
- S(poofing): PagerDuty API token per environment em Worker secret.
- T(ampering): alert YAML git-tracked; immutable history.
- R(epudiation): audit emit via WI-S09-004 inheritance for silence/un-silence.
- I(nformation disclosure): alert payload runbook_url + summary; no PII.
- D(enial of Service): PagerDuty SLA + Twilio fallback.
- E(scalation of Privilege): PagerDuty seat ACL.

**LINDDUN**:
- L(inkability): per-tier aggregation (NOT raw tenant_id em alerts).
- I(dentifiability): alert payload bounded; no PII (CTRL-PRIV-001 inheritance).
- N(on-repudiation): audit log via WI-S09-004 alert config changes.
- D(etectability): alert dispatched + acked via PagerDuty audit.
- D(isclosure): alert payload non-sensitive metadata.
- U(nawareness): customer-facing impact via S-13 status page (deferred).
- N(on-compliance): N/A (alerts internal observability; no automated decisions).

## 27. Knowledge Transfer

Tech talk (1.5h): "S-09 Alerts: Multi-Burn-Rate + PagerDuty + Auto-Quarantine"; doc `docs/dev/alerts-architecture.md`; onboarding test 8 questions: multi-burn-rate vs threshold-based (Google SRE Workbook Ch 5), 4 windows + 14.4x/6x multipliers, runbook URL discipline (§14.s09.3), auto-quarantine flapping (§14.s09.2 SRE Workbook Ch 8), ManualOverride SLI exclusion (Lote 10.8bis P1-NEW-3), Lote 10.8-tris P1-NEW-1 YAML count discipline, PagerDuty MTTA < 5min target, Twilio fallback.

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Alert flapping = oncall fatigue | H | H | MEDIUM | H | LOW | Multi-burn-rate + auto-quarantine + alert review weekly |
| R-002 | MTTA SEV-1 > 5min | M | L | HIGH | M | LOW | PagerDuty Pro + escalation rotation + Twilio fallback |
| R-003 | PagerDuty outage | L | L | MEDIUM | L | LOW | Twilio backup; secondary fallback |
| R-004 | False-positive alert | M | M | MEDIUM | M | LOW | Multi-burn-rate (vs threshold) reduces 100x |
| R-005 | Missed real SEV-1 (under-alerting) | L | M | CRITICAL | L | LOW | Multi-burn-rate canonical; promtool test fixtures |
| R-006 | Runbook URL drift | L | L | MEDIUM | L | LOW | CI gate runbook-coverage |
| R-007 | NEW SLI without alert | M | L | MEDIUM | L | LOW | CI gate slo-alert-coverage |
| R-008 | YAML count drift (Lote 10.8-tris lesson) | M | L | LOW | L | LOW | CI hook canonical count verification |
| R-009 | ManualOverride SLI inflation drift | L | M | MEDIUM | L | LOW | Lote 10.8bis P1-NEW-3 inheritance; recording rule filter |
| R-010 | Cardinality drift recording rules | L | M | MEDIUM | L | LOW | WI-S09-001 inheritance; aggregated tier |
| R-011 | Cost regression PagerDuty seats > 10% | L | L | LOW | L | LOW | §14.s09.7 gate |
| R-012 | Alert config change without audit | L | L | MEDIUM | L | LOW | WI-S09-004 inheritance audit trail |

## 29. Review Checkpoints

D+0 design (Architect; multi-burn-rate calibration); D+1 SRE (PagerDuty + escalation); D+2 AppSec (PagerDuty API token + audit trail); D+3 code review (CI hooks); D+5 chaos validation (synthetic burn injection); D+6 PRR HIGH_RISK 12 sign-offs.

## 30. Sign-off (HIGH_RISK 12 — framework §33.5.4.3 cap; Lote 10.8bis P1-2)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation; **emphatic on multi-burn-rate calibration + PagerDuty escalation**_ |
| 4 | Security Lead | _TBD; **mandatory** — PagerDuty API token + audit trail_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos synthetic burn + property test 100k_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; mandatory — alert audit trail SOC 2_ |
| 10 | Privacy | _TBD; mandatory — no PII em alert payload_ |
| 11 | Architect | _TBD; **mandatory** — multi-burn-rate canonical + 4-window discipline + Lote 10.8-tris YAML count lesson absorbed; consolidates Crypto SME advisory race-correctness via ADR-0034_ |
| 12 | AppSec | _TBD; **mandatory** — runbook coverage + SLO coverage CI gates_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.9) | Criação WI-S09-006; HIGH_RISK; SOTA pós-Lote 10.7bis + Lote 10.8bis/tris lessons absorbed: 5-tier canonical Tier (P0-7); 100k nightly property test (P1-3); column drift no `_ms` suffix (P0-3); sign-off cap 12 (Lote 10.8bis P1-2); INV §3.12 (Lote 10.9bis P0-B corrected from §3.13/§3.14 — direct regression of Lote 10.8bis P1-13) (Lote 10.8bis P1-13). NEW alert YAML files (~56 rules; 7 SLIs × 8 rules each). NEW PagerDuty 5 services Terraform. NEW auto-quarantine script. **Lote 10.8-tris P1-NEW-1 lesson absorbed**: rigorous YAML count == narrative claim CI verification (avoid bis-introduced drift). **Lote 10.8bis P1-NEW-3 lesson absorbed**: ManualOverride excluded from SLI burn rate computation. Multi-burn-rate Sloth-style 4-window per Google SRE Workbook Ch 5. Runbook URL discipline + SLO coverage CI gates. Cardinality discipline inheritance from WI-S09-001. Audit trail inheritance from WI-S09-004. |

## 32. Anti-patterns evitados

- ❌ Threshold-based alerts (multi-burn-rate canonical); ❌ Alerts sem runbook_url; ❌ Alerts sem corresponding RB-*.md; ❌ Skip auto-quarantine flapping; ❌ Skip ManualOverride SLI exclusion; ❌ Cardinality unbound em recording rules; ❌ Skip promtool CI gate; ❌ Skip synthetic PagerDuty weekly test; ❌ YAML count drift (Lote 10.8-tris lesson).

---

**Fim WI-S09-006.** Próximo: WI-S09-007 (synthetic canary 3 regiões + dashboard health-check + runbook dry-run).
