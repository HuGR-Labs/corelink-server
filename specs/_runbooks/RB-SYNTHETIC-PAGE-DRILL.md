---
id: "RB-SYNTHETIC-PAGE-DRILL"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-14"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "oncall", "synthetic-page", "drill", "mtta", "rb-synthetic-page-drill", "wi-s20-006", "24-7"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-synthetic-pager` was absorbed into `corelink-telemetry` via inline `mod synthetic_pager;` per SEAL `specs/_audits/sealed/2026-05-26-w35-p2-telemetry-absorption.md`. Canonical consumer path is now `corelink_telemetry::synthetic_pager::*` (e.g. `corelink_telemetry::synthetic_pager::Region`). Operational commands (`cargo run -p corelink-synthetic-pager-cli`, CF Worker bindings) using the absorbed crate name still work for HISTORICAL log/grep reference but new automation should use the umbrella.

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.

# RB-SYNTHETIC-PAGE-DRILL — Weekly Synthetic Page Drill Procedure

> **Parent WI:** [WI-S20-006](../04_sprints/S20/work_items/WI-S20-006-incident-response-24-7-pagerduty-3-regions-synthetic-page-weekly.md)
> **Spec contract refs:** §10.s20.8 (GA Evidence Gate D+60 criterion) · §5.5 R-S17-12
> **Canonical reference:** Google SRE Workbook Ch 13 + PagerDuty Synthetic Monitoring Pattern

## 1. Scope and intent

This runbook codifies the weekly **synthetic page drill** that
exercises the 24/7 follow-the-sun rotation introduced by WI-S20-006.
A drill is a controlled SEV-2 page emitted by the
`corelink-synthetic-pager` CF Cron Worker through a dedicated
PagerDuty service (`synthetic-drill` routing key; severity =
`sev2_synthetic`); the active region's primary on-call MUST
acknowledge within 5 minutes (`MTTA_BUDGET_MS`). Drill outcomes are
persisted in D1 `synthetic_page_drills` (migration 0042) and surfaced
on `dashboards/grafana/DASH-ONCALL-24-7.json`.

The drill is the operational evidence that:

1. The PagerDuty schedule for the active region is correctly populated.
2. The primary's mobile / SMS / email vectors deliver below budget.
3. The escalation chain fires when MTTA is breached.
4. The 24/7 follow-the-sun rotation has no coverage gaps (GA Evidence
   Gate D+60 — 30-day sustained `< 5 min MTTA p99`).

## 2. Cadence

| Window  | Trigger                                  | Region rotated   |
| ------- | ---------------------------------------- | ---------------- |
| Weekly  | CF Cron `0 14 * * 1` (Mondays 14:00 UTC) | 4-week cycle (see §3) |

The Monday-14:00-UTC anchor is inside the Americas shift (16:00→00:00
UTC) but explicitly NOT — the cron defaults to the active region per
the §3 cycle, NOT to the cron-emit clock.

## 3. Region rotation (4-week cycle)

| Week N % 4 | Target region    | Emit timestamp shape                     |
| ---------- | ---------------- | ---------------------------------------- |
| 0          | Americas         | Mon 20:00 UTC (mid-shift)                |
| 1          | EMEA             | Mon 04:00 UTC (mid-shift)                |
| 2          | APAC             | Mon 12:00 UTC (mid-shift)                |
| 3          | Boundary handoff | Sun 23:59 UTC (Americas→EMEA handoff)    |

The boundary handoff variant emits at the region boundary itself to
exercise the §11.2 RB-ONCALL-POLICY 5-min async handoff path.

## 4. Drill emit procedure

1. **CF Cron Worker fires** at the canonical anchor for week N.
2. Worker computes `Region` via `corelink_synthetic_pager::Region::
   for_utc_hour(now_hour)` (or the explicit boundary-week selector).
3. Worker mints `SyntheticDrillId` (`SP-` + UUIDv4 stripped) and a
   correlation id (PAT-CORRELATION-ID-001).
4. Worker POSTs to PagerDuty Events API v2 with:
   - `routing_key`: `<SYNTHETIC_DRILL_ROUTING_KEY>` (bound to the
     `synthetic-drill` service, NOT to any production service).
   - `event_action`: `trigger`.
   - `dedup_key`: the `drill_id` (so a re-emit collapses).
   - `payload.severity`: `info` (PagerDuty native severity; the
     synthetic-drill semantic severity is in `payload.custom_details.
     synthetic_severity = "sev2_synthetic"`).
5. Worker stamps `emit_ts_ms` in D1 `synthetic_page_drills` with
   `outcome = 'unacked'` (placeholder) and `engineer_slug = NULL`.

## 5. Ack receive procedure

1. PagerDuty's webhook receiver (`apps/server` route
   `/webhooks/pagerduty/synthetic`) ingests the ack event.
2. Receiver computes `mtta_ms = ack_ts_ms - emit_ts_ms` and classifies
   via `decide_drill_outcome`:
   - `mtta_ms <= 5 min` → `acked` (GREEN).
   - `5 min < mtta_ms <= 15 min` → `escalated` (AMBER; §6 escalation
     fires).
   - No ack after 15 min hard window → `unacked` (RED; §6 escalation
     fires).
3. Receiver UPSERTs `synthetic_page_drills` with the ack fields +
   ack vector (mobile / SMS / email) + final outcome.
4. Receiver emits Prometheus counters
   `corelink_synthetic_drill_total{region, outcome}` +
   `corelink_synthetic_drill_mtta_ms{region, ack_vector}` histogram.

## 6. Escalation if MTTA breached

| Trigger                                                  | Action                                                                                    | Owner                          |
| -------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ------------------------------ |
| `outcome = escalated` (single occurrence)                | PagerDuty Tier 2 paged automatically (per RB-ONCALL-POLICY §6); 5-Why ticket filed.       | Tier 2 oncall                  |
| `outcome = unacked` (single occurrence)                  | PagerDuty Tier 3 paged + Owner notified; mandatory 5-Why post-mortem within 24h.          | Tier 3 oncall + Owner          |
| Same region `escalated` twice in 4-week window           | Region staffing review opened; ADR-0034 reconsidered for that region.                     | Owner                          |
| Same region `unacked` twice in 90 days                   | **HARD blocker** for GA Evidence Gate D+60 criterion; sprint-close gate fails.            | Owner + Final Approver         |
| Drill emit fails (PagerDuty Events API 5xx / timeout)    | CF Cron re-tries 3× with exponential backoff (2s / 4s / 8s); after that, page Owner directly via Twilio fallback. | CF Cron Worker (automatic) |

## 7. GA Evidence Gate D+60 criterion

Per spec contract §10.s20.8, GA is **blocked** until the synthetic
drill outcome is `acked` for **≥ 30 consecutive days** (≥ 4
consecutive weekly drills with `outcome = 'acked'` AND `mtta_ms` p99
across the window `<= 5 min`).

The dashboard panel `DASH-ONCALL-24-7 → Drill streak (last 30d)`
shows the current streak; PR `seal/ga-ready` is blocked by CI
checking `corelink-synthetic-pager`'s D1 mirror against this
criterion.

## 8. Operator checklist (per-drill)

- [ ] Confirm CF Cron fired (`wrangler tail` shows synthetic emit log
      with `drill_id`).
- [ ] Confirm D1 row created with `outcome = 'unacked'` placeholder.
- [ ] Confirm PagerDuty incident created on the `synthetic-drill`
      service (NOT on a production service).
- [ ] Confirm primary on-call received the page on at least the
      preferred mobile-push vector (no fallback to SMS / email unless
      mobile-push genuinely failed — flagged in dashboard).
- [ ] Confirm ack within 5 min (visual; PagerDuty incident timeline).
- [ ] Confirm D1 row UPSERTed with `outcome = 'acked'` + populated
      ack fields.
- [ ] Confirm Prometheus counter incremented.
- [ ] If `outcome != 'acked'`: file 5-Why ticket per §6 escalation
      tier.

## 9. Manual override / break-glass

- Manual drill (out of band; e.g. validating a staffing change): use
  the CLI `cargo run -p corelink-synthetic-pager-cli -- emit --region
  emea --reason "STAFF-CHANGE-REVIEW"` (CLI binary is deferred to PRR
  ship gate; binary entrypoint is the production wrangler path).
- Manual ack injection (test the receiver in isolation): POST to
  `/webhooks/pagerduty/synthetic` with a forged signed body (HMAC
  with `PAGERDUTY_WEBHOOK_SECRET`); manual injections are tagged
  `correlation_id` with `MANUAL-` prefix and excluded from the GA
  Evidence Gate streak counter.

## 10. Privacy / audit

- `engineer_slug` is an opaque slug (e.g. `eng-001`); display names
  are NEVER persisted to D1 per CTRL-PRIV-001.
- Drill records are retained 365 days then purged via daily GC cron
  (deferred to PRR ship gate; equivalent to the `oncall_pages` GC
  cron).
- Every drill emit + ack + classification is audit-chained via
  `corelink-audit-chain` with `audit_event_type`:
  - `corelink.synthetic_drill.emitted`
  - `corelink.synthetic_drill.acked`
  - `corelink.synthetic_drill.escalated`
  - `corelink.synthetic_drill.unacked`

## 11. Change log

| Version | Date       | Author                        | Change                                   |
| ------- | ---------- | ----------------------------- | ---------------------------------------- |
| 1.0.0   | 2026-05-14 | Gustavo (via Claude Sonnet)   | Initial RB-SYNTHETIC-PAGE-DRILL (WI-S20-006). |
