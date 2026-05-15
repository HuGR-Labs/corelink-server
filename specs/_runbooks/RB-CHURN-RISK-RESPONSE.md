---
id: "RB-CHURN-RISK-RESPONSE"
type: "runbook"
doc_status: "ACTIVE"
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
tags: ["runbook", "retention", "churn", "raci", "escalation", "discounts", "r-prep", "ga", "post-launch"]
---

# RB-CHURN-RISK-RESPONSE — Churn risk response runbook

> **Status:** ACTIVE. Internal-only. Owned by Founder; operated by AE / CSE / SRE on call (depending on signal class).
> **Scope:** the operational side of the retention motion. Customer-facing artifacts (signal catalog, retention plays, cancellation flow, quarterly review) live under [`marketing/retention/`](../../marketing/retention/).
> **Companion docs:** [`../../marketing/retention/CHURN-RISK-SIGNALS.md`](../../marketing/retention/CHURN-RISK-SIGNALS.md), [`../../marketing/retention/RETENTION-PLAYBOOK.md`](../../marketing/retention/RETENTION-PLAYBOOK.md), [`../../marketing/retention/CANCELLATION-FLOW.md`](../../marketing/retention/CANCELLATION-FLOW.md), [`../../marketing/retention/quarterly-review-template.md`](../../marketing/retention/quarterly-review-template.md).
> **Companion runbooks:** [`RB-CUSTOMER-SUPPORT-T-90.md`](./RB-CUSTOMER-SUPPORT-T-90.md) (T+0..T+90 customer-support tiers), [`RB-POSTMORTEM-PROCESS.md`](./RB-POSTMORTEM-PROCESS.md) (delivery of post-mortem after SLO breach), [`RB-DSR-GDPR.md`](./RB-DSR-GDPR.md) (DSR erasure path triggered from cancellation flow), [`RB-DPO-ESCALATION.md`](./RB-DPO-ESCALATION.md) (BYOK kill-switch security branch).

---

## 1. Trigger

This runbook applies whenever ONE OR MORE of the following occurs:

- A signal in [`CHURN-RISK-SIGNALS.md`](../../marketing/retention/CHURN-RISK-SIGNALS.md) fires for a paid tenant.
- A tenant initiates the in-product cancellation flow (per [`CANCELLATION-FLOW.md`](../../marketing/retention/CANCELLATION-FLOW.md) Step 1).
- A Founder-discretionary "save motion" is opened on a tenant outside the signal taxonomy.

Free-tier signals are out of scope (Free tier is a CAC line per [`../../marketing/sales/PRICING-WORKSHEET.md`](../../marketing/sales/PRICING-WORKSHEET.md) — no retention motion attached).

---

## 2. Signal ownership matrix (RACI)

`R` = responsible, `A` = accountable, `C` = consulted, `I` = informed.

| Signal | Responsible | Accountable | Consulted | Informed |
|---|---|---|---|---|
| S-01 Usage drop > 50% WoW | CSE | Founder | AE | SRE |
| S-02 Ticket > 7d unresolved | Founder | Founder | Support ticket owner | AE, CSE |
| S-03 Payment retry > 3 | AE | Founder | Finance | Stripe automation |
| S-04 Admin removed | AE | Founder | CSE | — |
| S-05 Audit query → 0 | CSE | AE | — | DPO |
| S-06 SLO breach | SRE on-call (technical) + Founder (commercial) | Founder | DPO | AE, CSE |
| S-07 Pricing revisit | AE | AE | CSE | — |
| S-08 QBR skip | CSE | CSE | AE | — |
| S-09 Seat util < 30% | AE | AE | CSE | Finance |
| S-10 Export spike | Founder | Founder | CSE, SRE | DPO |
| S-11 NPS detractor | Founder if ≤3; AE if 4–6 | Founder | CSE | — |
| S-12 Doc thrash | Marketing automation | CSE | — | — |
| S-13 Hit ratio ↓ > 20 pp | CSE + Solutions Eng | CSE | SRE | AE |
| S-14 Auto-renew opt-out | Founder | Founder | AE | Finance |
| S-15 BYOK kill-switch armed (outside drill) | Founder + DPO (parallel) | Founder | SRE | Legal, AE |

**Single accountable per signal.** Accountable owns the SLA clock. If accountable is unreachable, the role escalates per §6 escalation tree.

---

## 3. Triage gate

Every signal fires into a queue; **no signal auto-pages and no signal auto-emails the customer.** Triage gate (per signal class):

- **High-severity (S-01, S-02, S-03, S-04, S-06, S-10, S-14, S-15):** triage by the Accountable role within 4 business hours of fire. Triage decision: `confirm` → invoke the corresponding play (24 h SLA from fire, not from triage); `dismiss` → mark benign with reason in CRM (feeds [`quarterly-review-template.md`](../../marketing/retention/quarterly-review-template.md) §2 FP rate).
- **Medium-severity (S-05, S-07, S-08, S-09, S-11, S-13):** triage within 24 h. Same decision tree, 72 h SLA from fire.
- **Low-severity (S-12):** automated comms; weekly aggregate review by CSE.

Triage MUST be recorded in CRM with: signal id, fire timestamp, triage decision, triage rationale (1 sentence), play id (if confirmed), play assignee.

---

## 4. Multi-signal escalation

When ≥ 2 distinct signals fire for the same tenant within a rolling 14-day window:

- **2 medium signals concurrent →** treat as **high-severity** (CEO touch within 24 h regardless of original severity).
- **1 high + 1 medium or higher →** open a written **save plan** in the CRM tenant record. Save plan template: 1 page, ≤ 5 bullets, owner = Founder, review cadence weekly until closed.
- **3+ signals of any severity →** Founder + AE + CSE huddle within 48 h. Decision: continue save motion (with named owner, named offer, named close date) OR enter clean-exit motion ([`CANCELLATION-FLOW.md`](../../marketing/retention/CANCELLATION-FLOW.md) §4 fast path) OR strategic forfeit (rare; document rationale).

---

## 5. Discount authority approval matrix

Replicates the customer-facing matrix in [`RETENTION-PLAYBOOK.md`](../../marketing/retention/RETENTION-PLAYBOOK.md) — this is the **internal** authority view with explicit signoff requirements.

| Save lever | Magnitude | Pre-authorized | Requires Founder co-sign | Requires Founder + Finance dual-sign | Audit trail |
|---|---|---|---|---|---|
| Complimentary Free → Starter upgrade ($0 for 90 d) | up to 25/quarter | AE | beyond cap | n/a | CRM tenant note |
| Starter → Team 50% / 3 mo | up to 10/quarter | AE | yes, by week-end | beyond cap | CRM tenant note + #retention Slack post |
| Team → Enterprise custom negotiation | any | n/a | yes, all cases | yes if below-COGS | DPA addendum + CRM note |
| Same-tier renewal save ≤ 25% / 3 mo | up to 25/quarter | AE / CSE | n/a | n/a | CRM tenant note |
| Same-tier renewal save 25–50% / 3 mo | any | n/a | yes, all cases | n/a | CRM + Slack post |
| Same-tier renewal save > 50% / 3 mo OR free months | any | n/a | yes | yes | CRM + Slack + Finance memo |
| SLA-credit per contract clause | per contract | automated | n/a | n/a | Stripe invoice memo |
| Below-COGS save (any tier) | any | n/a | n/a | yes | CRM + Slack + Finance memo + rationale paragraph |
| Pro-services hours bundled | any | n/a | yes | n/a | DPA addendum |
| Bespoke (credit memo, free months, hardware-equivalent) | any | n/a | n/a | yes | DPA addendum + Founder memo |

### Approval workflow

1. AE / CSE drafts the offer in the CRM tenant record with: customer name, tier, current $/mo, proposed $/mo, term length, expected save ARR, rationale.
2. If pre-authorized → AE / CSE executes immediately; logs the approval.
3. If Founder co-sign → AE / CSE pings #retention Slack with the CRM link; Founder approves in-channel (visible audit trail).
4. If Founder + Finance dual-sign → email thread to Founder + Finance with the proposal; both reply "approved" before AE executes.
5. **No retroactive approvals.** If an AE extended a discount beyond their pre-authorized authority, it must be ratified within 1 business day by the appropriate signer OR the offer is rescinded with apology to the customer (preserve trust).

### Per-tenant lifetime cap

Default: 2 save plays per tenant per calendar year. Third trigger = "save or release" decision at Founder level — repeat-saves indicate mis-fit. This cap may be overridden by Founder with explicit memo.

---

## 6. Escalation tree (when accountable is unreachable)

```
Primary accountable
        │
        ▼  (4 h business unresponsive on high-severity)
Secondary owner from §2 table (Consulted role)
        │
        ▼  (8 h business unresponsive)
Founder (Gustavo)
        │
        ▼  (24 h business unresponsive AND signal is high-severity AND tenant is Enterprise)
External advisor pool (per H-15) — commercial advisor takes the call
```

For BYOK / security-tinged signals (S-15) the escalation goes through [`RB-DPO-ESCALATION.md`](./RB-DPO-ESCALATION.md) on the security branch in parallel with the commercial branch.

For SLO-breach signals (S-06) the on-call SRE owns the technical post-mortem branch per [`RB-POSTMORTEM-PROCESS.md`](./RB-POSTMORTEM-PROCESS.md); commercial branch (this runbook) runs in parallel.

---

## 7. Recording and audit trail

Every signal fire, triage decision, play touch, and offer extended is recorded in the CRM tenant record AND emits a corresponding entry in the internal `corelink-retention` audit log (Postgres table managed by CSE). Quarterly review per [`quarterly-review-template.md`](../../marketing/retention/quarterly-review-template.md) reads from this table.

The CRM tenant record MUST contain, at minimum:

- All signal fires (date, signal id, triage decision, triage rationale).
- All plays opened (date, play id, owner, first-touch timestamp, outcome).
- All offers extended (date, magnitude, approver, rationale, accepted/declined, term).
- All cancellation-flow events (started/abandoned/confirmed/re-activated).
- Open save plan (if multi-signal).

This record is **read-only** for the customer except via DSR — they don't see retention internals.

---

## 8. Connection to cancellation flow

When a tenant initiates the cancellation flow per [`CANCELLATION-FLOW.md`](../../marketing/retention/CANCELLATION-FLOW.md):

- **Step 1 click** emits `corelink.retention.cancel_started` internal event. CSE + AE are paged via #retention Slack channel within 1 minute.
- **Step 2 survey response** routes the Step 3 retention-offer variant per [`CANCELLATION-FLOW.md`](../../marketing/retention/CANCELLATION-FLOW.md) §3 matrix.
- **Step 4 confirm** emits `corelink.billing.subscription.cancelled` audit chain event (customer-visible) AND `corelink.retention.cancel_confirmed` internal event. AE + CSE + Founder are notified; if no save-plan was opened pre-cancel, a written postmortem is owed at the next quarterly review (§2 missed-churn protocol).
- **Step 5 receipt** is auto-generated; no action required.
- **30-day re-activation window** is owned by CSE who monitors `corelink.billing.subscription.reactivated` events.
- **30-day final purge** is owned by the `cleanup-cancelled-tenants.py` cron job (per [`CANCELLATION-FLOW.md`](../../marketing/retention/CANCELLATION-FLOW.md) §4); CSE is informed.

---

## 9. Quarterly review obligations

Per [`quarterly-review-template.md`](../../marketing/retention/quarterly-review-template.md):

- **CSE** assembles signal data (fires, TPs, FPs, saves per signal). Due 5 business days before the review meeting.
- **AE** assembles play outcomes (triggered, in-SLA, saves, $ retained). Due 5 business days before.
- **Finance** assembles revenue impact (churned ARR, discount $ extended, NRR/GRR). Due 5 business days before.
- **Founder** authors §5 Decisions and §6 Action Items during the 90-min review meeting; commits the dated review file the same day.

---

## 10. Templates

- Customer-facing comms templates: [`RETENTION-PLAYBOOK.md`](../../marketing/retention/RETENTION-PLAYBOOK.md) Appendix.
- Save-plan template (1 page, ≤ 5 bullets):

  ```
  Tenant: {name} | Tier: {tier} | ARR at risk: ${X}
  Signals fired (date): {S-XX, S-XX}
  Hypothesis (1 sentence): {root cause}
  Save lever (1 sentence): {discount / upgrade / commercial restructure}
  Owner: {name}
  Close-by date: {YYYY-MM-DD}
  Outcome (filled at close): {saved $X / churned $X / strategic-forfeit}
  ```

- Multi-signal huddle agenda (48 h after 3rd signal):
  1. State of the tenant (CSE; 5 min)
  2. Commercial position (AE; 5 min)
  3. Technical position (SRE if relevant; 5 min)
  4. Save / exit / forfeit decision (Founder; 10 min, written outcome in CRM)

---

## 11. Pre-GA status

This runbook is **provisioned but not exercised** at GA day-1. First quarterly review (per §9) is 2026-Q3 — three months after the first paid renewal cohort, which is the earliest meaningful data window. Pre-GA churn = lighthouse churn (handled per [`RB-LIGHTHOUSE-PHASE-MANAGEMENT.md`](./RB-LIGHTHOUSE-PHASE-MANAGEMENT.md), not this runbook).

Signal alerting rules (PromQL + SQL cron) must be wired before T+30d post-GA — earliest day a paid tenant can fire S-01 (the 7-day vs trailing 4-week window requires 5 weeks of data).

---

## Cross-references

- Customer-facing signal catalog: [`../../marketing/retention/CHURN-RISK-SIGNALS.md`](../../marketing/retention/CHURN-RISK-SIGNALS.md)
- Customer-facing retention plays: [`../../marketing/retention/RETENTION-PLAYBOOK.md`](../../marketing/retention/RETENTION-PLAYBOOK.md)
- Cancellation flow design: [`../../marketing/retention/CANCELLATION-FLOW.md`](../../marketing/retention/CANCELLATION-FLOW.md)
- Quarterly review template: [`../../marketing/retention/quarterly-review-template.md`](../../marketing/retention/quarterly-review-template.md)
- Customer support T+0..T+90: [`RB-CUSTOMER-SUPPORT-T-90.md`](./RB-CUSTOMER-SUPPORT-T-90.md)
- Post-mortem process: [`RB-POSTMORTEM-PROCESS.md`](./RB-POSTMORTEM-PROCESS.md)
- DSR / GDPR Art. 17: [`RB-DSR-GDPR.md`](./RB-DSR-GDPR.md)
- DPO escalation: [`RB-DPO-ESCALATION.md`](./RB-DPO-ESCALATION.md)
- Pricing + COGS: [`../../marketing/sales/PRICING-WORKSHEET.md`](../../marketing/sales/PRICING-WORKSHEET.md)
- Objection handling (renewal context): [`../../marketing/sales/OBJECTION-HANDLING.md`](../../marketing/sales/OBJECTION-HANDLING.md)
- Lighthouse playbook: [`../../marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`](../../marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md)
- GA post-launch wave: [`../../ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §8
