---
id: "RB-POSTMORTEM-PROCESS"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags:
  - "runbook"
  - "postmortem"
  - "blameless"
  - "process"
  - "s17"
  - "wi-s17-004"
---

# RB-POSTMORTEM-PROCESS — Blameless Postmortem Process

## 1. Summary

This runbook defines **when** CoreLink writes a postmortem, **on what
timeline**, **who owns each section**, and **what the sign-off bar is**.
It is the operational complement to the templates at
`templates/incident.md` and `templates/postmortem.md` introduced by
WI-S17-004.

Authoritative references:

- Google SRE Workbook, Ch 10 ("Postmortem Culture: Learning From Failure").
- CoreLink spec contract §5.4 R-S17-11 (sprint linkage).
- CoreLink quality standard 14.s17.3 (blameless culture, SRE-lead review).

> **Blameless pledge.** Every postmortem under this process is written
> in the third-person systemic voice. We name **systems and processes**;
> we do not name **people** as proximate causes. Anyone acting in good
> faith with the information available at the time is, by policy, not
> the subject of the document.

---

## 2. When to write a postmortem

A postmortem is **mandatory** when any one of these is true:

| Trigger | Mandatory? | Notes |
|---|---|---|
| Incident classified **SEV-1** | Yes | Always. |
| Incident classified **SEV-2** | Yes | Always. |
| Any **user-visible** incident (regardless of severity) | Yes | "User-visible" = at least one external tenant saw a degraded API response, missing feature, wrong data, or a status-page event. |
| Synthetic SEV-2 chaos / DR drill | Yes | Required for drill validation per WI-S17-004 acceptance. |
| Near-miss caught by monitoring before customer impact | **Optional but encouraged** | Author may downgrade to a "lessons learned" note if SRE-lead agrees in writing. |
| SEV-3 internal-only with no customer impact | Optional | Default: skip; author may write one if it surfaces a systemic issue worth recording. |

A postmortem is **forbidden from being skipped** if the incident
triggered an `EVT-016` (`HUMAN_SIGNOFF`) audit event or a customer
status-page posting.

---

## 3. Timeline

Counted from the moment the incident is **resolved** (status →
`resolved` or `closed` in the incident doc), in calendar days.

| Milestone | Deadline | Owner |
|---|---|---|
| Incident doc finalized (`status: closed`, all timeline rows merged) | within **24 h** | Incident commander |
| Postmortem **draft** posted for review | within **5 calendar days** | Postmortem author (default: incident commander, may delegate) |
| First-round comments addressed; postmortem at `doc_status: REVIEW` | within **10 calendar days** | Author |
| Two-reviewer sign-off complete; postmortem at `doc_status: SEALED` | within **14 calendar days** | SRE lead + peer reviewer |
| Action items entered in the linked sprint's backlog with `sprint_decision` recorded | within **14 calendar days** | Sprint owner of `sprint_link` |

**Escalation if the 14-day clock is missed**: SRE lead pages the owner;
if still unresolved at 21 days, the sprint owner of the **current**
sprint inherits the postmortem as a blocker on sprint sign-off
(per R-S17-11).

---

## 4. Section ownership

Each section of `templates/postmortem.md` has a default owner. Authors
may reassign but the **default** is the table below — used to chase
missing inputs.

| Postmortem section | Default owner | Notes |
|---|---|---|
| §1 Summary | Author | 2–4 paragraphs, blameless voice. |
| §2 Impact | Author + Product (if customer-facing) | Tenant counts, SLO burn, contractual impact. |
| §3 Root Causes (incl. 5-Why) | Author + Tech lead of affected service | The 5-Why **must** terminate in a system / process root, never a person. |
| §4 Trigger | Author | One sentence + timestamp + source artifact. |
| §5 Resolution | Incident commander | Mitigation, verification, data-repair. |
| §6 Detection | SRE / observability owner of the affected SLO | TTD, alert names, detection gaps. |
| §7 Action Items + Lessons Learned | Author + Sprint owner | Action items typed (`prevent` / `detect` / `mitigate` / `process` / `documentation`) with owner team + due date. Sprint owner records `sprint_decision` per R-S17-11. |
| §8 Timeline | Scribe (designated during the bridge) | All UTC; correlation_id propagated. |
| §9 Reviews & sign-off | SRE lead | Drives the two-reviewer process. |
| §10 Cross-references | Author | Runbooks executed, related incidents, drill links. |

**Special reviewers** (added to §9 when applicable):

- **Security Lead** — any incident with confidentiality / integrity
  impact, suspected compromise, or audit-chain anomaly.
- **Privacy Officer** — any incident touching PII, BYOK CMK state, DSR
  workflows, or cross-region data residency.
- **Cost Owner** — incidents whose mitigation involved a > $1k/day
  emergency spend (e.g., emergency capacity, fallback to a paid tier).

---

## 5. Sign-off bar (blameless enforcement)

A postmortem reaches `doc_status: SEALED` only when **all** of the
following hold:

1. `blameless_pledge: true` in front matter (template default;
   may never be set `false`).
2. Two reviewers signed off in §9: **SRE lead** + **one peer outside
   the affected team**. Authors may not self-certify.
3. The body has been scanned (manually or via the optional
   `scripts/blameless_lint.py` if present) for blameful language
   patterns: phrases like "X failed to", "X should have", "fault of",
   "negligent", or any first-person finger-pointing. Any hit must be
   rewritten in systemic voice before sign-off.
4. Every action item has `owner team`, `due` date, and
   `sprint_decision` populated.
5. `sprint_link` resolves to a real sprint (`S-XX`), and that sprint's
   owner has recorded an accept / reject / defer on every action item.

If sign-off bar (3) is failed at review time, the SRE-lead reviewer
**must** request a rewrite and trigger blameless-culture reinforcement
(see §6) — they may not sign off "with comments".

---

## 6. Culture violations and reinforcement

If a postmortem reaches review with name-shaming, blame voice, or
`blameless_pledge: false`:

1. SRE-lead reviewer marks the PM `doc_status: DRAFT` (revert), with a
   review comment citing the specific lines and the rewrite required.
2. Author rewrites in systemic voice; resubmits.
3. The pattern is logged in the SRE quarterly culture review.
4. If the same author / team produces a second blameful PM within a
   quarter, a 30-minute targeted refresher is scheduled (not punitive;
   restorative).

This process is itself blameless: the **document** is what fails review,
not the person.

---

## 7. Storage & discoverability

- **Templates**: `templates/incident.md`, `templates/postmortem.md`.
- **Live postmortems**: `specs/_postmortems/PM-YYYY-MM-DD-NNN-<slug>.md`.
- **Live incident docs**: `specs/_postmortems/INC-YYYY-MM-DD-NNN-<slug>.md`
  (or under the incident-tracking system of record, with the doc link in
  the postmortem's §10).
- **Drill-related postmortems** (synthetic chaos / DR drills) live in
  `specs/_postmortems/` alongside production postmortems; the drill
  origin is recorded in the front-matter `tags` (`synthetic`, `chaos`,
  `dr-drill`) and the postmortem's §10 cross-references the drill
  catalog entry (WI-S17-001) and the drill tracker
  (`specs/04_runbooks/_dry_run_log.md`, WI-S17-003).

---

## 8. Sprint linkage (R-S17-11)

The `sprint_link` front-matter field is **mandatory**. Linkage is
*retroactive*: the postmortem is linked to the sprint that is
**in flight** when the postmortem is drafted, regardless of when the
incident occurred. The linked sprint's owner:

1. Reviews every action item in §7 within the 14-day clock.
2. Records `accepted` / `rejected` / `deferred` (with rationale for
   `rejected` or `deferred`) in the action-items table.
3. Files accepted action items into the sprint backlog (or a successor
   sprint's backlog with the sprint owner's written agreement).

If the linked sprint seals before all action items are decided, the
remaining items roll into the next sprint and the postmortem's
`sprint_link` is updated (the supersession is recorded via the standard
`supersedes` / `superseded_by` chain on the postmortem doc).

---

## 9. Metrics emitted

| Metric | Type | Labels | Purpose |
|---|---|---|---|
| `corelink_postmortem_total` | counter | `severity`, `blameless` | Alerts if `blameless="false"` ever fires. |
| `corelink_postmortem_draft_age_days` | gauge | `id` | Pages SRE lead when > 5. |
| `corelink_postmortem_seal_age_days` | gauge | `id` | Pages SRE lead when > 14. |
| `corelink_incident_mtta_seconds` | histogram | `severity` | SEV-1 target < 300 s. |
| `corelink_incident_mttr_seconds` | histogram | `severity` | SEV-1 target < 1800 s. |

All metric names are `snake_case` per the metrics naming convention.

---

## 10. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo Schneiter | Initial runbook authored under WI-S17-004; codifies the blameless postmortem process aligned with Google SRE Workbook Ch 10. |

---

**End RB-POSTMORTEM-PROCESS.**
