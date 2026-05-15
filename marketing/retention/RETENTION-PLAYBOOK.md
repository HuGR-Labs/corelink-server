---
id: "R-PREP-RETENTION-PLAYBOOK"
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
tags: ["marketing", "retention", "playbook", "plays", "comms", "discounts", "r-prep", "ga", "post-launch"]
---

# CoreLink Retention Playbook — Per-Signal Response Plays

> **Audience.** Customer Success, Account Executives, Founder (for high-severity touches).
> **Purpose.** For each signal in [`CHURN-RISK-SIGNALS.md`](./CHURN-RISK-SIGNALS.md), the literal response play: who acts, in what window, with what comms template, with what discount authority. Pair with [`../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md) for the operational RACI + escalation tree.
> **Companion docs.** [`../sales/OBJECTION-HANDLING.md`](../sales/OBJECTION-HANDLING.md) (renewal-conversation patterns), [`../sales/PRICING-WORKSHEET.md`](../sales/PRICING-WORKSHEET.md) (discount math + COGS floor), [`../lighthouse-kit/CUSTOMER-PLAYBOOK.md`](../lighthouse-kit/CUSTOMER-PLAYBOOK.md) (the lighthouse engagement contract).
> **Roadmap link.** [`../../ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §8 — retention motion activates from T+1d.

---

## Discount authority matrix

| From → To | Discount | Term | Approver | Notes |
|---|---|---|---|---|
| Free → Starter | Complimentary upgrade ($0 for 90 d) | 3 mo | AE / CSE | Auto-approve up to 25 tenants/quarter; beyond → Founder |
| Starter → Team | 50% off list | 3 mo | AE / CSE with Founder notice | Recoverable in month 4 at full list |
| Team → Enterprise | Custom negotiation | per contract | Founder | Multi-year tied; price-lock per [`OBJECTION-HANDLING.md`](../sales/OBJECTION-HANDLING.md) Obj-3 |
| Any tier same-tier renewal save | Up to 25% off / 3 mo | 3 mo | AE / CSE | Documented in CRM with signal ID |
| Any tier same-tier renewal save | 25–50% off / 3 mo | 3 mo | Founder | Required for >25% |
| Any tier same-tier renewal save | > 50% off OR free month(s) | any | Founder + Finance | Margin floor per [`PRICING-WORKSHEET.md`](../sales/PRICING-WORKSHEET.md) COGS table |
| Enterprise | Bespoke (credit memo, free month, pro-services hours) | per contract | Founder | Document in DPA addendum |

**Floor rule.** No discount may push the resulting price below the COGS line in [`PRICING-WORKSHEET.md`](../sales/PRICING-WORKSHEET.md) without Founder + Finance dual-sign-off. Discounting below COGS is permitted as a strategic save but must be logged with rationale.

**Per-tenant discount cap (calendar year).** Max two save plays per tenant per calendar year. Third trigger goes straight to Founder for "save or release" decision — repeat-saves are a leading indicator of mis-fit.

---

## High-severity plays (CEO touch ≤ 24 h)

### P-HIGH-USAGE-DROP — signal S-01 (usage drop > 50% WoW)

- **Owner:** Founder (Gustavo) primary; CSE shadow.
- **First touch:** within 24 h.
- **Channel:** direct email to primary admin + Slack Connect channel ping (if Team/Pro/Enterprise).
- **Goal of first contact:** understand cause (CI migration / holiday / dissatisfaction). Do not pitch save until cause is known.
- **Comms template:**
  > Subject: Quick check-in — your CoreLink usage
  >
  > Hi {name},
  >
  > Our usage dashboard flagged that {tenant} ran significantly fewer cache operations this past week than your trailing four-week pattern. Nothing on our end is broken — I'm just making sure things are okay on yours.
  >
  > Three quick questions:
  > 1. Anything we should know? (Internal migration, holiday, change of priorities — all completely normal.)
  > 2. Anything blocking you from getting value out of CoreLink today?
  > 3. Would a 15-min call this week help, or is async fine?
  >
  > No agenda beyond making sure you're getting what you signed up for.
  >
  > — Gustavo (Founder, CoreLink)
- **Escalation path:** if no response in 5 business days → AE follows up via LinkedIn; if no response in 10 business days → flag for "release" decision at next quarterly review.
- **Discount authority:** up to 50% / 3 mo (AE may pre-authorize); larger requires Founder co-sign.

### P-HIGH-STUCK-TICKET — signal S-02 (open ticket > 7 d)

- **Owner:** Founder primary; original ticket owner (support) co-acts.
- **First touch:** within 24 h.
- **Channel:** ticket reply + direct call offer to the requester.
- **Goal of first contact:** unblock the technical issue first. Apology and discount discussion only after resolution path is clear.
- **Comms template:**
  > Subject: RE: {ticket subject} — escalating to me directly
  >
  > Hi {name},
  >
  > Your ticket has been open longer than our SLA commits. That's on us, not you. I'm taking ownership directly until it's resolved.
  >
  > Here's what I see and what I plan to do: {2-3 sentence technical summary + next step + ETA}.
  >
  > I'll have an update for you by {EOD next business day}. If at any point you want a call, my calendar is {link}.
  >
  > — Gustavo
- **Escalation path:** ticket converts to incident per `specs/_runbooks/RB-CUSTOMER-SUPPORT-T-90.md` if root cause is product-side; post-mortem deliverable to customer per `specs/_runbooks/RB-POSTMORTEM-PROCESS.md`.
- **Discount authority:** at-fault SLA breach earns a credit memo of one month's tier base equivalent (auto-approve); upgrade-bridge negotiation requires Founder.

### P-HIGH-PAYMENT-FAIL — signal S-03 (payment retry > 3)

- **Owner:** Founder primary; billing automation runs in parallel.
- **First touch:** within 24 h.
- **Channel:** direct email — NOT a dunning email. Dunning is already running via Stripe Smart Retries; this is the human override.
- **Goal of first contact:** distinguish "card expired, will update" from "we are leaving."
- **Comms template:**
  > Subject: Quick note on your CoreLink billing
  >
  > Hi {name},
  >
  > Our payment processor has had a few failed retries on the card on file for {tenant}. Usually this is something simple — expiration, bank issue, new card not yet rotated.
  >
  > Two options:
  > - Update the card at {billing portal URL} and you're done.
  > - If something else is going on — switching tools, budget freeze — just hit reply and tell me. I'd rather know than guess.
  >
  > Either way, your service is **not** at risk this week. We don't suspend until day 14.
  >
  > — Gustavo
- **Escalation path:** at day 14 of failed retries, automated suspension per [`CANCELLATION-FLOW.md`](./CANCELLATION-FLOW.md) §6 (grace-window policy). Suspension is reversible; deletion follows the same 30-day retention window.
- **Discount authority:** one-time 50% / 1 month bridge offer auto-authorized for genuine budget squeeze; document in CRM.

### P-HIGH-ADMIN-CHANGE — signal S-04 (admin removed without replacement)

- **Owner:** Founder primary; AE supports.
- **First touch:** within 24 h.
- **Channel:** outbound to remaining org contacts via LinkedIn + email; if Slack Connect channel exists, ping the channel.
- **Goal of first contact:** identify the new champion. The original champion is often gone (job change, restructure).
- **Comms template:**
  > Subject: Champion handoff at {tenant}?
  >
  > Hi {best-guess new contact},
  >
  > I noticed {departed admin} is no longer listed as an admin on your CoreLink account. We want to make sure the handoff is clean and there's no friction for whoever picks this up.
  >
  > Can you let me know who the right point of contact is now? Happy to do a 20-min onboarding session with them for free, so they don't have to start cold.
  >
  > — Gustavo
- **Escalation path:** if no new champion identified within 14 d → flag tenant as "orphaned" in CRM and treat as high-risk at next renewal.
- **Discount authority:** complimentary onboarding session for new champion (zero cost); no other discount triggered automatically.

### P-HIGH-SLO-BREACH — signal S-06 (SLO breach in last 30 d)

- **Owner:** Founder primary; SRE on-call delivers the technical post-mortem.
- **First touch:** within 24 h of breach close (not breach open).
- **Channel:** direct email + scheduled call offer.
- **Goal of first contact:** deliver post-mortem, own the failure, propose the credit per the customer agreement's SLA-credit clause.
- **Comms template:**
  > Subject: SLA breach on {date} — post-mortem + credit
  >
  > Hi {name},
  >
  > Between {start} and {end} on {date}, you experienced {SLO} degradation that fell outside our committed envelope. I'm including the full post-mortem (attached) and the automatic credit on your next invoice ({amount}).
  >
  > Three asks:
  > 1. Read the post-mortem — I want to know if anything in our root-cause framing doesn't match what you observed on your side.
  > 2. 30-min call this week to walk through the fixes we've shipped since.
  > 3. Tell me, honestly, whether this changes your confidence in renewing.
  >
  > — Gustavo
- **Escalation path:** if customer indicates renewal at risk → automatic engagement of Founder + AE for save-play planning per [`RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md) §6.
- **Discount authority:** SLA-credit per contract (automatic); additional save discount up to 50% / 3 mo if customer flags renewal risk.

### P-HIGH-EXPORT-SPIKE — signal S-10 (egress / export 10× baseline)

- **Owner:** Founder primary.
- **First touch:** within 24 h.
- **Channel:** direct email — neutral tone. The escape hatch is a feature, not a betrayal; treat it as such.
- **Goal of first contact:** understand whether the customer is leaving, scaling, or running a routine archival.
- **Comms template:**
  > Subject: Big export from {tenant} — anything we should know?
  >
  > Hi {name},
  >
  > We saw a significant `corelink cas export` run from your tenant — about 10× your normal pattern. The export tool exists exactly for use cases like that, so no concern from a permissions or capacity standpoint.
  >
  > I'm reaching out just in case the context is "we're evaluating alternatives" — if it is, I'd rather hear it directly so I can either save the relationship or help you exit cleanly. Both are real options for me.
  >
  > If it's a backup / archival, ignore this note.
  >
  > — Gustavo
- **Escalation path:** if customer confirms evaluation of alternatives → full save motion: in-person / on-video call, technical deep-dive on the gap they perceive, custom commercial offer per Founder + Finance.
- **Discount authority:** up to custom negotiation (Founder).

### P-HIGH-NO-RENEW — signal S-14 (auto-renewal opt-out)

- **Owner:** Founder primary.
- **First touch:** within 24 h.
- **Channel:** direct email + immediate call request.
- **Goal of first contact:** they have already decided. The save play is to reverse the decision before term end.
- **Comms template:**
  > Subject: You turned off auto-renew on {tenant}
  >
  > Hi {name},
  >
  > I saw the auto-renew flag came off this morning. I'd like 20 minutes of your time before {current_term_end} to understand why and see if there's anything we can do about it. Possible outcomes from that conversation, in my order of preference:
  >
  > 1. We fix what's broken and you flip auto-renew back on.
  > 2. We adjust the commercial terms and you renew at a different shape.
  > 3. We help you exit cleanly — data export, contractual wind-down, references for an alternative.
  >
  > All three are real. Calendar: {link}.
  >
  > — Gustavo
- **Escalation path:** if customer agrees to a call → standard save sequence (Founder + AE prepare commercial counter-offer per discount matrix). If customer declines → enter clean-exit motion per [`CANCELLATION-FLOW.md`](./CANCELLATION-FLOW.md) §4.
- **Discount authority:** custom negotiation (Founder); full discount matrix available.

### P-HIGH-BYOK-ARMED — signal S-15 (BYOK kill-switch armed outside drill)

- **Owner:** Founder + DPO (parallel; security takes priority on the security branch).
- **First touch:** within 24 h.
- **Channel:** direct call to primary security contact; email back-up.
- **Goal of first contact:** distinguish security incident (escalate per [`RB-DPO-ESCALATION.md`](../../specs/_runbooks/RB-DPO-ESCALATION.md)) from "we are preparing to leave."
- **Comms template:**
  > Subject: BYOK kill-switch armed — please confirm intent
  >
  > Hi {name},
  >
  > Your BYOK kill-switch armed at {timestamp} and we don't see a scheduled drill on the calendar for that window. Per our agreement we don't proactively disarm — that's your control, and it works as designed.
  >
  > Two reasons we'd reach out:
  >
  > 1. **Security incident on your side** — if so, my DPO ({DPO contact}) is the right channel and I'm copying them.
  > 2. **Operational pre-departure** — if you're decommissioning, no judgement, but I'd like to know so I can help you exit cleanly.
  >
  > 15-min call today? Calendar: {link}.
  >
  > — Gustavo
- **Escalation path:** if security incident → standard `RB-DPO-ESCALATION.md` path (this play stops, security takes over); if departure → standard high-severity save motion.
- **Discount authority:** Enterprise tier → custom negotiation (Founder).

---

## Medium-severity plays (AE / CSE touch ≤ 72 h)

### P-MED-AUDIT-COLD — signal S-05 (audit query rate → 0)

- **Owner:** AE / CSE.
- **First touch:** within 72 h.
- **Channel:** email + optional 15-min call.
- **Goal:** check in with the compliance / governance stakeholder; verify CoreLink still aligns with their compliance posture.
- **Comms template:** see Appendix template T-01.
- **Discount authority:** none triggered; possible upsell to BYOK / Enterprise if compliance need grew.

### P-MED-PRICING-VISIT — signal S-07 (pricing-page revisit cluster)

- **Owner:** AE.
- **First touch:** within 72 h.
- **Channel:** email; consultative.
- **Goal:** if upsizing → standard expansion motion; if down-shopping → save motion.
- **Comms template:** see Appendix template T-02.
- **Discount authority:** standard up to 25% / 3 mo (AE pre-authorized).

### P-MED-QBR-SKIP — signal S-08 (QBR declined / no-show)

- **Owner:** CSE.
- **First touch:** within 72 h.
- **Channel:** reschedule offer + abbreviated 15-min option.
- **Goal:** keep the cadence alive; do not over-pressure.
- **Comms template:** see Appendix template T-03.
- **Discount authority:** none.

### P-MED-LOW-SEAT — signal S-09 (seat utilization < 30% for 60 d)

- **Owner:** AE.
- **First touch:** within 72 h.
- **Channel:** email; offer proactive downgrade to lower tier OR adjusted seat count.
- **Goal:** preempt downgrade-at-renewal by offering it now in exchange for term extension.
- **Comms template:** see Appendix template T-04.
- **Discount authority:** down-tier swap is revenue-protective, not a discount; if customer wants stay-and-discount, up to 25% / 3 mo.

### P-MED-DETRACTOR — signal S-11 (NPS detractor ≤ 6)

- **Owner:** Founder if score ≤ 3; AE if 4–6.
- **First touch:** within 72 h.
- **Channel:** email referencing the verbatim — never pretend you didn't see it.
- **Goal:** acknowledge, learn, decide if it's fixable.
- **Comms template:** see Appendix template T-05.
- **Discount authority:** AE up to 25% / 3 mo if the NPS comment indicates pricing friction; Founder for value-friction (different conversation).

### P-MED-HIT-RATIO-DROP — signal S-13 (hit ratio ↓ > 20 pp)

- **Owner:** CSE + Solutions Eng.
- **First touch:** within 72 h.
- **Channel:** email with attached technical diagnostic.
- **Goal:** restore hit ratio. Free diagnostic session is the lead-in.
- **Comms template:** see Appendix template T-06.
- **Discount authority:** none unless degradation was product-side (then SLA credit applies).

---

## Low-severity plays (educational comms ≤ 7 d)

### P-LOW-DOC-THRASH — signal S-12 (doc thrash without ticket)

- **Owner:** Marketing automation; CSE reviews monthly aggregate.
- **First touch:** within 7 d.
- **Channel:** automated educational email — Cookiebot-respectful.
- **Goal:** unstick passively; surface the docs page the user is wrestling with + offer a free 15-min Solutions session.
- **Comms template:** see Appendix template T-07.
- **Discount authority:** none.

---

## Appendix — comms templates (T-01 through T-07)

### T-01 — Audit-cold check-in

> Subject: How is CoreLink fitting your audit cadence?
>
> Hi {name},
>
> Quick check-in on the compliance side. We noticed your audit-chain queries have been quiet over the last couple of weeks, which usually just means you're not in an audit window — but it's a good moment for me to ask: is the audit chain still doing what you need? Anything we should add, expose differently, or document better?
>
> Happy to do a 15-min walkthrough if it'd help.
>
> — {CSE name}

### T-02 — Pricing revisit

> Subject: Anything on the pricing page I can help with?
>
> Hi {name},
>
> Our system flagged that some folks at {tenant} have been looking at our pricing page this week. That usually means one of two things: you're growing into a new tier (great, let me help) or you're re-evaluating (also fine, let me help differently). Which one is closer?
>
> Happy to share the internal usage-to-tier mapping that powers the public calculator if it's useful — sometimes the recommendation differs from the obvious read.
>
> — {AE name}

### T-03 — QBR reschedule

> Subject: Let's reschedule our QBR
>
> Hi {name},
>
> No worries on missing the QBR — calendars get hard. If a full 45-min slot is too much, we can do a 15-min async-friendly version: I'll send a one-page status doc, you reply with anything you want to flag, we close it out. Tell me what's easier.
>
> — {CSE name}

### T-04 — Seat right-sizing

> Subject: A leaner shape for {tenant} on CoreLink?
>
> Hi {name},
>
> Looking at the last 60 days, your team has been using about {N} of the {M} seats your tier covers. That's fine — the seats are there if you need them — but if it's a stable pattern, we can move you down a tier (or adjust seat count on your current tier) without disrupting any of your usage. The savings is roughly {$X} per month.
>
> Want me to put together the swap proposal?
>
> — {AE name}

### T-05 — NPS detractor

> Subject: Following up on your NPS response
>
> Hi {name},
>
> Thank you for the candid NPS response — I read it. You wrote: "{verbatim}". I want to take that seriously, so I'm reaching out personally rather than sending you a templated follow-up survey.
>
> Two things I want to understand:
> 1. Is the issue you described still happening, or has it been resolved?
> 2. If it's still happening, what would "fixed" look like to you?
>
> Reply when you have a moment — even a one-line answer helps me prioritize.
>
> — {Founder or AE depending on score}

### T-06 — Hit ratio drop

> Subject: Your cache hit ratio dropped this week
>
> Hi {name},
>
> Heads up — your 7-day rolling cache hit ratio on {tenant} dropped about {X} percentage points compared with your baseline. That usually means one of: a CI config change, a region pin mismatch, a retention policy that's too short for your artifact churn, or a new workload added recently.
>
> I've attached a diagnostic from our side. If you want, we can spend 20 min on a screenshare to walk through it — no cost — and figure out whether it's a real regression or a workload shape change that just needs a config tweak.
>
> — {CSE name}

### T-07 — Doc thrash (automated)

> Subject: Stuck on {topic}?
>
> Hi there,
>
> We noticed folks at your team have been on {doc page} a few times this week. If you're stuck, we have a free 15-min "ask anything" slot with a Solutions Engineer — book it at {link}, no obligation. We'll either unblock you or improve the doc, both of which are wins for us.
>
> If you're not stuck, ignore this — we're not going to nag.
>
> — The CoreLink team

---

## Cross-references

- Signal catalog: [`CHURN-RISK-SIGNALS.md`](./CHURN-RISK-SIGNALS.md)
- Cancellation flow design: [`CANCELLATION-FLOW.md`](./CANCELLATION-FLOW.md)
- Quarterly KPI review: [`quarterly-review-template.md`](./quarterly-review-template.md)
- Internal ops runbook + RACI: [`../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md`](../../specs/_runbooks/RB-CHURN-RISK-RESPONSE.md)
- Discount math + COGS floor: [`../sales/PRICING-WORKSHEET.md`](../sales/PRICING-WORKSHEET.md)
- Renewal conversation patterns: [`../sales/OBJECTION-HANDLING.md`](../sales/OBJECTION-HANDLING.md)
- Lighthouse engagement playbook: [`../lighthouse-kit/CUSTOMER-PLAYBOOK.md`](../lighthouse-kit/CUSTOMER-PLAYBOOK.md)
- GA post-launch wave: [`../../ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §8
