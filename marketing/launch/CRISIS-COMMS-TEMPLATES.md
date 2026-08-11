---
id: "CRISIS-COMMS-TEMPLATES"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "VPSec + VPMkt (dual)"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "ROADMAP-TO-GA.md §8 (R-8 Launch)"
tags:
  - "marketing"
  - "launch"
  - "crisis-comms"
  - "incident"
  - "templates"
  - "lgpd"
  - "gdpr"
  - "wt-r8-1"
---

# Crisis Comms — Ready-to-Send Templates

> **Purpose:** five ready-to-send templates covering the most-likely crisis scenarios during and after the GA launch window. Each scenario has: a **status-page message**, a **customer email**, a **public tweet** (when appropriate), and an **internal protocol** (who-decides-what-by-when).
> **Audience:** war room (`LAUNCH-CHECKLIST-V2.md` §1 roles), VPSec, VPMkt, CEO.
> **Hard rule:** **DO NOT fabricate facts.** When in doubt, write "we are investigating" and update the timeline every 30 min. Inaccurate early statements destroy trust more than slow accurate ones.
> **Hard rule:** **Customer-impacting messages require CEO sign-off** — except SEV1 status-page auto-publish (per `STATUS-PAGE-SPEC.md` §5).

---

## Index

| Scenario | Trigger | Templates included | Regulatory clock starts? |
|---|---|---|---|
| **A** | SEV1 service outage > 5 min | Status page (auto) + customer email + tweet | No |
| **B** | Data privacy incident (suspected unauthorized access) | Internal-first protocol + 72h regulator notice prep + customer email | **YES** — LGPD Art. 48 / GDPR Art. 33 (72h) |
| **C** | Pricing / billing bug at scale | Refund-first email + status post + cause statement | No (but Stripe coordination) |
| **D** | BYOK CMK compromise rumor (unsubstantiated) | Fact-vs-rumor template | No, unless confirmed (then escalate to Scenario B) |
| **E** | External pentester finds vuln before disclosure window | Coordinated-disclosure response | No, unless exploited in wild |
| **F** | (Bonus) Launch defer comms | Internal + customer + external | No |

---

## Scenario A — SEV1 service outage > 5 min

**Trigger:** PagerDuty SEV1 declaration on any of C1..C6 (API, CAS Read, CAS Write, BYOK, Audit, Billing) sustained > 5 min, or any incident that violates a customer-facing invariant.

### A.1 Status page — auto-publish template (SEV1)

> **Fired automatically by PagerDuty → Statuspage integration. No human approval. Refined post-publish.**

```
TITLE: Investigating — <ComponentName> issue

We are aware of an issue affecting <ComponentName>. Engineering is
investigating. Customers may experience <generic impact: elevated errors /
elevated latency / unavailability>.

We will post an update within 30 minutes, or sooner.

— CoreLink SRE
Status: Investigating
```

### A.2 Status page — first refined update (within 30 min)

```
TITLE: Identified — <one-line cause>

UPDATE (HH:MM UTC):
We have identified the cause as <plain-English cause: "a deployment
regression in the CAS write path", "a degraded BYOK vendor dependency",
etc.>. <Specific user impact in one sentence.>

We are <one-sentence remediation: rolling back the change / failing over to
the secondary region / coordinating with our BYOK vendor>.

Next update in 30 minutes.
```

### A.3 Status page — resolution

```
TITLE: Resolved — <component> restored

UPDATE (HH:MM UTC):
Service has been restored. <Component> has been operating normally for the
past 15 minutes. We will publish a public post-incident retrospective within
14 days at humangr.com/corelink/blog/incidents.

We apologize for the disruption. Thank you for your patience.
```

### A.4 Customer email (sent to affected tenants after page is live)

> **Sent by:** CS-OC. **Approval:** VPMkt or CEO. **Timing:** within 1 hour of incident declaration; do not wait for resolution.

```
Subject: [CoreLink] Service disruption — <Component> — <date HH:MM UTC>

Hi <name>,

We're writing to let you know that CoreLink experienced a service disruption
affecting <Component> starting at <HH:MM UTC>. Based on our telemetry, your
account <was / was not> affected during this window.

What happened:
<One paragraph plain-English cause. NO jargon. NO blame on vendors by name
unless we've already confirmed publicly and Legal has cleared.>

Impact on your account:
<Specifics: blob writes failed; cache misses elevated; admin console
unavailable; etc. If you have lighthouse SLA, mention the SLA credit
mechanism.>

What we're doing:
<One paragraph: immediate remediation + medium-term prevention.>

Live updates: https://hugrl.betteruptime.com
Public retrospective: published within 14 days at humangr.com/corelink/blog/incidents

If you have questions or believe your impact was greater than what we've
described, please reply to this email and we'll get back to you within 4
business hours. SLA-eligible customers: SLA credits will be applied
automatically; no action required.

— The CoreLink team
```

### A.5 Public tweet (only after we have a verified statement)

> **Sent by:** VPMkt. **Approval:** CEO. **Timing:** never before the status page is updated.

```
We're aware of an issue affecting <Component> at <HH:MM UTC>. Engineering
is on it. Live updates: https://hugrl.betteruptime.com — full retro to follow.
```

### A.6 Internal protocol

1. PagerDuty fires → status page auto-publishes (§A.1).
2. SRE-OC + CTO join war room within 5 min.
3. WR-COORD pings VPMkt + CEO within 10 min.
4. First refined status update (§A.2) goes live within 30 min.
5. Customer email (§A.4) drafted within 45 min; CEO approves; sent within 60 min.
6. Tweet (§A.5) drafted; CEO approves; sent only if incident sustains > 30 min.
7. Resolution status (§A.3) goes live within 15 min of recovery.
8. Public retrospective drafted within 7d; published within 14d per `STATUS-PAGE-SPEC.md` §9.

---

## Scenario B — Data privacy incident (suspected unauthorized access)

**Trigger:** any of:
- WAF / audit-chain anomaly suggesting tenant-isolation violation.
- Discovery of unauthorized access to a CAS blob, audit log, or BYOK envelope key.
- Pentester / external researcher reports a credible isolation bypass.
- Insider-threat alert (admin console anomaly).

**Regulatory clock:** **72-hour notification window starts at the moment of "reasonable awareness"** under both LGPD Art. 48 (ANPD + affected data subjects when "relevant risk") and GDPR Art. 33 (supervisory authority within 72h). Document the awareness timestamp in the war room log immediately.

### B.1 Internal-first protocol (first 4 hours)

| Step | Owner | Timeline | Action |
|---|---|---|---|
| B-1 | SRE-OC | T+0 | PagerDuty SEV1 — "Suspected privacy incident, internal only." Do NOT auto-publish to status page (privacy SEV1 takes a different path). |
| B-2 | VPSec | T+15 min | Convene Incident Privacy Cell: VPSec, CTO, Legal, DPO. CEO informed but not commanding. |
| B-3 | VPSec | T+30 min | Snapshot evidence: audit chain, WAF logs, admin console actions. Freeze state. Per `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md` evidence-preservation steps. |
| B-4 | DPO | T+1h | Make written **awareness determination**: timestamp + scope + categories of data + estimated # of subjects. Document at `specs/_audits/privacy-incidents/PI-<date>.md`. |
| B-5 | Legal | T+2h | Determine **notification obligations** by jurisdiction: LGPD (Brazil), GDPR (EU), state laws (US). Build notice matrix. |
| B-6 | CEO + DPO + Legal | T+4h | Decision: **public statement now / public statement on confirmation / public statement at 72h notice fire**. Default = **at 72h notice fire** unless active exploitation requires earlier disclosure. |
| B-7 | VPSec | T+4h | Update internal incident ticket. **Status page silent until B-6 decision.** |

### B.2 72-hour regulator notice prep (template)

> **Per LGPD Art. 48 (Brazilian ANPD) — Portuguese translation also prepared. Per GDPR Art. 33 (lead supervisory authority).**
> **Legal-cleared template.** Do not deviate without Legal review.

```
TO: <Lead supervisory authority — e.g., ANPD (Brazil), CNIL (France) /
    lead DPA per One-Stop-Shop mechanism>
FROM: <DPO name>, Data Protection Officer, HuGR Labs / CoreLink
DATE: <YYYY-MM-DD HH:MM UTC>
RE: Notification of Personal Data Breach pursuant to <LGPD Art. 48 / GDPR
    Art. 33>

Section 1 — Nature of the breach
On <date HH:MM UTC>, CoreLink became aware of <plain description>.
The breach <was confirmed / is under investigation>.
Categories of data potentially affected: <e.g., account email, tenant
metadata, customer-managed-key envelope references; explicitly note
whether content-addressable blob contents were exposed>.

Section 2 — Approximate number of data subjects and records
<Number>, drawn from <method of estimation>.

Section 3 — Likely consequences
<Plain-English assessment.>

Section 4 — Measures taken or proposed
<Containment, remediation, customer notification plan.>

Section 5 — Contact for follow-up
<DPO name + email + phone>

We will provide an updated notification as further information becomes
available.

— <Signature>
```

**Filing channel:** ANPD via the `comunicado-incidente` portal; lead EU DPA per Art. 56 One-Stop-Shop. Filings logged in `specs/_audits/privacy-incidents/PI-<date>.md`.

### B.3 Customer email (sent at 72h notice fire OR sooner if confirmed exploitation)

> **Sent by:** DPO. **Approval:** CEO + Legal. **Timing:** simultaneous with regulator notice, or earlier if active exploitation.

```
Subject: [CoreLink] Important security update for your account

Dear <Customer>,

We are writing to inform you of a security incident that may affect your
CoreLink account.

What happened (factual, plain English):
<One paragraph. No vendor blame unless Legal-cleared. No speculation about
motive or actor.>

What information was involved:
<Categories of data — explicit. If unknown, say "we are still determining
the precise scope.">

What we have done:
<Containment + remediation actions taken so far.>

What you can do:
<Concrete recommendations: rotate any shared secrets, check audit logs we
provide, contact security@humangr.com for help.>

We have notified <regulator name(s)> as required by <LGPD Art. 48 /
GDPR Art. 33>. We will publish a public post-incident retrospective when
the investigation is complete.

For questions: security@humangr.com — monitored 24/7 during this period.

We are sorry. Trust is the product, and we will be transparent through
this.

— <CEO name>, CEO, HuGR Labs / CoreLink
```

### B.4 Public statement (status page + blog + tweet)

> **Only after B-6 decision approves public disclosure.**

Use `STATUS-PAGE-SPEC.md` §5 SEV1 messaging, but with **explicit privacy-incident framing** drafted by VPSec + Legal + VPMkt. **No template** — these must be hand-crafted per incident. Required elements: what we know, what we don't know, what we're doing, regulator notice status, customer recourse.

### B.5 Cross-references

- `SECURITY.md` — disclosure policy + contact.
- `specs/_security/vulnerability-disclosure-policy.md` — canonical spec.
- `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md` — overlapping protocol for pentester-discovered incidents.
- `specs/_runbooks/RB-SECURITY-VULNERABILITY-INTAKE.md` — intake process.

---

## Scenario C — Pricing / billing bug at scale

**Trigger:** Stripe webhook backlog OR invoice-generation regression OR metering aggregation bug detected — affecting > 1 customer OR > $1,000 in mis-billing.

**Posture:** **Refund-first.** We refund before we explain. The reputational cost of a billing bug is dominated by how slowly we make customers whole.

### C.1 Internal protocol

1. **Pause billing automation** for affected component within 30 min of detection.
2. **Identify affected accounts** within 2 hours.
3. **Issue refunds / credits** within 24 hours — no customer email needed first.
4. **Then** send customer email (§C.2) explaining what happened.
5. **Public cause statement** (§C.3) on the blog within 7d.

### C.2 Customer email (sent after refund processed)

```
Subject: [CoreLink] We refunded your account — here's what happened

Hi <name>,

A billing issue caused your CoreLink account to be charged incorrectly on
<date>. The error: <one sentence plain English>.

We have already refunded <$X> to the payment method on file. You should see
the refund within <Stripe-stated window: typically 5-10 business days>. No
action is required from you.

Why we caught it: <one sentence — e.g., "our reconciliation pipeline
flagged a divergence between metering aggregates and invoice totals">.
What we changed: <one sentence — e.g., "we paused automated invoicing until
the reconciliation passes daily; we added a guardrail that blocks invoices
exceeding 2x the rolling 30-day average without human approval">.

We are sorry. Billing should never be a surprise. If you believe the refund
amount is wrong, reply to this email — we'll get back to you within one
business day.

— <CEO name>
```

### C.3 Public cause statement (blog post)

> **Drafted by:** VPMkt + Finance + CTO. **Approved by:** CEO. **Published:** within 7d.

```
Title: A billing bug, and what we did about it

On <date>, a billing regression caused <N> CoreLink customers to be
overcharged a total of <$X>. We caught the bug at <HH:MM UTC>, paused
automated invoicing, refunded every affected account within 24 hours, and
have since shipped <specific guardrail> to prevent recurrence.

What happened: <2-3 paragraphs, plain English, no jargon.>
Who was affected: <numeric scope; no customer names.>
What we did: <refund timeline; engineering fix; new guardrail.>
What we learned: <one paragraph, blameless.>

We hold ourselves to the standard that billing is part of the product. We
fell short here. We are doing the work to make sure we don't again.

If you have any questions, billing@humangr.com is always staffed.

— CoreLink
```

---

## Scenario D — BYOK CMK compromise rumor (unsubstantiated)

**Trigger:** social media or press report claiming a CoreLink customer's customer-managed key (CMK) has been compromised, but CoreLink has not internally confirmed.

**Posture:** **Fact-vs-rumor.** Speak only to what we have verified. Do not deny something that might be true; do not confirm something that might be false. The template separates **what we have evidence for**, **what we are investigating**, and **what we will publish next**.

### D.1 Public statement (tweet + status page banner)

```
We're aware of <reports / claims> regarding <topic>. We have begun an
internal review. Based on <CoreLink-internal telemetry / audit chain
inspection>, we have <no evidence / preliminary evidence> of <specific
claim>. We will share verified findings within <timeframe: 24h / 48h>.

For confirmed customer impact, hugrl.betteruptime.com will publish.
For security disclosures, contact security@humangr.com.

— CoreLink Security
```

### D.2 Customer email (only to specifically named customers if rumor names them)

```
Subject: [CoreLink] Update on recent reports

Dear <Customer>,

You may have seen recent <reports / posts> regarding <topic>. We wanted to
write to you directly.

Based on our internal review as of <HH:MM UTC>:
- Verified: <specific facts we can stand behind>
- Under investigation: <specific items we have not yet resolved>
- Not confirmed: <specific claims in the public report that we have no
  evidence for>

If the investigation reveals customer impact to your account, we will
inform you within <timeframe>, and we will follow the protocols described
in our DPA <Section X>.

For questions: security@humangr.com.

— <CEO name>
```

### D.3 Internal protocol

1. **Do not respond on social media in the first hour.** Let VPSec verify state first.
2. **Audit chain inspection** by VPSec + CTO within 4h.
3. **BYOK vendor coordination** if rumor mentions a specific vendor (AWS KMS / GCP KMS / Azure / Vault).
4. **If confirmed:** escalate to Scenario B (privacy incident protocol).
5. **If unsubstantiated:** publish §D.1 within 4h; do not engage with replies; commit to update within 48h.

---

## Scenario E — External pentester finds vuln before disclosure window

**Trigger:** an external researcher (not under contract) publicly discloses a CoreLink vulnerability before the coordinated-disclosure window CoreLink advertises in `SECURITY.md` and `.well-known/security.txt`.

**Posture:** **Acknowledge research, defend the customer, coordinate not litigate.** Per `SECURITY.md` we offer a 90-day coordinated disclosure window; if a researcher publishes early, we do not threaten legal action — we move quickly to patch and credit good-faith research.

### E.1 Public statement (tweet + status page if exploitable)

```
We've seen <researcher name>'s report on <topic>. Thank you for the
research. Our security team is reviewing.

If you believe your account may be affected, contact
security@humangr.com — we are responding 24/7.

We will publish a verified statement within <timeframe: 24h-48h>.

— CoreLink Security
```

### E.2 Internal protocol

1. **Acknowledge publicly within 4h** (§E.1). Public silence > 4h is misread as evasion.
2. **Triage the finding** per `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md`.
3. **Determine exploitability in the wild** within 24h. If exploited → Scenario B.
4. **Patch + disclosure timeline communication** to the researcher within 48h. Offer Hall of Fame credit (`/security/hall-of-fame`).
5. **Public retrospective + patch announcement** within disclosure window agreed with researcher.
6. **Do not threaten legal action.** Good-faith research is in-scope per `SECURITY.md`.

### E.3 Researcher email (direct outreach)

```
Subject: Your CoreLink security research — coordination

Hi <researcher name>,

We saw your report on <topic>. Thank you for the work and for reaching out
publicly — we know that takes effort.

We have started our internal triage at <HH:MM UTC>. Our preliminary
assessment is <one sentence: confirmed / disputed / partially confirmed>.

We'd like to coordinate the disclosure timeline. Our standard window
(documented at SECURITY.md) is up to 90 days, but we can move faster if a
fix is straightforward. Could we set up a call this week?

We credit good-faith research in our Hall of Fame
(https://humangr.com/corelink/security/hall-of-fame) and have a bug bounty for
in-scope findings.

— <VPSec name>, Security, HuGR Labs / CoreLink
```

---

## Scenario F — Launch defer comms (bonus, but operationally critical)

**Trigger:** Engineering Gate revokes APPROVED status at any T-24h..T-1h check (`LAUNCH-CHECKLIST-V2.md` rows L1, L11, L13).

### F.1 Internal — all-hands message (within 30 min of defer decision)

```
Team,

Engineering Gate has determined we are not ready to launch tomorrow. The
launch is deferred to <new date OR "TBD until <specific blocker> is
resolved">.

Why: <one sentence factual blocker>.

What now:
- Press kit recall: PR firm is contacting embargo'd journalists.
- Lighthouse customers: CS is sending a defer note.
- Internal: no public statements until <CEO> messages the team.

This is not failure. It is the engineering gate working. We will launch
when we are ready.

— <CEO name>
```

### F.2 Lighthouse customer note

```
Subject: [CoreLink] Launch timing update

Hi <name>,

We're rescheduling the CoreLink GA launch from <original date> to <new
date OR "TBD">. <One factual sentence: engineering judgement / pre-flight
finding / etc.>

Your case study and account access are unaffected. Your lighthouse SLA
remains in effect.

We'll send a final confirmation 48h before the new launch date.

Thank you for your patience.

— <CEO name>
```

### F.3 Public-facing (only if rumors begin)

> Hold; do not pre-announce a defer. Public announcement of a new date goes out when the new date is confirmed by Engineering Gate.

### F.4 Journalist recall (PR firm)

PR firm sends embargo-recall to all journalists; offer to re-embargo for new date once known.

---

## Hard rules (all scenarios)

1. **Do not fabricate.** Say "we are investigating" before saying anything specific.
2. **Customer email before tweet.** Affected customers learn from us, not from Twitter.
3. **CEO signs off on customer-facing comms** — except SEV1 auto-publish.
4. **Privacy incidents = 72-hour clock.** Document the awareness timestamp immediately.
5. **No vendor blame** until Legal-cleared.
6. **Blameless retros.** Per `specs/_runbooks/RB-POSTMORTEM-PROCESS.md`.
7. **Single source of truth for incident status: `hugrl.betteruptime.com`.** All other channels (tweets, customer emails, press) point back to it.

---

## Cross-references

- `LAUNCH-CHECKLIST-V2.md` — operationally invokes these templates at L1 (F), L11 (F), L28-L30 (A/B), L36 (D-like abuse).
- `STATUS-PAGE-SPEC.md` — §A.1 is the auto-publish template referenced in `STATUS-PAGE-SPEC.md` §5.
- `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md` — operational overlap for Scenarios B and E.
- `specs/_runbooks/RB-SECURITY-VULNERABILITY-INTAKE.md` — overlapping intake protocol.
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — blameless retro template.
- `SECURITY.md`, `apps/docs/static/.well-known/security.txt` — public disclosure policy.
- `specs/_security/vulnerability-disclosure-policy.md` — canonical spec.
