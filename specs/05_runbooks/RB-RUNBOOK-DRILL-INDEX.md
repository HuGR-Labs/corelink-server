---
id: "RB-RUNBOOK-DRILL-INDEX"
type: "runbook"
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
tags: ["runbook", "s17", "drill-index", "pat-runbook-drill-001", "fm-202", "p0", "p1", "cadence", "wi-s17-003"]
---

# RB-RUNBOOK-DRILL-INDEX — P0/P1 Runbook Dry-run Catalog (PAT-RUNBOOK-DRILL-001 + FM-202)

> **WI-S17-003** — Canonical catalog of P0/P1 runbooks subject to the monthly dry-run cadence (PAT-RUNBOOK-DRILL-001 / Quality Standard 14.s17.2).
> **Cadence canonical:** 30 days (fixed seconds, not calendar months).
> **Drift trigger:** `duration_actual / duration_expected > 2.0` → `outcome=drift_flagged` + FM-202 post-mortem.

---

## 1. Purpose

This index is the single source of truth for the CF Cron monthly overdue scan (`scan_overdue` in `corelink-runbook-tracker`):

1. Every runbook listed below is in scope for the PAT-RUNBOOK-DRILL-001 monthly cadence.
2. The CF Cron job loads this list, queries D1 `runbook_drills` for each `runbook_id`'s `MAX(executed_at)`, and emits an `OverdueAlert` event when the most recent drill is `≥ 30 days` stale (or absent).
3. The S-20 GA gate requires the **P0/P1 subset** (25 of 47 total) to have a drill recorded within a rolling 90-day window.

Out of scope here (continuous coverage post-GA): P2/P3 runbooks — listed in `specs/05_quality/runbooks/` but not subject to monthly cadence at S-17.

## 2. Schema

Each entry uses the canonical [`RunbookId`](../../crates/corelink-runbook-tracker/src/lib.rs) shape `RB-<UPPER ALNUM/HYPHEN>+` plus drill cadence metadata:

| Field | Meaning |
|---|---|
| `runbook_id` | Canonical id (matches D1 `runbook_drills.runbook_id`). |
| `priority` | `p0` (storage / DB / auth blocking) or `p1` (degraded / compliance). |
| `expected_duration_min` | Wall-clock target from runbook body (`compute_drift` denominator). |
| `last_drill_date` | ISO-8601 date of most recent drill (`null` if never). |
| `next_due_date` | `last_drill_date + 30 days` (or `unscheduled` if never). |

## 3. P0/P1 Catalog (S-17 baseline; 25 entries)

```yaml
runbooks:
  # P0 — storage / data integrity
  - runbook_id: RB-FM-051
    priority: p0
    expected_duration_min: 10
    last_drill_date: 2026-05-05
    next_due_date: 2026-06-04
  - runbook_id: RB-FM-054
    priority: p0
    expected_duration_min: 15
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-060
    priority: p0
    expected_duration_min: 12
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-062
    priority: p0
    expected_duration_min: 20
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-305
    priority: p0
    expected_duration_min: 18
    last_drill_date: null
    next_due_date: unscheduled

  # P0 — DB / region
  - runbook_id: RB-FM-057
    priority: p0
    expected_duration_min: 25
    last_drill_date: 2026-05-12
    next_due_date: 2026-06-11
  - runbook_id: RB-FM-105
    priority: p0
    expected_duration_min: 30
    last_drill_date: null
    next_due_date: unscheduled

  # P0 — auth / supply chain
  - runbook_id: RB-FM-007
    priority: p0
    expected_duration_min: 20
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-160
    priority: p0
    expected_duration_min: 12
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-253
    priority: p0
    expected_duration_min: 25
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-258
    priority: p0
    expected_duration_min: 30
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-303
    priority: p0
    expected_duration_min: 25
    last_drill_date: null
    next_due_date: unscheduled

  # P0 — BYOK / privacy
  - runbook_id: RB-BYOK-REVOKE
    priority: p0
    expected_duration_min: 30
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-KEY-COMPROMISE
    priority: p0
    expected_duration_min: 45
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-HSM-UNAVAILABLE
    priority: p0
    expected_duration_min: 30
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-DATA-RESIDENCY-LEAK
    priority: p0
    expected_duration_min: 25
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-BREACH-NOTIF
    priority: p0
    expected_duration_min: 60
    last_drill_date: null
    next_due_date: unscheduled

  # P1 — meta-drill (FM-202 canonical mensal)
  - runbook_id: RB-FM-202
    priority: p1
    expected_duration_min: 15
    last_drill_date: 2026-05-19
    next_due_date: 2026-06-18

  # P1 — billing / abuse
  - runbook_id: RB-BILLING-001
    priority: p1
    expected_duration_min: 20
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-302
    priority: p1
    expected_duration_min: 25
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-250
    priority: p1
    expected_duration_min: 30
    last_drill_date: null
    next_due_date: unscheduled

  # P1 — admin / config / rollout
  - runbook_id: RB-FM-205
    priority: p1
    expected_duration_min: 20
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-201
    priority: p1
    expected_duration_min: 15
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-FM-206
    priority: p1
    expected_duration_min: 20
    last_drill_date: null
    next_due_date: unscheduled
  - runbook_id: RB-ROLLOUT-STUCK
    priority: p1
    expected_duration_min: 25
    last_drill_date: null
    next_due_date: unscheduled
```

## 4. Cadence math (canonical)

* **Window:** 30 days (canonical const `CADENCE_WINDOW_SECS = 30 * 24 * 60 * 60` in `corelink-runbook-tracker`).
* **Overdue:** `now - last_drill_ts >= CADENCE_WINDOW_SECS` (saturating; clock-skew safe — `now < last_drill_ts` never flags).
* **Drift:** `outcome = drift_flagged` when `duration_actual / duration_expected > 2.0` strict (exactly 2.0 → not flagged; spec §9.3 strict canonical).

## 5. CF Cron flow

1. Cron fires monthly (`0 12 1 * *` UTC).
2. Worker loads this catalog (parsed at build time into static `&[RunbookId]`).
3. Worker calls `scan_overdue(&recorder, &catalog, now)` against D1.
4. Each `OverdueAlert` is emitted as `EVT-017.runbook_overdue` audit event + Prometheus `corelink_runbook_dry_run_total{outcome="overdue"}` counter increment.
5. Slack alert to `#sre-oncall` channel.

## 6. Adding a runbook to the catalog

1. Confirm the runbook is P0/P1 (priority label in its front matter).
2. Append a YAML entry in §3 with `expected_duration_min` matching the runbook's "Estimated execution time" header.
3. Set `last_drill_date: null` + `next_due_date: unscheduled` until the first drill is recorded.
4. Run `python3 scripts/validate_specs.py` — this doc MUST pass the front-matter schema.

## 7. References

* `specs/04_sprints/S17/work_items/WI-S17-003-*.md` — parent WI.
* `crates/corelink-runbook-tracker/src/lib.rs` — pure-logic tracker + `scan_overdue` + drift math.
* `migrations/d1/0033_runbook_drills.sql` — D1 schema.
* `specs/05_quality/resilience_patterns.md` — PAT-RUNBOOK-DRILL-001 canonical.
* `specs/05_quality/failure_modes.md` — FM-202 mitigation.

---

**Fim RB-RUNBOOK-DRILL-INDEX.**
