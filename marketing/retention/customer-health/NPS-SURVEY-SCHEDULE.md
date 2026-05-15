---
id: "RETENTION-NPS-SURVEY-SCHEDULE"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "VPMkt + CS Lead (dual)"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "ROADMAP-TO-GA.md §R-prep (post-GA retention)"
tags:
  - "marketing"
  - "retention"
  - "nps"
  - "survey"
  - "schedule"
  - "customer-health"
  - "post-ga"
  - "wt-r-prep-customer-health"
---

# NPS Survey Schedule — When, How, What

> **Purpose:** define exactly **when** CoreLink sends NPS / satisfaction surveys to which tenants, **what** the survey asks, and **how** the response feeds `HEALTH-SCORE-METHODOLOGY.md` §3.6 and `CSM-PLAYBOOK.md` §4 outreach triggers.
> **Audience:** CS Lead (owner), VPMkt (comms approval), Eng (transactional-email + in-app prompt implementer), Founder (review).
> **Companion:** `HEALTH-SCORE-METHODOLOGY.md` consumes the responses; `CSM-PLAYBOOK.md` §4 acts on detractor / promoter outcomes.
> **Hard rules:**
> 1. **Survey fatigue cap** — no tenant principal receives **more than 1 NPS survey per 30 days**, regardless of trigger overlaps. The next-eligible trigger wins; later ones are suppressed and logged.
> 2. **Consent** — surveys are transactional emails permitted under CASL/CAN-SPAM/LGPD (legitimate-interest basis for product-quality measurement) but every survey email has a one-click unsubscribe that suppresses all future NPS for that principal. In-app prompts respect a dashboard-level "Don't ask again for 90 days" cookie.
> 3. **Privacy** — NPS responses are stored against `tenant_id` AND `principal_id` (so a CSM can see which person on a team is the detractor). Aggregate-only reporting outside CS.
> 4. **No incentives** — we do not pay for NPS responses. (Incentives bias the scale up and corrupt the trend over time.)

---

## 1. Five trigger points (canonical)

| # | Trigger | Window | Audience | Survey type | Channel | Owner |
|---|---|---|---|---|---|---|
| 1 | **Day 7 post-signup** | Exactly 7 calendar days after `tenant.created_at` | All paying tenants; lighthouse Day 7 from `Engaged` state | **Onboarding satisfaction** (3 questions, no NPS scale) | Email primary + in-app prompt fallback | CS Lead |
| 2 | **Day 30 post-first-pay** | 30 days after the first **successful** invoice payment | All paying tenants (skipped for lighthouse free period) | **Value-realisation NPS** (NPS 0–10 + 1 free-text) | Email | CS Lead |
| 3 | **Quarterly standard NPS** | Every 90 days, anchored to tenant's `next_quarterly_nps_at` (set on first qualifying eligibility) | All paying tenants > 90d old | **Standard NPS** (NPS 0–10 + 1 follow-up) | Email + in-app prompt | CS Lead |
| 4 | **Post-incident (SEV1+)** | 5 business days after the incident is closed (post-mortem published) | Tenants impacted by the SEV1+ per the incident's `impact.tenants_affected` list | **Incident-recovery NPS** (NPS 0–10 + 2 questions) | Email only (in-app would be tone-deaf) | VPMkt (comms approval) + CS Lead (send) |
| 5 | **Pre-renewal** | 30 days before annual contract expiry; OR 14 days before month-to-month auto-renew anniversary for tenants who have explicitly opted into renewal surveys | Annual-contract tenants (mandatory); MTM tenants (opt-in) | **Renewal-likelihood NPS** (NPS 0–10 + 1 question on what would change response) | Email + CSM call follow-up if score ≤ 6 | CS Lead |

**Conflict resolution.** If two triggers fall within the same 30-day window for the same tenant, priority order is:

```
4 (post-incident) > 5 (pre-renewal) > 2 (D+30 value) > 3 (quarterly) > 1 (D+7 onboarding)
```

The higher-priority trigger sends; the lower is **suppressed** (not delayed). A suppressed trigger is logged in `cs.nps_send_audit` with reason `suppressed_by:<higher_trigger>`.

---

## 2. Trigger 1 — Day 7 post-signup (Onboarding Satisfaction)

**Goal:** detect onboarding friction early enough to fix the relationship. **Not** an NPS-scale survey — NPS is too coarse for first-week sentiment.

**3 questions.** Sent 7 calendar days after `tenant.created_at` at 14:00 in the tenant's primary timezone (fall back to UTC if unknown).

### 2.1 Email body (canonical)

```
Subject: How's CoreLink working out so far?

Hi {first_name},

You signed up for CoreLink a week ago. Quick 60-second check — three questions:

1. Did you get a first successful cache PUT and GET working?
   [Yes — same day]   [Yes — within the week]   [No, still working on it]   [No, gave up]

2. How clear was the onboarding documentation?
   [Very clear]   [Mostly clear]   [Confusing in places]   [Lost in the docs]

3. What's the single biggest thing we could fix right now?
   [free text, 500 char max]

Reply to this email or click here: {survey_link}

Thanks — we read every response.

— {csm_name}, CoreLink Customer Success
```

### 2.2 In-app prompt (fallback if email unopened after 48h)

Compact modal on dashboard login, dismissible. Same 3 questions. Shown at most once; if dismissed, recorded as `declined` (not retried).

### 2.3 Response handling

| Answer pattern | Action |
|---|---|
| Q1 = "No, gave up" | Same-day CSM outreach (CSM-PLAYBOOK.md §4.1) |
| Q1 = "No, still working on it" + Q2 = "Lost in the docs" | CSM outreach within 3 business days |
| Q1 = "Yes — same day" + Q2 = "Very clear" | No outreach; log as positive signal; consider for case-study pipeline at D+90 |
| Q3 contains "billing" / "price" | Auto-tag for VPMkt review weekly |
| No response (email + in-app declined or ignored) | Logged; no escalation; D+30 trigger still runs |

Q1 and Q2 answers do **not** feed the health score directly (they're qualitative). They feed §3.2 dashboards as a leading indicator.

---

## 3. Trigger 2 — Day 30 post-first-pay (Value-Realisation NPS)

**Goal:** capture NPS at the moment the customer has actually paid for and used the product for a month. Highest-quality NPS signal because it ties to a real payment event.

**Eligibility.** First **successful** invoice payment (not signup; not free trial conversion to paid). Skipped for lighthouse customers until they convert to paid post-attestation.

### 3.1 Email body (canonical)

```
Subject: A month in — how likely are you to recommend CoreLink?

Hi {first_name},

You've been a CoreLink customer for a full billing cycle. Quick question:

How likely are you to recommend CoreLink to a colleague or peer team?

  0  1  2  3  4  5  6  7  8  9  10
  (not at all likely)         (extremely likely)

{survey_link_with_score_prefilled_per_button}

One follow-up: what's the most important reason for your score?
[free text, 500 char max]

Thanks — your answer is read by our CS Lead within 2 business days.

— CoreLink Customer Success
```

### 3.2 Response handling

| NPS bucket | Score | Action (CSM-PLAYBOOK.md §4.2) |
|---|---|---|
| Detractor | 0–6 | CSM outreach within 2 business days. Investigate root cause. |
| Passive | 7–8 | CSM outreach within 7 business days. Ask "what would make this a 9?" |
| Promoter | 9–10 | CSM thank-you within 5 business days. Flag for: referral ask, case-study candidate, reference-call candidate. |

Response feeds `HEALTH-SCORE-METHODOLOGY.md` §3.6 (N input) for the next 90 days.

---

## 4. Trigger 3 — Quarterly standard NPS

**Goal:** trend NPS over time per tenant. Stable cadence for benchmark comparability.

**Anchor.** On tenant's 91st day post-signup (or post-first-pay if later), set `next_quarterly_nps_at = today + 90d`. Each subsequent send rolls the anchor forward 90d. Skipped if any other trigger fired in the same 30-day window (see §1 conflict resolution).

### 4.1 Email body (canonical)

```
Subject: Your quarterly check-in — 60 seconds

Hi {first_name},

Quick quarterly NPS — how likely are you to recommend CoreLink today?

  0  1  2  3  4  5  6  7  8  9  10

{survey_link_with_score_prefilled_per_button}

If you'd like to add one sentence on why, we read every response:
[free text, 500 char max]

— CoreLink Customer Success
```

### 4.2 In-app prompt

Shown on dashboard login if email unopened after 5 days. Same single NPS scale + optional free text. Dismissible — sets 90d cookie on `dismiss`.

### 4.3 Trending

We compute and publish (internal-only by default):

- Per-tenant NPS trend (last 4 quarters minimum).
- Cohort NPS (signup-month cohorts).
- Aggregate NPS (rolling 90d).

Trend reversals (drop ≥ 3 points QoQ) trigger CSM outreach per `CSM-PLAYBOOK.md` §4.3.

---

## 5. Trigger 4 — Post-incident (SEV1+)

**Goal:** measure recovery — did our incident response and comms preserve the relationship, or did it break it?

**Eligibility.** Any incident classified as **SEV1 or SEV2** per `specs/_runbooks/RB-ONCALL-POLICY.md` severity matrix that has a non-empty `impact.tenants_affected` list AND a published post-mortem. Sent **5 business days after post-mortem publication**, never sooner.

**Why 5 business days?** Too soon = raw frustration response that conflates incident severity with our response. Too late = sentiment moves on. 5 business days is the empirical sweet spot from launch-era data (see `RB-CUSTOMER-SUPPORT-T-90.md` §10 — initially expert-set; will be tuned by data).

### 5.1 Email body (canonical)

```
Subject: Following up on the {incident_id} outage — quick question

Hi {first_name},

You were affected by the {incident_short_name} incident on {incident_date}.
Our post-mortem is at {postmortem_url}.

Two questions to help us improve:

1. How likely are you to recommend CoreLink today?
   0  1  2  3  4  5  6  7  8  9  10
   {survey_link_with_score_prefilled_per_button}

2. What — if anything — would have made our response better?
   [free text, 1000 char max]

Your honest answer matters more than a polite one. Reply directly to me
if you'd rather talk one-to-one.

— {founder_or_csm_lead_name}
   CoreLink
```

**Notable elements:**
- Signed by **Founder or CSM Lead by name** (not generic CS) — incident-recovery NPS is high-stakes.
- Direct-reply path **must** route to a human within 4 business hours during business hours.
- No in-app prompt fallback — too tone-deaf to ask mid-dashboard about a recent outage.
- Free-text limit is 1000 char (not 500) — people who write incident NPS write more.

### 5.2 Response handling

| NPS | Action |
|---|---|
| 0–4 | **Same-day** Founder reach-out (call or email). Root-cause must be captured in CRM. |
| 5–6 | CSM Lead reach-out within 1 business day. |
| 7–8 | CSM thank-you within 5 business days. Capture free-text for retro. |
| 9–10 | (Rare post-incident.) CSM acknowledgement. Flag as exceptional-customer for retention. |

A SEV1+ incident NPS ≤ 4 from **two or more tenants** triggers a launch-readiness retro per `marketing/launch/POST-MORTEM-RUNBOOK.md` (if it exists post-GA).

---

## 6. Trigger 5 — Pre-renewal (Renewal-Likelihood NPS)

**Goal:** detect renewal risk early enough to act. NPS at this point is more predictive than at any other moment.

**Eligibility.**
- **Annual contract tenants:** mandatory, 30 days before contract expiry.
- **Month-to-month tenants:** opt-in only (set at signup via `renewal_survey_opt_in: true`). 14 days before each annual anniversary, not monthly.

### 6.1 Email body (canonical)

```
Subject: Your CoreLink renewal is coming up — one question

Hi {first_name},

Your CoreLink contract renews on {renewal_date} ({days_until_renewal} days
away). Before then, one question:

How likely are you to renew CoreLink as it stands today?

  0  1  2  3  4  5  6  7  8  9  10

{survey_link_with_score_prefilled_per_button}

If your score is below 9, what would change your answer?
[free text, 1000 char max]

Whatever you say here goes straight to {csm_name} (your CSM) and the
CoreLink founder. We'd rather know now than be surprised at renewal.

— {csm_name}, CoreLink Customer Success
```

### 6.2 Response handling

| NPS | Action |
|---|---|
| 0–4 | **Immediate** (same-day) CSM Lead + Founder call request. Renewal-loss is now `Critical`-tier per CSM-PLAYBOOK.md §3.3. |
| 5–6 | CSM call within 3 business days. Renewal-loss now `At-Risk`-tier. |
| 7–8 | CSM check-in within 7 business days. "What would make this a 9?" |
| 9–10 | CSM thank-you within 7 business days. Pursue: upsell-eligibility review, multi-year discount offer (per pricing policy). |

A pre-renewal NPS ≤ 6 forces the tenant's health tier to `At-Risk` (overriding the computed tier from `HEALTH-SCORE-METHODOLOGY.md`) for the remaining days until renewal. Documented in `HEALTH-SCORE-METHODOLOGY.md` §5 future addendum.

---

## 7. In-app prompt design

Used as fallback for triggers 1 and 3 only (never 4, 5, or 2 — those are too important to risk dashboard-context bias).

### 7.1 Visual spec

- Top-of-dashboard banner (not modal blocking) **OR** corner toast (configurable).
- 240 × 96 px corner toast OR full-width 64px banner.
- Brand colour primary; large dismiss `×`.
- Renders **only on the user's first 3 dashboard sessions** after the email-send. After 3 dismisses or 14 days, suppress permanently for that survey instance.

### 7.2 In-app content (Trigger 1 example)

```
[CoreLink-logo]  How's your first week going?
                 3 quick questions →  [Open]  [Later]  [×]
```

Clicking `[Open]` opens a side-panel with the 3 questions inline. `[Later]` snoozes 24h. `[×]` dismisses for 14 days.

### 7.3 Accessibility

- WCAG 2.2 AA compliant.
- Keyboard-navigable; ESC dismisses.
- Screen-reader announces "Optional survey from CoreLink, 3 questions".

---

## 8. Storage, retention, reporting

| Table | Columns | Retention |
|---|---|---|
| `cs.nps_send_audit` | `(send_id, tenant_id, principal_id, trigger_type, sent_at, channel, suppressed, suppression_reason)` | 5 years |
| `cs.nps_response` | `(send_id, tenant_id, principal_id, score, free_text, responded_at, channel)` | 5 years; free-text purged at 18 months unless customer is still active |
| `cs.nps_unsubscribe` | `(principal_id, unsubscribed_at, scope)` | Indefinite (legal basis: consent withdrawal record) |

**Reporting outputs:**

- **Daily** `cs.nps_dashboard` view: rolling 90d aggregate NPS, by trigger type, by tenant tier (Healthy/At-Risk/Critical), by signup cohort.
- **Weekly** Founder email: NPS this week, all detractors with free-text quoted (anonymised where unsubscribed), trend vs last week.
- **Quarterly** board report: aggregate NPS trend, detractor → churn correlation, promoter → upsell correlation.

---

## 9. Quality gates

| Gate | Rule | Owner |
|---|---|---|
| Send quality | < 5% bounce rate per send; > 95% deliverability | CS Lead |
| Response rate | ≥ 20% for triggers 1, 2, 4, 5; ≥ 15% for trigger 3 | CS Lead |
| Acknowledgement SLA | Every detractor reach-out happens within the SLA listed above; missed acks trigger CS Lead Slack alert | CSM Lead |
| Free-text triage | 100% of free-text responses read by a human within 5 business days; categorised into pre-defined themes | CS Lead |

If any gate misses for **2 consecutive weeks**, trigger a CS retro (CSM-PLAYBOOK.md §10).

---

## 10. Suppression and unsubscribe

- One-click unsubscribe in every email footer.
- Unsubscribe scope is per-principal, all-triggers (we don't offer "unsubscribe trigger 3 only" — too complex and rarely wanted).
- Unsubscribed principals are still in `nps_send_audit` (with `suppressed = true`, reason `unsubscribed`) for compliance.
- Tenant-level unsubscribe (entire org opts out of NPS): supported via Admin → Settings → "NPS opt-out". Defaults: opted-in.
- Unsubscribed principals cannot be re-subscribed except by their own action (self-service in dashboard `Settings → Communications`).

---

## 11. Cross-references

- **`HEALTH-SCORE-METHODOLOGY.md`** §3.6 — consumes most-recent NPS response as input `N`.
- **`CSM-PLAYBOOK.md`** §4 — response-handling playbook by NPS bucket.
- **`HEALTH-DASHBOARD-SPEC.md`** §"NPS trend" panel — visualises trends.
- **`CHURN-RISK-SIGNALS.md`** (sibling worktree): low NPS is one of multiple churn signals; cross-tracked.
- **`specs/_runbooks/RB-CUSTOMER-SUPPORT-T-90.md`** §10 — incident post-mortem mention loops here for §5 timing.
- **`marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`** §Comms protocol — lighthouse customers have additional named-CS comms; NPS schedule still applies but trigger 4 (post-incident) is via Slack Connect + email (not email only).
- **`ROADMAP-TO-GA.md`** §R-prep — post-GA retention preparation.

---

**Fim NPS-SURVEY-SCHEDULE.**
