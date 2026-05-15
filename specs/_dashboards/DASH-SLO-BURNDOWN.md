---
id: "DASH-SLO-BURNDOWN"
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
tags: ["dashboard", "slo", "burndown", "error-budget", "sre", "soc2-cc4-1"]
---

# DASH-SLO-BURNDOWN — SLO Burndown (30-day rolling)

## Metadata

- **Purpose:** single pane showing rolling 30-day error-budget burn for every SLO in `specs/03_architecture/slo_catalog.md`. Used by SRE for weekly SLO review and by IR commander to triage which SLO is closest to exhaustion.
- **Audience:** SRE on-call, SRE Lead, Engineering leadership.
- **Refresh:** 1m.
- **Variables:** `$env` (prod default), `$slo` (multi-select: all SLO-* IDs).
- **Time range:** default `now-30d` (30-day rolling); selectable `now-7d` / `now-1d` for incident view.

## Panels

| # | Title                                    | Type            | Query (PromQL)                                                                                                                                                                   | Threshold / Alert                                  |
|---|------------------------------------------|-----------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------|
| 1 | SLO Catalog — current burn (all SLOs)    | stat-grid       | `corelink_slo_burn_rate{slo=~"$slo", window="1h", env="$env"}`                                                                                                                    | red ≥ 6 (SEV-2 multi-burn); orange ≥ 1; green < 1 |
| 2 | Error-budget remaining (per SLO)         | bar-gauge       | `1 - (sum by (slo) (increase(corelink_slo_total_total{env="$env"}[30d])) - sum by (slo) (increase(corelink_slo_good_total{env="$env"}[30d]))) / (sum by (slo) (increase(corelink_slo_total_total{env="$env"}[30d])) * (1 - corelink_slo_target_ratio))` | red < 10 % remaining                              |
| 3 | Multi-window burn — Availability CP      | timeseries (4)  | `corelink_slo_burn_rate{slo="SLO-AVAIL-CP", window=~"5m\|1h\|6h\|24h", env="$env"}`                                                                                                | alert-line at 14.4 (5m fast-burn SEV-1)            |
| 4 | Multi-window burn — CAS GET avail        | timeseries (4)  | `corelink_slo_burn_rate{slo="SLO-AVAIL-CAS-GET", window=~"5m\|1h\|6h\|24h", env="$env"}`                                                                                            | 14.4 (5m) / 6 (1h) / 3 (6h) / 1 (24h)              |
| 5 | Multi-window burn — CAS PUT avail        | timeseries (4)  | `corelink_slo_burn_rate{slo="SLO-AVAIL-CAS-PUT", window=~"5m\|1h\|6h\|24h", env="$env"}`                                                                                            | 14.4 / 6 / 3 / 1                                   |
| 6 | Multi-window burn — AC avail             | timeseries (4)  | `corelink_slo_burn_rate{slo="SLO-AVAIL-AC", window=~"5m\|1h\|6h\|24h", env="$env"}`                                                                                                | 14.4 / 6 / 3 / 1                                   |
| 7 | Latency burn — CAS GET p99               | timeseries (4)  | `corelink_slo_burn_rate{slo="SLO-LAT-CAS-GET", window=~"5m\|1h\|6h\|24h", env="$env"}`                                                                                              | 14.4 / 6 / 3 / 1                                   |
| 8 | Latency burn — CAS PUT p99               | timeseries (4)  | `corelink_slo_burn_rate{slo="SLO-LAT-CAS-PUT", window=~"5m\|1h\|6h\|24h", env="$env"}`                                                                                              | 14.4 / 6 / 3 / 1                                   |
| 9 | Latency burn — AC hit                    | timeseries (4)  | `corelink_slo_burn_rate{slo="SLO-LAT-AC-HIT", window=~"5m\|1h\|6h\|24h", env="$env"}`                                                                                               | 14.4 / 6 / 3 / 1                                   |
| 10 | Correctness — CAS integrity             | timeseries      | `corelink_slo_burn_rate{slo="SLO-CORRECT-CAS", window="1h", env="$env"}`                                                                                                            | ANY > 0 = SEV-1 (zero-tolerance)                   |
| 11 | Correctness — Tenant isolation          | timeseries      | `corelink_slo_burn_rate{slo="SLO-CORRECT-ISO", window="1h", env="$env"}`                                                                                                            | ANY > 0 = SEV-1                                    |
| 12 | Freshness — Billing events              | timeseries      | `corelink_slo_burn_rate{slo="SLO-FRESH-BILLING", window="6h", env="$env"}`                                                                                                          | ≥ 3 SEV-3                                          |
| 13 | Freshness — DSR erasure                 | timeseries      | `corelink_slo_burn_rate{slo="SLO-FRESH-DSR-ERASURE", window="24h", env="$env"}`                                                                                                     | ≥ 1 SEV-2                                          |
| 14 | Admin plane — Config propagation        | timeseries      | `corelink_slo_burn_rate{slo="SLO-ADMIN-CONFIG-PROPAGATION", window="1h", env="$env"}`                                                                                               | ≥ 6 SEV-2                                          |
| 15 | Admin plane — Dual-approval latency     | timeseries      | `corelink_slo_burn_rate{slo="SLO-ADMIN-DUAL-APPROVAL-LATENCY", window="6h", env="$env"}`                                                                                            | ≥ 3 SEV-3                                          |
| 16 | Admin plane — Rotation overlap          | timeseries      | `corelink_slo_burn_rate{slo="SLO-ADMIN-ROTATION-OVERLAP", window="24h", env="$env"}`                                                                                                | ≥ 1 SEV-3                                          |
| 17 | Admin plane — Rollback recovery         | timeseries      | `corelink_slo_burn_rate{slo="SLO-ADMIN-ROLLBACK-RECOVERY", window="1h", env="$env"}`                                                                                                | ≥ 6 SEV-2                                          |
| 18 | DR — Region failover RTO                | timeseries      | `corelink_slo_burn_rate{slo="SLO-RTO-REGION-FAILOVER", window="24h", env="$env"}`                                                                                                   | ≥ 1 SEV-2                                          |
| 19 | DR — Region failover RPO                | timeseries      | `corelink_slo_burn_rate{slo="SLO-RPO-REGION", window="24h", env="$env"}`                                                                                                            | ≥ 1 SEV-2                                          |
| 20 | Ops — MTTA SEV-1                        | timeseries      | `corelink_slo_burn_rate{slo="SLO-ONCALL-MTTA-SEV1", window="24h", env="$env"}`                                                                                                      | ≥ 1 SEV-3                                          |
| 21 | Ops — MTTR SEV-1                        | timeseries      | `corelink_slo_burn_rate{slo="SLO-ONCALL-MTTR-SEV1", window="24h", env="$env"}`                                                                                                      | ≥ 1 SEV-3                                          |
| 22 | Reliability — Backup verification       | timeseries      | `corelink_slo_burn_rate{slo="SLO-BACKUP-VERIFICATION", window="24h", env="$env"}`                                                                                                   | ≥ 1 SEV-2                                          |
| 23 | Replication lag — R2                    | timeseries      | `corelink_slo_burn_rate{slo="SLO-REPLICATION-LAG-R2", window="1h", env="$env"}`                                                                                                     | ≥ 6 SEV-2                                          |
| 24 | Replication lag — D1                    | timeseries      | `corelink_slo_burn_rate{slo="SLO-REPLICATION-LAG-D1", window="1h", env="$env"}`                                                                                                     | ≥ 6 SEV-2                                          |
| 25 | Replication lag — KV                    | timeseries      | `corelink_slo_burn_rate{slo="SLO-REPLICATION-LAG-KV", window="1h", env="$env"}`                                                                                                     | ≥ 6 SEV-2                                          |
| 26 | Replication lag — DO                    | timeseries      | `corelink_slo_burn_rate{slo="SLO-REPLICATION-LAG-DO", window="1h", env="$env"}`                                                                                                     | ≥ 6 SEV-2                                          |
| 27 | Efficiency — Dedup ratio                | timeseries      | `corelink_slo_burn_rate{slo="SLO-DEDUP-RATIO", window="24h", env="$env"}`                                                                                                            | ≥ 1 SEV-3                                          |
| 28 | Deploy safety burn                      | timeseries      | `corelink_slo_burn_rate{slo="SLO-DEPLOY-SAFE", window="6h", env="$env"}`                                                                                                             | ≥ 3 SEV-2                                          |
| 29 | Multi-burn alert hit-list (table)        | table           | `topk(20, max by (slo, window) (corelink_slo_burn_rate{env="$env"} > 1))`                                                                                                          | sort desc                                          |
| 30 | Runbook quick-links (panel-of-links)    | text            | static map: SLO-* → `specs/_runbooks/RB-<x>.md`                                                                                                                                    | n/a                                                |

## SLO IDs covered
SLO-AVAIL-CP, SLO-AVAIL-CAS-GET, SLO-AVAIL-CAS-PUT, SLO-AVAIL-AC, SLO-AVAIL-EXEC, SLO-LAT-CAS-GET, SLO-LAT-CAS-PUT, SLO-LAT-CAS-PUT-MULTIPART, SLO-LAT-AC-HIT, SLO-CORRECT-CAS, SLO-CORRECT-ISO, SLO-FRESH-BILLING, SLO-FRESH-DSR-ERASURE, SLO-DEPLOY-SAFE, SLO-ADMIN-CONFIG-PROPAGATION, SLO-ADMIN-DUAL-APPROVAL-LATENCY, SLO-ADMIN-ROTATION-OVERLAP, SLO-ADMIN-ROLLBACK-RECOVERY, SLO-RTO-REGION-FAILOVER, SLO-RPO-REGION, SLO-ONCALL-MTTA-SEV1, SLO-ONCALL-MTTR-SEV1, SLO-BACKUP-VERIFICATION, SLO-REPLICATION-LAG-R2, SLO-REPLICATION-LAG-D1, SLO-REPLICATION-LAG-KV, SLO-REPLICATION-LAG-DO, SLO-DEDUP-RATIO.

## Alert IDs covered
`slo-*-fast`, `slo-*-slow`, `slo-*-medium`, `slo-*-ticket` (per SLO; defined in `observability/alerts/slo.yaml`).

## Runbook IDs linked
RB-SLO-AVAIL-FAST-BURN, RB-SLO-LATENCY-BREACH, RB-CAS-INTEGRITY-VIOLATION, RB-TENANT-ISOLATION-BREACH, RB-ACTIVE-FAILOVER, RB-BACKUP-VERIFICATION-FAILURE.

## Compliance hooks
- SOC 2 CC4.1 (continuous monitoring), CC7.2 (anomaly detection), A1.1 (availability commitment).
- ISO 27001 A.12.1.3 (capacity), A.16.1 (incident management).
