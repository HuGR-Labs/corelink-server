---
id: "AUDIT-S20-30D-STAGING-EVIDENCE"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-007"
tags:
  - "audit"
  - "s20"
  - "30d-staging"
  - "ga-evidence"
  - "evidence-framework"
  - "evt-021"
  - "draft"
---

# AUDIT-S20-30D-STAGING-EVIDENCE — 30-Day Sustained Staging Evidence Framework (Pre-GA)

> **Status:** DRAFT (framework only). The 30-day observation window opens on **D+30** (S-20 implementation gate closed) and closes on **D+60** (GA Evidence Gate). Synthetic data below is **placeholder** until real per-day metrics land; this document is the **canonical evidence collector** the GA gate will consume.
> **Parent:** [WI-S20-007](../04_sprints/S20/work_items/WI-S20-007-30d-staging-tla-4-runbooks-90d-sbom-closing-prr.md)
> **EVT:** EVT-021 (GA Evidence Pack — 30d staging stability).

---

## 1. Purpose

Provide the **schema + collection mechanism + acceptance verdict** for the 30-day sustained staging observation window required by:

- `_spec_contract.md` §6.1 — "30d sustained staging: zero SEV-1; < 3 SEV-2 not resolved (concurrent observation period; documented post-S-17 chaos automation 4-week period)".
- `WI-S20-007` §2.1 deliverable S20-007-D1.
- Engineering Gate DoD binary criterion.

The framework is **complete pre-observation**; the **verdict section §6** is **DRAFT pending the real 30d window**.

---

## 2. Observation window math

| Phase | Window | Source |
|---|---|---|
| S-17 chaos automation 4w concurrent | D-30 → D+0 (sprint kickoff) | S-17 WI-S17-001..007 cumulative chaos cadence |
| S-20 implementation (gate work) | D+0 → D+30 | this sprint |
| **S-20 30d sustained observation** | **D+30 → D+60** | this document |
| Cumulative coverage pré-GA | ~50 days sequential, **NÃO overlap** | per Lote 10.20 codex P1 canonical alignment |
| GA Evidence Gate decision | **D+60** | PRR-S20-CLOSING |

The 30-day window is **sequential** to the S-17 4-week chaos coverage (not overlapping) so the cumulative pré-GA observation is **~50 days** of production-equivalent staging.

---

## 3. Acceptance criteria (binary; GA gate)

| # | Criterion | Threshold | Source of truth |
|---|---|---|---|
| C1 | **SEV-1 incidents in 30d** | **0** | PagerDuty + DASH-GA-READINESS `corelink_30d_staging_sev_count_gauge{severity="sev_1"}` |
| C2 | **SEV-2 incidents NOT resolved at D+60** | **≤ 3** | PagerDuty + DASH-GA-READINESS same gauge severity="sev_2" |
| C3 | **SLO-AVAIL-CAS-PUT** sustained | **≥ 99.9%** rolling 30d | DASH-CAS-AVAILABILITY |
| C4 | **SLO-AVAIL-CAS-GET** sustained | **≥ 99.9%** rolling 30d | DASH-CAS-AVAILABILITY |
| C5 | **SLO-LAT-CAS-GET** p99 sustained | **< 300ms** rolling 30d | DASH-CAS-LATENCY |
| C6 | **SLO-FRESH-DSR-ERASURE** sustained | **≤ 30d** per request | DASH-PRIVACY |
| C7 | **SLO-FRESH-BILLING** reconciliation drift sustained | **< 0.1%** over 24h | DASH-BILLING |
| C8 | **SLO-INCIDENT-RESPONSE-SYNTHETIC-PAGE** p99 sustained | **< 5 min** (S-20 novo; weekly synthetic page per WI-S20-006) | DASH-INCIDENT-RESPONSE |
| C9 | **Chaos drills executed weekly** | **≥ 4 drills** in window | S-17 cadence inherited; per-week artefact in `specs/_audits/2026-*-chaos-*.md` |
| C10 | **DR drill semestral** | **≥ 1 in 6-month rolling window** | S-17 DR drill schedule (next: 2026-06-15) |
| C11 | **Runbook drills monthly** | **≥ 1 in 30d window** | S-17 cadence; this doc §5 cross-links monthly drills |
| C12 | **Audit chain integrity (INV-AUDIT-APPEND-ONLY)** verified daily | **30/30 days clean** | TLA+ `audit_immutability` + `scripts/verify_audit_chain.py` (S-09) daily cron |
| C13 | **Zero data residency violations** (INV-DATA-RESIDENCY + INV-REGION-NO-CROSS-LEAK) | **0** in 30d | `corelink_cross_region_violations_total` Prometheus counter + DASH-RESIDENCY |

Any **single criterion missing** = **REJECTED** verdict → remediation cycle + window restart per spec contract §19 (criteria C1, C3..C8, C12, C13 are **non-waivable**).

---

## 4. Per-day evidence schema

Each day in the 30-day window writes a row to `specs/_audits/30d-staging/<YYYY-MM-DD>.md` with the front matter below.

```yaml
---
id: "AUDIT-S20-30D-DAY-<NN>"
type: "evidence-row"
date: "2026-MM-DD"
day_index: <1..30>
sev_1_count: 0
sev_2_count: 0
sev_2_unresolved_carryover: 0
slo_avail_put_pct: 99.95
slo_avail_get_pct: 99.97
slo_lat_get_p99_ms: 187
slo_fresh_dsr_max_d: 12
slo_fresh_billing_drift_pct: 0.04
slo_incident_response_p99_s: 92
chaos_drill_ran_today: false
runbook_drill_ran_today: false
audit_chain_verified: true
residency_violations: 0
---
```

A daily aggregator (`scripts/aggregate_30d_evidence.py` — **planned WI-S20-007.2**) rolls the 30 rows into the **§6 verdict table**.

---

## 5. Concurrent S-17 chaos cadence cross-links

The S-17 chaos automation 4-week period (D-30..D+0) already provides:

- Weekly region-outage chaos drill: 4 ran (artefacts: `specs/_audits/2026-05-14-region-outage-chaos-s14.md` + 3 follow-ons during S-17 cycle).
- Weekly BYOK kill-switch drill: 4 ran (rotation AWS / GCP / Azure / Vault; one artefact: `specs/_audits/2026-05-14-byok-kill-switch-drill-aws.md`).
- Monthly tabletop: 1 ran (`specs/_audits/2026-05-14-s17-tabletop-byok-revoke.md`).

During the S-20 30-day observation window (D+30..D+60) the **same cadence continues**:

- 4 chaos drills (one per week).
- 1 monthly tabletop (mid-window).
- 1 semestral DR drill (scheduled 2026-06-15 — falls inside this window).
- Daily audit-chain integrity verification cron (INV-AUDIT-APPEND-ONLY).

---

## 6. Verdict (DRAFT — pending 30d observation window)

> **Status:** **DRAFT** — the 30d observation window opens at D+30 (post-S-20 implementation SEAL) and closes at D+60. The table below is **synthetic placeholder** populated once real per-day metrics land. **DO NOT** treat as evidence-grade until `doc_status: SEALED` is set on this audit.

| Criterion | Threshold | Observed (placeholder) | Verdict |
|---|---|---|---|
| C1 SEV-1 | 0 | **pending** | **DRAFT** |
| C2 SEV-2 unresolved | ≤ 3 | **pending** | **DRAFT** |
| C3 SLO-AVAIL-CAS-PUT | ≥ 99.9% | **pending** | **DRAFT** |
| C4 SLO-AVAIL-CAS-GET | ≥ 99.9% | **pending** | **DRAFT** |
| C5 SLO-LAT-CAS-GET p99 | < 300ms | **pending** | **DRAFT** |
| C6 SLO-FRESH-DSR-ERASURE | ≤ 30d | **pending** | **DRAFT** |
| C7 SLO-FRESH-BILLING drift | < 0.1% | **pending** | **DRAFT** |
| C8 SLO-INCIDENT-RESPONSE p99 | < 5 min | **pending** | **DRAFT** |
| C9 Chaos drills weekly | ≥ 4 | **pending** | **DRAFT** |
| C10 DR drill semestral | ≥ 1 | **pending** | **DRAFT** |
| C11 Runbook drills monthly | ≥ 1 | **pending** | **DRAFT** |
| C12 Audit chain integrity | 30/30 days | **pending** | **DRAFT** |
| C13 Residency violations | 0 | **pending** | **DRAFT** |

**Overall verdict (pending):** **DRAFT** — the GA Engineering Gate cannot pass until this table is populated with real observation data and resealed.

---

## 7. References

- WI-S20-007 §2.1, §4 deliverable S20-007-D1, §6.1.
- WI-S20-006 — incident response 24/7 cadence (synthetic page weekly).
- WI-S17-006/007 — chaos automation 4-week pre-condition.
- SLO-CATALOG cumulative — see `specs/03_architecture/slo_catalog.md` (Lote 9.5+).
- INV-AUDIT-APPEND-ONLY — verified by TLA+ `specs/tla/audit_immutability.tla` (already GREEN in CI per S-12).
- DASH-GA-READINESS — Grafana dashboard (live, embedded panels DASH-30D-STAGING + DASH-CUMULATIVE-WAIVER + DASH-TLA-CI).

**Fim AUDIT-S20-30D-STAGING-EVIDENCE (DRAFT).**
