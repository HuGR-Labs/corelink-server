---
id: "RB-VENDOR-RISK-QUARTERLY-REVIEW"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-R5-3-GAP-14-VENDOR-RISK"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from: ["VENDOR-RISK-REGISTER-2026-05-15", "VENDOR-RISK-METHODOLOGY-2026-05-15"]
tags: ["runbook", "soc2", "cc9.2", "vendor-risk", "quarterly", "gap-14"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §10.

# RB-VENDOR-RISK-QUARTERLY-REVIEW — 90-day vendor risk review cycle

> **Status:** ACTIVE. Owned by VP-Sec. Triggered by the quarterly calendar reminder
> (90-day cadence) **or** by any ad-hoc trigger listed in
> `VENDOR-RISK-METHODOLOGY.md §6`.

## 1. Purpose

Operationalise the vendor-risk management program defined in
`VENDOR-RISK-METHODOLOGY.md`. Closes SOC 2 CC9.2 evidence requirement
that vendor risk is **periodically re-assessed**, not just baselined.

## 2. Cadence

| Quarter | Window (year N) | Review focus |
|---|---|---|
| Q1 | Feb 15 → Mar 15 | All Critical vendors + any flagged Important |
| Q2 | May 15 → Jun 15 | All Critical vendors + half of Important + any open ad-hoc actions |
| Q3 | Aug 15 → Sep 15 | All Critical vendors + the other half of Important |
| Q4 | Nov 15 → Dec 15 | All Critical vendors + all Standard vendors (annual sweep) + methodology-doc refresh |

**Baseline:** 2026-05-15 (this runbook's creation date) → next due **2026-08-15** (Q3 2026).

## 3. Pre-flight (review-window start, T-7 days)

| Step | Action | Owner | Output |
|---|---|---|---|
| 3.1 | Open the next-review GitHub Issue from the saved template | VP-Sec | Issue link |
| 3.2 | Generate the per-quarter scope list: from `VENDOR-RISK-REGISTER.md` extract every vendor whose `Next review` ≤ window-end | VP-Sec | Markdown list pasted into Issue |
| 3.3 | Cross-check against Drata vendor module for any vendor added/removed since last review | VP-Sec | Delta noted |
| 3.4 | Pre-fill review template in `specs/_audits/YYYY-MM-DD-vendor-risk-quarterly.md` (one new audit file per quarter) | VP-Sec | Audit file scaffolded |
| 3.5 | Email vendor outreach (template §7 below) to every vendor whose attestation will be > 12 months old at window-end | VP-Sec | Outreach log entries |

## 4. Per-vendor review steps (T-0 → T+14 days)

For **each** vendor in scope:

| Step | Action | Owner | Acceptance |
|---|---|---|---|
| 4.1 | Pull the vendor's current attestation evidence (SOC 2 Type II report, ISO 27001 cert, FedRAMP letter) from Drata or Trust-Portal URL | VP-Sec | Document hash + retrieval date logged |
| 4.2 | Review the vendor's sub-processor list for additions / changes | VP-Sec | Delta annotated |
| 4.3 | Review the vendor's incident history for the trailing 12 months (vendor postmortems, public outages, CISA advisories) | VP-Sec | Incident-count + severity logged |
| 4.4 | Re-score Inherent risk per methodology §3 | VP-Sec | Score with delta vs prior review |
| 4.5 | Re-score CEF per methodology §4 (explicit checklist of which conditions are met) | VP-Sec | CEF justification text |
| 4.6 | Compute Residual = Inherent × CEF | VP-Sec | Number rounded to 1 decimal |
| 4.7 | If residual ≥ 8.0 → escalate to Risk Committee per methodology §5 | VP-Sec | Risk Committee meeting invite |
| 4.8 | If residual changed by ≥ 2.0 in either direction → annotate in audit file | VP-Sec | Delta narrative |
| 4.9 | Update register row: `Last review`, `Next review`, Inherent, CEF, Residual | VP-Sec | Git diff PR |
| 4.10 | If vendor is Critical → refresh the per-vendor DD file in `specs/_compliance/vendor-dd/` | VP-Sec | DD file updated |

## 5. Post-review (T+14 → T+21 days)

| Step | Action | Owner | Output |
|---|---|---|---|
| 5.1 | Commit register + DD updates as a single PR with title `chore(vendor-risk): YYYY-QN quarterly review` | VP-Sec | PR merged |
| 5.2 | Update `specs/03_architecture/compliance_matrix.md` if any CC9.2-mapped CTRL status changes | VP-Sec | Matrix updated |
| 5.3 | Update Drata vendor module to mirror the register (manual upload of changed rows where Drata API is not write-capable) | VP-Sec | Drata "Last sync" reflects today |
| 5.4 | Append summary entry to `specs/_compliance/SOC2-EVIDENCE-ROLLUP-YYYY-MM-DD.md` (or current rollup) under CC9.2 | VP-Sec | Rollup updated |
| 5.5 | Risk Committee read-out (15 min standing slot) covering: scope reviewed, escalations, open actions, methodology-doc changes if any | VP-Sec + Risk Committee | Meeting minutes |
| 5.6 | Close the quarterly GitHub Issue with a link to the audit file | VP-Sec | Issue closed |

## 6. Escalation flow

| Trigger | Action |
|---|---|
| Any vendor residual ≥ 8.0 | Risk Committee review **within 30 days**; compensating-control plan within 60 days. |
| Any Critical vendor residual ≥ 13.0 | Risk Committee review **within 7 days**; vendor-replacement plan within 90 days. |
| Any vendor with two consecutive quarters at residual ≥ 8.0 | Written exception signed by VP-Sec + Final Approver (Gustavo) **or** vendor replacement begins. |
| Any ad-hoc trigger (methodology §6) | Immediate re-review using §4 steps; do **not** wait for calendar window. |

## 7. Comms templates

### 7.1 Vendor outreach — attestation refresh

```
Subject: [CoreLink / HuGR Labs] Vendor risk review — request for current SOC 2 / ISO 27001 attestation

Hi {{vendor_contact_name}},

CoreLink (HuGR Labs, Inc.) runs a quarterly vendor risk review program aligned to
SOC 2 CC9.2. We have you in our current review window ({{quarter_start}} → {{quarter_end}}).

Could you share or confirm access to the following, current as of today:

1. Your most recent SOC 2 Type II report (or ISO 27001 certificate, or FedRAMP letter).
2. Your current sub-processor list and any changes since {{last_review_date}}.
3. A summary of any SEV-1 incidents in the trailing 12 months (or confirmation of zero).
4. Confirmation that the DPA on file ({{dpa_url}}) is the current version.

We use Drata as our compliance platform — if you support Drata's auto-pull
integration we can skip steps 1-3. Otherwise a PDF or Trust-Portal share-link is
perfect.

Please reply within 10 business days so we can complete the review on schedule.

Thanks,
{{vp_sec_name}}
VP-Sec, HuGR Labs Inc. (CoreLink)
security@hugr.dev
```

### 7.2 Vendor outreach — escalation (residual ≥ 8.0)

```
Subject: [CoreLink / HuGR Labs] Vendor risk escalation — request for control discussion

Hi {{vendor_contact_name}},

In our most recent quarterly vendor-risk review (cycle {{quarter}}), {{vendor_name}}
moved into our "Mitigate" residual-risk band. We'd like to schedule a 30-min call
with your security team to:

1. Walk through the specific factor(s) driving the score change.
2. Discuss any compensating controls you'd recommend on our side.
3. Align on any roadmap items on your side that would change the assessment.

This is a forward-looking conversation, not a contractual notice. Could you
propose two windows in the next 10 business days?

Thanks,
{{vp_sec_name}}
VP-Sec, HuGR Labs Inc. (CoreLink)
security@hugr.dev
```

### 7.3 Risk Committee read-out template

```
Subject: Vendor Risk Quarterly Read-out — {{quarter}}

Scope reviewed: {{n_critical}} Critical / {{n_important}} Important / {{n_standard}} Standard.

Headline results:
- Vendors at "Escalate" band (≥ 13.0): {{list_or_none}}
- Vendors at "Mitigate" band (8.0..12.9): {{list_or_none}}
- Vendors with residual delta ≥ 2.0 since last review: {{list_or_none}}
- Open methodology-doc change requests: {{list_or_none}}
- Ad-hoc reviews triggered this quarter: {{count}} ({{triggers_summary}})

Recommended actions:
1. {{action_1}}
2. {{action_2}}

Sign-off:
- VP-Sec: {{name}} / {{date}}
- Final Approver: Gustavo Schneiter / {{date}}
```

## 8. Failure modes and recovery

| Failure | Detection | Recovery |
|---|---|---|
| Vendor does not respond to attestation refresh email within 10 business days | VP-Sec follow-up not received | Send escalation email; if still no response within 5 more days → mark CEF upward by one step (worse) per methodology §4 row "attestation > 12 months old" |
| Drata vendor-module sync fails | Drata sync runbook `RB-DRATA-SYNC-FAILURE.md` alarms | Fall back to manual-upload for that vendor's evidence stream; raise ad-hoc review |
| Review window slips past calendar quarter | GitHub Issue still open at end of quarter | VP-Sec records reason in audit file; reschedules within next 15 days; flags any vendor whose review is > 105 days delayed for ad-hoc escalation |
| Risk Committee read-out cannot meet quorum | Quarterly slot misses | Reschedule within 14 days; document quorum-loss reason; no audit-file commit until read-out occurs |
| Vendor refuses to share evidence | Vendor reply explicitly declines | Treat as CEF=1.00 for that vendor; escalate; consider vendor replacement plan |

## 9. Related artefacts

- `specs/_compliance/VENDOR-RISK-METHODOLOGY.md` — scoring rubric
- `specs/_compliance/VENDOR-RISK-REGISTER.md` — current scores
- `specs/_compliance/vendor-dd/*.md` — Critical-vendor DD files
- `specs/_runbooks/RB-DRATA-SYNC-FAILURE.md` — Drata-side failure handling
- `specs/_runbooks/RB-DPA-CHANGE.md` — DPA template change handling
- `legal/sub-processors.md` — public sub-processor disclosure
- `specs/03_architecture/compliance_matrix.md` §CC9.2

## 10. Change log

| Version | Date | Author | Notes |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo Schneiter (via Sonnet builder, R5-3 GAP-14) | Initial runbook. |
