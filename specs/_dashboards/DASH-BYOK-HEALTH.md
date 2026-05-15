---
id: "DASH-BYOK-HEALTH"
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
tags: ["dashboard", "byok", "kms", "encryption", "rotation", "security", "soc2-cc6-1"]
---

# DASH-BYOK-HEALTH — BYOK Envelope Op Latency + Rotation Status

## Metadata

- **Purpose:** end-to-end health of the BYOK envelope-encryption path across all 4 supported KMS providers (AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault). Surfaces envelope-op p99 latency, fault rate, rotation status, and pending kill-switch tests. Used by Security Lead weekly and during enterprise onboarding.
- **Audience:** Security Lead, SRE, Compliance.
- **Refresh:** 1m.
- **Variables:** `$env` (prod), `$provider` (aws / gcp / azure / vault / all), `$tenant_id` (multi).
- **Time range:** default `now-7d`.

## Panels

| # | Title                                     | Type         | Query                                                                                                                              | Threshold / Alert                                    |
|---|-------------------------------------------|--------------|-------------------------------------------------------------------------------------------------------------------------------------|------------------------------------------------------|
| 1 | Envelope ops/sec by provider              | timeseries   | `sum by (provider) (rate(corelink_byok_envelope_ops_total{env="$env", provider=~"$provider"}[1m]))`                                  | n/a                                                  |
| 2 | Envelope-op p99 latency by provider       | timeseries   | `histogram_quantile(0.99, sum by (provider, le) (rate(corelink_byok_envelope_op_duration_seconds_bucket{env="$env"}[5m])))`           | alert ≥ 250ms per provider                           |
| 3 | Envelope-op error rate by provider        | timeseries   | `sum by (provider) (rate(corelink_byok_envelope_ops_total{env="$env", outcome="error"}[5m])) / sum by (provider) (rate(corelink_byok_envelope_ops_total{env="$env"}[5m]))` | alert ≥ 0.001                                       |
| 4 | Active KEK ID per provider                | table         | `corelink_byok_active_kek_id{env="$env"}` (label-only readout per tenant × provider)                                                  | n/a                                                  |
| 5 | KEK rotation status (last 90d)            | table         | `corelink_byok_kek_rotation_age_seconds{env="$env"} / 86400`                                                                          | alert if age > 90d (rotation overdue)               |
| 6 | Tenants in rotation-overlap window         | stat          | `count(corelink_byok_kek_rotation_overlap{env="$env"} > 0)`                                                                           | alert any > 7d (overlap window exceeded)            |
| 7 | Kill-switch drill last-run age             | stat          | `time() - corelink_byok_kill_switch_drill_last_run_timestamp{env="$env"}`                                                            | alert ≥ 90d (drill overdue)                          |
| 8 | DEK cache hit ratio                        | timeseries   | `avg by (provider) (corelink_byok_dek_cache_hit_ratio{env="$env"})`                                                                   | alert if < 0.95 (cost / latency risk)               |
| 9 | Provider outage propagation (fail-open?)   | stat-grid    | `corelink_byok_provider_circuit_state{env="$env"}` (0=closed, 1=half-open, 2=open)                                                    | red if any provider = 2 sustained 5m                 |
| 10 | Tenant → provider mapping (count)          | bar           | `count by (provider) (corelink_byok_active_kek_id{env="$env"})`                                                                       | n/a                                                  |
| 11 | Customer-managed key revocation events     | timeseries   | `sum by (provider) (increase(corelink_byok_kek_revoked_total{env="$env"}[24h]))`                                                       | alert any spike (potential incident)                 |
| 12 | SLO burn — admin rotation overlap          | timeseries   | `corelink_slo_burn_rate{slo="SLO-ADMIN-ROTATION-OVERLAP", env="$env"}`                                                                 | ≥ 1 = SEV-3 page Security Lead                      |

## SLO IDs covered
SLO-ADMIN-ROTATION-OVERLAP, SLO-ADMIN-CONFIG-PROPAGATION (KEK-config propagation), SLO-AVAIL-CP (envelope-op latency budget).

## Alert IDs covered
`byok-envelope-p99-breach`, `byok-error-rate`, `byok-rotation-overdue`, `byok-kill-switch-overdue`, `byok-provider-circuit-open`, `byok-kek-revoked-spike`.

## Runbook IDs linked
RB-BYOK-KILL-SWITCH-DRILL, RB-BYOK-ROTATION-OVERDUE, RB-BYOK-PROVIDER-OUTAGE, RB-BYOK-KEK-REVOKED-INCIDENT.

## Compliance hooks
- SOC 2 CC6.1 (logical access — cryptographic protection), CC6.7 (encryption in transit + at rest), CC7.2.
- ISO 27001 A.10.1 (cryptographic controls).
- NIST SP 800-57 (key management lifecycle).
