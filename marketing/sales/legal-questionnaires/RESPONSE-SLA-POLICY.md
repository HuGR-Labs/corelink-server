---
id: "SALES-RESPONSE-SLA-POLICY"
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
parent: "R-PREP-SALES-ENABLEMENT"
tags: ["sales", "legal", "questionnaire", "sla", "response-time", "procurement", "r-prep", "ga"]
---

# Pre-Sales Legal Response SLA Policy

> **Audience:** enterprise procurement teams (external) and CoreLink CS / SE (internal). This is the public commitment for how long CoreLink takes to respond to SIG / CAIQ / vendor questionnaires + the internal scheduling discipline that supports it.
>
> **Posture:** we publish a hard target *and* an internal stretch goal. We track every response against both and publish a quarterly "responses-shipped vs SLA-met" metric in the weekly compliance digest (`specs/_compliance/weekly-digests/`).

---

## 1. Public turnaround commitments

CoreLink commits to the following maximum response windows, measured from **countersigned NDA on file** to **bundled response delivered**:

| Response type | Public SLA | Internal stretch goal | Conditions |
|---|---|---|---|
| **SIG Lite** (Shared Assessments 2026) | **5 business days** | 3 business days | Standard 14-category questionnaire; ≤ 130 questions |
| **SIG Full** (Shared Assessments 2026) | **10 business days** | 7 business days | Extended questionnaire; > 1,000 questions; includes deep-dive controls |
| **CSA CAIQ v4.0.x** | **7 business days** | 5 business days | 197 questions across 17 CCM v4 domains |
| **Custom vendor questionnaire** (≤ 30 questions) | **3 business days** | 2 business days | Short-form; fold answer-key into cover letter |
| **Custom vendor questionnaire** (31–80 questions) | **5 business days** | 4 business days | Standard four-part response (cover + answer + evidence + DPA) |
| **Custom vendor questionnaire** (81+ questions) | **10 business days** | 7 business days | Large form; triage by domain; lean on canonical phrasings |
| **Auditor / 3PAO evidence-pack request** | **2 business days** | 1 business day | NDA on file with engaged auditor; bundle per `EVIDENCE-PACK-INDEX.md` §2.4 |
| **Single ad-hoc question** (procurement clarification) | **1 business day** | Same business day | Email reply via `trust@corelink.dev` |
| **DPA redline** | **5 business days** | 3 business days | Standard CoreLink DPA `v1.0.0`; external counsel routes deviations > ±15% |
| **Sub-processor change-notice acknowledgement** | **30 calendar days advance** | n/a | Per DPA §6 + GDPR Art. 28 §2 + LGPD Art. 27 §4º — this is the customer-facing commitment, not internal SLA |

**Business day definition:** Monday–Friday, excluding US federal holidays + Brazilian national holidays (`feriados nacionais`) since DPO + Founder both observe both calendars. Public holidays add to the SLA on a same-day-of-week basis (i.e. a US holiday inside the window adds 1 business day).

---

## 2. What starts the clock

The SLA window starts when **all** of the following are true:

1. Countersigned NDA on file with CoreLink Legal (`legal@corelink.dev`).
2. Questionnaire received via `trust@corelink.dev` (procurement-routing alias).
3. Questionnaire is **complete** — i.e. all prospect-side metadata cells (vendor name, scope, contact, date, framework version) are filled. Incomplete forms get a 1-business-day clarification round-trip that pauses the SLA.
4. If the form requires a vendor portal upload, the portal credentials and access have been provisioned to a CoreLink response account.

The clock pauses if:

- Prospect changes the scope mid-response (new framework, new questions, new module).
- Prospect requests a custom artifact not in `EVIDENCE-PACK-INDEX.md` (extension allowed for one round).
- A material clarification is needed from prospect (e.g. PHI scoping question for a build-cache vendor).

The clock **does not** pause for:

- CoreLink staffing constraints.
- Auditor (Schellman) scheduling.
- Sub-processor evidence refresh cycles.
- Internal compliance digest cadence.

If we miss the public SLA, we publish the slip in the weekly digest with root-cause analysis. This is a hard accountability commitment.

---

## 3. How we hit these numbers — internal mechanics

### 3.1 Pre-staged response inventory

The five docs under `marketing/sales/legal-questionnaires/` are pre-staged answer banks. SIG Lite + CAIQ v4 are populated cell-by-cell; vendor template + evidence index are reusable scaffolding. **A SIG Lite response is a copy-paste-plus-watermark exercise, not a write-from-scratch exercise.**

Mean response time targets (internal, per response):

| Step | Target time |
|---|---|
| Triage form against pre-staged templates | 30 min |
| Identify questions outside standard scope | 30 min |
| Lift canonical phrasings into prospect's form | 1.5 h |
| Bundle evidence pack per `EVIDENCE-PACK-INDEX.md` | 1 h |
| Watermark + sign cover letter | 30 min |
| QA cross-check (commit-SHA cite, link resolution, no overstatement) | 1 h |
| Send + log in CRM | 15 min |
| **Total per SIG Lite response** | **≤ 4.75 h** |

If a SIG Lite response takes more than 6 hours, it is escalated to DPO for root-cause analysis (either the form deviates from standard SIG, or our canonical phrasings need a refresh).

### 3.2 RACI for response cycle

| Role | Responsibility |
|---|---|
| **CS / SE owner** (R) | First-line response drafting; CRM logging; prospect comms |
| **DPO** (A) | Approval to release any compliance claim; redline arbiter on custom forms |
| **Founder** (A) | Final sign-off on cover letter; redline arbiter on DPA |
| **Legal Counsel** (C) | DPA redlines; engagement-letter additions |
| **Security Lead** (C) | Technical-claim accuracy review; auditor walkthrough lead |
| **External Counsel** (C) | DPA deviations > ±15%; jurisdiction conflicts |
| **Prospect Procurement** (I) | Notified of submission + SLA estimate |

### 3.3 Weekly cadence

Every Friday, CS reviews the response queue against SLA:

- Responses in window: counted.
- Responses approaching SLA cutoff (≤ 1 business day): escalated.
- Responses overdue: post-mortem in next compliance digest.

The weekly compliance digest (`specs/_compliance/weekly-digests/`) includes a "Responses shipped this week" section: count, average turnaround, SLA hit-rate.

---

## 4. Special-case responses

### 4.1 Federal / public-sector tender

Federal and EU public-sector tenders often have hard submission deadlines (e.g. UK Crown Commercial Service, US GSA, Brazilian governo federal). For these:

- CoreLink treats the deadline as the **maximum** SLA; we will work to deliver 2 business days ahead.
- If the tender contains FedRAMP or HITRUST requirements, our response leads with the canonical decline (per `VENDOR-QUESTIONNAIRE-RESPONSE-TEMPLATE.md` §"Common custom-form patterns" Pattern C). No softening.
- For SOC 2 evidence requirements: we deliver the readiness rollup under NDA + the roadmap; we do not pretend a Type I report is in hand when it isn't.

### 4.2 Audit / 3PAO walkthrough

When an engaged 3PAO requests a walkthrough (typically a 5-day SOC 2 or ISO Stage 1 / Stage 2 engagement):

- 2-business-day SLA for evidence-pack delivery (per §1).
- Walkthrough scheduled within 5 business days of pack delivery.
- DPO + Security Lead block calendar for the walkthrough week.
- `specs/_compliance/AUDITOR-WALKTHROUGH-SCRIPT.md` is the single source for the walkthrough flow.

### 4.3 Lighthouse customer + their downstream auditor

Lighthouse customers (per `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`) get fast-track:

- Same-day acknowledgement on procurement / audit inquiries routed via their CS contact.
- 2-business-day evidence-pack turnaround.
- Direct DPO + Founder access for material questions.

### 4.4 Existing tenant — annual re-attestation cycle

For existing enterprise tenants doing their annual vendor recertification:

- Bundle the public Trust Center snapshot + DPA + sub-processor list as a "no-changes-since-last-year" package within 2 business days.
- If material changes did occur, full SIG / CAIQ refresh per standard SLA.
- Sub-processor changes are pushed proactively via the 30-day advance-notice channel; tenants should not need to ask.

---

## 5. What CoreLink will *not* commit to

To set expectations honestly:

- **24-hour SIG Full turnaround.** Some prospects request this. SIG Full is > 1,000 questions; 24 hours is not realistic and we will state so.
- **Custom-questionnaire pre-fill before NDA.** We will not share artifact-level evidence before NDA is on file. Public Trust Center is the pre-NDA reference.
- **Bespoke control coverage that we don't actually have.** If a question maps to a control we don't have, we say so. The canonical phrasings (HIPAA out-of-scope, FedRAMP not pursued) are non-negotiable.
- **Auditor walkthroughs with prospects who are not engaged auditors.** Procurement teams get the documented rollup; live walkthroughs are for 3PAO / engaged auditor scope only.
- **SLA on questionnaires received outside the `trust@corelink.dev` channel.** If a prospect's BD lead emails a SIG Lite to a personal CoreLink address, the SLA clock starts only when the email is forwarded to `trust@corelink.dev`. This routing discipline preserves audit trail.

---

## 6. Escalation path

If a prospect believes their response is overdue or insufficient:

| Step | Contact | Window |
|---|---|---|
| 1. Status check | `trust@corelink.dev` (auto-acknowledged 1 business day) | Day 0 |
| 2. Escalate to DPO | `dpo@corelink.dev` (cc `trust@corelink.dev`) | Day +1 |
| 3. Escalate to Founder | `gustavo@humangr.com` (cc DPO + `trust@corelink.dev`) | Day +2 |

Escalation reasons that justify Founder-level: SLA miss with no posted root-cause; material disagreement on a compliance claim; suspected contractual conflict (DPA + their template); engagement-blocking deadline.

---

## 7. Public SLA published locations

This policy is referenced from:

- `apps/docs/docs/trust/index.mdx` §Contact (link to this file via post-GA Trust Center publish).
- `marketing/sales/FAQ-MASTER.md` §Compliance (linked from C1 / C2 / C3 / C4 / C6).
- `marketing/sales/legal-questionnaires/SIG-LITE-2026-pre-filled.md` §How to use this file.
- `marketing/sales/legal-questionnaires/CAIQ-V4-pre-filled.md` §How to use.
- `marketing/sales/legal-questionnaires/VENDOR-QUESTIONNAIRE-RESPONSE-TEMPLATE.md` §Pre-flight checklist.
- `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` §Enterprise BYOK paths.

---

## 8. Tracking + accountability

Weekly digest fields (added to `specs/_compliance/weekly-digests/` schema):

- `responses.shipped_this_week` — count.
- `responses.average_turnaround_business_days` — float.
- `responses.sla_hit_rate` — float, 0–1.
- `responses.overdue_open` — count.
- `responses.escalated_to_founder` — count.

Quarterly review:

- Hit-rate trend chart (target ≥ 95% per quarter).
- Slip post-mortems (any miss with root-cause).
- SLA target adjustment proposal (if hit-rate consistently > 99% — tighten the public SLA).

---

## 9. Change log

| Version | Date | Author | Notes |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo Schneiter | Initial publish covering SIG Lite (5d) / SIG Full (10d) / CAIQ v4 (7d) + custom-form tiers + auditor walkthrough fast-track. |

---

## Related

- `marketing/sales/legal-questionnaires/SIG-LITE-2026-pre-filled.md`
- `marketing/sales/legal-questionnaires/CAIQ-V4-pre-filled.md`
- `marketing/sales/legal-questionnaires/VENDOR-QUESTIONNAIRE-RESPONSE-TEMPLATE.md`
- `marketing/sales/legal-questionnaires/EVIDENCE-PACK-INDEX.md`
- `marketing/sales/FAQ-MASTER.md` §Compliance
- `apps/docs/docs/trust/` — public trust center
- `specs/_compliance/weekly-digests/` — accountability tracking
- `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` §Enterprise tier

## Contact

| What you need | Where to send it |
|---|---|
| SIG / CAIQ / vendor-questionnaire submission | `trust@corelink.dev` |
| SLA escalation | `dpo@corelink.dev` |
| Founder escalation | `gustavo@humangr.com` |
| Privacy / DSR | `privacy@corelink.dev` |
| Security vulnerability report | `security@corelink.dev` |
