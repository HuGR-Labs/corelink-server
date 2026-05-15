---
id: "DASH-CUSTOMER-TRAFFIC"
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
tags: ["dashboard", "traffic", "tenant", "plan", "product", "soc2-cc4-1"]
---

# DASH-CUSTOMER-TRAFFIC — Customer Tier & Top-Tenant Traffic

## Metadata

- **Purpose:** customer-segmented traffic view — RPS / bandwidth / latency / errors split by `plan` (free / team / enterprise) and top-N `tenant_id`. Used by Product for adoption signal, Support for "is the noisy customer noisy?", and Sales for enterprise capacity questions.
- **Audience:** Product Lead, Support L2/L3, Sales engineering, SRE Lead.
- **Refresh:** 30s.
- **Variables:** `$env` (prod), `$region` (multi), `$plan` (free/team/enterprise/all), `$tenant_id` (multi-select; top-50 enumerated + "other").
- **Time range:** default `now-24h`; 7d/30d for trend.

> **Cardinality discipline** (per `observability_model.md` §11.2): `tenant_id` panels MUST use `topk(N, …)` or `$tenant_id` filter. NEVER `group by (tenant_id)` over 100k cardinality on a global panel.

## Panels

| # | Title                                  | Type        | Query (PromQL)                                                                                                                                       | Threshold / Alert                                  |
|---|----------------------------------------|-------------|-------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------|
| 1 | RPS by plan tier                       | timeseries  | `sum by (plan) (rate(corelink_cas_requests_total{env="$env", region=~"$region"}[1m])) + sum by (plan) (rate(corelink_ac_requests_total{env="$env", region=~"$region"}[1m]))` | n/a                                                |
| 2 | Active tenants (24h) by plan           | stat        | `count by (plan) (count by (plan, tenant_id) (rate(corelink_cas_requests_total{env="$env"}[24h]) > 0))`                                              | n/a                                                |
| 3 | Top-20 tenants by RPS                  | table       | `topk(20, sum by (tenant_id, plan) (rate(corelink_cas_requests_total{env="$env"}[5m]) + rate(corelink_ac_requests_total{env="$env"}[5m])))`           | alert if single tenant > 10 % of global RPS        |
| 4 | Top-20 tenants by egress bytes         | table       | `topk(20, sum by (tenant_id, plan) (rate(corelink_bandwidth_egress_bytes_total{env="$env"}[5m])))`                                                   | alert if single tenant > 25 % of global egress      |
| 5 | Cache hit ratio by plan                | timeseries  | `avg by (plan) (corelink_cache_hit_ratio{env="$env"})`                                                                                                | enterprise expected ≥ 0.85; alert if < 0.70       |
| 6 | p99 CAS GET latency by plan            | timeseries  | `histogram_quantile(0.99, sum by (plan, le) (rate(corelink_cas_get_duration_seconds_bucket{env="$env"}[5m])))`                                        | enterprise tier-A breach if > 0.20 s                |
| 7 | Error rate by plan                     | timeseries  | `sum by (plan) (rate(corelink_cas_errors_total{env="$env"}[5m]) + rate(corelink_ac_errors_total{env="$env"}[5m])) / sum by (plan) (rate(corelink_cas_requests_total{env="$env"}[5m]) + rate(corelink_ac_requests_total{env="$env"}[5m]))` | alert if any plan > 0.005 sustained 10m            |
| 8 | Storage used (per-tenant top-20)       | bar         | `topk(20, max by (tenant_id, plan) (corelink_storage_used_bytes{env="$env"}))`                                                                       | alert if single tenant > plan quota                |
| 9 | Blob count growth (top-20 tenants)     | timeseries  | `topk(20, deriv(corelink_blob_count{env="$env"}[1h]))`                                                                                                | alert if growth > 100k blobs/hour (abuse signal)   |
| 10 | Rate-limit fires per tenant            | table        | `topk(20, sum by (tenant_id) (increase(corelink_ratelimit_requests_total{env="$env", outcome="denied"}[1h])))`                                       | alert if same tenant top-1 for 24h consecutive     |
| 11 | New-tenant onboarding (24h)            | stat         | `count(count by (tenant_id) (increase(corelink_cas_requests_total{env="$env"}[24h]) > 0) unless count by (tenant_id) (increase(corelink_cas_requests_total{env="$env"}[24h] offset 24h) > 0))` | trend only                                         |
| 12 | Enterprise SLA breach hit-list         | table        | `corelink_slo_burn_rate{plan="enterprise", window="1h", env="$env"} > 1`                                                                              | alert page if > 0 enterprise breaches sustained 5m |

## SLO IDs covered
SLO-AVAIL-CAS-GET, SLO-AVAIL-CAS-PUT, SLO-AVAIL-AC, SLO-LAT-CAS-GET (tier-A enterprise targets).

## Alert IDs covered
`enterprise-sla-burn-fast`, `enterprise-sla-burn-slow`, `abuse-tenant-top1-24h`, `tenant-quota-breach`.

## Runbook IDs linked
RB-TENANT-ABUSE-RESPONSE, RB-ENTERPRISE-INCIDENT-COMMS, RB-CUSTOMER-SUPPORT-T-90.

## Compliance hooks
- SOC 2 CC4.1 (monitoring), A1.1 (availability per customer commitment).
- ISO 27001 A.9.4 (access control), A.13.1 (network security monitoring).
