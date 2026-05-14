---
id: "RB-ONCALL-POLICY"
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
tags: ["runbook", "oncall", "rotation", "fatigue", "burnout-prevention", "rb-oncall-policy", "wi-s17-005"]
---

# RB-ONCALL-POLICY — Oncall Rotation, Fatigue, and Handoff Policy

> **Parent WI:** [WI-S17-005](../04_sprints/S17/work_items/WI-S17-005-oncall-pagerduty-schedule-fadigue-tracking-dashboard.md)
> **Spec contract refs:** §5.5 R-S17-12 / §5.5 R-S17-13 / §9.8 / §15 rows 9–10
> **Canonical reference:** Google SRE Workbook Ch 8 + PagerDuty Incident Response Documentation

## 1. Scope and intent

This runbook codifies the canonical oncall policy enforced by
`corelink-oncall` (Rust) + PagerDuty Schedule API + Grafana Cloud
dashboard `DASH-ONCALL-FATIGUE`. It defines the cadence, handoff
procedure, fatigue threshold response, and escalation chain. The
policy is the operational complement to the Lote 10.17 codex P1 fix:
alerts alone NÃO suficientes — HARD thresholds trigger automatic
rotation handoff + mandatory recovery rotation skip.

## 2. Rotation cadence (weekly)

| Tier   | Role                            | Rotation cadence       | Shift duration cap | Protection period |
| ------ | ------------------------------- | ---------------------- | ------------------ | ----------------- |
| Tier 1 | Primary responder               | Weekly                 | 7 days (hard cap)  | 14 days (no oncall) |
| Tier 2 | Escalate (5min unack)           | Weekly                 | 7 days (hard cap)  | 14 days (no oncall) |
| Tier 3 | Architect / Security if needed  | Weekly                 | 7 days (hard cap)  | 14 days (no oncall) |

**Hard caps**:

- A shift NEVER exceeds 7 days. The PagerDuty schedule enforces this
  via `rotation_turn_length_seconds: 604800`. Extensions require HR
  approval + explicit override in `infra/pagerduty/schedule.yaml`.
- A 14-day post-shift protection period bars the outgoing oncall
  from being re-rostered on any tier. The protection window is
  enforced by `RotationLedger::start_shift` (returns
  `OncallError::InvalidRotation` if violated) and mirrored in
  PagerDuty's restriction list.

## 3. Handoff procedure (15-min sync per shift change)

Every shift change runs a 15-min sync between the outgoing and
incoming oncall. The template lives at
`specs/_templates/oncall_handoff.md` and is populated end-of-shift.

Checklist:

1. **Outgoing oncall** runs the handoff template + ensures all
   active incidents/alerts/follow-ups are documented.
2. **Incoming oncall** reads the template, confirms understanding of
   active incidents/follow-ups, asks clarifying questions.
3. **15-min sync** (canonical knowledge transfer per spec contract
   §15 row 9) — outgoing walks incoming through the state.
4. **PagerDuty schedule** auto-rotates at shift boundary; both
   engineers verify they appear in the correct slot via the
   PagerDuty UI before sync ends.
5. **Audit emit**: `corelink.oncall.shift_started` (incoming) +
   `corelink.oncall.shift_ended` (outgoing) records appended to the
   audit chain BEFORE the ledger mutation per
   `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`.

## 4. Fatigue threshold response

The canonical threshold matrix (Lote 10.17 codex P1 fix; see
`crates/corelink-oncall/src/threshold.rs`):

| Counter                          | Soft (alert + 1:1)     | HARD (auto-handoff or block)           |
| -------------------------------- | ---------------------- | -------------------------------------- |
| Sev1 / shift                     | > 2 alert + manager 1:1 | > 3 automatic rotation handoff + 48h hold |
| Sev2 / shift                     | > 5 alert              | > 8 automatic rotation handoff + 48h hold |
| Pages / 30d in non-rotation      | > 10 burnout signal    | > 15 mandatory 1-month rotation block  |

### 4.1 Soft threshold response

- Grafana panel `DASH-ONCALL-FATIGUE` lights up the affected counter
  (yellow → orange).
- PagerDuty emits a Sev2 ticket to the oncall manager queue.
- Manager schedules a 1:1 with the affected engineer within 24h.
- No automatic schedule mutation; oncall continues current shift.

### 4.2 HARD threshold response — mandatory recovery rotation skip

- `RotationLedger::evaluate_and_handoff_if_needed` returns
  `HandoffDecision::RotateToBackup` (Sev1>3 or Sev2>8) or
  `HandoffDecision::MandatoryRotationBlock` (Pages>15 non-rotation).
- `PagerDutyClient::handoff_to_backup` is invoked: schedule mutates
  to the backup engineer; outgoing oncall is added to the PagerDuty
  exception list for 48h (or 30d for `MandatoryRotationBlock`).
- Audit chain emits `corelink.oncall.fatigue_threshold_breached`
  followed by `corelink.oncall.handoff_executed` BEFORE the
  schedule mutation (fail-CLOSED).
- Incident commander confirms handoff per `oncall_handoff.md`
  template; manager + SRE lead are paged via the canonical
  escalation chain (§5).
- For `MandatoryRotationBlock`: HR conversation per company policy +
  manager's manager notified; engineer removed from rotation for
  1 month minimum.

## 5. Escalation chain

```
Sev0/Sev1 alert
  ↓ (immediate page)
Tier 1 primary responder
  ↓ (5min unack → escalate per Tier::escalation_delay_seconds())
Tier 2 escalate
  ↓ (10min unack → escalate)
Tier 3 architect / security
  ↓ (no further automatic escalation arm; manual page to)
SRE lead + Engineering Director
  ↓ (Sev0 only)
CTO + Customer comms
```

PagerDuty escalation policy IDs:

- `tier_1_to_tier_2_after_seconds: 300`
- `tier_2_to_tier_3_after_seconds: 600`
- Tier 3 → manual escalation (no auto-escalation past Tier 3 to
  avoid wake-paging the CTO on a flapping alert).

## 6. MTTA / MTTR targets (Sev1)

- **MTTA target**: < 5 min (300 s). Instrumented via
  `corelink_incident_mtta_seconds{severity=sev1, tier}`. Alert at
  > 240s (4 min — head-room warning).
- **MTTR target**: < 30 min (1800 s). Instrumented via
  `corelink_incident_mttr_seconds{severity=sev1, tier}`. Alert at
  > 1500s (25 min — head-room warning).
- 30-day P50 + P99 visualised on `DASH-ONCALL-FATIGUE` panels 4/5.

## 7. Runbook URL standardisation (per S-09 R-S09-14)

Every PagerDuty alert payload MUST include a `runbook_url` field
pointing to the canonical runbook (this file for oncall policy
disputes; FM-specific runbooks for per-FM responses). PRs that ship
an alert without `runbook_url` fail the
`.github/workflows/alert-rules-validate.yml` CI gate (deferred to
PRR ship gate; matches S-09 invariant).

## 8. Regional coverage (24/7 future-proof)

| Region    | GA at S-17 | Notes                                                   |
| --------- | ---------- | ------------------------------------------------------- |
| US-East   | yes        | Primary rotation; canonical UTC schedule anchor.        |
| EU-West   | yes        | Secondary rotation; covers UTC+0/+1 sleep-hour window.  |
| APAC      | no (pós-S-20) | Placeholder in `infra/pagerduty/schedule.yaml`; APAC GA deferred per spec contract §5.5. |

## 9. Reconcile + drift detection

- Quarterly: `corelink-oncall` D1 mirror reconciled against PagerDuty
  Schedule API; drift > 1 shift surfaces as a Sev2 ticket.
- Daily: `oncall_pages` GC prunes rows older than 60 days
  (rolling-30d window + 30d retention pad).

## 10. Manual override / break-glass

- Manual rotation extension (> 7d) requires `--break-glass` flag on
  the PagerDuty CLI + HR notification + 5-Why post-mortem mandatory
  within 7 days.
- Manual handoff (out of band) emits
  `corelink.oncall.handoff_executed` with `decision: manual` (not
  one of the canonical 4 `HandoffDecision` arms — surface in the
  audit chain for audit review).

## 11. Change log

| Version | Date       | Author                          | Change                                   |
| ------- | ---------- | ------------------------------- | ---------------------------------------- |
| 1.0.0   | 2026-05-14 | Gustavo (via Claude Opus 4.7)   | Initial RB-ONCALL-POLICY for WI-S17-005. |
