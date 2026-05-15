---
id: "LIGHTHOUSE-KIT-04-WEEKLY-CHECKIN-AGENDA"
type: "marketing"
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
parent: "WI-S20-004"
tags: ["lighthouse", "marketing", "weekly", "check-in", "agenda", "observation"]
---

# 04 — Weekly Check-in Agenda (30 min)

> **Use:** Recurring weekly call during the D+10..D+40 observation window. Four total per cohort: W1 / W2 / W3 / W4.
> **Cadence:** Same day-of-week and time each week; reuse the calendar series.
> **Attendees:** CoreLink CS engineer (host) + customer engineering / SRE lead + (optional) Engineer S-20 lead if technical topic; (optional) Privacy Officer if DPA-related.
> **Notes:** every meeting's outcome appended to attestation §3.3 in real time.

---

## Time-boxed agenda

| Block | Duration | Owner |
|---|---|---|
| 1. SLO observations from last week | 8 min | CoreLink CS |
| 2. Customer-side updates: blockers + integration changes | 6 min | Customer |
| 3. Next-week activities + drills | 5 min | CoreLink CS |
| 4. Open feedback — feature requests + friction | 7 min | Customer |
| 5. Action items + owners + due dates | 4 min | Both |

Total: 30 min. Hard stop. If a topic needs more than its block, schedule a separate working session.

---

## Block 1 — SLO observations from last week (8 min)

Pre-loaded by CoreLink CS from the per-customer Grafana dashboard (filtered by `customer_id`).

**Standing report:**

| SLO | Target | This-week actual | Trend vs last week | Status |
|---|---|---|---|---|
| Availability — `cas_put` (rolling 7d) | ≥ 99.9% | _99.9X%_ | _↑ / → / ↓_ | _green / yellow / red_ |
| Availability — `cas_get` (rolling 7d) | ≥ 99.9% | _99.9X%_ | _↑ / → / ↓_ | _green / yellow / red_ |
| P99 latency — `cas_get` (rolling 7d) | < 300 ms | _XXX ms_ | _↑ / → / ↓_ | _green / yellow / red_ |
| Cache hit ratio (informational) | n/a | _XX.X%_ | _↑ / → / ↓_ | informational |
| Billing reconciliation drift (rolling 24h) | < 0.1% | _0.0X%_ | _↑ / → / ↓_ | _green / yellow / red_ |
| Incidents this week | 0 expected | _N_ | n/a | _green / yellow / red_ |

**Talking points:**
- Any red / yellow gets discussed in detail. Green can be confirmed in 30 seconds.
- If any SLO went red this week, the breach is flagged for attestation §3.1 and a remediation plan is shared by end of meeting.

**Enterprise BYOK only — additional row:**

| BYOK kill-switch p99 (last drill) | ≤ 5 min | _X min_ | _↑ / → / ↓_ | _green / yellow / red_ |

---

## Block 2 — Customer updates (6 min)

Open-ended; customer drives. Suggested prompts:

- Any change to your CI integration since last week (new jobs, new build configs)?
- Any change in traffic profile (campaign, release, conference push)?
- Any blockers we should know about (auth, quotas, schema, region pin)?
- Any planned change next week that affects cache behavior?

**Capture:** verbatim in CS notes; if a blocker is identified, it becomes an action item in Block 5.

---

## Block 3 — Next-week activities & drills (5 min)

CoreLink CS pre-loads what's planned:

- Customer-side: any maintenance windows, planned releases, deprecation cleanups.
- CoreLink-side: scheduled platform work that touches the customer's region; planned BYOK chaos drill (Enterprise); scheduled SOC 2 Type 1 audit-related artifact collection (Enterprise).
- Communication plan for any scheduled drill.

Customer confirms or pushes back on schedule.

---

## Block 4 — Open feedback (7 min)

Highest-value block; protect the full 7 minutes.

**Prompts (rotated week-to-week):**

- W1 prompt: "Walk me through what surprised you about the integration so far — good or bad."
- W2 prompt: "If we could ship one feature in the next 90 days that would make this materially better for you, what is it?"
- W3 prompt: "Where in the developer experience do you still feel friction?"
- W4 prompt: "What needs to be true at attestation for you to feel comfortable signing without hesitation?"

**Capture:**
- Feature requests → added to product backlog, tagged `source:lighthouse-{slot}`.
- Bugs → opened in customer-visible Jira project within 24h.
- Friction notes → CS internal log; surfaced at the quarterly roadmap review call.

---

## Block 5 — Action items + owners + due dates (4 min)

Standing format — every meeting closes with this table populated.

| # | Action | Owner (name) | Owner side | Due | Status |
|---|---|---|---|---|---|
| 1 | _e.g. Issue PAT for new CI job_ | _Alice_ | CoreLink | _next Tue_ | _open_ |
| 2 | _e.g. Confirm region pin for new repo_ | _Bob_ | Customer | _next Tue_ | _open_ |

**Closing checklist (host runs this in the final 60 seconds):**

- [ ] All action items have a named owner (not "the team").
- [ ] All action items have a due date (not "ASAP" or "next").
- [ ] Any SLO breach has a remediation action item.
- [ ] Any feature request is logged.
- [ ] Meeting notes appended to attestation §3.3 within 24h.

---

## Per-week emphasis

| Week | Day relative to D+0 | Special emphasis |
|---|---|---|
| W1 | D+15 | Integration health; confirm baseline measurements; flag early ergonomic surprises |
| W2 | D+22 | Mid-window SLO review; validate cache hit ratio stabilizing; first BYOK drill review (Enterprise) |
| W3 | D+29 | Incident retrospective (if any); confirm path to attestation; preview attestation form draft |
| W4 | D+36 | Attestation prep — customer signatory confirmed; procurement signer confirmed; §2 actuals draft circulated |

---

## Cancellation / rescheduling

- Customer may reschedule once per window without penalty.
- A skipped check-in must be made up within 7 days (async written update from CS + customer ack acceptable).
- Two consecutive cancellations escalates to CS lead; three consecutive escalates to founder.

---

## Post-meeting

CS writes the recap (≤ 200 words) and sends within 4 business hours. The recap appends to attestation §3.3 and serves as audit evidence for the engagement.

Recap template:

```
Subject: CoreLink × {Customer} — W{N} check-in recap

SLOs this week: {green/yellow/red summary}
Notable observations: {1-2 bullets}
Action items:
  - {item} — {owner} — {due}
  - {item} — {owner} — {due}
Next check-in: {date} {time TZ}

— {CS engineer}
```

---

**Fim 04-WEEKLY-CHECKIN-AGENDA.**
