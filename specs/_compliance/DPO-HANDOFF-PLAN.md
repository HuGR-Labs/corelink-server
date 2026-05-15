---
id: "DPO-HANDOFF-PLAN"
type: "compliance_plan"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "GAP-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "DPO-APPOINTMENT-2026-05-15"
  - "DPO-RESPONSIBILITIES-MATRIX"
  - "LGPD-FULL-AUDIT-2026-05-15"
  - "COMPLIANCE-MATRIX"
tags:
  - "lgpd"
  - "lgpd-art-41"
  - "gdpr-art-37"
  - "dpo"
  - "succession"
  - "handoff"
  - "gap-01"
  - "governance"
  - "hiring"
  - "knowledge-transfer"
---

# DPO Handoff Plan — Interim → Permanent (90-day plan)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** 90-day plan to transition CoreLink's DPO role from interim (Gustavo Schneiter, founder) to permanent hire, with search timeline, knowledge-transfer schedule, Board approval gates, and re-signature ceremony for all attestations.
>
> **Trigger:** GAP-01 (`SOC2-EVIDENCE-ROLLUP-2026-05-15.md §5` + `compliance_matrix.md §9` row GAP-01).
> **Hard deadline:** before first EU enterprise tenant contract (GDPR Art. 37 obligation) AND before Q3-2026 (T+1m of GA, per `compliance_matrix.md §9` row GAP-01).
>
> **Companion docs:**
> - `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md` — interim designation (this plan supersedes the interim designation on permanent appointment).
> - `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md` — RACI handed over to permanent DPO.
> - `specs/_runbooks/RB-DPO-ESCALATION.md` — escalation tree handed over.
> - `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` — monthly cadence handed over.

---

## 1. Plan summary

| Phase | Duration | Milestone | Owner |
|---|---|---|---|
| **Phase 0 — Plan ratification** | D-7 (relative to plan start) → D+0 | Board + interim DPO + Advisor approve this plan; budget allocated | Board |
| **Phase 1 — Search kickoff** | D+0 → D+30 | Job description finalised, recruiter engaged, advisor-pool referrals tapped, initial candidate funnel ≥ 10 | Interim DPO + Advisor + Board |
| **Phase 2 — Interview + assessment** | D+15 → D+60 | 3-stage interview process complete; finalist shortlist ≤ 3; reference checks complete | Interim DPO + Advisor + Board |
| **Phase 3 — Offer + start** | D+45 → D+75 | Offer accepted, start date confirmed, NDA + LGPD addendum signed | Board |
| **Phase 4 — Knowledge transfer** | D+60 → D+90 | 30-day overlap, 25-item checklist co-execution, attestation re-signature | Interim DPO + Permanent DPO |
| **Phase 5 — Handover ceremony** | D+90 | Permanent DPO sole signer; interim DPO releases role; ANPD re-registration | Permanent DPO + Board |

> **Total duration:** 90 days from Phase 1 kickoff to Phase 5 ceremony. Phase 0 is preparatory and may extend (recruiter contracts, Board ratification). Phase 4 may compress if permanent DPO ramps faster than 30 days.

---

## 2. Phase 0 — Plan ratification (D-7 → D+0)

**Pre-conditions:**
- Interim DPO appointment is filed with ANPD (per `DPO-APPOINTMENT-2026-05-15.md §7`).
- Advisor pool (R5-8 / H-15) has at least 1 onboarded Privacy advisor.
- Board (interim: Gustavo Schneiter) has reviewed budget for permanent hire (target compensation $120-180k/yr for BR-based DPO; $180-250k/yr for EU-based; or fractional/external retainer $4-8k/month).

**Activities:**
- Interim DPO drafts job description (template in §3.1 below).
- Board ratifies budget envelope.
- Recruiter engaged or referral search initiated.
- This plan signed by Board + Interim DPO + Advisor.

**Exit criteria:** Phase 1 may begin.

---

## 3. Phase 1 — Search kickoff (D+0 → D+30)

### 3.1 Job description template

| Field | Value |
|---|---|
| **Title** | Data Protection Officer (DPO) — Brazil + Global (LGPD + GDPR cross-jurisdiction) |
| **Reports to** | HuGR Labs Board (direct, no operational filtering — see `DPO-APPOINTMENT-2026-05-15.md §2`) |
| **Employment type** | Full-time preferred; fractional / external retainer accepted as bridge if full-time slips |
| **Location** | Brazil (any state); remote acceptable; quarterly in-person Board briefing |
| **Required qualifications** | LGPD certification (e.g., EXIN PDPP-BR, ANPD-recognised CDP, ABPDP-Br); CIPP/E or CIPP/EU; 5+ years privacy / compliance experience; PT-BR fluency + EN business level; familiarity with SaaS / cloud-native architectures |
| **Strongly preferred** | GDPR DPO accreditation; SOC 2 / ISO 27001 cross-framework experience; ANPD direct-handling experience (at least 1 prior ANPD inquiry resolved) |
| **Disqualifying conflicts** | Active role at a CoreLink customer (avoids contractual conflict); active role at a sub-processor in `legal/sub-processors.md` |
| **Compensation range** | $120-180k/yr BR-based · $180-250k/yr EU-based · $4-8k/month external retainer |
| **Start date** | D+45 to D+75 (per this plan) |

### 3.2 Sourcing channels

| Channel | Owner | Notes |
|---|---|---|
| Advisor-pool referrals (R5-8) | Interim DPO | 2 Privacy advisors expected onboarded; each provides 1-2 referrals |
| LinkedIn search (LGPD + DPO keywords) | Recruiter | $5-10k retainer; BR-focused |
| ABPDP-Br membership directory | Interim DPO | Direct outreach to active members |
| ANPD certification registry | Interim DPO | Recent certificate holders (preference for 2024-2025 cohort) |
| EXIN / IAPP credential holders (BR) | Recruiter | Cross-referenced with LinkedIn |

### 3.3 Funnel targets

| Metric | Target by D+30 |
|---|---|
| Inbound candidates (resumes) | ≥ 30 |
| Initial-screen passes | ≥ 10 |
| Hiring-manager screen passes | ≥ 5 |
| Full-loop interview candidates | ≥ 3 |

---

## 4. Phase 2 — Interview + assessment (D+15 → D+60)

### 4.1 Interview loop (3 stages)

| Stage | Interviewer | Duration | Focus |
|---|---|---|---|
| **S1 — Recruiter screen** | Recruiter | 30min | Credentials + compensation + start-date fit |
| **S2 — Interim DPO + Advisor screen** | Interim DPO + 1 Advisor | 90min | LGPD substance, conflict-of-interest screen, DSR pipeline walkthrough |
| **S3 — Board interview** | Board (interim: founder) + 2nd Advisor | 60min | Reporting line, independence stance, ANPD posture |
| **S4 — Technical deep-dive** | Interim DPO + Security Lead | 60min | DPIA authoring sample, breach declaration role-play, sub-processor risk assessment |
| **Practical assessment** | Take-home (8h max) | async | Author a mock DPIA on a CoreLink WI (anonymised); draft an ANPD response letter |

### 4.2 Reference checks (mandatory before offer)

| Reference type | Minimum count |
|---|---|
| Direct manager (last 2 roles) | 2 |
| Privacy peer (prior DPO or CPO) | 1 |
| Regulatory contact (ANPD or peer DPA — if candidate has handled inquiries) | 1 (if applicable) |
| Legal counsel reference (prior controller's external counsel) | 1 |

### 4.3 Decision gate

**Approval requires:**
- Interim DPO (C) — substantive endorsement, no veto.
- 2 Advisors (C) — convergent endorsement.
- Board (A) — final sign-off.

**Disqualifying signals during interview:**
- Candidate proposes to combine DPO with active operational role at HuGR (re-introduces conflict of interest documented in `DPO-APPOINTMENT-2026-05-15.md §4`).
- Candidate's reference check surfaces unresolved ANPD complaint against prior controller.
- Candidate has active equity / advisory position at a CoreLink customer or sub-processor.

---

## 5. Phase 3 — Offer + start (D+45 → D+75)

### 5.1 Offer package

| Component | Required content |
|---|---|
| Employment contract (CLT or PJ) | Standard HuGR contract + privacy-governance clause §11 (independence safeguards mirroring `DPO-APPOINTMENT-2026-05-15.md §3`) |
| NDA | Standard HuGR NDA + LGPD addendum (data-handling obligations even post-termination) |
| LGPD addendum | DPO-specific clause: cannot be dismissed for performing DPO duties (GDPR Art. 38(3) + ANPD Resolução 18/2024 §3.III) |
| Reporting-line letter | Confirms direct Board reporting (no operational filter) |
| Comp + equity grant | Per Board-ratified envelope |

### 5.2 Pre-start activities

| Activity | Owner | Target |
|---|---|---|
| Background check (criminal + financial, per BR labor norms) | Recruiter / HR | D+50 |
| Reference check archive in `legal/dpo-hire-evidence/` | Interim DPO | D+55 |
| NDA + LGPD addendum signed | Permanent DPO + Board | D+60 |
| Onboarding access provisioned (LGPD-Drata, Vanta, internal Notion / docs, R2 buckets read-only) | Interim DPO + SRE Lead | D+75 |
| ANPD pre-registration paperwork drafted (update to `DPO-APPOINTMENT-2026-05-15.md §7`) | Interim DPO + Legal | D+75 |

---

## 6. Phase 4 — Knowledge transfer (D+60 → D+90)

### 6.1 30-day overlap structure

| Week | Theme | Interim DPO role | Permanent DPO role |
|---|---|---|---|
| W1 (D+60..D+67) | Org + roles + responsibilities | Lead orientation | Observe + read inheritance docs |
| W2 (D+67..D+74) | DSR pipeline + consent ledger | Lead 2 live DSRs | Shadow + co-sign 1 DSR |
| W3 (D+74..D+81) | Breach + ANPD handling | Lead tabletop dry-run (`IR-TABLETOP-SCHEDULE-2026.md`) | Lead 1 tabletop dry-run |
| W4 (D+81..D+88) | Attestation refresh + Board briefing | Co-author 90-day attestation refresh | Lead Board briefing + countersign attestation |
| W5 (D+88..D+90) | Ceremony + handover | Release role | Take role (see §7 ceremony) |

### 6.2 Mandatory knowledge-transfer reading list (in order)

1. `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md` (this interim designation — will be superseded)
2. `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md`
3. `specs/_runbooks/RB-DPO-ESCALATION.md`
4. `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`
5. `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md`
6. `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`
7. `specs/_compliance/LGPD-ROPA-2026-05-15.md`
8. `specs/03_architecture/privacy_model.md`
9. `specs/03_architecture/compliance_matrix.md §4 + §9`
10. `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
11. `specs/_runbooks/RB-DSR-TICKET-TRIAGE.md` + `RB-DSR-LGPD-FULL.md`
12. `legal/sub-processors.md` + `specs/_compliance/VENDOR-RISK-REGISTER.md`
13. `legal/dpia/` (template + 2 completed RIPDs)
14. `legal/lia/` (LIA documents)
15. `legal/breach-notification/` (templates + ANPD contacts)
16. `legal/privacy-notice/v*.{pt-BR,en-US,es-MX}.md`
17. `legal/dpa/` (canonical DPA + amendments)
18. `crates/corelink-privacy-*` (high-level architecture; read code comments + module docstrings)
19. `crates/corelink-dsr/` (DSR pipeline)
20. `specs/_compliance/AUDITOR-WALKTHROUGH-SCRIPT.md` (SOC 2 walkthrough)

### 6.3 Co-execution checklist

The permanent DPO must co-execute (with interim DPO observing) at least one full instance of each of the following before sole sign-off:

| Activity | Source | Co-execution minimum |
|---|---|---|
| Monthly LGPD DPO checklist (25 items) | `LGPD-DPO-MONTHLY-CHECKLIST.md` | 1 full month cycle |
| DSR Access fulfillment (non-destructive) | `RB-DSR-TICKET-TRIAGE.md` | 1 live ticket |
| DSR Erasure approval (destructive) | `RB-DSR-LGPD-FULL.md` | 1 live ticket OR 1 tabletop scenario |
| Breach declaration tabletop | `IR-TABLETOP-SCHEDULE-2026.md` (privacy track) | 1 scenario |
| Sub-processor onboarding review | `VENDOR-RISK-METHODOLOGY.md` + `legal/sub-processors.md` | 1 vendor (real or scenario) |
| DPIA / RIPD authoring | `legal/dpia/template.md` | 1 mock or live DPIA |
| ANPD outbound communication drafting | `legal/breach-notification/anpd-contacts.md` | 1 mock letter |
| Audit-trail spot-check (50 records) | `LGPD-DPO-MONTHLY-CHECKLIST.md §C` | 1 cycle |
| Quarterly Board briefing | `DPO-APPOINTMENT-2026-05-15.md §2` | 1 cycle |

---

## 7. Phase 5 — Handover ceremony (D+90)

### 7.1 Ceremony agenda (90-minute Board meeting)

1. Interim DPO confirms knowledge-transfer completion (per §6.3 checklist).
2. Permanent DPO confirms acceptance of role (signed).
3. Board approves permanent appointment (resolution recorded).
4. **Attestation re-signature ceremony** (in order):
   - `LGPD-RESIDENCY-ATTESTATION-{current-date}.md` — re-signed with permanent DPO + Security Lead + Compliance (three distinct individuals where possible).
   - `LGPD-FULL-AUDIT-{current-date}.md` — re-signed.
   - `SOC2-EVIDENCE-ROLLUP-{current-date}.md` — re-signed.
   - `DPO-APPOINTMENT-{new-date}.md` — created, supersedes `DPO-APPOINTMENT-2026-05-15.md`.
5. Public publication of permanent DPO (privacy notice + `apps/docs/.../lgpd-full.mdx`).
6. ANPD re-registration filed within 5 BD post-ceremony (per `DPO-APPOINTMENT-2026-05-15.md §7.4`).
7. GAP-01 closed in `SOC2-EVIDENCE-ROLLUP` + `compliance_matrix.md §9`.

### 7.2 Post-ceremony actions

| Action | Owner | Deadline |
|---|---|---|
| Update privacy notice (all locales) | Permanent DPO + Legal | D+95 |
| Update `apps/docs/docs/explanation/privacy/lgpd-full.mdx` | Permanent DPO | D+95 |
| Update `DPO-APPOINTMENT-{new-date}.md` and supersede interim doc | Permanent DPO | D+90 (ceremony day) |
| ANPD re-registration submitted | Permanent DPO | D+95 |
| Quarterly Board briefing rhythm established | Permanent DPO | D+90 (first briefing scheduled D+180) |
| Close GAP-01 in `SOC2-EVIDENCE-ROLLUP` | Compliance / Final approver | D+95 |

---

## 8. Risks + mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Permanent DPO search exceeds 90 days | M | M | External-retainer DPO (fractional, e.g., from advisor pool R5-8) as bridge; ANPD accepts external DPO per Resolução 18/2024 |
| Selected candidate withdraws after offer | M | M | Maintain backup finalist for 30 days; re-open search if backup unavailable |
| Permanent DPO's prior conflict surfaces in reference check post-offer | L | H | Mandatory ANPD-history check in §4.2; offer is contingent on clear references |
| ANPD inquiry lands during handoff window | M | H | Interim DPO retains accountability through D+90 (overlap period); permanent DPO observes |
| EU enterprise tenant signs LOI before D+90 | M | H | Accelerate Phase 4 to 14-day overlap; permanent DPO may take sole sign-off on GDPR Art. 37 designation immediately, with LGPD ceremony catching up |
| Permanent DPO discovers material undisclosed risk in CoreLink controls during overlap | L | M | Risk filed in `LGPD-FULL-AUDIT` §4 residual register; corrective action plan ratified by Board before D+90 |
| Founder (interim DPO) cannot release role due to single-signer concentration | M | M | Board (formally a separate body even when populated by same individual) ratifies release; advisor countersign on first 90 days of permanent DPO solo signing |

---

## 9. Budget envelope (Board-ratified)

| Line item | Range | Notes |
|---|---|---|
| Recruiter retainer | $5-10k (one-time) | If used; advisor referrals may eliminate |
| Background check | $300-500 | Per BR labor norms |
| Permanent DPO compensation | $120-250k/yr | Range varies by location + full-time vs fractional |
| LGPD/CIPP certification reimbursement | $2-5k/yr | DPO training budget |
| Legal review of contract (H-11) | $3-5k (one-time) | Privacy-governance clause |
| ANPD re-registration | $0 | Free online form |
| **Total Year 1** | **$130-275k** | Aligns with H-15 advisor-pool budget in `ROADMAP-TO-GA.md §9` |

---

## 10. Cross-references

- **Interim appointment (supersedee at D+90):** `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md`
- **Responsibilities + RACI (carried forward):** `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md`
- **Escalation runbook (carried forward):** `specs/_runbooks/RB-DPO-ESCALATION.md`
- **Monthly checklist (carried forward):** `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`
- **LGPD full audit:** `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md §1.12`
- **Residency attestation (re-signed at D+90):** `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md §7`
- **SOC 2 evidence rollup (re-signed at D+90):** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **Compliance matrix §9 GAP-01 row:** `specs/03_architecture/compliance_matrix.md`
- **ROADMAP-TO-GA §9 H-15 advisor pool / DPO hire:** `ROADMAP-TO-GA.md`
- **Tabletop schedule (privacy track):** `specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md`
- **Vendor risk register:** `specs/_compliance/VENDOR-RISK-REGISTER.md`

---

## 11. Sign-off

| Role | Name | Signature | Date |
|---|---|---|---|
| Interim DPO | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Board (chair) | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| External Privacy Advisor (R5-8) | _pending advisor onboarding_ | `__________________` | _2026-__-__ |
| Compliance / Final approver | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |

---

**Fim de DPO-HANDOFF-PLAN.** This plan is executed once; on completion at Phase 5 ceremony, this doc is archived with status `DONE` and the new `DPO-APPOINTMENT-{YYYY-MM-DD}.md` becomes canonical.
