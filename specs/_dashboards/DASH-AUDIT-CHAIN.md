---
id: "DASH-AUDIT-CHAIN"
type: "observability_model"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Security Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["dashboard", "audit", "merkle", "append-only", "compliance", "soc2-cc7-2", "soc2-cc4-1"]
---

# DASH-AUDIT-CHAIN — Audit-Chain Merkle Health

## Metadata

- **Purpose:** visibility into the append-only Merkle audit chain that backs every audit event (CTRL-AUDIT-APPEND-ONLY / INV-AUDIT-APPEND-ONLY). Surfaces head depth, append throughput, daily integrity-verify pass/fail, and tamper signals. Used by Security + Compliance daily.
- **Audience:** Security Lead, Compliance Officer, Privacy Officer.
- **Refresh:** 1m.
- **Variables:** `$env` (prod), `$region` (multi).
- **Time range:** default `now-30d`.

## Panels

| # | Title                                      | Type        | Query                                                                                                                            | Threshold / Alert                          |
|---|--------------------------------------------|-------------|-----------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------|
| 1 | Audit-chain head depth                     | stat         | `corelink_audit_chain_head_depth{env="$env"}`                                                                                      | alert if no growth > 5m (stall)            |
| 2 | Append throughput (events/sec)             | timeseries   | `sum by (region) (rate(corelink_audit_chain_appends_total{env="$env"}[1m]))`                                                       | alert if 0 for 5m sustained                |
| 3 | Append latency p99                         | timeseries   | `histogram_quantile(0.99, sum by (le) (rate(corelink_audit_chain_append_duration_seconds_bucket{env="$env"}[5m])))`                  | alert ≥ 100ms                              |
| 4 | Daily integrity-verify pass/fail (30d)     | timeseries   | `corelink_audit_chain_integrity_verify_outcome{env="$env"}` (1=pass, 0=fail)                                                       | red ANY fail = SEV-1                       |
| 5 | TLA+ checker pass rate (90d)               | stat         | `avg_over_time(corelink_audit_chain_tla_check_pass{env="$env"}[90d])`                                                              | alert if < 1.0 (zero-tolerance)             |
| 6 | Head divergence across regions             | stat         | `max(corelink_audit_chain_head_depth{env="$env"}) - min(corelink_audit_chain_head_depth{env="$env"})`                              | alert > 100 events (replication lag)       |
| 7 | Event-type rate (top-N CloudEvents)        | bar          | `topk(10, sum by (cloudevent_type) (rate(corelink_audit_chain_appends_total{env="$env"}[1h])))`                                     | n/a                                        |
| 8 | Sink-failure rate (R2 + D1 + Neon outbox)  | timeseries   | `sum by (sink) (rate(corelink_audit_chain_sink_errors_total{env="$env"}[5m]))`                                                       | alert any sink > 0.001 error/s             |
| 9 | DSR receipt issuance rate                  | timeseries   | `sum (rate(corelink_audit_chain_appends_total{env="$env", cloudevent_type="dev.hugr.corelink.dsr.receipt.issued.v1"}[1h]))`        | n/a                                        |
| 10 | BYOK rotation events                       | timeseries   | `sum by (provider) (increase(corelink_audit_chain_appends_total{env="$env", cloudevent_type=~"dev.hugr.corelink.byok..*"}[24h]))`   | n/a                                        |
| 11 | Tamper-attempt detections (90d)            | stat         | `sum(increase(corelink_audit_chain_tamper_attempts_total{env="$env"}[90d]))`                                                        | red ANY > 0 = SEV-1                        |
| 12 | Chain age — head root timestamp            | stat         | `time() - corelink_audit_chain_head_timestamp{env="$env"}`                                                                          | alert > 60s (no recent append in 60s)     |

## SLO IDs covered
SLO-CORRECT-CAS (correctness depends on audit-chain integrity), SLO-FRESH-DSR-ERASURE (receipt issuance).

## Alert IDs covered
`audit-chain-stall`, `audit-chain-integrity-fail`, `audit-chain-tamper`, `audit-chain-sink-failure`, `audit-chain-head-divergence`.

## Runbook IDs linked
RB-AUDIT-CHAIN-STALL, RB-AUDIT-CHAIN-INTEGRITY-VIOLATION, RB-AUDIT-CHAIN-TAMPER-RESPONSE, RB-DSR-RECEIPT-FAILURE.

## Compliance hooks
- SOC 2 CC4.1 (continuous monitoring), CC7.2 (anomaly detection), CC9.1 (data integrity), PI1.2/PI1.3/PI1.5 (processing integrity).
- ISO 27001 A.12.4 (logging + monitoring), A.18.1.3 (records protection).
- LGPD Art. 37 (records of processing).
