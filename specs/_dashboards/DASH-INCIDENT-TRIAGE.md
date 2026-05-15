---
id: "DASH-INCIDENT-TRIAGE"
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
tags: ["dashboard", "incident", "ir", "triage", "sre", "soc2-cc7-3"]
---

# DASH-INCIDENT-TRIAGE — Active Incident Triage

## Metadata

- **Purpose:** the dashboard the IR commander pulls up when PagerDuty fires. Cross-correlates active alerts × SLO burn × recent deploys × DR posture in one pane so the first 5 minutes of an incident are spent acting, not searching.
- **Audience:** Oncall L1/L2, IR commander, SRE Lead.
- **Refresh:** 10s (incident-time only — accept high read cost).
- **Variables:** `$env` (prod), `$region` (multi), `$incident_window` (5m / 15m / 1h).
- **Time range:** default `now-1h`.

## Panels

| # | Title                                  | Type         | Query                                                                                                                                                | Threshold / Alert                          |
|---|----------------------------------------|--------------|-------------------------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------|
| 1 | Active alerts (firing)                 | table        | `ALERTS{alertstate="firing", env="$env"}`                                                                                                            | sort by severity then `startsAt`           |
| 2 | SEV-1 fast-burn SLOs                   | stat-grid    | `corelink_slo_burn_rate{window="5m", env="$env"} > 14.4`                                                                                              | red ANY > 0                                |
| 3 | SEV-2 medium-burn SLOs                 | stat-grid    | `corelink_slo_burn_rate{window="6h", env="$env"} > 3`                                                                                                 | orange ANY > 0                             |
| 4 | Error rate (global, 1m)                | timeseries   | `sum (rate(corelink_cas_errors_total{env="$env"}[1m]) + rate(corelink_ac_errors_total{env="$env"}[1m])) / sum (rate(corelink_cas_requests_total{env="$env"}[1m]) + rate(corelink_ac_requests_total{env="$env"}[1m]))` | alert at 0.005 sustained 5m                 |
| 5 | p99 latency (CAS GET / PUT / AC)        | timeseries (3)| `histogram_quantile(0.99, sum by (op, le) (rate(corelink_cas_get_duration_seconds_bucket{env="$env"}[1m])))`                                          | alert tier-A breach                         |
| 6 | Recent deploys (last 6h)               | annotations  | `corelink_deploy_event_total` events overlay                                                                                                          | n/a                                         |
| 7 | Upstream errors (R2 / D1 / KV / DO)    | timeseries   | `sum by (resource) (rate(corelink_resource_errors_total{env="$env"}[1m]))`                                                                            | alert per backend > baseline × 5            |
| 8 | Cross-region replication lag           | timeseries   | `max by (resource, region) (corelink_replication_lag_seconds{env="$env"})`                                                                            | alert lag > 60s (R2) / 5s (D1)              |
| 9 | DR posture (failover-ready?)           | stat         | `corelink_dr_failover_ready{env="$env"}`                                                                                                              | red if 0; green if 1                       |
| 10 | Audit-chain head depth                 | stat         | `corelink_audit_chain_head_depth{env="$env"}`                                                                                                          | alert if stalled > 5m                       |
| 11 | Top 5 error codes (1h)                 | bar          | `topk(5, sum by (error_code) (increase(corelink_cas_errors_total{env="$env"}[1h]) + increase(corelink_ac_errors_total{env="$env"}[1h])))`              | n/a                                         |
| 12 | Top 10 tenants impacted                | table        | `topk(10, sum by (tenant_id) (rate(corelink_cas_errors_total{env="$env"}[5m]) + rate(corelink_ac_errors_total{env="$env"}[5m])))`                       | n/a                                         |
| 13 | Status page → incident link            | text         | static — link to status page + IR commander chair runbook                                                                                              | n/a                                         |
| 14 | Trace exemplars (slow / error)         | exemplar     | linked Tempo: `traces{service=~"worker-cp\|container-exec", duration > 1s OR status=ERROR}`                                                            | n/a                                         |
| 15 | Recent post-mortems (90d)              | table        | static — pulls from `specs/_postmortems/`                                                                                                              | n/a                                         |

## SLO IDs covered
All — this dashboard pivots on whatever SLO is burning.

## Alert IDs covered
All `*-fast` and `*-slow` SLO multi-burn alerts; `upstream-r2-*`, `upstream-d1-*`, `replication-lag-*`, `audit-chain-stall`, `dr-not-ready`.

## Runbook IDs linked
RB-SEV1-IC-CHAIR, RB-ACTIVE-FAILOVER, RB-COLD-RESTORE-FROM-ZERO, RB-CAS-INTEGRITY-VIOLATION, RB-TENANT-ISOLATION-BREACH, RB-UPSTREAM-DEGRADATION.

## Compliance hooks
- SOC 2 CC7.3 (incident response), CC7.4 (recovery), CC7.5 (continuity).
- ISO 27001 A.16 (incident management), A.17 (continuity).
- NIST SP 800-61 Rev.2 alignment (IR tabletop pre-prod rehearsal).
