---
id: "DASH-INDEX"
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
tags: ["observability", "dashboards", "grafana", "slo", "r-prep", "soc2-cc4-1"]
---

# DASH-INDEX — CoreLink GA Dashboards Catalog (Master)

> **Status:** DRAFT. Promotes to ACTIVE when ≥ 2 reviewers (SRE Lead + Security Lead) attest.
>
> **Purpose:** master catalog of the **concrete Grafana dashboards** CoreLink ships at GA. Companions the canonical telemetry sources:
> - `specs/03_architecture/observability_model.md` (metrics naming, labels canon, alert clock, retention budget) — this catalog extends §8 of that doc with executable specs.
> - `specs/03_architecture/slo_catalog.md` (30+ SLOs — Avail / Latency / Correctness / Freshness / Ops / DR / Replication) — each SLO referenced here MUST exist there.
>
> **Audit cross-link:**
> - `ROADMAP-TO-GA.md` §6 — Wave R-6 sustained-staging observation criteria (0 SEV-1, ≤ 3 SEV-2, audit-chain daily verify) require dashboards to make burn visible.
> - `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` CC4.1 (CTRL-COMP-001, continuous compliance) — dashboards are the "ongoing monitoring of controls" evidence stream.

## 1. Catalog (12 dashboards)

| DASH ID                       | Name                              | Audience                  | Refresh | Primary SLOs / signals                                                     | Spec file                                   |
|-------------------------------|-----------------------------------|---------------------------|---------|----------------------------------------------------------------------------|---------------------------------------------|
| `DASH-SLO-BURNDOWN`           | SLO Burndown (30-day rolling)     | SRE, Eng leadership       | 1m      | All 30+ SLOs from `slo_catalog.md` §4.* — multi-window burn (5m/1h/6h/24h) | `DASH-SLO-BURNDOWN.md`                      |
| `DASH-CUSTOMER-TRAFFIC`       | Customer Tier & Top-Tenant Traffic | Product, Sales, Support  | 30s     | Per-`plan` RPS, bandwidth, top-N `tenant_id` (sampled)                     | `DASH-CUSTOMER-TRAFFIC.md`                  |
| `DASH-INCIDENT-TRIAGE`        | Incident Triage (active)          | Oncall, IR commander      | 10s     | Alert fire correlation with SLO burn + recent deploys + DR posture         | `DASH-INCIDENT-TRIAGE.md`                   |
| `DASH-CAPACITY-PLANNING`      | Capacity Planning (R2/D1/KV/DO)   | SRE, Eng Mgr              | 5m      | `corelink_resource_utilization_ratio` per backend + 30/90d linear forecast | `DASH-CAPACITY-PLANNING.md`                 |
| `DASH-BYOK-HEALTH`            | BYOK Envelope & Rotation Health   | Security Lead, SRE        | 1m      | 4-provider (AWS/GCP/Azure/HashiCorp) envelope op p99 + rotation status     | `DASH-BYOK-HEALTH.md`                       |
| `DASH-AUDIT-CHAIN`            | Audit-Chain Merkle Health         | Security, Compliance, Privacy | 1m  | `audit_chain_head_depth`, append throughput, daily integrity-verify pass    | `DASH-AUDIT-CHAIN.md`                       |
| `DASH-RATELIMIT-ABUSE`        | Rate-Limit Fires & Abuse Triage   | Security, SRE             | 30s     | `corelink_ratelimit_requests_total{outcome="denied"}` + top-N abusers      | `DASH-RATELIMIT-ABUSE.md`                   |
| `DASH-DSR-PIPELINE`           | DSR Pipeline (S-11)               | Privacy Officer, Legal    | 5m      | Open DSRs by right (access/erasure/portability/rectify) + SLA burn          | `DASH-DSR-PIPELINE.md`                      |
| `DASH-BILLING`                | Billing Pipeline (Stripe + DLQ)   | Finance, Eng              | 1m      | Stripe webhook 2xx/5xx, billing-events freshness, DLQ depth                | `DASH-BILLING.md`                           |
| `DASH-RELIABILITY`            | Reliability (Uptime + Error Budget)| Exec, Eng leadership     | 1m      | Uptime per SLO target tier + remaining error budget per service             | `DASH-RELIABILITY.md`                       |
| `DASH-DR-STATUS`              | DR / Failover Readiness            | SRE, Exec on incident     | 1m      | Cross-region replication lag (R2/D1/KV/DO) + last-drill freshness          | `DASH-DR-STATUS.md`                         |
| `DASH-COMPLIANCE-HEALTH`      | Compliance & GAP Register Health   | Compliance, DPO, Exec     | 1h      | GAP register state, drill cadence compliance, evidence freshness            | `DASH-COMPLIANCE-HEALTH.md`                 |

> **Cardinality reminder (`observability_model.md` §11.2):** dashboards that scope by `tenant_id` (CUSTOMER-TRAFFIC, RATELIMIT-ABUSE, DSR) MUST apply top-N or sampling — global panels MUST drop `tenant_id`.

## 2. Conventions

### 2.1 Naming
- Dashboard UID: `corelink-<dash-id-in-lowercase>` (per `observability_model.md` §8).
- Panel IDs: monotonically increasing within a dashboard; never reused.
- Variables: `$env`, `$region`, `$plan`, `$tenant_id` (only on tenant-scoped dashboards), `$slo` (only on SLO-burndown).

### 2.2 Refresh cadence policy
| Refresh | Use                                                                 |
|---------|---------------------------------------------------------------------|
| 10s     | Incident triage only (cost: high cardinality on live alert panels)  |
| 30s     | Customer-facing real-time (CUSTOMER-TRAFFIC, RATELIMIT-ABUSE)       |
| 1m      | Default (SLO, AUDIT-CHAIN, BYOK, BILLING, RELIABILITY, DR-STATUS)   |
| 5m      | Slow-moving (CAPACITY, DSR pipeline)                                |
| 1h      | Periodic compliance views (COMPLIANCE-HEALTH)                       |

### 2.3 Alert thresholds inside dashboards
Each panel that mirrors an alert in `observability/alerts/*.yaml` MUST set its threshold to the **same** multi-burn-rate value as the alert (e.g. SEV-1 fast-burn 14.4× over 5m). Drift between dashboard threshold and alert rule is a CI failure (validator: `scripts/validate_specs.py` future check + grep gate).

### 2.4 Cross-link discipline (per dashboard spec)
Every `DASH-*.md` spec MUST include sections:
- **SLO IDs covered** — list from `slo_catalog.md` §4.*
- **Alert IDs covered** — list from `observability/alerts/*.yaml`
- **Runbook IDs linked** — list from `specs/_runbooks/RB-*.md`
- **Compliance hooks** — SOC 2 CC IDs, ISO clauses, NIST AC/SC IDs (where applicable)

## 3. Provisioning

### 3.1 JSON template
- `specs/_dashboards/templates/grafana-dashboard.json.template` — minimal Grafana 11 dashboard with variable substitution slots (`{{DASH_ID}}`, `{{TITLE}}`, `{{REFRESH}}`, `{{PANELS_JSON}}`).

### 3.2 Generator
- `scripts/gen-grafana-provision.py` — reads each `specs/_dashboards/DASH-*.md`, extracts panel definitions (PromQL + viz + thresholds) and emits `infra/grafana/dashboards/<dash-id>.json` for Grafana provisioning.
- Invocation: `python3 scripts/gen-grafana-provision.py` (idempotent; CI-runnable; diff-checks against `infra/grafana/dashboards/` in pre-merge gate).

### 3.3 Validation
- All PromQL metric names referenced in any `DASH-*.md` MUST exist in `specs/03_architecture/observability_model.md` §4 (taxonomy). Validator: `scripts/gen-grafana-provision.py --validate` (alias for dry-run + metric-name cross-check).

## 4. SOC 2 CC4.1 evidence wiring

CC4.1 ("monitoring activities") is satisfied by the **continuous existence + freshness of these dashboards**, attested via:
1. `infra/grafana/dashboards/*.json` checked into git (generator output).
2. `evidence/dashboards/last-rendered.json` updated by nightly cron (panel-by-panel render with synthetic auth → captured to evidence bucket).
3. CTRL-COMP-001 references this INDEX directly (see `SOC2-EVIDENCE-ROLLUP-2026-05-15.md` row CC4.1).

## 5. Change control

- New dashboard: PR adds `DASH-<NAME>.md` + entry in §1 table + regenerated JSON. Reviewer = SRE Lead.
- Panel addition: PR edits the specific `DASH-*.md` only.
- Removal: requires ADR (`specs/03_architecture/adrs/`) explaining loss of signal coverage.

---

**End of DASH-INDEX.**
