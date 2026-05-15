---
id: "DASH-COMPLIANCE-HEALTH"
type: "observability_model"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Compliance Officer"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["dashboard", "compliance", "soc2", "iso27001", "gap-register", "drills", "soc2-cc4-1"]
---

# DASH-COMPLIANCE-HEALTH — GAP Register + Drill-Cadence + Evidence Freshness

## Metadata

- **Purpose:** the compliance-state-of-the-union dashboard. Visualises the GAP register (open / in-progress / closed), drill cadence compliance (BCP/DR + IR-tabletop + chaos), evidence freshness across all auditable streams, and waiver status. Used by Compliance Officer weekly, by Exec monthly, and by external auditor during fieldwork.
- **Audience:** Compliance Officer, DPO, Security Lead, Exec, external auditor (read-only).
- **Refresh:** 1h (compliance moves slowly; sub-hour is wasteful).
- **Variables:** `$env` (prod), `$framework` (soc2 / iso27001 / lgpd / pci / all).
- **Time range:** default `now-90d`.

## Panels

| # | Title                                         | Type         | Query                                                                                                                              | Threshold / Alert                          |
|---|-----------------------------------------------|--------------|-------------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------|
| 1 | GAP register state (open / WIP / closed)      | bar          | `sum by (state) (corelink_gap_register_count{env="$env", framework=~"$framework"})`                                                  | alert open > 5 ≥ 60d                       |
| 2 | GAP age distribution (open only)               | histogram    | `histogram_quantile(0.95, sum by (le) (corelink_gap_age_seconds_bucket{env="$env", state="open"}))`                                  | alert p95 > 90d                            |
| 3 | Top-10 oldest open GAPs                       | table        | `topk(10, corelink_gap_age_seconds{env="$env", state="open"})`                                                                       | sort desc                                  |
| 4 | Drill cadence compliance (BCP/DR + IR-TT)     | stat-grid    | `corelink_drill_cadence_compliance{env="$env"}` per drill type (DR-15 / DR-16 / IR-TT / chaos)                                       | red if any < 1.0 (overdue)                 |
| 5 | Chaos drill weekly cadence (Wave R-6)          | timeseries   | `sum (increase(corelink_chaos_drill_executed_total{env="$env"}[7d]))`                                                                | alert < 1 (weekly requirement)             |
| 6 | Evidence freshness — by control ID            | table         | `time() - corelink_evidence_last_collected_timestamp{env="$env"}` grouped by `ctrl_id`                                              | red > stream-specific TTL (DAILY/WEEKLY/MONTHLY) |
| 7 | SOC 2 control coverage (CC1-CC9 + A1/PI1/P)   | heatmap      | `corelink_soc2_control_coverage_ratio{env="$env"}` by `cc_id`                                                                        | red < 1.0 per control                      |
| 8 | ISO 27001 Annex A coverage (114 controls)     | heatmap      | `corelink_iso27001_control_coverage_ratio{env="$env"}` by `annex_id`                                                                | red < 1.0                                  |
| 9 | Active waivers (with expiry)                  | table         | `corelink_active_waivers{env="$env"}` joined with `corelink_waiver_expiry_timestamp`                                                 | alert expiry < 7d                          |
| 10 | Waiver burn (cumulative, R-prep)             | timeseries   | `sum (corelink_active_waivers{env="$env"})`                                                                                          | alert > waiver-budget threshold            |
| 11 | Sub-processor change events (90d, 30d-notice)  | timeseries   | `sum (increase(corelink_audit_chain_appends_total{env="$env", cloudevent_type=~"dev.hugr.corelink.subprocessor..*"}[90d]))`         | alert objection events > 0                  |
| 12 | Privacy notice translations active            | stat-grid    | `corelink_privacy_notice_active_version{env="$env"}` by locale                                                                       | red if any locale stale > 30d              |
| 13 | Pentest cadence freshness                     | stat          | `time() - corelink_pentest_last_run_timestamp{env="$env"}`                                                                          | alert > 365d (annual cadence)              |
| 14 | Audit-chain daily verify pass rate (90d)      | timeseries    | `avg_over_time(corelink_audit_chain_integrity_verify_outcome{env="$env"}[90d])`                                                     | alert < 1.0                                |
| 15 | RCA action backlog (post-mortem follow-up)    | stat          | `count(corelink_rca_actions_open{env="$env"})`                                                                                       | alert > 10 open ≥ 30d                      |

## SLO IDs covered
SLO-BACKUP-VERIFICATION, SLO-FRESH-DSR-ERASURE, SLO-ADMIN-DUAL-APPROVAL-LATENCY (compliance gates touch these).

## Alert IDs covered
`gap-register-stale`, `drill-cadence-overdue`, `evidence-stale`, `waiver-expiring`, `pentest-overdue`, `audit-chain-integrity-fail`, `subprocessor-objection`, `privacy-notice-locale-stale`, `rca-action-backlog`.

## Runbook IDs linked
RB-GAP-REGISTER-REVIEW, RB-DRILL-OVERDUE-RECOVERY, RB-EVIDENCE-FRESHNESS-RECOVERY, RB-WAIVER-EXPIRY-RENEWAL, RB-SUBPROCESSOR-CHANGE.

## Compliance hooks
- SOC 2 CC4.1 (continuous monitoring of controls — this dashboard IS the monitoring), CC4.2 (corrective actions), CC9.1 (operational management).
- ISO 27001 A.18.2 (information security reviews), Clause 9.1 (monitoring + measurement).
- LGPD Art. 50 (governance program), GDPR Art. 30 (records of processing).
