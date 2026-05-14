---
id: "WAIVER-S14-001"
type: "waiver"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
wi: "WI-S14-008"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
gates_waived:
  - "WI-S14-008 Legal externo review D+30 sprint window gate"
compensating_control: "Legal externo review in progress; no enterprise customer DPA signed while active; D+60 GA Evidence Gate hard deadline"
expires_at: "9999-12-31"
revalidation_trigger: "legal_externo_sign_off_letter_committed OR ga_evidence_gate_d60"
rationale: "6-week legal review lead may exceed sprint window; waiver documents accepted residual risk with explicit expiry and revalidation; activate only if D+30 missed"
tags:
  - "waiver"
  - "legal-externo"
  - "dpa"
  - "tia"
  - "timeline"
  - "s14"
  - "compliance"
supersedes: null
superseded_by: null
---

# WAIVER-S14-001 — Legal Externo Review Timeline Exception
## WI-S14-008 · CoreLink S-14

> **Status**: INACTIVE (template only — activate if Legal externo review misses D+30 target).
>
> **IMPORTANT**: Activating this waiver means the DPA and TIA templates are NOT yet legally reviewed by an external qualified law firm. No enterprise customer DPA may be signed while this waiver is active, unless the Legal externo firm provides a partial sign-off on reviewed sections.

---

## 1. Waiver Identification

| Field | Value |
|---|---|
| **ID** | WAIVER-S14-001 |
| **WI** | WI-S14-008 |
| **Sprint** | S-14 |
| **Issued by** | `[OWNER NAME]` |
| **Issue date** | `[DATE — fill on activation]` |
| **Expiry** | 90 days from issue date: `[DATE + 90 DAYS]` |
| **Status** | `[INACTIVE / ACTIVE / RESOLVED]` |

---

## 2. Trigger Condition

**Trigger**: Legal externo review of `legal/dpa-residency-amendment.md` and `legal/tia-template.md` not completed by D+30 of WI-S14-008 sprint window.

**D+30 target date**: `[FILL ON ACTIVATION]`
**Actual completion status at D+30**: `[FILL ON ACTIVATION]`

---

## 3. Compliance Gap Description

| Requirement | Standard | Gap |
|---|---|---|
| DPA template legally reviewed | GDPR Art. 28 · WI-S14-008 §11 | Legal externo review not completed by D+30 |
| TIA legally reviewed | EDPB Recommendations 01/2020 · GDPR Art. 46 | Legal externo review not completed by D+30 |
| Legal Counsel 12th sign-off | WI-S14-008 §30 · sprint.md §14 | Legal externo sign-off letter not yet committed |

**Risk assessment**: HIGH. DPA and TIA templates available but not yet externally verified. Risk is time-bounded (Legal externo review in progress; completion expected D+60).

---

## 4. Compensating Controls

While WAIVER-S14-001 is active, the following controls are in place:

| Control | Description |
|---|---|
| **No enterprise customer DPA signed** | Sales holds all DPA signings pending Legal externo sign-off (or partial sign-off on reviewed sections) |
| **Templates marked PENDING** | `doc_status: PENDING_LEGAL_REVIEW` maintained in DPA + TIA metadata |
| **Legal externo engagement active** | External firm review in progress (engagement started D+0) |
| **Prospect transparency** | Sales informs prospects: "DPA template is in Legal review; expected completion [DATE]" |
| **Internal review only** | Owner + Compliance Officer + Privacy Officer have reviewed templates; gap is external Legal sign-off only |
| **Compliance Officer monitoring** | Weekly status check with Legal externo firm |

---

## 5. Revalidation Triggers

This waiver is resolved (WAIVER-S14-001 status → `RESOLVED`) when **any** of the following occur:

1. **Legal externo sign-off letter committed** to `specs/_audits/[DATE]-legal-externo-review-s14.md` and `doc_status` updated to `LEGAL_REVIEWED` in DPA + TIA templates.
2. **GA Evidence Gate D+60** — hard deadline. If Legal externo review not complete by D+60, escalate to SEV-2: pause GA readiness sign-off for S-14; notify Owner.

**Expiry without resolution**: If waiver expires (90 days) without resolution, escalate to SEV-1 and halt all enterprise DPA engagements until Legal externo review is complete.

---

## 6. Sign-off (activate when waiver is issued)

> Fill this section on activation. Until activated, this template has no legal effect.

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | `[DATE]` | `[pending]` |
| 2 | Final Approver | Gustavo Schneiter | `[DATE]` | `[pending]` |
| 3 | Compliance Officer | `[TBD]` | `[DATE]` | `[pending]` |
| 4 | Privacy Officer | `[TBD]` | `[DATE]` | `[pending]` |
| 5 | Legal Counsel (partial) | `[TBD — in-progress firm]` | `[DATE]` | `[pending]` |

---

## 7. Resolution Record

> Fill on resolution.

| Field | Value |
|---|---|
| **Resolved** | `[DATE]` |
| **Resolution reason** | `[legal_externo_sign_off_committed / ga_d60_gate / other]` |
| **Evidence** | `[path to sign-off letter]` |
| **Resolved by** | `[Owner name]` |

---

## 8. Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo Schneiter (via Claude Sonnet 4.6) | Template created, WI-S14-008. Status: INACTIVE. |

---

*WAIVER-S14-001 · Version 1.0.0 · 2026-05-14 · WI-S14-008.*
