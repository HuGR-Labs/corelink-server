---
id: "DASH-DSR-PIPELINE"
type: "observability_model"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Privacy Officer"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["dashboard", "dsr", "privacy", "lgpd", "gdpr", "soc2-p-consent", "s-11"]
---

# DASH-DSR-PIPELINE — Data Subject Request Pipeline

## Metadata

- **Purpose:** end-to-end DSR pipeline observability across the S-11 privacy spine — receive → verify → queue → in-progress → per-backend ack (12 backends) → completed → receipt issued. Surfaces open DSRs by right type, age, SLA burn, and per-backend latency.
- **Audience:** Privacy Officer, DPO, Legal Liaison, SRE on-call.
- **Refresh:** 5m (DSRs are batched; sub-minute is wasteful).
- **Variables:** `$env` (prod), `$right` (access / erasure / portability / rectification / restriction / objection), `$jurisdiction` (LGPD / GDPR / CCPA / PIPEDA / all).
- **Time range:** default `now-30d`.

## Panels

| # | Title                                        | Type        | Query                                                                                                                                  | Threshold / Alert                              |
|---|----------------------------------------------|-------------|-----------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------|
| 1 | Open DSRs by right                           | bar          | `sum by (right) (corelink_dsr_open_count{env="$env", jurisdiction=~"$jurisdiction"})`                                                    | alert any > 50 (potential pipeline stall)      |
| 2 | DSR submission rate (24h)                    | timeseries   | `sum by (right) (rate(corelink_dsr_submitted_total{env="$env"}[1h]))`                                                                    | n/a                                            |
| 3 | DSR SLA burn (erasure, 30d window)            | timeseries   | `corelink_slo_burn_rate{slo="SLO-FRESH-DSR-ERASURE", window="24h", env="$env"}`                                                          | ≥ 1 SEV-2 (page Privacy Officer)               |
| 4 | DSR age distribution (open)                   | histogram    | `histogram_quantile(0.95, sum by (le, right) (corelink_dsr_age_seconds_bucket{env="$env", state!="completed"}))`                          | alert p95 > 25d (regulatory 30d limit)         |
| 5 | DSR state funnel (last 30d)                   | sankey       | aggregation of `corelink_dsr_state_transitions_total{env="$env"}` between received → verified → queued → in_progress → completed         | alert if stage drop ≥ 10 % (verification fail) |
| 6 | Per-backend ack latency p95 (12 backends)    | table        | `histogram_quantile(0.95, sum by (backend, le) (rate(corelink_dsr_backend_ack_duration_seconds_bucket{env="$env"}[7d])))`                  | alert any backend > 24h                        |
| 7 | DSR denied / refused (with legal ground)     | table        | `topk(20, increase(corelink_dsr_denied_total{env="$env"}[30d]) > 0)` grouped by (`legal_ground`)                                          | n/a (audit signal)                             |
| 8 | DSR failed (retry exhausted)                  | stat         | `sum(increase(corelink_dsr_failed_total{env="$env"}[7d]))`                                                                                | alert ANY > 0 = SEV-2 page                    |
| 9 | Consent grants / revocations (24h)            | timeseries   | `sum by (purpose) (rate(corelink_consent_events_total{env="$env", event=~"granted\|revoked"}[1h]))`                                       | alert revocation spike (3-sigma over 7d)       |
| 10 | Consent lapse propagation latency            | timeseries   | `histogram_quantile(0.99, sum by (le) (rate(corelink_consent_lapse_propagation_seconds_bucket{env="$env"}[1h])))`                          | alert > 300s (≤ 5min target)                   |
| 11 | Privacy-notice version coverage              | stat-grid     | `corelink_privacy_notice_active_version{env="$env"}` by locale (pt-BR / en-US / es-MX)                                                    | alert if any locale stale > 30d after publish  |
| 12 | DSR receipt issuance success rate            | timeseries    | `sum(rate(corelink_dsr_receipt_issued_total{env="$env"}[1h])) / sum(rate(corelink_dsr_completed_total{env="$env"}[1h]))`                  | alert < 1.0 (every completed DSR must issue)   |
| 13 | Residency violations (PAT-ROUTING-PINNED-001)| stat          | `sum(increase(corelink_residency_violations_total{env="$env"}[30d]))`                                                                     | alert ANY > 0 = SEV-1 (fail-CLOSED 451)        |

## SLO IDs covered
SLO-FRESH-DSR-ERASURE, SLO-CORRECT-ISO (residency).

## Alert IDs covered
`dsr-sla-burn-erasure`, `dsr-failed-retry-exhausted`, `consent-revoke-propagation-slow`, `consent-revocation-spike`, `residency-violation`, `privacy-notice-locale-stale`.

## Runbook IDs linked
RB-DSR-FAILURE-RECOVERY, RB-DPO-ESCALATION, RB-RESIDENCY-VIOLATION-RESPONSE, RB-CONSENT-PROPAGATION-FAILURE.

## Compliance hooks
- SOC 2 P-CONSENT (privacy criteria, full P-series).
- LGPD Art. 18 (data subject rights), Art. 8 (consent), Art. 23 (data residency).
- GDPR Art. 15-22 (subject rights), Art. 7 (consent), Art. 30 (records of processing).
