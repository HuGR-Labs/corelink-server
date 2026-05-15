---
id: "DASH-DR-STATUS"
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
tags: ["dashboard", "dr", "failover", "replication", "rto", "rpo", "soc2-a1-2", "soc2-cc7-5"]
---

# DASH-DR-STATUS — DR / Failover Readiness

## Metadata

- **Purpose:** continuous readout of the four pillars of DR readiness — cross-region replication lag, backup verification freshness, drill cadence compliance, and last-flip readiness probe. Used in the weekly DR review and consulted live when a region degrades (companion to RB-ACTIVE-FAILOVER).
- **Audience:** SRE Lead, SRE on-call, Compliance, Exec on incident.
- **Refresh:** 1m.
- **Variables:** `$env` (prod), `$region_primary` (multi), `$region_sibling` (multi).
- **Time range:** default `now-7d`.

## Panels

| # | Title                                          | Type        | Query                                                                                                                              | Threshold / Alert                              |
|---|------------------------------------------------|-------------|-------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------|
| 1 | DR overall readiness (composite)               | stat         | `corelink_dr_failover_ready{env="$env"}`                                                                                            | red if 0 = SEV-2 page                          |
| 2 | R2 cross-region replication lag                | timeseries   | `max by (region) (corelink_replication_lag_seconds{resource="r2", env="$env"})`                                                     | alert > 300s (5min RPO)                        |
| 3 | D1 cross-region replication lag                | timeseries   | `max by (region) (corelink_replication_lag_seconds{resource="d1", env="$env"})`                                                     | alert > 5s                                     |
| 4 | KV cross-region replication lag                | timeseries   | `max by (region) (corelink_replication_lag_seconds{resource="kv", env="$env"})`                                                     | alert > 60s                                    |
| 5 | DO state sync-age                              | timeseries   | `max by (do_class) (corelink_replication_lag_seconds{resource="do", env="$env"})`                                                   | alert > 60s                                    |
| 6 | Replication-lag SLO burn (R2/D1/KV/DO)         | stat-grid    | `corelink_slo_burn_rate{slo=~"SLO-REPLICATION-LAG-.*", window="1h", env="$env"}`                                                    | ≥ 6 SEV-2                                      |
| 7 | Backup verification — last-run age              | stat         | `time() - corelink_backup_verification_last_run_timestamp{env="$env"}`                                                              | alert > 86400 (24h cadence)                    |
| 8 | Backup verification — daily pass-rate (30d)    | timeseries   | `avg_over_time(corelink_backup_verification_outcome{env="$env"}[30d])`                                                              | alert < 1.0 = SEV-2                            |
| 9 | DR-15 cold-restore drill freshness             | stat         | `time() - corelink_dr_drill_last_run_timestamp{drill="dr-15", env="$env"}`                                                          | alert > 90d                                    |
| 10 | DR-16 active-failover drill freshness           | stat         | `time() - corelink_dr_drill_last_run_timestamp{drill="dr-16", env="$env"}`                                                          | alert > 30d (monthly cadence)                  |
| 11 | RTO target compliance (last drill)              | stat         | `corelink_dr_drill_rto_seconds{env="$env"}`                                                                                          | red > 900 (DR-16 ≤ 15min) / > 14400 (DR-15 ≤ 4h) |
| 12 | RPO target compliance (last drill)              | stat         | `corelink_dr_drill_rpo_seconds{env="$env"}`                                                                                          | red > 300 (≤ 5min)                             |
| 13 | Active write-lease region                      | stat         | `corelink_active_write_lease_region{env="$env"}` (label readout)                                                                    | n/a                                            |
| 14 | Last failover event (annotation)               | annotations  | `corelink_failover_events_total` overlay                                                                                            | n/a                                            |
| 15 | Region-degraded probes (synthetic)              | timeseries   | `avg by (region) (corelink_region_synthetic_health_ratio{env="$env"})`                                                              | alert any region < 0.99                        |

## SLO IDs covered
SLO-RTO-REGION-FAILOVER, SLO-RPO-REGION, SLO-REPLICATION-LAG-R2, SLO-REPLICATION-LAG-D1, SLO-REPLICATION-LAG-KV, SLO-REPLICATION-LAG-DO, SLO-BACKUP-VERIFICATION.

## Alert IDs covered
`dr-not-ready`, `replication-lag-r2`, `replication-lag-d1`, `replication-lag-kv`, `replication-lag-do`, `backup-verify-overdue`, `backup-verify-fail`, `dr-drill-overdue-dr15`, `dr-drill-overdue-dr16`, `region-synthetic-degraded`.

## Runbook IDs linked
RB-ACTIVE-FAILOVER, RB-COLD-RESTORE-FROM-ZERO, RB-BACKUP-VERIFICATION-FAILURE, RB-REPLICATION-LAG-INVESTIGATE, RB-DR-DRILL-FAILURE.

## Compliance hooks
- SOC 2 A1.2 (availability — backup + recovery), CC7.5 (continuity), CC9.1 (data integrity).
- ISO 27031 §8.4 (BCM testing), ISO 27001 A.17.1 (continuity), A.12.3 (backup).
