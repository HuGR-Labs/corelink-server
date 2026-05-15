---
id: "DASH-RATELIMIT-ABUSE"
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
tags: ["dashboard", "ratelimit", "abuse", "security", "ddos", "soc2-cc6-6"]
---

# DASH-RATELIMIT-ABUSE — Rate-Limit Fires & Abuse Triage

## Metadata

- **Purpose:** real-time view of rate-limit decisions (per tenant, per IP, per ASN, per global bucket) plus a leaderboard of top abusers. Used by Security for abuse response, by SRE during DDoS-like incidents, and by Support to validate legitimate-vs-malicious traffic claims.
- **Audience:** Security Lead, SRE on-call, Support L3.
- **Refresh:** 30s.
- **Variables:** `$env` (prod), `$region` (multi), `$bucket` (tenant / ip / global / asn).
- **Time range:** default `now-6h`.

## Panels

| # | Title                                       | Type         | Query                                                                                                                          | Threshold / Alert                          |
|---|---------------------------------------------|--------------|---------------------------------------------------------------------------------------------------------------------------------|--------------------------------------------|
| 1 | Rate-limit denials/sec by bucket            | timeseries   | `sum by (bucket) (rate(corelink_ratelimit_requests_total{env="$env", outcome="denied"}[1m]))`                                    | alert global bucket > 1k/s sustained 2m    |
| 2 | Denied/permitted ratio                      | timeseries   | `sum by (bucket) (rate(corelink_ratelimit_requests_total{env="$env", outcome="denied"}[5m])) / sum by (bucket) (rate(corelink_ratelimit_requests_total{env="$env"}[5m]))` | alert ≥ 0.05 sustained 10m                  |
| 3 | Top-20 abusing tenants (1h denied)          | table        | `topk(20, sum by (tenant_id) (increase(corelink_ratelimit_requests_total{env="$env", bucket="tenant", outcome="denied"}[1h])))`   | alert if same tenant top-1 for 24h         |
| 4 | Top-20 abusing IPs (1h denied)              | table        | `topk(20, sum by (asn, country_code) (increase(corelink_ratelimit_requests_total{env="$env", bucket="ip", outcome="denied"}[1h])))` | alert single ASN > 30 % of IP denials       |
| 5 | Top-20 abusing ASNs (1h)                    | table        | `topk(20, sum by (asn) (increase(corelink_ratelimit_requests_total{env="$env", bucket="ip", outcome="denied"}[1h])))`              | n/a                                        |
| 6 | Auth failures by error_code (1h)             | bar          | `sum by (error_code) (increase(corelink_auth_errors_total{env="$env", error_code=~"AUTH_.*"}[1h]))`                              | alert AUTH_REPLAY spike (credential stuffing) |
| 7 | Geographic denial heatmap                   | geomap        | `sum by (country_code) (rate(corelink_ratelimit_requests_total{env="$env", outcome="denied"}[5m]))`                              | n/a                                        |
| 8 | Anomaly score (3-sigma vs 7d baseline)       | timeseries   | `(sum (rate(corelink_ratelimit_requests_total{env="$env", outcome="denied"}[5m])) - avg_over_time(sum (rate(corelink_ratelimit_requests_total{env="$env", outcome="denied"}[5m]))[7d:5m])) / stddev_over_time(sum (rate(corelink_ratelimit_requests_total{env="$env", outcome="denied"}[5m]))[7d:5m])` | alert ≥ 3 (anomaly)                         |
| 9 | Active circuit-breaker states (per tenant)  | stat-grid    | `corelink_ratelimit_circuit_state{env="$env"}` (0=closed, 1=half-open, 2=open)                                                   | alert ANY = 2 sustained 1h                  |
| 10 | Global bucket utilisation                  | gauge         | `corelink_ratelimit_global_bucket_util_ratio{env="$env"}`                                                                         | alert ≥ 0.8                                 |
| 11 | Anti-replay nonces seen (top tenants)       | table         | `topk(20, sum by (tenant_id) (increase(corelink_auth_replay_blocked_total{env="$env"}[1h])))`                                     | alert spike                                 |
| 12 | Pentest / red-team annotation overlay       | annotations   | static — `corelink_pentest_active` event overlay                                                                                  | n/a (signal hygiene)                        |

## SLO IDs covered
SLO-AVAIL-CP (over-rate-limiting hurts availability), SLO-CORRECT-ISO (abuse cross-tenant signals).

## Alert IDs covered
`ratelimit-global-spike`, `ratelimit-tenant-abuse-24h`, `ratelimit-asn-concentration`, `auth-replay-spike`, `ratelimit-anomaly-3sigma`.

## Runbook IDs linked
RB-TENANT-ABUSE-RESPONSE, RB-DDOS-MITIGATION, RB-AUTH-REPLAY-INVESTIGATE, RB-CIRCUIT-BREAKER-OPEN.

## Compliance hooks
- SOC 2 CC6.6 (logical access — prevention of unauthorized access), CC7.2 (anomaly detection).
- ISO 27001 A.13.1 (network controls), A.9.2.4 (management of secret authentication info).
- NIST SP 800-53 SC-5 (DoS protection).
