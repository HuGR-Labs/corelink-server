---
id: "RETENTION-CSM-PLAYBOOK"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "CS Lead + Founder (dual)"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "ROADMAP-TO-GA.md §R-prep (post-GA retention)"
tags:
  - "marketing"
  - "retention"
  - "csm"
  - "playbook"
  - "customer-success"
  - "post-ga"
  - "hiring"
  - "wt-r-prep-customer-health"
---

# Customer Success Manager (CSM) Playbook

> **Purpose:** the operational playbook for the **CoreLink CSM role** — applies the moment the first CSM is hired (target post-GA, T+60 .. T+120). Covers: account assignment rules, weekly check-in cadence, escalation matrix, role success metrics, and the seven canonical playbooks keyed to tier transitions + NPS responses.
> **Audience:** the CSM(s) once hired (primary user); CS Lead (manager); Founder (escalation level 3); Support Lead (handoff peer); VPMkt (case-study / referral pipeline peer).
> **Companions:**
> - `HEALTH-SCORE-METHODOLOGY.md` — what the score means.
> - `HEALTH-DASHBOARD-SPEC.md` — the screen you live in.
> - `NPS-SURVEY-SCHEDULE.md` — what surveys to expect responses from.
> - `specs/_runbooks/RB-CUSTOMER-SUPPORT-T-90.md` — support-side counterpart; you and Support Lead share Tier-2 escalations.
> - `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` — lighthouse customers have a parallel comms protocol; this playbook applies in steady-state, the lighthouse playbook applies during their 60-day engagement.
> **Hard rules:**
> 1. **Every paying tenant has exactly one assigned CSM.** No tenants left orphaned, no shared accountability.
> 2. **Action-queue follow-through ≥ 80%** (per `HEALTH-DASHBOARD-SPEC.md` §14). This is the CSM's primary KPI.
> 3. **No silent churn.** If a tenant churns, the CSM must have logged ≥ 1 documented intervention in the 60 days prior. If they haven't, that's a CSM-process failure, not a tenant failure.
> 4. **Honest > polite.** Detractor responses, renewal risks, and bad-news comms come from CSM directly, not from canned templates.

---

## 1. Role definition

### 1.1 What the CSM does

- **Owns the relationship** for assigned tenants from first paid invoice through renewal and beyond.
- **Acts on the action queue daily** (per `HEALTH-DASHBOARD-SPEC.md` §3).
- **Runs weekly check-ins** per §3 cadence below.
- **Triages NPS responses** per `NPS-SURVEY-SCHEDULE.md` SLAs.
- **Escalates** to CS Lead / Founder per §6 matrix.
- **Logs every touch** in CRM (`cs.tenant_touches`); without logs, the dashboard math breaks and the role isn't measurable.
- **Feeds product** — every recurring customer complaint becomes a one-line entry in `feedback.product_input` weekly digest to Eng PM.

### 1.2 What the CSM does NOT do

- **Tier-1 support** — that's Support T1 per `RB-CUSTOMER-SUPPORT-T-90.md` §3. CSM picks up escalations only.
- **Pricing negotiation > 10% discount** — escalates to Founder.
- **Contract redlines** — Legal (currently external counsel; Founder orchestrates).
- **Security questionnaire responses** — VPSec.
- **Incident comms during P0** — Incident Commander owns; CSM may relay to their tenants only after IC approves.

### 1.3 First CSM hire profile

(For Founder + CS Lead use when recruiting.)

- 3+ years CSM at a B2B infra / dev-tools / platform-engineering company.
- Comfortable reading dashboards and writing one-line SQL for ad-hoc queries.
- Has handled detractor → recovery → promoter conversion at least once.
- Comfortable writing direct comms (no "circle back" language).
- Bonus: experience with content-addressable storage / build systems / supply-chain security.

---

## 2. Account assignment rules

### 2.1 Assignment triggers

A tenant gets assigned to a CSM on the **first of**:

- **First successful paid invoice** — paying tenant → assigned CSM.
- **Conversion from lighthouse to paid** — re-assigned from lighthouse-engineer to a steady-state CSM (or kept with same human if available).
- **Tier change to At-Risk or Critical** while previously unassigned (e.g., free trial mid-conversion) — assigned same business day.

### 2.2 Assignment algorithm (when ≥ 2 CSMs)

Priority order (apply each rule top-down until tie-broken):

1. **Continuity rule** — if a CSM has prior relationship with anyone on the tenant's roster (e.g., the eng lead worked at a former CSM-managed account), assign that CSM.
2. **Geography rule** — tenant timezone primary overlap with CSM working hours (>= 4h overlap).
3. **Language rule** — tenant primary language coverage (pt-BR / en-US / es-419 / es-ES). CSM must cover the language; native preferred, fluent acceptable.
4. **Workload balance** — assign to the CSM with the fewest **weighted** active tenants. Weighting: Enterprise BYOK = 2.0, Team = 1.0, free-trial = 0.3.
5. **Tie-break** — round-robin by CSM hire date.

### 2.3 Re-assignment

- **Routine re-balance** quarterly by CS Lead.
- **On-demand** when a CSM goes on leave > 5 business days (CS Lead covers; reassignment if leave > 4 weeks).
- **Customer request** — if a tenant requests a different CSM, CS Lead reviews; default is to grant.
- **CSM departure** — re-assigned within 5 business days; outgoing CSM does explicit warm handoffs to all > 60 in score (Healthy) and same-week handoffs to At-Risk / Critical.

### 2.4 Account caps

| CSM seniority | Max weighted tenants |
|---|---:|
| New CSM (< 90d tenure) | 20 |
| Established CSM (≥ 90d tenure) | 40 |
| Senior CSM (> 365d tenure or prior CSM role) | 60 |
| CS Lead (player-coach during first 6 months of CS team) | 80 cap; preferred 50 to leave bandwidth for management |

If a CSM exceeds their cap by > 10% for > 30 days, that triggers a hire conversation with Founder.

---

## 3. Weekly check-in cadence

### 3.1 Healthy tier (score 70–100)

| Tenant type | Cadence | Format | Duration |
|---|---|---|---|
| Enterprise BYOK | Monthly | Video call | 30 min |
| Team — high activity | Quarterly | Async update (email + dashboard share) + opt-in call | 0–20 min |
| Team — low activity | Quarterly | Async update only | 0 (email) |

**Healthy ≠ ignored.** Even Healthy tenants get a quarterly nudge + dashboard share. The cadence is light because the tenant is fine; it is not zero.

### 3.2 At-Risk tier (score 40–69)

| Tenant type | Cadence | Format | Duration |
|---|---|---|---|
| All | Every 2 weeks (minimum) until tier exit | Video call OR substantive email + Loom | 30 min |
| If declining 7d in a row | Weekly | Video call required | 30 min |

**Within 7 days** of tier entry: CSM outreach is mandatory (per `HEALTH-SCORE-METHODOLOGY.md` §5). Outreach is a **diagnostic conversation** — what's friction? What changed? What would help?

### 3.3 Critical tier (score 0–39)

| Step | Timing |
|---|---|
| CSM internal triage with CS Lead | Within 24h of tier entry |
| First Founder loop-in (email) | Within 48h |
| Joint CSM + CS Lead + Founder call to tenant | Offered within 5 business days |
| Weekly review until exit | Every Monday with CS Lead until tier change |
| Recovery plan documented | Within 5 business days, stored in `cs.recovery_plans` table |

Recovery plan template:

```
Tenant: {tenant_id}
Tier entry date: {date}
Score on entry: {score}
Driving inputs (top 2): {input1, input2}
Likely root cause: {1-sentence}
30-day target: exit Critical (score ≥ 40 stable for 3 days)
Owner: {csm_name}
Founder loop-in: {date_of_first_email}

Actions:
  1. {action with owner + due date}
  2. ...

Check-in cadence: weekly Monday with CS Lead until exit.

Date filed: {today}
Approval (CS Lead signature): {name + date}
```

### 3.4 New tenants (first 90 days)

Onboarding-specific cadence overrides tier-based cadence:

| Day | Touch |
|---|---|
| D+0 | Welcome email from assigned CSM, signed by name |
| D+7 | Onboarding satisfaction survey (Trigger 1 per NPS-SURVEY-SCHEDULE.md §2) — CSM reviews response |
| D+14 | 30-min "are you getting value?" call (Enterprise) OR email check-in (Team) |
| D+30 | NPS value-realisation (Trigger 2) — CSM reviews + acts |
| D+60 | Quarterly cadence kicks in based on tier |

---

## 4. NPS response playbooks

(Cross-reference: `NPS-SURVEY-SCHEDULE.md` §3–6.)

### 4.1 Trigger 1 — Day 7 onboarding survey

| Response | CSM action | SLA |
|---|---|---|
| Q1 = "No, gave up" | Same-day call request; investigate root cause; offer to walk-through CLI live | Same business day |
| Q1 = "No, still working" + Q2 = "Lost in docs" | Email + Loom with the specific docs walkthrough; offer call | 3 business days |
| Q1 = "Yes — same day" + Q2 = "Very clear" | Thank-you note; flag for D+90 case-study consideration | 7 business days |
| Q3 = "billing concern" | Acknowledge + route to billing@; CSM follows up after billing resolves | 3 business days |
| Q3 = "performance concern" | Engineering-routed; CSM owns relationship; eng owns diagnosis | 5 business days |
| Q3 = "compete with X" (mention of competitor) | CSM logs competitor mention; founder-loop weekly digest | 7 business days |

### 4.2 Trigger 2 — Day 30 value-realisation NPS

| NPS | CSM action | SLA |
|---|---|---|
| 0–4 (deep detractor) | **Call request** with subject "30-day check-in"; honest diagnostic; offer escalation to Founder if relationship is at risk | 1 business day |
| 5–6 (mild detractor) | Email-first; offer call; ask "what would change this?" | 2 business days |
| 7–8 (passive) | Email; "what would make this a 9?" question; collect input for product | 7 business days |
| 9–10 (promoter) | Thank-you + 3 asks: (1) would you do a 30-min reference call for prospects? (2) refer a peer? (3) case-study candidate? | 5 business days |

### 4.3 Trigger 3 — Quarterly NPS

Same response brackets as 4.2, but **also** check for QoQ drop. If NPS dropped ≥ 3 points QoQ:

- **Trend-reversal protocol:** Even if absolute NPS is still in passive/promoter range, a sharp drop triggers a CSM call within 7 business days. Free-text response is the first place to look.

### 4.4 Trigger 4 — Post-incident NPS

| NPS | CSM action | SLA |
|---|---|---|
| 0–4 | **Founder reach-out same day** (CSM may join). Tenant is Critical regardless of computed score for the next 30 days. | Same business day |
| 5–6 | CSM Lead reach-out within 1 business day; honest conversation about whether our incident response missed | 1 business day |
| 7–8 | CSM thank-you; capture free-text for the incident retro | 5 business days |
| 9–10 (rare) | Personalised thank-you signed by Founder | 7 business days |

### 4.5 Trigger 5 — Pre-renewal NPS

| NPS | CSM action | SLA |
|---|---|---|
| 0–4 | **Same-day** call request, CSM + CS Lead + Founder. Treat as Critical for renewal. | Same business day |
| 5–6 | CSM Lead call within 3 business days. Discuss what would change the answer. | 3 business days |
| 7–8 | CSM check-in within 7 days. "What would make this a 9?" | 7 business days |
| 9–10 | Thank-you + upsell exploration (multi-year discount per pricing policy, if applicable) | 7 business days |

---

## 5. Tier-transition playbooks

(Cross-reference: `HEALTH-SCORE-METHODOLOGY.md` §5.)

### 5.1 Healthy → At-Risk

1. Within 7 days: CSM email + offer call. Subject: "Quick check — anything I can help with?"
2. CSM reviews the per-tenant drilldown (`HEALTH-DASHBOARD-SPEC.md` §8) and identifies the top 2 driving inputs.
3. Initial hypothesis written in CRM (≤ 3 sentences) before the call.
4. After call: documented action items, owner, due date.
5. Cadence shifts to every 2 weeks per §3.2.

### 5.2 At-Risk → Critical

1. Same business day: CSM Slack-pings CS Lead.
2. Within 24h: triage call CSM + CS Lead.
3. Within 48h: Founder email loop-in. Template in §7.
4. Within 5 business days: recovery plan filed per §3.3.
5. Weekly Monday review until exit.

### 5.3 Healthy → Critical (skip tier, anti-flap rule fired)

Treat as 5.2 but accelerate: Founder loop-in within 24h, joint call within 48h. This pattern usually indicates a catastrophic event (cancelled contract, key team departure, major incident impact).

### 5.4 At-Risk → Healthy (recovery)

1. Document the recovery in CRM (what worked).
2. Quarterly cadence resumes.
3. Consider for promoter pipeline if NPS supports.
4. **No celebration emails to the customer.** Recovery is the floor we promised; not a milestone.

### 5.5 Critical → At-Risk (partial recovery)

1. Recovery plan stays open until full exit to Healthy.
2. Bi-weekly check-ins continue.
3. Founder updated weekly in the §7 weekly digest.

### 5.6 Active churn (any tier → churned)

1. Exit interview offered (30 min, optional). Whether they accept or not, CSM completes a **churn post-mortem** in `cs.churn_postmortems`:
   - Last 90 days of health score
   - Last 90 days of touches
   - Stated reason from exit interview (if conducted)
   - CSM's hypothesis on real reason (may differ from stated)
   - One product / process change that might have prevented it
2. Churn post-mortems are reviewed monthly by CS Lead + Founder. Patterns feed `feedback.product_input`.

---

## 6. Escalation matrix

| Level | Owner | When to engage | How |
|---|---|---|---|
| L0 | CSM (self) | Routine | — |
| L1 | CS Lead | Tier transition to Critical; recurring pattern across ≥ 2 tenants; CSM stuck > 24h; customer-requested CSM change | Slack `@cs-lead` or email |
| L2 | Founder (Gustavo) | Critical tier > 7 days; renewal at risk; customer requests Founder conversation; any allegation of misconduct from a tenant | Email gustavo@humangr.com or Slack `@gustavo` |
| L3 | Founder + Legal | Contract dispute; data dispute; allegation of breach; threat of public-facing complaint | Founder forwards to external counsel |
| Engineering on-call | Eng on-call | CSM-witnessed product bug needing same-day eng attention | PagerDuty page following `RB-CUSTOMER-SUPPORT-T-90.md` §6 ticket→incident conversion criteria |
| Support T1/T2 | Support Lead | CSM-handed ticket needing T1/T2 work | Slack `@support-shift-lead` |
| VPSec | VPSec | Security concern, BYOK question CSM can't answer, security questionnaire request from customer | Email security@corelink.dev |

**Escalation timing rule:** L0 → L1 within 24h of being stuck. L1 → L2 within 24h if L1 can't move it. **Never sit on a stuck ticket for the weekend.**

---

## 7. CSM weekly digest (to Founder)

Every Friday by EOB the CSM (or each CSM if > 1) sends to Founder + CS Lead:

```
Subject: CSM weekly — {csm_name} — Week of {date}

Portfolio: {N} active tenants ({Healthy}/{At-Risk}/{Critical})

This week:
  - Tier transitions: {list with names + directions}
  - New action-queue rows acted on: {count}
  - Action-queue rows untouched > 7d: {count} (target: 0)
  - NPS responses received: {N} ({detractors}/{passives}/{promoters})
  - Detractor outreach completed within SLA: {Y}/{Y_required}

Top concerns this week (max 3):
  1. {tenant_id}: {one-line concern + next step}
  2. ...

Wins this week (max 3):
  1. {tenant_id}: {recovery / promoter signal / etc.}
  2. ...

Help I need from you:
  - {founder action item, if any}

CRM hygiene:
  - All touches logged: yes/no
  - Recovery plans current: yes/no
```

Founder reads within 24h; replies only if action is needed.

---

## 8. CSM tooling

### 8.1 Required

- **Health Dashboard** (`HEALTH-DASHBOARD-SPEC.md`) — primary screen.
- **CRM** — TBD vendor at GA-time; placeholder columns: `(tenant_id, principal_id, touch_at, touch_type, summary, csm)`. CSMs MUST log every touch within 24h.
- **Slack Connect channels** — one per Enterprise BYOK tenant, opt-in for Team tenants.
- **Email** — gmail-equivalent with shared `cs@corelink.dev` inbox for unassigned + handoff coverage.
- **Loom or equivalent** — for async video updates (Healthy tier quarterly check-ins).

### 8.2 Optional but recommended

- **Calendly** for tenant-facing scheduling.
- **Notion or equivalent** for personal note-taking before CRM-entry.

### 8.3 Forbidden tools

- **DM-only customer comms** — every customer touch must surface to the CRM. Slack DMs to a customer principal that aren't copied to the Slack Connect channel and logged in CRM = forbidden.
- **Personal email** — customer comms come from `firstname@corelink.dev` only.

---

## 9. CSM success metrics (the role's KPIs)

| Metric | Target | Source | Cadence |
|---|---|---|---|
| Net Retention Rate (NRR) | ≥ 110% on assigned portfolio | Billing + churn | Monthly |
| Gross Retention Rate (GRR) | ≥ 95% on assigned portfolio | Billing + churn | Monthly |
| Action-queue follow-through | ≥ 80% of action-queue rows touched within 48h | `HEALTH-DASHBOARD-SPEC.md` §14 | Monthly |
| Critical-tier exit time | Median ≤ 30 days from Critical entry to At-Risk or Healthy | `cs.health_score_audit` tier history | Quarterly |
| Detractor → promoter conversion | ≥ 1 detractor → promoter conversion per quarter on assigned portfolio | NPS responses | Quarterly |
| NPS response SLA compliance | 100% of NPS responses acknowledged within trigger-specific SLA | `NPS-SURVEY-SCHEDULE.md` §9 + CRM | Monthly |
| CRM hygiene | ≥ 95% of touches logged within 24h | Audit | Weekly (random 10% sample) |
| Churn post-mortem completion | 100% of churned tenants have a post-mortem within 14 days of churn | `cs.churn_postmortems` | Per-event |

**Performance review cadence:** CS Lead reviews monthly informally, formally quarterly. Bonus tied to NRR + GRR + action-queue follow-through (60/20/20 split) per Founder compensation policy (TBD post-GA).

---

## 10. Retros and continuous improvement

### 10.1 Weekly CS retro

Every Friday, 30 min, CS Lead + all CSMs. Standing agenda:

1. Action-queue follow-through review (5 min).
2. Tier transitions of the week (10 min, focus on Critical).
3. NPS response trends (5 min).
4. Recurring product issues (5 min) — output to `feedback.product_input`.
5. One process change to try next week (5 min).

### 10.2 Quarterly CS retro

90 min, CS Lead + all CSMs + Founder. Reviews:

- All KPIs (§9) trend over the quarter.
- All churn post-mortems.
- All Critical tier entries + exits.
- Re-baseline of `HEALTH-SCORE-METHODOLOGY.md` (per §8 of that doc).
- Hiring conversation if CSM caps approaching breach.

### 10.3 Annual CS retro

Half-day. Above + comp review + role progression + Founder strategic input.

---

## 11. Transparency-on-request to customers

(Cross-reference: `HEALTH-SCORE-METHODOLOGY.md` §7.)

A tenant may request to see their own health score, the inputs, and the score history. Default disclosure rules:

- **Lighthouse customers:** disclosed automatically each quarter via the lighthouse CS engineer (per `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` §Phase 2 weekly check-in).
- **Enterprise BYOK paid customers:** disclosed on request, no questions asked.
- **Team paid customers:** disclosed on request; CSM explains the methodology.
- **Free trial / suspended:** not disclosed — score is internal-only during these states.

When disclosing:
- Show the score, the tier, the 6 inputs (normalised), and the weights.
- Do NOT show comparative ranking against other tenants.
- Do show the score history (sparkline, last 90 days).
- Be honest about driving inputs. If their support friction is high because they keep escalating, say so — kindly.

---

## 12. Cross-references

- **`HEALTH-SCORE-METHODOLOGY.md`** — score + tier definitions consumed throughout this playbook.
- **`HEALTH-DASHBOARD-SPEC.md`** — the CSM's primary tool.
- **`NPS-SURVEY-SCHEDULE.md`** — survey triggers + this playbook's §4 response actions.
- **`CHURN-RISK-SIGNALS.md`** (sibling worktree `wt-churn-retention`) — raw-signal catalogue feeding both score and CSM intuition.
- **`marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`** — for tenants currently in their 60-day lighthouse engagement, that playbook supersedes this one until they exit to steady-state.
- **`specs/_runbooks/RB-CUSTOMER-SUPPORT-T-90.md`** — Support Lead is the CSM's peer for ticket escalations; ticket→incident criteria there apply.
- **`marketing/launch/SUPPORT-DASHBOARD-SPEC.md`** — adjacent dashboard.
- **`ROADMAP-TO-GA.md`** §R-prep — post-GA retention package; CSM hire planned during R-prep / post-GA window.

---

**Fim CSM-PLAYBOOK.**
