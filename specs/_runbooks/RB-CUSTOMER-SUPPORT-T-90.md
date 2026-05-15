---
id: "RB-CUSTOMER-SUPPORT-T-90"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Support Lead + VPMkt (dual)"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "ROADMAP-TO-GA.md §8 (R-8 GA Launch)"
tags:
  - "runbook"
  - "support"
  - "customer-support"
  - "t-90"
  - "triage"
  - "sla"
  - "p0"
  - "p1"
  - "p2"
  - "p3"
  - "post-ga"
  - "r-prep"
  - "wt-r-prep-support-runbook"
---

# RB-CUSTOMER-SUPPORT-T-90 — Customer Support Runbook (First 90 Days Post-GA)

> **Purpose:** the comms-side playbook for inbound customer tickets in the **first 90 days post-GA** (T+0 .. T+90d). Engineering on-call handles incidents (`RB-ONCALL-POLICY.md` + `ONCALL-ESCALATION-MATRIX.md`); this runbook handles **tickets** — the customer-facing intake, triage, response, and escalation track that runs in parallel.
> **Audience:** Support Lead (Tier-1 + Tier-2 agents), VPMkt, VPSec (for security tickets), CS-OC (Customer Success on-call), Engineering on-call (for ticket→incident conversions only).
> **Scope:** standard support traffic only. Confirmed SEV1 incidents bypass this matrix and go directly to `ONCALL-ESCALATION-MATRIX.md` §3.
> **Window:** T+0 to T+90 days. After T+90 this runbook is reviewed + either re-baselined (v1.1) or replaced by the steady-state support spec (TBD post-GA).
> **Hard rule:** **a ticket can become an incident; an incident does not become a ticket.** Conversion criteria in §6.

---

## 1. Scope & non-scope

### In-scope
- Inbound customer questions, bug reports, billing disputes, account-management, sandbox/tier mechanics.
- DSR intake (handed off to `RB-DSR-TICKET-TRIAGE.md`).
- Security questionnaire requests (routed to VPSec).
- Lighthouse-customer tickets (handled per `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` §Comms protocol; Slack Connect channel is primary).

### Out-of-scope (do NOT triage here)
- **Live incidents** — PagerDuty fires, status-page goes orange/red, IC takes over (`RB-ONCALL-POLICY.md`).
- **Pentest / responsible-disclosure intake** — `RB-SECURITY-VULNERABILITY-INTAKE.md`.
- **Privacy incidents** — Scenario B in `marketing/launch/CRISIS-COMMS-TEMPLATES.md`; immediate VPSec/DPO escalation, not ticketing.
- **Press / journalist inquiries** — VPMkt direct.
- **Regulator inquiries** — Legal direct.

---

## 2. Channels

| Channel | Owner | Hours | Intake target | First-touch SLA |
|---|---|---|---|---|
| `support@corelink.dev` (primary) | Support-T1 | 24/7 (follow-the-sun pre-GA: Gustavo + 1 backup) | Zendesk-equivalent ticket queue (TBD vendor; placeholder `support.corelink.dev`) | Per §4 by severity |
| Status page subscribers (`status.corelink.dev`) | VPMkt | Auto-publish | Not a support channel — outbound only; tickets generated automatically when subscribers reply | n/a |
| In-app help widget (Intercom-style; `apps/docs` footer + dashboard chrome) | Support-T1 | Business hours (T+0..T+30); 24/7 from T+30 | Backfills to `support@corelink.dev` queue with `channel:widget` tag | Same as email per §4 |
| `billing@corelink.dev` | Support-T1 → Finance escalation | 24/7 | Same queue, `category:billing` tag | Per §4 |
| `security@corelink.dev` | VPSec | 24/7 | **Bypasses support queue** — direct VPSec; reference `SECURITY.md` | ≤ 4h ack |
| `dsr@corelink.dev` | Support-T1 → DSR pipeline | 24/7 | Routed through `RB-DSR-TICKET-TRIAGE.md` | ≤ 24h ack (per LGPD/GDPR 30-day window) |
| `incidents@corelink.dev` | Engineering on-call (read-only for support) | 24/7 | Inbound replies to incident customer-emails (see CRISIS-COMMS §A.4) | Handled by IC, not support |
| Slack Connect (lighthouse + named enterprise tenants only) | Support-T2 + CS engineer | Business hours; PagerDuty for P0 | Per `CUSTOMER-PLAYBOOK.md` §Comms protocol | ≤ 1h tagged business / ≤ 4h outside |

> **Channel hard rule:** every public-facing channel ultimately backfills to **one** unified ticket queue. The queue is the single source of truth for ticket state, SLA timing, and audit. No ticket lives only in Slack DM or email.

---

## 3. Triage matrix — severity decision tree

A ticket is assigned exactly one severity at first-touch. Severity MAY be raised (P3→P2→P1→P0) by the agent or shift lead; downgrades require shift-lead approval and audit-log note.

```
INBOUND TICKET
       │
       ▼
Q1: Is the customer reporting production unavailability,
    data loss, cross-tenant boundary breach, billing
    charge they cannot reverse, or active exploitation?
       │
       ├── YES ──▶ P0 (production down)
       │           Convert to INCIDENT if telemetry confirms (see §6).
       │
       └── NO
              │
              ▼
       Q2: Is a customer-facing capability (writes, reads,
           audit, BYOK, signups, billing portal, dashboard)
           IMPAIRED for this customer — degraded latency,
           intermittent errors, partial feature failure?
              │
              ├── YES ──▶ P1 (impaired)
              │           Convert to INCIDENT if multi-tenant signal (see §6).
              │
              └── NO
                     │
                     ▼
              Q3: Is the customer LIMITED in some way —
                  feature works but constrained, sandbox-tier
                  hit, quota reached, slow-but-functional,
                  doc-gap blocking adoption, billing
                  reconciliation question (no actual charge
                  error)?
                     │
                     ├── YES ──▶ P2 (limited)
                     │
                     └── NO ──▶ P3 (question)
                                Question, feature request,
                                "how do I…", security
                                questionnaire request, DPA
                                copy request, etc.
```

### 3.1 Severity quick-reference

| Sev | One-liner | Examples |
|---|---|---|
| **P0** | Production down for this customer; money/data/security at stake | "All our CI builds are failing — cache returns 500"; "We were double-charged $40k"; "We see data from another tenant in our audit log" |
| **P1** | Capability impaired but customer is still working | "Cache hit ratio dropped from 85% to 40% since yesterday"; "BYOK key health check intermittently failing"; "Dashboard loads but SLO panel is blank" |
| **P2** | Limited / bounded annoyance / sandbox/quota | "Hit the 100GB sandbox cap, need upgrade path"; "Audit query timing out at 30d range"; "Billing line item looks wrong but I'm not sure"; "Doc on BYOK rotation is unclear" |
| **P3** | Question / feature request / informational | "Do you support Pants 2.20?"; "Can I get your DPA?"; "When will GitLab Runner adapter ship?"; "Security questionnaire (50 questions)" |

### 3.2 Reflex routing (auto-tags on intake)

| Heuristic | Action |
|---|---|
| Body mentions "breach", "compromised", "exfil", "cross-tenant" | Auto-page VPSec; tag `security:suspected`; hold P-level until VPSec triages |
| Body mentions GDPR/LGPD Articles 15–22 keywords (access, erasure, portability, rectification, objection, restriction) | Route to `RB-DSR-TICKET-TRIAGE.md`; default P2 unless customer asserts unusual scope |
| Body mentions "refund", "double-charged", "Stripe", "invoice wrong" with $-amount > $5,000 | Auto-tag P0 pending Finance confirmation |
| Lighthouse-customer slot id in From or Subject | Route to assigned CS engineer; default P-level + 1 (raise floor) |
| Subject contains `[urgent]` or `[outage]` | Floor at P1; agent decides P0 vs P1 within 15 min |
| Sender is `noreply@*` from status page subscriber list | Likely auto-reply; close as `no-action` |
| Same customer files ≥ 3 tickets in 60 min | Auto-tag `pattern:multi-ticket`; raise floor by 1 (P3→P2, P2→P1) and notify shift lead |

---

## 4. Response SLAs per severity

| Severity | First-touch ack | First-fix attempt / substantive response | Resolution target |
|---|---|---|---|
| **P0** | ≤ 15 min, 24/7 | ≤ 1h | ≤ 4h (or convert to incident per §6) |
| **P1** | ≤ 1h, 24/7 | ≤ 4h | ≤ 24h |
| **P2** | ≤ 4h, business hours | ≤ 24h | ≤ 72h (3 business days) |
| **P3** | ≤ 24h, business days | ≤ 48h | ≤ 5 business days |

> **Business hours definition (T+0..T+90):** 09:00–18:00 in the Support shift lead's local timezone, Mon–Fri, excluding the `holiday-freeze` window (Dec 22 – Jan 2) per `RB-ONCALL-POLICY.md` §4.
> **24/7 coverage** (P0/P1) is achieved via the follow-the-sun pre-GA staffing — Gustavo (US/PT) + 1 contracted backup (TBD region) + lighthouse-CS rotation overlap. From T+30, dedicated support shift coverage begins.

### 4.1 SLA clock-start rules

- Clock starts at **ticket creation timestamp in the queue**, NOT at the customer's send time.
- Clock pauses when ticket is in `Pending-customer` state (waiting for customer reply).
- Clock pauses during the `holiday-freeze` window for P2 + P3 only (P0/P1 keep 24/7 clock).
- Clock does NOT pause for vendor dependencies (Stripe, Clerk, BYOK vendor) — that's on us.

### 4.2 SLA breach handling

If first-touch ack SLA breaches:
1. Auto-page the next-tier support role (T1 unack → T2 paged; T2 unack → Support Lead paged).
2. SLA breach event written to ticket audit log.
3. Counted in weekly support-sync report (§5.5).
4. Customer receives automatic "we missed our ack window — escalating internally" comms via template `SR-BREACH-APOLOGY` in `SUPPORT-RESPONSE-TEMPLATES.md`.

---

## 5. Per-severity escalation path

### 5.1 P0 escalation

```
Customer ticket P0
       │
       ▼
[T+0] Support T1 acks within 15 min, opens ticket, posts to #support-live
       │
       ▼
[T+0] Auto-page on-call engineer L1 (PagerDuty route `pd-support-p0`)
       │
       ▼
[T+15min] If telemetry confirms multi-tenant signal OR data-integrity OR security
          → CONVERT TO INCIDENT (see §6); IC takes over; this ticket links to incident id
       │
       ▼
[T+1h] Substantive response (status update or first-fix attempt)
       │
       ▼
[T+15min cadence] Continuous comms until resolution per §7
```

**P0 escalation roles:**

| Role | Trigger | Action |
|---|---|---|
| Support T1 agent | First-touch | Ack ≤ 15 min, page L1, draft response |
| On-call engineer L1 | Auto-page from PD | Triage telemetry; decide ticket-only vs incident-conversion within 15 min |
| Support Shift Lead | If T1 unack > 10 min OR ticket sits in queue > 5 min | Take over T1 role; page T2 backup |
| Engineering Manager L2 (`ONCALL-ESCALATION-MATRIX.md` §1) | If L1 unack > 5 min OR ticket→incident converted | IC takes over comms; support ticket becomes incident-comms feed |
| VPSec | If `security:suspected` tag fires | Within 30 min — triage as Scenario B (CRISIS-COMMS) or downgrade |
| Founder (Gustavo) | If P0 unresolved > 4h OR customer demands escalation | Direct customer outreach |

### 5.2 P1 escalation

```
Customer ticket P1
       │
       ▼
[T+0] Support T1 acks within 1h, opens ticket
       │
       ▼
[T+1h] Triage: real degradation or perceived? Check tenant-scoped telemetry.
       │
       ├── Real, multi-tenant ──▶ Convert to incident (§6); IC + SEV2 path
       │
       └── Real, single-tenant ──▶ Continue P1 ticket track
       │
       ▼
[T+4h] Substantive response (root cause hypothesis OR mitigation)
       │
       ▼
[Hourly] Comms until resolved (per §7)
       │
       ▼
[T+24h] Resolved OR escalated to Engineering for code fix
```

**P1 escalation roles:**

| Role | Trigger | Action |
|---|---|---|
| Support T1 agent | First-touch | Ack, telemetry pull, draft response |
| Support Shift Lead | If T1 ack delayed > 30 min OR fix-attempt fails | Take over; loop in Engineering on-call (non-paging Slack ping) |
| Engineering on-call L1 | Shift Lead Slack ping; non-paging | Investigate root cause within 4h |
| Engineering Manager L2 | If unresolved > 12h OR pattern across tenants | Promote to incident |
| Founder (Gustavo) | If unresolved > 24h | Customer outreach |

### 5.3 P2 escalation

```
Customer ticket P2
       │
       ▼
[T+0..T+4h] Support T1 acks within 4h
       │
       ▼
[T+24h] Substantive response (answer, fix, workaround, or scoped commitment)
       │
       ▼
[Daily standup] Reviewed in daily support-sync (10 min, async-OK)
       │
       ▼
[T+72h] Resolved OR explicit hand-off owner named with target date
```

**P2 escalation roles:**

| Role | Trigger | Action |
|---|---|---|
| Support T1 agent | First-touch + ongoing | Owns ticket end-to-end |
| Support Shift Lead | Daily standup review | Triage anything aging > 48h |
| Engineering / Product | If feature gap or non-trivial bug | Owner named; target date set; ticket links to issue |

### 5.4 P3 escalation

```
Customer ticket P3
       │
       ▼
[T+0..T+24h] Support T1 acks within 24h
       │
       ▼
[T+48h] Substantive response
       │
       ▼
[Weekly support sync] Reviewed in weekly sync (Fri 30 min)
       │
       ▼
[T+5d] Resolved OR converted to feature request / doc gap
```

**P3 escalation roles:**

| Role | Trigger | Action |
|---|---|---|
| Support T1 agent | First-touch + ongoing | Owns ticket; canned templates allowed |
| Support Shift Lead | Weekly sync | Aggregate themes; identify top-10 categories for dashboard (§9) |
| Product / DevRel | Feature request volume | Aggregate into roadmap signal monthly |

### 5.5 Cadence summary

| Cadence | Reviews | Audience | Owner |
|---|---|---|---|
| `#support-live` Slack (real-time) | P0/P1 active | Support team + on-call eng | Support Shift Lead |
| Daily standup (10 min, async-OK) | All P0/P1/P2 aging > 24h | Support team + Shift Lead | Support Shift Lead |
| Weekly support sync (Fri, 30 min) | Top P2 themes + all P3 + dashboard review | Support team + VPMkt + VPSec + Product | Support Lead |
| Monthly support retro (last Fri of month, 60 min) | SLA scorecard + top-10 issue categories + NPS trend + breach root-cause | Support team + VPMkt + Founder | Support Lead |

---

## 6. When to convert a ticket to an incident

A ticket CONVERTS to an incident when **any** of the following becomes true:

| Conversion trigger | Confirmed by | Action |
|---|---|---|
| Multi-tenant signal — same symptom reported by ≥ 2 distinct tenants within 60 min | Support Shift Lead via cross-ticket query | Page on-call eng; declare SEV2 minimum |
| Telemetry confirms customer report matches a production-side anomaly | On-call eng | Declare SEV per `ONCALL-ESCALATION-MATRIX.md` §2 |
| Customer reports data-integrity violation (wrong bytes, cross-tenant data) | On-call eng + VPSec | Declare SEV1 — Scenario B path in CRISIS-COMMS |
| Billing bug affecting > 1 customer OR > $1,000 | Finance + Support Shift Lead | Trigger CRISIS-COMMS Scenario C; pause billing automation |
| Customer reports active exploitation / "we think we're being attacked" | VPSec | Declare SEV1; CRISIS-COMMS Scenario B/D path |
| SLA breach affecting a lighthouse customer's attestation window | CS engineer + Support Lead | Page L2; lighthouse phase-management runbook |
| Status-page green but customer P0s aging > 1h | Support Shift Lead | Force-elevate to incident; "absence of telemetry is not absence of problem" |

### 6.1 Comms switch at conversion

The moment a ticket converts to an incident:

1. **Ticket stays open** — becomes the incident's customer-comms feed for that tenant.
2. **IC takes over** comms decisions (`ONCALL-ESCALATION-MATRIX.md` §1).
3. **Status page** updated per `CRISIS-COMMS-TEMPLATES.md` §A.1.
4. **Customer email** sent per `CRISIS-COMMS-TEMPLATES.md` §A.4 (replaces support's individual reply for the duration).
5. **Support cadence** switches from §7 to incident cadence (every 15 min for SEV1 per ONCALL-ESCALATION-MATRIX §3).
6. **Ticket label** updated: `converted-to-incident:<incident_id>`.
7. **Audit log** captures the conversion timestamp + decision-maker.

### 6.2 Demotion (incident → ticket)

The reverse path is **rare** and requires Support Lead + IC joint sign-off. Used when an incident was over-declared and telemetry / forensics shows single-tenant scope with no cross-tenant or integrity impact. The original SEV declaration remains in audit; only the live state changes.

---

## 7. Comms cadence per severity

Cadence governs **how often** the customer hears from us while a ticket is open. All templates referenced live in `marketing/launch/SUPPORT-RESPONSE-TEMPLATES.md`.

| Severity | First-touch | Investigation hold | Root cause found | Fix deployed | Confirmed resolved |
|---|---|---|---|---|---|
| **P0** | `SR-FIRST-TOUCH-P0` ≤ 15 min | `SR-INVESTIGATING-HOLD-P0` every 15 min | `SR-ROOT-CAUSE` (pre-fix) | `SR-FIX-DEPLOYED` | `SR-RESOLVED` |
| **P1** | `SR-FIRST-TOUCH-P1` ≤ 1h | `SR-INVESTIGATING-HOLD-P1` every 1h | `SR-ROOT-CAUSE` | `SR-FIX-DEPLOYED` | `SR-RESOLVED` |
| **P2** | `SR-FIRST-TOUCH-P2` ≤ 4h | Daily update (`SR-INVESTIGATING-HOLD-P2`) | `SR-ROOT-CAUSE` (combined with resolution often) | n/a (often single-touch fix) | `SR-RESOLVED` |
| **P3** | `SR-FIRST-TOUCH-P3` ≤ 24h | n/a (single substantive response usual) | n/a | n/a | `SR-RESOLVED` (on close) |

### 7.1 Comms hard rules
- **Every customer-facing message gets the agent's name** — no shared aliases until T+30 dedicated support shift exists.
- **No silence longer than the cadence window.** If we can't update, send `SR-INVESTIGATING-HOLD-*` with "no new info" — silence is worse than repetition.
- **Templates are starting points, not final text.** Agents personalize the `{{tenant_context}}` block per ticket.
- **Lighthouse customers** get parallel updates in Slack Connect channel — never instead of email.
- **At conversion to incident** (§6.1), cadence escalates to incident cadence (every 15 min P0 per ONCALL-ESCALATION §3).

---

## 8. Ticket workflow states

```
   NEW ──▶ ACKNOWLEDGED ──▶ INVESTIGATING ──▶ AWAITING-FIX ──▶ RESOLVED ──▶ CLOSED
                │                  │                │                            ▲
                │                  │                │                            │
                ▼                  ▼                ▼                            │
         PENDING-CUSTOMER (SLA clock pauses) ─────────────────────────────────────┘
                │
                ▼
         CONVERTED-TO-INCIDENT (no longer counted in support SLA; incident SLA applies)
```

**State transitions:**

| From → To | Trigger | Audit-log required |
|---|---|---|
| NEW → ACKNOWLEDGED | First-touch sent | yes (sets ack timestamp) |
| ACKNOWLEDGED → INVESTIGATING | Agent begins substantive work | no |
| INVESTIGATING → AWAITING-FIX | Engineering owns; agent waiting | yes (set ETA) |
| Any → PENDING-CUSTOMER | Customer reply required | yes (pause reason) |
| PENDING-CUSTOMER → INVESTIGATING | Customer reply received | yes (resume) |
| Any → CONVERTED-TO-INCIDENT | §6 trigger | yes (incident id link) |
| AWAITING-FIX → RESOLVED | Fix deployed + customer confirms OR no-reply after 48h | yes |
| RESOLVED → CLOSED | 72h no reply after resolution OR explicit close | no |
| CLOSED → REOPENED | Customer replies within 7d | yes (reopen count) |

---

## 9. Dashboard hooks

This runbook drives the support team's dashboard in `marketing/launch/SUPPORT-DASHBOARD-SPEC.md`. The runbook is the data definition; the spec is the rendering.

Quick reference of metrics this runbook generates:

| Metric | Source | Refresh |
|---|---|---|
| Open tickets by severity | Ticket queue | 60s |
| First-touch SLA burn-down | Ticket created-at vs ack-at | 5 min |
| Top-10 issue categories (rolling 7d / 30d) | Ticket tags (manually-applied at close) | hourly |
| Ticket→incident conversion rate | `converted-to-incident:*` label | 1h |
| SLA breach count (rolling 7d) | Audit log | 1h |
| Customer NPS sample (post-resolution survey) | Survey send 24h after CLOSED | weekly |

---

## 10. T+0 .. T+90 ramp plan

| Window | Staffing | Channels live | Notes |
|---|---|---|---|
| T+0 to T+7 | Founder (Gustavo) + 1 contracted backup; 24/7 page coverage; **all P0/P1 routed to founder** | `support@`, `billing@`, status page, in-app widget (read-only first 48h) | First-touch SLAs tight; everything else expand-tolerant |
| T+7 to T+30 | Add 1 dedicated support agent (T1); founder retains shift-lead role | In-app widget enabled write-mode; Slack Connect for lighthouse | Daily standup begins T+10 |
| T+30 to T+60 | 2 T1 agents (US + EU overlap = ~16h coverage); founder is L2 fallback only | All channels live; DSR pipeline at steady-state | Weekly support sync formalised |
| T+60 to T+90 | 3 T1 agents (24/7 follow-the-sun); contracted T2 specialist (billing + security routing) | Steady-state | Monthly retro formalised; v1.1 of this runbook drafted from observed data |

> **Pre-GA gating:** at T-7 in `LAUNCH-CHECKLIST-V2.md`, support staffing for T+0..T+7 MUST be confirmed (named humans, paged-rotation tested). H-3 (PagerDuty) routes for `pd-support-p0/p1/p2/p3` MUST exist.

---

## 11. Cross-references

### Engineering-side (incident track)
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — SEV1/2/3 × L1/L2/L3 escalation
- `specs/_runbooks/RB-ONCALL-POLICY.md` — rotation, fatigue, handoff
- `specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md` — war-room coordinator playbook
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — blameless retro

### Comms / marketing
- `marketing/launch/CRISIS-COMMS-TEMPLATES.md` — Scenarios A-F crisis templates (incident-level)
- `marketing/launch/SUPPORT-RESPONSE-TEMPLATES.md` — **this runbook's companion** — 15 ticket-level templates
- `marketing/launch/STATUS-PAGE-SPEC.md` — status page (outbound channel)
- `marketing/launch/SUPPORT-DASHBOARD-SPEC.md` — dashboard rendering spec
- `marketing/launch/LAUNCH-CHECKLIST-V2.md` — pre-launch gating
- `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` — lighthouse comms protocol overlay

### DSR / privacy
- `specs/_runbooks/RB-DSR-TICKET-TRIAGE.md` — **DSR-specific intake** (sibling)
- `specs/05_quality/runbooks/RB-DSR-INTAKE-FAILURE.md` — DSR backend failure mode
- `specs/05_quality/runbooks/RB-DSR-ERASURE-INCOMPLETE.md` — DSR erasure failure mode

### Roadmap / launch
- `ROADMAP-TO-GA.md` §8 (Wave R-8 GA Launch) + §9 H-3 (PagerDuty)
- `apps/docs/docs/tutorials/quickstart-10min.mdx` — first-touch self-serve path

---

**Fim RB-CUSTOMER-SUPPORT-T-90.**
