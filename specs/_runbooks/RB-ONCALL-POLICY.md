---
id: "RB-ONCALL-POLICY"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "oncall", "rotation", "fatigue", "burnout-prevention", "rb-oncall-policy", "wi-s17-005", "wi-s20-006", "24-7", "follow-the-sun"]
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

## 11. 24/7 follow-the-sun region split (WI-S20-006)

WI-S20-006 extends the WI-S17-005 single-region rotation to **3
regions × 8h shifts** for 24/7 coverage without on-call sleep-hour
sacrifice. Region assignment is computed by
`corelink_synthetic_pager::Region::for_utc_hour` and mirrored 1:1 in
the PagerDuty Schedule API restriction layer.

### 11.1 Region windows (UTC anchors)

| Region   | UTC window      | Local anchor (timezone range)    | PagerDuty schedule slug          |
| -------- | --------------- | -------------------------------- | -------------------------------- |
| Americas | 16:00 → 00:00   | UTC-8 .. UTC-5 (PT/MT/CT/ET)     | `corelink-oncall-americas-{tier}` |
| EMEA     | 00:00 → 08:00   | UTC+0 .. UTC+3 (GMT/CET/EET)     | `corelink-oncall-emea-{tier}`     |
| APAC     | 08:00 → 16:00   | UTC+8 .. UTC+11 (HKT/JST/AEST)   | `corelink-oncall-apac-{tier}`     |

The closed-loop 3 × 8h cycle covers a full UTC day exactly. The
7-day shift cap from §2 applies **per region** (an engineer rotates
out every 7 days within their region; they never cross regions).

### 11.2 Handoff at region boundary

At each region boundary (00:00 / 08:00 / 16:00 UTC) the outgoing
region's primary executes the §3 handoff template against the
incoming region's primary. Handoffs at region boundaries are
expected to be ≤ 5 min on a quiet shift (no transit time; remote
async via the handoff template in `specs/_templates/oncall_handoff.md`).
Synchronous handoff (15 min sync) MUST occur when:

- Any open SEV-0 / SEV-1 incident is in flight.
- A planned change-window is active (per `specs/_runbooks/RB-CHANGE-WINDOW.md`).
- The previous region's drill outcome was `Escalated` or `Unacked`
  (see `RB-SYNTHETIC-PAGE-DRILL.md`).

### 11.3 Staffing (GA target + ADR-0034 acceptance)

| Region   | Tier 1 staffing             | Tier 2 staffing             | Tier 3 staffing                    |
| -------- | --------------------------- | --------------------------- | ---------------------------------- |
| Americas | Owner + Final Approver dual-hat (Gustavo) OR contracted SRE | External advisor pool (ADR-0034 Option C) | Same as Tier 2 |
| EMEA     | Contracted SRE (engaged Q3-Q4) | External advisor pool       | Same as Tier 2                     |
| APAC     | Contracted SRE (engaged Q3-Q4) | External advisor pool       | Same as Tier 2                     |

At GA, the EMEA / APAC primaries MUST be contracted (Owner solo-tier
dual-hat is regulatory-acceptable per ADR-0034 only for Americas
because of the Owner's UTC-3 anchor). The audit
`specs/_audits/2026-05-14-s20-oncall-24-7-readiness.md` captures the
go/no-go staffing assessment.

### 11.4 Holiday + parental-leave coverage

- Each regional primary has a designated **backup primary** rostered
  in PagerDuty's overrides layer; backup MUST be activated ≥ 14 days
  in advance for planned absence (holiday, vacation).
- Parental-leave coverage: the parental-leave engineer is removed
  from all PagerDuty schedules for the full leave period + an
  additional 14-day return-ramp window during which they are Tier 3
  shadow only. This window is enforced by
  `RotationLedger::start_shift` returning `OncallError::InvalidRotation`
  if violated.
- Force-majeure single-region degradation: if a region loses its
  primary mid-shift (e.g. medical emergency), the adjacent region's
  primary picks up the residual shift segment via PagerDuty's
  override; the cap is the §2 7-day rolling limit (no engineer
  exceeds 7 days even across an emergency-pickup).

### 11.5 Synthetic page weekly drill

`corelink-synthetic-pager` (WI-S20-006) emits a weekly synthetic
SEV-2 page through the dedicated `synthetic-drill` PagerDuty service
(severity = `sev2_synthetic`; production escalation rules MUST NOT
match this severity). The drill rotates across regions on a 4-week
cycle (Week N % 4 selects {Americas, EMEA, APAC, boundary-handoff}).
The full procedure + escalation if MTTA breached lives in
`specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md`.

### 11.6 APAC post-GA roll-forward

Per WI §2.1, ADR-0034 explicitly defers APAC sub-splits (ANZ vs
JP-KR vs IN) to post-GA Q1 demand-driven. The
`corelink_synthetic_pager::Region` enum is `#[non_exhaustive]` so
additive growth lands without a breaking change.

## 12. Change log

| Version | Date       | Author                          | Change                                   |
| ------- | ---------- | ------------------------------- | ---------------------------------------- |
| 1.0.0   | 2026-05-14 | Gustavo (via Claude Opus 4.7)   | Initial RB-ONCALL-POLICY for WI-S17-005. |
| 1.1.0   | 2026-05-14 | Gustavo (via Claude Opus 4.7)   | Add §11 24/7 follow-the-sun 3-region split + holiday/parental-leave coverage + synthetic page weekly cross-reference (WI-S20-006). |
