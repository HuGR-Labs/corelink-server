---
id: "PM-YYYY-MM-DD-NNN"
type: "post_mortem"
incident_id: "INC-YYYY-MM-DD-NNN"
severity: "SEV-2"                                # enum: SEV-1 | SEV-2 | SEV-3
doc_status: "DRAFT"                              # enum: DRAFT | REVIEW | SEALED
owner: "TEMPLATE_AUTHOR"
reviewers:
  - role: "sre_lead"
    name: "TEMPLATE_SRE_LEAD"
  - role: "eng"
    name: "TEMPLATE_PEER_REVIEWER_OUTSIDE_AFFECTED_TEAM"
final_approver: "TEMPLATE_FINAL_APPROVER"
blameless_pledge: true                           # culture enforcement — NEVER false
sprint_link: "S-XX"                              # retroactive linkage per R-S17-11
audit_status: "ACTIVE"
version: "1.0.0"
created: "YYYY-MM-DD"
updated: "YYYY-MM-DD"
supersedes: null
superseded_by: null
tags: ["postmortem", "blameless", "sev-2"]
---

# PM-YYYY-MM-DD-NNN — {{title}}

> **Template Version:** 1.0.0
> **Source:** Google SRE Workbook Ch 10 "Postmortem Culture: Learning From
> Failure" + Atlassian Blameless Postmortem Template, adapted for CoreLink
> (WI-S17-004).
>
> **Blameless pledge** (must be honored by every contributor to this doc):
> *We focus on the conditions and systems that allowed the incident to
> happen, not on the people who happened to be on the keyboard. Anyone
> acting in good faith with the information available at the time will
> not be named or held individually responsible in this document.*

---

## 1. Summary

A 2–4 paragraph executive summary. Reader should leave knowing **what
broke, who was affected, how long, and the single most important
lesson**. Blameless voice (third-person systemic).

---

## 2. Impact

- **Severity**: SEV-{1\|2\|3} (per `specs/_runbooks/RB-POSTMORTEM-PROCESS.md`).
- **Duration of customer impact**: minutes / hours.
- **Tenants / users affected**: count + IDs (or "internal only").
- **SLO budget burned**: which SLO, what % of monthly budget.
- **Revenue / contractual impact**: estimate or "none".
- **Trust / reputational impact**: status-page postings, customer-facing
  communications issued.

---

## 3. Root Causes

Multiple causes are normal — incidents rarely have a single cause.

### 3.1 Trigger

The proximate change / event that made the latent failure visible
(deploy, traffic spike, dependency outage, config change, expiry…).

### 3.2 Contributing conditions

The pre-existing system / process conditions that, combined with the
trigger, produced the incident. List 2–5.

### 3.3 5-Why analysis

Walk down from the surface symptom to a systemic root. Each "why"
points at a system / process / architectural property, **not a person**.

1. **Why did the user-visible failure occur?** …
2. **Why?** …
3. **Why?** …
4. **Why?** …
5. **Why?** (root cause: system / process / architectural property) …

---

## 4. Trigger

Re-state the trigger event in one sentence, with its exact UTC timestamp
and source artifact (commit SHA / config diff / external event).

---

## 5. Resolution

- Mitigation that stopped the bleeding (with timestamp).
- Verification: how we know it actually worked.
- Permanent fix status: shipped / planned (link to PR / action item).
- Rollback / data-repair operations performed.

---

## 6. Detection

- How the incident was detected (alert / synthetic / customer).
- TTD (time-to-detect) and whether it met target.
- Detection gaps: monitoring that should have fired earlier and did not
  — these become action items.

---

## 7. Action Items + Lessons Learned

### 7.1 Action items

Every action item has **owner team + due date + status**. Sprint owner
of the linked sprint accepts / rejects / defers each, per R-S17-11.

| ID | Type | Item | Owner team | Due | Sprint decision | Status |
|---|---|---|---|---|---|---|
| AI-1 | prevent | … | … | YYYY-MM-DD | accepted \| rejected \| deferred | open |
| AI-2 | detect | … | … | YYYY-MM-DD | … | open |
| AI-3 | mitigate | … | … | YYYY-MM-DD | … | open |
| AI-4 | process | … | … | YYYY-MM-DD | … | open |

Action-item **types** (Google SRE taxonomy): `prevent`, `detect`,
`mitigate`, `process`, `documentation`.

### 7.2 Lessons learned

**What went well**

- …

**What went poorly** (system / process gaps, not people)

- …

**Where we got lucky**

- …

---

## 8. Timeline

All timestamps UTC, ISO-8601, with the incident's `correlation_id`.

| ts (UTC) | actor | event | source |
|---|---|---|---|
| YYYY-MM-DDTHH:MM:SSZ | … | … | … |

---

## 9. Reviews & sign-off

| Role | Name | Decision | Date |
|---|---|---|---|
| Author | {{name}} | drafted | YYYY-MM-DD |
| SRE Lead (canonical reviewer) | {{name}} | approved / change-requested | YYYY-MM-DD |
| Peer reviewer (outside affected team) | {{name}} | approved | YYYY-MM-DD |
| Security Lead (if security incident) | {{name}} | approved | YYYY-MM-DD |
| Privacy Officer (if privacy incident) | {{name}} | approved | YYYY-MM-DD |
| Final approver | {{name}} | sealed | YYYY-MM-DD |

Two reviewers minimum (SRE lead + one peer outside the affected team).
Authors may not self-certify blamelessness.

---

**End PM-YYYY-MM-DD-NNN.**
