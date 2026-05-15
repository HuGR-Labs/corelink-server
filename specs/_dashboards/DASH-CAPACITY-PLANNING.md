---
id: "DASH-CAPACITY-PLANNING"
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
tags: ["dashboard", "capacity", "forecast", "r2", "d1", "kv", "do", "soc2-a1-1"]
---

# DASH-CAPACITY-PLANNING — Resource Growth & 30/90-day Forecast

## Metadata

- **Purpose:** capacity headroom view per upstream backend (R2 / D1 / KV / Durable Objects / Container slots / Workers CPU-ms) with linear forecast. Used monthly by SRE + Eng Mgr in the capacity review meeting and ad-hoc when an enterprise customer is sizing onboarding load.
- **Audience:** SRE Lead, Eng Mgr, Finance (cost cross-link).
- **Refresh:** 5m.
- **Variables:** `$env` (prod), `$region` (multi), `$forecast_window` (30d / 90d / 180d).
- **Time range:** default `now-90d`.

## Panels

| # | Title                                       | Type        | Query                                                                                                                              | Threshold / Alert                              |
|---|---------------------------------------------|-------------|-------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------|
| 1 | R2 storage used (per region)                | timeseries  | `sum by (region) (corelink_storage_used_bytes{env="$env"})`                                                                          | alert at 70 % of contracted ceiling             |
| 2 | R2 storage — 30/90d forecast                | timeseries  | `predict_linear(sum by (region) (corelink_storage_used_bytes{env="$env"})[30d:1d], 90 * 86400)`                                       | alert if ETA-to-cap < 60d                      |
| 3 | R2 ops/sec utilisation                       | gauge       | `corelink_resource_utilization_ratio{resource="r2_pps", env="$env"}`                                                                  | alert ≥ 0.8 (saturation)                       |
| 4 | D1 row count (per database)                  | timeseries  | `sum by (database) (corelink_d1_row_count{env="$env"})`                                                                                | alert if approaching 10G row limit              |
| 5 | D1 qps utilisation                           | gauge       | `corelink_resource_utilization_ratio{resource="d1_qps", env="$env"}`                                                                  | alert ≥ 0.8                                    |
| 6 | D1 row-count — 90d forecast                  | timeseries  | `predict_linear(sum by (database) (corelink_d1_row_count{env="$env"})[30d:1d], 90 * 86400)`                                            | alert ETA-to-cap < 90d                          |
| 7 | KV ops/sec utilisation                       | gauge       | `corelink_resource_utilization_ratio{resource="kv_ops", env="$env"}`                                                                  | alert ≥ 0.8                                    |
| 8 | KV key count                                  | timeseries  | `sum by (namespace) (corelink_kv_key_count{env="$env"})`                                                                              | alert ≥ 100M / namespace                       |
| 9 | Durable Objects — storage per DO              | timeseries  | `sum by (do_class) (corelink_do_storage_bytes{env="$env"})`                                                                          | alert single DO > 10 GiB                       |
| 10 | DO — concurrency slots                        | gauge       | `corelink_resource_utilization_ratio{resource="do_storage", env="$env"}`                                                              | alert ≥ 0.8                                    |
| 11 | Workers CPU-ms / day (per service)            | timeseries  | `sum by (service) (increase(corelink_workers_cpu_ms_total{env="$env"}[1d]))`                                                          | budget alert at 70 % monthly cap                |
| 12 | Container exec slots (utilisation)            | gauge       | `corelink_resource_utilization_ratio{resource="container_slots", env="$env"}`                                                         | alert ≥ 0.8                                    |
| 13 | Tenant count growth (90d)                     | timeseries  | `count(count by (tenant_id) (corelink_storage_used_bytes{env="$env"}))`                                                               | trend; no threshold                            |
| 14 | Blob count growth (per tier hot/warm/cold)    | timeseries  | `sum by (tier) (corelink_blob_count{env="$env"})`                                                                                     | alert hot tier > 80 % of tier budget           |
| 15 | Dedup ratio (efficiency)                      | timeseries  | `avg (corelink_dedup_ratio{env="$env"})`                                                                                              | alert if ratio < 1.5 (cost drift)              |
| 16 | Reclaimed bytes / day (GC)                    | timeseries  | `sum by (tier) (increase(corelink_gc_reclaimed_bytes_total{env="$env"}[1d]))`                                                          | n/a                                            |
| 17 | Forecast hit-list (capacity ETAs < 90d)       | table       | manual rollup of #2 + #6 with `predict_linear`                                                                                        | red if ≥ 1 backend < 60d ETA                   |

## SLO IDs covered
SLO-AVAIL-CP, SLO-AVAIL-CAS-GET, SLO-AVAIL-CAS-PUT (saturation indirectly drives availability burn).

## Alert IDs covered
`capacity-r2-saturation`, `capacity-d1-saturation`, `capacity-kv-saturation`, `capacity-do-saturation`, `capacity-forecast-eta-60d`.

## Runbook IDs linked
RB-CAPACITY-EXPAND-R2, RB-CAPACITY-EXPAND-D1, RB-COST-REGRESSION-INVESTIGATE.

## Compliance hooks
- SOC 2 A1.1 (availability commitment via capacity planning), CC4.1.
- ISO 27001 A.12.1.3 (capacity management).
