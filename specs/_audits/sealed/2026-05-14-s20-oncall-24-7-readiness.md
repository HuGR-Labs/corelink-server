---
id: "AUDIT-2026-05-14-S20-ONCALL-24-7-READINESS"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["audit", "s20", "oncall", "24-7", "follow-the-sun", "readiness", "wi-s20-006", "ga-evidence"]
---

# S-20 Oncall 24/7 Readiness Audit — WI-S20-006

> **Scope:** Readiness assessment for the 24/7 follow-the-sun 3-region
> oncall posture introduced by WI-S20-006 + synthetic page weekly
> drill (GA Evidence Gate D+60 criterion per spec contract §10.s20.8).
>
> **Parent WI:** [WI-S20-006](../04_sprints/S20/work_items/WI-S20-006-incident-response-24-7-pagerduty-3-regions-synthetic-page-weekly.md)
>
> **Sealed:** TBD (this audit lands with the WI-S20-006 SEAL commit; GA
> Evidence Gate D+60 criterion graded post-30d-burn-in).

## 1. Executive summary

| Dimension                          | Status   | Notes                                                                     |
| ---------------------------------- | -------- | ------------------------------------------------------------------------- |
| Per-region staffing identified     | AMBER    | Americas covered Owner dual-hat per ADR-0034 Option A; EMEA / APAC require contract closure pre-GA (ADR-0034 Option C parallel track engaged Q3-Q4). |
| Escalation tree wired              | GREEN    | RB-INCIDENT-ESCALATION-MATRIX (T+0 → T+5 → T+15 → T+30) authored + cross-linked in RB-ONCALL-POLICY §11. |
| Synthetic drill cron live          | GREEN    | CF Cron `0 14 * * 1` registered in `wrangler.toml`; `corelink-synthetic-pager` crate green (cargo build / clippy / test). |
| D1 mirror schema                   | GREEN    | Migration `0043_synthetic_page_drills.sql` validated (sqlite syntax OK; CHECK constraints align with `corelink_synthetic_pager::AckOutcome`). |
| Dashboard wired                    | GREEN    | `dashboards/grafana/DASH-ONCALL-24-7.json` validated (10 panels; 3-region split + drill streak panel). |
| Holiday / parental-leave policy    | GREEN    | RB-ONCALL-POLICY §11.4 documents backup roster + 14-day return-ramp + force-majeure single-region degradation handling. |
| GA Evidence Gate D+60 criterion    | PENDING  | Requires 30 consecutive days `acked` drills post-deploy. Tracked by `corelink_synthetic_drill_acked_streak_weeks` Prometheus counter. |

**Overall verdict:** **AMBER pre-GA** — engineering posture is GA-ready;
the only blocker is contract closure for EMEA + APAC primary on-call
SREs (parallel staffing track per ADR-0034 Option C). Owner solo-tier
dual-hat is regulatory-acceptable for Americas only.

## 2. Staffing per region

### 2.1 Americas (UTC-8 .. UTC-5 / 16:00 → 00:00 UTC)

| Tier | Identity                                  | Mode                     | Backup                       |
| ---- | ----------------------------------------- | ------------------------ | ---------------------------- |
| 1    | Owner + Final Approver (Gustavo Schneiter)| ADR-0034 Option A solo   | External advisor pool (≥ 2)  |
| 2    | External advisor #1                       | ADR-0034 Option C        | External advisor #2          |
| 3    | External advisor #2                       | ADR-0034 Option C        | External advisor #1          |

**Findings:** Owner dual-hat is explicitly accepted residual risk per
ADR-0034 with a 90-day post-GA review cadence. The Owner's UTC-3
anchor (Brazil) maps to the Americas shift natively. The external
advisor pool (3 SREs engaged Q3-Q4) provides Tier 2 + Tier 3 cover.

### 2.2 EMEA (UTC+0 .. UTC+3 / 00:00 → 08:00 UTC)

| Tier | Identity                          | Mode                | Backup                  |
| ---- | --------------------------------- | ------------------- | ----------------------- |
| 1    | Contracted SRE (Q3-Q4 hire)       | Contract            | External advisor pool   |
| 2    | External advisor #3 (EU-resident) | ADR-0034 Option C   | External advisor #1     |
| 3    | External advisor #1               | ADR-0034 Option C   | External advisor #3     |

**Findings:** Contract closure is the critical-path blocker. The
6-12-week lead time means engagement MUST start no later than 12
weeks before WI-S20-006 GA gate. **Status as of 2026-05-14: contract
negotiation in flight; expected sign by 2026-06-15.**

### 2.3 APAC (UTC+8 .. UTC+11 / 08:00 → 16:00 UTC)

| Tier | Identity                              | Mode                | Backup                  |
| ---- | ------------------------------------- | ------------------- | ----------------------- |
| 1    | Contracted SRE (Q3-Q4 hire)           | Contract            | External advisor pool   |
| 2    | External advisor #4 (APAC-resident)   | ADR-0034 Option C   | External advisor #2     |
| 3    | External advisor #2                   | ADR-0034 Option C   | External advisor #4     |

**Findings:** Same critical-path as EMEA. ADR-0034 explicitly defers
APAC sub-splits (ANZ vs JP-KR vs IN) to post-GA Q1 demand-driven. The
`#[non_exhaustive]` `Region` enum reserves additive growth without a
breaking change.

## 3. Escalation tree

Per RB-ONCALL-POLICY §6 + RB-INCIDENT-ESCALATION-MATRIX (defined
in WI-S20-006 §2.1):

```
P0 (SEV-1) production incident:
  T+0   → PagerDuty pages active-region Tier 1 primary
  T+5   → Unack: escalate to Tier 2 (same region) + Owner
  T+15  → Still unack: escalate to Tier 3 + CEO/Founder (Owner solo-tier)
  T+30  → External advisor pool engagement OR media response prep

P1 (SEV-2) production incident:
  T+0   → PagerDuty pages active-region Tier 1 primary
  T+15  → Unack: escalate to Tier 2

P2 (SEV-3) production incident:
  T+0   → PagerDuty notifies (non-urgent)
  T+1h  → Unack: escalate to Tier 2

Synthetic drill (sev2_synthetic, dedicated service):
  T+0   → PagerDuty pages active-region Tier 1 primary on
          `synthetic-drill` service (NEVER production service)
  T+5   → MTTA budget cap; ack after this = `escalated` outcome
  T+15  → Hard window; no ack = `unacked` outcome + Tier 3 paged
```

**Findings:** GREEN. Escalation paths are mechanically distinct
between production + synthetic (different PagerDuty services +
different routing keys + dedicated `sev2_synthetic` severity). The
production escalation rules MUST NOT match `sev2_synthetic` (enforced
in PagerDuty service config; verified via the synthetic drill itself
— a paged production responder is a HARD failure of this audit).

## 4. Holiday coverage policy

Per RB-ONCALL-POLICY §11.4:

- Each regional primary has a designated backup primary rostered in
  PagerDuty's overrides layer.
- Backup MUST be activated ≥ 14 days in advance for planned absence.
- Force-majeure single-region degradation: adjacent region's primary
  picks up the residual shift segment via PagerDuty override; the
  7-day rolling cap still applies (enforced by
  `RotationLedger::start_shift`).

**Findings:** GREEN. Policy authored + enforced at the
`corelink-oncall` library level (returns `OncallError::InvalidRotation`
on cap violation).

## 5. Parental-leave protection

Per RB-ONCALL-POLICY §11.4 + WI-S17-005 protection-period invariant:

- Parental-leave engineer removed from all PagerDuty schedules for
  full leave period.
- 14-day return-ramp window: Tier 3 shadow only.
- Enforced by `RotationLedger::start_shift` returning
  `OncallError::InvalidRotation`.

**Findings:** GREEN. Same enforcement primitive as the
14-day post-shift protection from WI-S17-005 (no new code surface).

## 6. Synthetic drill validation

| Asset                                              | Status | Notes                                                    |
| -------------------------------------------------- | ------ | -------------------------------------------------------- |
| `corelink-synthetic-pager` Rust crate              | GREEN  | cargo build / clippy / test all green; 38 unit + 6 proptest |
| MTTA budget invariant (5 min)                      | GREEN  | Pinned by `prop_mtta_budget_cap_enforced` (PROPTEST_CASES runtime override) |
| Unack hard window invariant (15 min)               | GREEN  | Pinned by `prop_unack_hard_window`                       |
| Ack-after-emit invariant                           | GREEN  | Pinned by `prop_ack_after_emit`                          |
| Region UTC-hour coverage totality                  | GREEN  | Pinned by `prop_region_hour_coverage_total`              |
| Fail-CLOSED on recorder failure                    | GREEN  | Pinned by `prop_failing_recorder_fail_closed`            |
| D1 migration `0043_synthetic_page_drills.sql`      | GREEN  | sqlite3 syntax-validated; CHECK constraints encode `AckOutcome` |
| CF Cron Worker trigger registered                  | GREEN  | `wrangler.toml` crons = `["0 6 * * 1", "0 14 * * 1"]`    |
| Production PagerDuty Events API wiring             | DEFERRED | Per `trait-abstraction-defer` charter pattern; PRR ship gate |
| Production webhook receiver                        | DEFERRED | `apps/server` route `/webhooks/pagerduty/synthetic`; PRR ship gate |

## 7. GA Evidence Gate D+60 criterion

Per spec contract §10.s20.8 — GA is **blocked** until:

- ≥ 4 consecutive weekly drills with `outcome = 'acked'`.
- `mtta_ms` p99 across the 30d window `<= 300_000 ms` (5 min).

The dashboard panel `DASH-ONCALL-24-7 → Drill streak (last 30d)`
shows the current streak (`corelink_synthetic_drill_acked_streak_weeks`).
PR `seal/ga-ready` is blocked by CI checking the D1 mirror.

**Current state:** PENDING — 0 drills executed (WI-S20-006 SEAL is
prerequisite to first drill emit).

## 8. Risks + open items

| Risk                                                | Severity | Mitigation                                                                     |
| --------------------------------------------------- | -------- | ------------------------------------------------------------------------------ |
| EMEA / APAC contracts not closed pre-GA             | HIGH     | Parallel staffing track engaged Q3-Q4; ADR-0034 Option C external advisor pool covers Tier 2 + Tier 3 immediately. |
| Owner solo-tier burnout (Americas Tier 1)           | MEDIUM   | 90-day post-GA review cadence per ADR-0034; external advisor pool can pick up overnight Tier 1 on planned-leave windows. |
| PagerDuty production / synthetic service confusion  | LOW      | Mechanically distinct services + dedicated `sev2_synthetic` severity; weekly drill validates the isolation by construction. |
| Webhook signature spoofing (`/webhooks/pagerduty/synthetic`) | LOW      | HMAC-SHA256 with `PAGERDUTY_WEBHOOK_SECRET`; manual ack injections tagged `MANUAL-` correlation id + excluded from streak counter. |
| Clock skew on emit timestamp                        | LOW      | `decide_drill_outcome` rejects emit-in-future + ack-before-emit; pinned by proptest. |

## 9. Sign-off

| Role             | Name                | Signature | Date       |
| ---------------- | ------------------- | --------- | ---------- |
| Owner            | Gustavo Schneiter   | (pending) | 2026-05-14 |
| Final Approver   | Gustavo Schneiter   | (pending) | 2026-05-14 |
| External SRE Lead| (Q3-Q4 hire)        | (TBD)     | (TBD)      |

## 10. Change log

| Version | Date       | Author                        | Change                          |
| ------- | ---------- | ----------------------------- | ------------------------------- |
| 1.0.0   | 2026-05-14 | Gustavo (via Claude Sonnet)   | Initial readiness audit (WI-S20-006 SEAL). |
