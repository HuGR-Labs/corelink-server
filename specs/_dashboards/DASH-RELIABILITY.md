---
id: "DASH-RELIABILITY"
type: "observability_model"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["dashboard", "reliability", "uptime", "error-budget", "exec", "soc2-a1-1"]
---

# DASH-RELIABILITY — Uptime per SLO + Error Budget per Service

## Metadata

- **Purpose:** the exec-facing reliability dashboard. Single-glance uptime per SLO (last 30/90/365d) and remaining error budget per service. Used in the weekly leadership review and in customer-facing reliability reports.
- **Audience:** Engineering leadership, Exec team, enterprise customers (sanitised view).
- **Refresh:** 1m.
- **Variables:** `$env` (prod), `$service` (worker-cp / container-exec / worker-gc / worker-billing / cli), `$window` (30d / 90d / 365d).
- **Time range:** default `now-30d`.

## Panels

| # | Title                                       | Type        | Query                                                                                                                              | Threshold / Alert                              |
|---|---------------------------------------------|-------------|-------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------|
| 1 | Uptime % (30d) per SLO                      | stat-grid   | `sum by (slo) (increase(corelink_slo_good_total{env="$env"}[$window])) / sum by (slo) (increase(corelink_slo_total_total{env="$env"}[$window]))` | red < 0.995; orange < 0.999; green ≥ 0.999     |
| 2 | Error budget remaining % per SLO            | bar-gauge   | `1 - ((1 - (sum by (slo) (increase(corelink_slo_good_total{env="$env"}[$window])) / sum by (slo) (increase(corelink_slo_total_total{env="$env"}[$window])))) / (1 - corelink_slo_target_ratio))` | red < 10 %; orange < 30 %; green ≥ 30 %        |
| 3 | Synthetic uptime canary (30d)               | timeseries  | `avg_over_time(corelink_synthetic_uptime_ratio{env="$env"}[5m])`                                                                    | alert < 0.995 sustained 15m                     |
| 4 | Top-5 SLOs burning fastest (30d)            | table        | `topk(5, (1 - sum by (slo) (increase(corelink_slo_good_total{env="$env"}[$window])) / sum by (slo) (increase(corelink_slo_total_total{env="$env"}[$window]))))` | sort desc                                       |
| 5 | Incidents (last 30d) by severity            | bar          | `sum by (severity) (increase(corelink_incidents_declared_total{env="$env"}[$window]))`                                                | alert > 0 SEV-1 in 30d (Wave R-6 criterion)    |
| 6 | MTTA (SEV-1) trend                          | timeseries   | `avg_over_time(corelink_oncall_mtta_seconds{severity="sev1", env="$env"}[7d])`                                                       | alert > 300s (5min SLA)                         |
| 7 | MTTR (SEV-1) trend                          | timeseries   | `avg_over_time(corelink_oncall_mttr_seconds{severity="sev1", env="$env"}[7d])`                                                       | alert > 1800s (30min SLA)                       |
| 8 | Deploy frequency vs change-fail rate         | timeseries   | `rate(corelink_deploy_events_total{env="$env"}[1d])` + `sum (rate(corelink_deploy_events_total{env="$env", outcome="rollback"}[1d])) / sum (rate(corelink_deploy_events_total{env="$env"}[1d]))` | DORA metric — change-fail ≥ 0.15 amber          |
| 9 | Error budget burn-down curve (90d)          | timeseries   | `1 - (sum by (slo) (increase(corelink_slo_good_total{env="$env"}[90d])) / sum by (slo) (increase(corelink_slo_total_total{env="$env"}[90d])))` | visualise vs allowable burn ramp                |
| 10 | Reliability tier compliance (per plan)      | stat         | per-plan SLO target attainment (composite)                                                                                          | enterprise tier-A must be ≥ 99.9 %             |
| 11 | Open post-mortems / RCA actions             | table        | static — pulls from `specs/_postmortems/`                                                                                            | alert ≥ 5 open ≥ 30d                            |
| 12 | Customer-facing uptime page                  | text         | static link to public status page                                                                                                    | n/a                                             |

## SLO IDs covered
ALL SLOs (this dashboard is the exec rollup).

## Alert IDs covered
`reliability-30d-budget-exhausted`, `synthetic-uptime-canary-fail`, `rca-action-stale-30d`, `change-fail-rate-high`.

## Runbook IDs linked
RB-ERROR-BUDGET-EXHAUSTED, RB-RELIABILITY-REVIEW-PREP, RB-SYNTHETIC-CANARY-FAILURE.

## Compliance hooks
- SOC 2 A1.1 (availability commitment per customer), CC4.1 (monitoring).
- ISO 27001 A.17.1 (continuity).
- Customer commitments — enterprise MSA reliability clauses.
