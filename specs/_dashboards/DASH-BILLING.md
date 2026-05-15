---
id: "DASH-BILLING"
type: "observability_model"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Finance / Eng Mgr"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["dashboard", "billing", "stripe", "dlq", "finance", "soc2-cc4-1"]
---

# DASH-BILLING — Billing Pipeline (Stripe + DLQ Health)

## Metadata

- **Purpose:** end-to-end billing pipeline observability — usage-event emission → aggregation → Stripe webhook delivery → DLQ depth → reconciliation. Used by Finance for revenue assurance and by Eng for pipeline integrity.
- **Audience:** Finance, Eng Mgr, SRE on-call (billing-events are SLO-tracked).
- **Refresh:** 1m.
- **Variables:** `$env` (prod), `$plan` (free / team / enterprise / all).
- **Time range:** default `now-7d`.

## Panels

| # | Title                                       | Type        | Query                                                                                                                              | Threshold / Alert                                   |
|---|---------------------------------------------|-------------|-------------------------------------------------------------------------------------------------------------------------------------|-----------------------------------------------------|
| 1 | Usage events emitted/sec (by type)          | timeseries  | `sum by (cloudevent_type) (rate(corelink_billing_events_total{env="$env", cloudevent_type=~"dev.hugr.corelink.cas.(written\|read).v1\|dev.hugr.corelink.execute.completed.v1"}[1m]))` | n/a                                                 |
| 2 | Billing-event freshness SLO burn            | timeseries  | `corelink_slo_burn_rate{slo="SLO-FRESH-BILLING", window="6h", env="$env"}`                                                          | ≥ 3 SEV-3 page Finance + SRE                        |
| 3 | Aggregation lag (event → aggregate)         | timeseries  | `histogram_quantile(0.99, sum by (le) (rate(corelink_billing_aggregation_lag_seconds_bucket{env="$env"}[5m])))`                       | alert ≥ 600s (10min)                                |
| 4 | Stripe webhook 2xx rate                     | timeseries  | `sum (rate(corelink_stripe_webhook_total{env="$env", status_class="2xx"}[5m])) / sum (rate(corelink_stripe_webhook_total{env="$env"}[5m]))` | alert < 0.995 sustained 15m                          |
| 5 | Stripe webhook latency p99                  | timeseries  | `histogram_quantile(0.99, sum by (le) (rate(corelink_stripe_webhook_duration_seconds_bucket{env="$env"}[5m])))`                       | alert > 2s                                          |
| 6 | DLQ depth (per queue)                       | stat-grid   | `corelink_billing_dlq_depth{env="$env"}` by queue (usage / webhook / invoice)                                                       | alert > 100 (any queue)                             |
| 7 | DLQ growth rate                             | timeseries  | `deriv(corelink_billing_dlq_depth{env="$env"}[1h])`                                                                                 | alert if growth > 10/min sustained 30m              |
| 8 | Failed events by error_code                 | bar         | `topk(10, sum by (error_code) (increase(corelink_billing_event_errors_total{env="$env"}[24h])))`                                     | alert UPSTREAM_STRIPE spike                          |
| 9 | Invoice generation success (last 30d)       | stat        | `sum(increase(corelink_invoice_generated_total{env="$env", outcome="ok"}[30d])) / sum(increase(corelink_invoice_generated_total{env="$env"}[30d]))` | alert < 0.999 (revenue assurance)                    |
| 10 | Revenue $$ by plan (last 30d, proxy)        | bar         | `sum by (plan) (increase(corelink_invoice_amount_usd_total{env="$env"}[30d]))`                                                       | trend                                               |
| 11 | Reconciliation drift (Stripe ↔ internal)    | stat        | `abs(corelink_revenue_internal_usd_total{env="$env"} - corelink_revenue_stripe_usd_total{env="$env"})`                              | alert > $100 / day (drift threshold)                |
| 12 | Dunning / past-due tenants                  | table        | `topk(20, corelink_tenant_past_due_amount_usd{env="$env"})`                                                                          | n/a                                                 |

## SLO IDs covered
SLO-FRESH-BILLING.

## Alert IDs covered
`billing-freshness-burn`, `stripe-webhook-error-rate`, `billing-dlq-growth`, `billing-aggregation-lag`, `invoice-generation-failure`, `revenue-reconciliation-drift`.

## Runbook IDs linked
RB-BILLING-DLQ-DRAIN, RB-STRIPE-WEBHOOK-FAILURE, RB-REVENUE-RECONCILIATION, RB-INVOICE-GENERATION-FAILURE.

## Compliance hooks
- SOC 2 CC4.1 (monitoring), CC9.1 (data integrity for financial reporting), PI1.4 (input completeness).
- ISO 27001 A.12.4 (logging).
- Internal: revenue-assurance attestation (Finance signs monthly).
