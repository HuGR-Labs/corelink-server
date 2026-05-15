---
id: "R-PREP-QUARTERLY-RETENTION-REVIEW"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "R-PREP-CHURN-RETENTION"
tags: ["marketing", "retention", "kpi", "quarterly-review", "template", "internal", "r-prep", "ga", "post-launch"]
---

# Quarterly Retention Review — Internal KPI Template

> **Audience.** Founder, AE, CSE, Finance. Internal only.
> **Purpose.** Recurring quarterly cadence to review: (a) retention KPIs, (b) churn-signal effectiveness, (c) retention-play conversion, (d) signal tuning iteration. Copy this file into a dated review doc each quarter (e.g. `marketing/retention/reviews/2026-Q3-review.md`).
> **Cadence.** Quarterly, owner = Founder, prep work distributed: CSE assembles signal data, AE assembles play outcomes, Finance assembles revenue impact, Founder authors decisions.
> **First instance.** 2026-Q3 (first full quarter post-GA). Pre-GA we have no churn data and signal tuning is theoretical.
> **Companion docs.** [`CHURN-RISK-SIGNALS.md`](./CHURN-RISK-SIGNALS.md), [`RETENTION-PLAYBOOK.md`](./RETENTION-PLAYBOOK.md), [`CANCELLATION-FLOW.md`](./CANCELLATION-FLOW.md), [`../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md), [`../sales/PRICING-WORKSHEET.md`](../sales/PRICING-WORKSHEET.md).

---

## How to use this template

1. Copy this file to `marketing/retention/reviews/{YYYY}-Q{N}-review.md` at the start of each quarter's review.
2. Fill the four sections (KPI dashboard, signal effectiveness, play effectiveness, decisions). Each section has a table with concrete fields — no narrative prose allowed in the table cells; that goes in the §5 Decisions section.
3. Review meeting: 90 minutes. Founder runs the agenda. Decisions in §5 carry into the next quarter as work items.
4. Commit the completed review to git the same day. Reviews are part of the audit trail.

---

## 1. KPI dashboard — quarter at a glance

Fill these with the quarter's actuals. Compute the deltas vs prior quarter and vs the same quarter a year prior (once we have ≥ 4 quarters of data).

| KPI | This Q | Prior Q | YoY Q | Notes |
|---|---|---|---|---|
| **Logo retention** (paid tenants retained / paid tenants active at quarter start) | __% | __% | __% | Target: ≥ 95% (Y1), ≥ 98% (Y2+) |
| **Net revenue retention** (expansion + retention - contraction - churn, % of starting ARR) | __% | __% | __% | Target: ≥ 110% (Y1), ≥ 120% (Y2+) |
| **Gross revenue retention** (1 - churned ARR / starting ARR) | __% | __% | __% | Target: ≥ 90% (Y1) |
| **Active paid tenants at quarter start** | __ | __ | __ | |
| **Active paid tenants at quarter end** | __ | __ | __ | |
| **Tenants churned this Q (count)** | __ | __ | __ | |
| **Churned ARR this Q ($)** | __ | __ | __ | |
| **Saves closed this Q (count)** | __ | __ | __ | Save = tenant who fired ≥ 1 signal AND completed renewal |
| **Saves closed this Q (ARR $)** | __ | __ | __ | |
| **Discount $ extended this Q** | __ | __ | __ | From all save plays |
| **NPS (paid tenants only)** | __ | __ | __ | Quarterly survey |
| **Lighthouse alumni retention** | __ | __ | __ | Subset; expected 100% for first 2 years |

### Tier breakdown

| Tier | Active at Q start | Active at Q end | Churned | New | Net Δ |
|---|---|---|---|---|---|
| Free | __ | __ | __ | __ | __ |
| Starter | __ | __ | __ | __ | __ |
| Team | __ | __ | __ | __ | __ |
| Pro | __ | __ | __ | __ | __ |
| Enterprise | __ | __ | __ | __ | __ |

### Cancellation reason mix (from `CANCELLATION-FLOW.md` Step 2 survey)

| Reason | Count this Q | % of churn | Prior Q % | Δ |
|---|---|---|---|---|
| Too expensive | __ | __% | __% | __ |
| Missing a feature | __ | __% | __% | __ |
| Found a better alternative | __ | __% | __% | __ |
| Project ended / no longer needed | __ | __% | __% | __ |
| Performance / reliability | __ | __% | __% | __ |
| Support issues | __ | __% | __% | __ |
| Other / prefer not to say | __ | __% | __% | __ |
| Survey skipped | __ | __% | __% | __ |

---

## 2. Signal effectiveness

For each signal in [`CHURN-RISK-SIGNALS.md`](./CHURN-RISK-SIGNALS.md), report:

- **Fires** — count of distinct (tenant_id, signal_id) firings this quarter.
- **True positives (TP)** — tenant who fired this signal AND subsequently churned within 90 d.
- **False positives (FP)** — tenant who fired AND was determined at triage to not be at risk (CSE marked the signal as benign in CRM).
- **Saves** — tenant who fired, was at risk, and was retained via a play.
- **Precision** — TP / (TP + FP).
- **Recall (Q-relative)** — TP / (total tenant churns this Q). A signal with 0% recall is invisible to actual churn → review.

| Signal | Fires | TP | FP | Saves | Precision | Recall | Decision |
|---|---|---|---|---|---|---|---|
| S-01 Usage drop >50% WoW | __ | __ | __ | __ | __% | __% | keep / re-threshold / retire |
| S-02 Ticket > 7d unresolved | __ | __ | __ | __ | __% | __% | |
| S-03 Payment retry > 3 | __ | __ | __ | __ | __% | __% | |
| S-04 Admin removed | __ | __ | __ | __ | __% | __% | |
| S-05 Audit query → 0 | __ | __ | __ | __ | __% | __% | |
| S-06 SLO breach | __ | __ | __ | __ | __% | __% | |
| S-07 Pricing revisit | __ | __ | __ | __ | __% | __% | |
| S-08 QBR skip | __ | __ | __ | __ | __% | __% | |
| S-09 Seat util <30% | __ | __ | __ | __ | __% | __% | |
| S-10 Export spike | __ | __ | __ | __ | __% | __% | |
| S-11 NPS detractor | __ | __ | __ | __ | __% | __% | |
| S-12 Doc thrash | __ | __ | __ | __ | __% | __% | |
| S-13 Hit ratio ↓ | __ | __ | __ | __ | __% | __% | |
| S-14 Auto-renew opt-out | __ | __ | __ | __ | __% | __% | |
| S-15 BYOK kill-switch armed | __ | __ | __ | __ | __% | __% | |

### Tuning rules

- **Retire** any signal with FP rate > 70% sustained over 2 consecutive quarters AND no save attributable.
- **Re-threshold** any signal with TP rate < 30% — adjust the heuristic (window size, magnitude bar) and re-evaluate next quarter.
- **Promote** any new candidate signal that triggered ad-hoc by CSE this quarter and would have caught a churn that no existing signal caught. Add to next quarter's [`CHURN-RISK-SIGNALS.md`](./CHURN-RISK-SIGNALS.md) as a new entry.
- **Missed churns** (tenant churned this Q with zero signals fired in the preceding 30 d) require a written one-paragraph postmortem in §5 Decisions — what signal would have caught it?

---

## 3. Play effectiveness

For each play in [`RETENTION-PLAYBOOK.md`](./RETENTION-PLAYBOOK.md), report:

- **Triggered** — count of times the play was opened (triage marked the signal real).
- **Touch completed within SLA** — play reached "first touch" within the SLA window (24h / 72h / 7d).
- **Save closed** — customer renewed / accepted offer / did not cancel after the play.
- **Save rate** — Saves / Triggered.
- **Avg discount $** — for plays involving a discount.
- **Avg time-to-save** — wall-clock from signal fire to save confirmed.

| Play | Triggered | Touch in SLA | Saves | Save rate | Avg discount $ | Avg time-to-save |
|---|---|---|---|---|---|---|
| P-HIGH-USAGE-DROP (S-01) | __ | __ | __ | __% | __ | __d |
| P-HIGH-STUCK-TICKET (S-02) | __ | __ | __ | __% | __ | __d |
| P-HIGH-PAYMENT-FAIL (S-03) | __ | __ | __ | __% | __ | __d |
| P-HIGH-ADMIN-CHANGE (S-04) | __ | __ | __ | __% | __ | __d |
| P-HIGH-SLO-BREACH (S-06) | __ | __ | __ | __% | __ | __d |
| P-HIGH-EXPORT-SPIKE (S-10) | __ | __ | __ | __% | __ | __d |
| P-HIGH-NO-RENEW (S-14) | __ | __ | __ | __% | __ | __d |
| P-HIGH-BYOK-ARMED (S-15) | __ | __ | __ | __% | __ | __d |
| P-MED-AUDIT-COLD (S-05) | __ | __ | __ | __% | __ | __d |
| P-MED-PRICING-VISIT (S-07) | __ | __ | __ | __% | __ | __d |
| P-MED-QBR-SKIP (S-08) | __ | __ | __ | __% | __ | __d |
| P-MED-LOW-SEAT (S-09) | __ | __ | __ | __% | __ | __d |
| P-MED-DETRACTOR (S-11) | __ | __ | __ | __% | __ | __d |
| P-MED-HIT-RATIO-DROP (S-13) | __ | __ | __ | __% | __ | __d |
| P-LOW-DOC-THRASH (S-12) | __ | __ | __ | __% | __ | __d |

### Cancellation-flow effectiveness

| Metric | This Q | Prior Q | Δ |
|---|---|---|---|
| Cancel flow started (Step 1 button clicked) | __ | __ | __ |
| Cancel flow completed (Step 4 confirm) | __ | __ | __ |
| Drop-out rate (started but did not confirm) | __% | __% | __ |
| Retention-offer accept rate (Step 3 "Accept offer") | __% | __% | __ |
| Re-activations within 30 d window | __ | __ | __ |
| DSR-erasure shortcut used (Step 5) | __ | __ | __ |

### Discount authority audit

| Authority level | Discounts issued | Total $ extended | Pct margin floor breach | Notes |
|---|---|---|---|---|
| AE / CSE (≤25% / 3 mo) | __ | __ | __% | |
| AE / CSE (25–50% / 3 mo, with Founder notice) | __ | __ | __% | |
| Founder (>50% or custom) | __ | __ | __% | |
| Below-COGS save (Founder + Finance dual-sign) | __ | __ | n/a | |

Any below-COGS save requires a one-line rationale captured in CRM and surfaced here.

---

## 4. Concentration & runway view

| Metric | Value | Threshold | Action if exceeded |
|---|---|---|---|
| Top-3 tenants as % of total ARR | __% | < 30% | If > 30%, surface concentration risk in next §5 Decisions |
| Single largest tenant as % of total ARR | __% | < 15% | If > 15%, dedicated Founder save plan documented |
| Tenants on contract auto-renew opt-out | __ | < 5 | If ≥ 5, escalate to weekly cadence next quarter |
| Save discount run-rate as % of ARR | __% | < 3% | If > 3%, retention plays may be subsidizing rather than retaining — review play targeting |

---

## 5. Decisions

Each decision below is binding for the next quarter. Format:

```
**D-{YYYYQN}-{N}** — title
Decision:
Owner:
Effective date:
Tied to: (signal id, play id, work item, doc)
```

Examples (delete in real review):

```
**D-2026Q3-1** — Retire S-12 (doc thrash)
Decision: Remove S-12 from CHURN-RISK-SIGNALS.md; sunset the automated educational comms.
Owner: Marketing automation team
Effective date: 2026-10-01
Tied to: S-12, P-LOW-DOC-THRASH

**D-2026Q3-2** — Tighten S-01 threshold to 60%
Decision: Change usage-drop threshold from > 50% to > 60% WoW. Current 50% generated too many false positives during CI rotation windows.
Owner: SRE (Prometheus alert rule)
Effective date: 2026-10-01
Tied to: S-01
```

Replace the examples with real decisions during each review.

---

## 6. Action items carry-forward

Open action items from prior quarter:

| AI id | Description | Owner | Due | Status |
|---|---|---|---|---|
| (none yet — first review) | | | | |

New action items this quarter:

| AI id | Description | Owner | Due | Status |
|---|---|---|---|---|
| | | | | |

---

## Cross-references

- Signal catalog: [`CHURN-RISK-SIGNALS.md`](./CHURN-RISK-SIGNALS.md)
- Retention plays: [`RETENTION-PLAYBOOK.md`](./RETENTION-PLAYBOOK.md)
- Cancellation flow: [`CANCELLATION-FLOW.md`](./CANCELLATION-FLOW.md)
- Internal ops runbook + RACI: [`../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md)
- Pricing + COGS floor: [`../sales/PRICING-WORKSHEET.md`](../sales/PRICING-WORKSHEET.md)
- Customer playbook: [`../lighthouse-kit/CUSTOMER-PLAYBOOK.md`](../lighthouse-kit/CUSTOMER-PLAYBOOK.md)
- Roadmap: [`../../ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §8
