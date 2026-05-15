---
id: "RB-TABLETOP-TEMPLATE"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "template", "tabletop", "game-day", "s17", "ops-maturity", "sre"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §1.

# RB-TABLETOP-TEMPLATE — Game Day Tabletop Exercise Template (60-min default; 4-hour quarterly)

> **Purpose:** standardised facilitation template for CoreLink game day
> tabletop exercises. Used quarterly per Quality Standard 14.s17.5; also
> usable ad-hoc for runbook readiness drills.
>
> **Cadence:** quarterly (CF Cron `0 6 1 3,6,9,12 *` — 06:00 UTC, 1st of
> March / June / September / December). One full 4-hour quarterly session;
> shorter 60-min targeted exercises in between.
>
> **Facilitator:** SRE Lead (oncall manager covers if SRE Lead unavailable).
> **Format:** synchronous; participants share screen / whiteboard; observer
> captures notes in real time.

---

## 1. Scenario header (fill in per exercise)

| Field | Value |
|---|---|
| Exercise ID | `<YYYY-MM-DD-tabletop-<short-name>>` |
| Date | `YYYY-MM-DD` |
| Duration | 60 min (targeted) · 4 h (quarterly) |
| Scenario | `<Region outage / BYOK revoke / Supply-chain typosquat / Insider exfil / custom>` |
| Trigger description | 2–3 sentence narrative the facilitator reads aloud at T+0 |
| In-scope systems | CF Workers · D1 · R2 · KV · DO · BYOK adapter (if relevant) · PagerDuty · Status page |
| Out-of-scope | Real prod traffic touch · paging real customers · pressing real BYOK revoke |
| Severity assumed | SEV-1 / SEV-2 / SEV-3 |
| Customer impact assumed | `<latency / availability / data-access / privacy>` |
| FM linkage | FM-XXX · FM-YYY |
| Runbooks invoked | RB-XXX · RB-YYY |

---

## 2. Participants & roles

| Role | Default participant | Responsibilities during exercise |
|---|---|---|
| Facilitator | SRE Lead | Reads inject, controls clock, blocks rabbit-holes, captures gaps. |
| Incident Commander (IC) | Rotating oncall primary | Owns decision tree, escalates, runs comms. |
| Scribe / Observer | Engineer non-oncall | Real-time notes; populates §6 debrief grid. |
| Comms officer | Product / Docs lead | Drafts customer comms, status-page update, internal Slack post. |
| Security advisor | AppSec (if scenario is security-tinted) | BYOK / supply-chain / insider escalation paths. |
| Privacy advisor | Privacy officer (if scenario touches PII / DSR) | LINDDUN reflexes; CTRL-PRIV-001 enforcement. |
| Customer success advisor | (optional) | Enterprise comms tone; SLA implication framing. |

Minimum quorum: facilitator + IC + scribe + 1 advisor (4 humans). Below
quorum → reschedule, do not run.

---

## 3. Decision tree (canonical paths)

```
T+0  Inject delivered
 ├─ Acknowledge SEV → page Tier 1 oncall (simulated)
 ├─ Open incident channel (#inc-<id> simulated)
 └─ Start clock (MTTA target < 5 min for SEV-1)

T+5  Diagnose
 ├─ Pull SLO board / fadigue dashboard / chaos catalog cross-ref
 ├─ Identify failure mode (FM-XXX)
 └─ Locate runbook (RB-XXX)

T+10 Decide
 ├─ Mitigate (failover / safe-mode / kill switch)
 ├─ Escalate (Tier 2 / Tier 3 / security / legal)
 └─ Communicate (status page draft / customer DM template)

T+20 Execute (simulated steps; do NOT touch prod)
 ├─ Walk the runbook step-by-step out loud
 ├─ Note where runbook lacks detail → gap
 └─ Note where runbook contradicts current code → drift

T+40 Verify recovery (simulated)
 ├─ Define "green" SLOs / fadigue thresholds
 └─ Define exit criteria for incident close

T+50 Debrief (see §6)
```

Time boxes are guidance, not rigid. Facilitator can compress or stretch
each box by ±25 % to match the scenario, but the total MUST land within
the declared duration (60 min or 4 h).

---

## 4. Injects (facilitator-only)

Facilitator prepares 2–4 mid-exercise twists ahead of time. Examples:

- T+15 inject: "PagerDuty itself is degraded — escalation goes manual."
- T+25 inject: "Customer X tweets about the outage."
- T+35 inject: "A second region begins to show the same symptoms."
- T+45 inject: "Legal asks if this is a notifiable breach."

Injects test adaptive response and reveal where runbooks silently assume
happy paths.

---

## 5. Observer note grid (live capture)

| T+ min | What the team did | Runbook step ref | Observed gap / friction | Sev (Low/Med/High) |
|---|---|---|---|---|
| | | | | |

---

## 6. Debrief grid (last 10 min of exercise)

| Theme | Finding | Action item | Owner | Due |
|---|---|---|---|---|
| Runbook accuracy | | | | |
| Tooling readiness | | | | |
| Comms / escalation | | | | |
| SLO / observability | | | | |
| Decision authority | | | | |
| Burnout / fatigue signal | | | | |

Rules:
1. **Blameless.** Phrase findings as *systems / process*, never *people*.
2. **Specific.** Every action item needs an owner + due date + tracker
   issue ID (or "create issue post-exercise" flagged in §7).
3. **Bounded.** Cap action items at 7 per exercise; more than that means
   the scenario was too broad — split next time.

---

## 7. Post-exercise outputs (mandatory)

1. Tabletop report committed at
   `specs/_audits/YYYY-MM-DD-s<NN>-tabletop-<short-name>.md` within 48 h.
2. Action items entered in tracker with `tabletop-<id>` label.
3. Runbook updates committed within 1 sprint (RB-XXX version bump).
4. Findings cross-linked in next sprint review.
5. If a **HIGH** gap surfaces, escalate to sprint owner same day; do not
   wait for the report.

---

## 8. Anti-patterns

- ❌ Run the scenario against real prod ("just one tiny safe step").
- ❌ Skip the scribe — no scribe means no debrief.
- ❌ Let the IC role default to the SRE Lead (defeats rotation purpose).
- ❌ Use a brand-new scenario the facilitator has not pre-walked.
- ❌ End the exercise without populating §6 grid.
- ❌ File action items without owners.
- ❌ Repeat the same scenario two quarters in a row (rotate the library).

---

## 9. Scenario library reference

Maintained at `specs/_templates/game_day_scenarios.md` (per WI-S17-006 §6.1).
Canonical library at GA:

- **A** — CF region outage (reuse DR drill cycle 1 patterns).
- **B** — BYOK CMK revoke under load (this template's worked example;
  see `specs/_audits/2026-05-14-s17-tabletop-byok-revoke.md`).
- **C** — Supply-chain typosquat (FM-156 / FM-157).
- **D** — Insider exfiltration (FM-258).

Rotate quarterly; introduce 1 new scenario per year.

---

## 10. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Sonnet WI-S17-006 builder) | Initial template — scenario header + roles + decision tree + injects + observer grid + debrief + outputs + anti-patterns. |
