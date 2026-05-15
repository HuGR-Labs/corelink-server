---
id: "ISO27001-INTERNAL-AUDIT-PROGRAM-2026-05-15"
type: "compliance_internal_audit_program"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-DEBT-012-ISO27001-GAP-CLOSURE"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "ISO27001-CROSSWALK-2026-05-15"
  - "ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15"
  - "ISO27001-ROADMAP-2026-05-15"
tags:
  - "iso27001"
  - "iso27001-2022"
  - "clause-9-2"
  - "internal-audit"
  - "isms"
  - "r-prep"
---

# ISO/IEC 27001:2022 — Internal Audit Program (Clause 9.2)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** define the internal audit programme required by ISO/IEC 27001:2022 Clause 9.2.2 — covering frequency, scope, methods, responsibilities, and reporting for the CoreLink ISMS. This document is the **`audit programme`** artifact the certification body will request at Stage 1; the **`audit results`** artifact (first-cycle internal-audit report) is produced at milestone M2 per `ISO27001-ROADMAP.md`.

## 1. Header

| Field | Value |
|---|---|
| Standard reference | **ISO/IEC 27001:2022 Clause 9.2** (Internal audit) + Clause 9.2.2 (Internal audit programme) |
| Related guidance | ISO/IEC 27007:2020 (guidelines for ISMS auditing) + ISO 19011:2018 (guidelines for auditing management systems) |
| ISMS scope | per `ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md §1` |
| Programme owner | Compliance Officer (Gustavo Schneiter interim) |
| Audit independence | Internal auditor must not audit their own work (Clause 9.2.2 c). For solo-founder phase, advisor pool member (`legal/legal-externo-engagement-contract.md`, e.g., CIPP-E / CISA-credentialed advisor) executes the internal audit; Owner cannot self-audit operational areas they own. |
| Programme cadence | **monthly process audits** + **quarterly control audits** + **annual full ISMS review** |
| First cycle | T+3m .. T+4m (per `ISO27001-ROADMAP.md` M2) — feeds Stage 1 audit Q4-2026 |
| Reporting destination | Management review (Clause 9.3) — per `ISO27001-MANAGEMENT-REVIEW-TEMPLATE.md` |
| Retention | Internal audit reports + corrective action register: 7 years in R2 Object Lock Governance Mode (CTRL-AUDIT-005) |

## 2. Programme objectives (Clause 9.2.1)

The internal audit programme verifies that the ISMS:

1. **Conforms** to the organization's own requirements (policies, procedures, runbooks) and to ISO/IEC 27001:2022.
2. **Is effectively implemented and maintained** — controls in the SoA (`ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md`) operate as designed; evidence streams produce auditor-grade artifacts.
3. **Surfaces non-conformities** before the certification body does — so corrective actions close inside the management-review cycle, not as Stage 1/Stage 2 audit findings.

## 3. Cadence (Clause 9.2.2 a — frequency and methods)

### 3.1 Monthly process audits (rotating thematic)

| Month | Theme | Scope (controls sampled) | Method |
|---|---|---|---|
| Jan | Access control + identity | A.5.15 · A.5.16 · A.5.17 · A.5.18 · A.6.7 · A.8.2 · A.8.3 · A.8.5 | Sample 10 user / PAT records · verify MFA freshness · verify RBAC scope match per actual API trace |
| Feb | Cryptography + BYOK | A.5.14 · A.5.34 · A.8.11 · A.8.12 · A.8.24 | BYOK kill-switch drill review + envelope-encryption attestation per provider |
| Mar | Supplier + 3rd-party risk | A.5.19 · A.5.20 · A.5.21 · A.5.22 · A.5.23 · A.6.6 | Sample 5 of 19 vendors against `VENDOR-RISK-METHODOLOGY.md`; verify SOC 2 refresh + DPA + sub-processor change-notification |
| Apr | Incident management + IR | A.5.24 · A.5.25 · A.5.26 · A.5.27 · A.6.8 | Review previous-quarter incidents + postmortems + IR-tabletop execution log |
| May | BC / DR + redundancy | A.5.29 · A.5.30 · A.8.13 · A.8.14 | Review DR-15 cold-restore + DR-16 active-failover + BIA per `BUSINESS-IMPACT-ANALYSIS.md` (post-GAP-ISO-04 close) |
| Jun | Asset management + classification | A.5.9 · A.5.10 · A.5.11 · A.5.12 · A.5.13 · A.5.32 · A.5.33 | Walk the formal asset register (post-GAP-ISO-01 close) + sample 5 information assets for labelling correctness |
| Jul | People + HR security | A.6.1 · A.6.2 · A.6.3 · A.6.4 · A.6.5 · A.7.7 | Sample 100% of onboarding / offboarding events YTD; verify AUP + offboarding checklist signatures |
| Aug | Network + segregation | A.5.23 · A.8.20 · A.8.21 · A.8.22 · A.8.23 | Sample per-tenant DO routing + R2 bucket policy + WAF ruleset version; verify segregation under load |
| Sep | Logging + monitoring + audit chain | A.5.28 · A.8.15 · A.8.16 · A.8.17 | Verify Merkle-chain integrity proof + Prometheus metric inventory + clock-drift alerts |
| Oct | Secure development + supply chain | A.5.21 · A.8.4 · A.8.7 · A.8.25 · A.8.26 · A.8.27 · A.8.28 · A.8.29 | Sample 5 deploys + verify Cosign signature + SBOM + reproducible build hash |
| Nov | Change management + config | A.5.7 · A.8.8 · A.8.9 · A.8.19 · A.8.31 · A.8.32 | Sample 10 PRs + verify branch protection + dual-approval for destructive ops |
| Dec | Privacy + DSR + records | A.5.34 · A.7.10 · A.7.14 · A.8.10 · A.8.11 · A.8.33 | Sample 5 DSR requests YTD; verify erasure attestation; verify test-information non-production policy |

> Monthly audits each consume ~0.5 person-day. The advisor pool member rotates themes; auditor signs off in the corrective-action register before the next month opens.

### 3.2 Quarterly control audits (deep dive)

Quarterly audits perform a deep evidence review on a **rotating quarter** of the SoA, ensuring **all 90 applicable Annex A controls are deep-audited at least once per certification cycle (3 years)**.

| Quarter | Annex A theme(s) deep-audited | Approximate control count |
|---|---|---|
| Q1 | A.5 Organizational (first half: A.5.1–A.5.18) | 18 controls |
| Q2 | A.5 Organizational (second half: A.5.19–A.5.37) + A.6 People | 19 + 8 = 27 controls |
| Q3 | A.7 Physical (in-scope rows only) + A.8 Technological (first half: A.8.1–A.8.17) | 12 + 17 = 29 controls |
| Q4 | A.8 Technological (second half: A.8.18–A.8.34, excluding A.8.30) | 16 controls |

> Quarterly deep audits each consume ~3 person-days. Output: per-control evidence-sufficiency rating (Sufficient / Insufficient / Missing) + remediation tickets for Insufficient/Missing rows.

### 3.3 Annual full ISMS review (Clause 9.2.1)

Once per year (Q1 of each calendar year, aligned with surveillance audit cycle post-certification):

- **Scope:** all 90 applicable controls + 4 mandatory clauses (4 Context · 5 Leadership · 6 Planning · 7 Support · 8 Operation · 9 Performance · 10 Improvement).
- **Inputs:**
  - SoA delta vs prior year (new controls / changed applicability / changed status).
  - All 12 monthly process audit reports + 4 quarterly control audit reports from the prior year.
  - Corrective-action register status (open / closed / overdue).
  - Risk-treatment plan updates (Clause 6.1.2 + 6.1.3).
  - External pentest + SOC 2 audit reports (cross-feed evidence).
- **Output:** annual ISMS audit report + recommendation memo to management review (Clause 9.3).
- **Duration:** ~10 person-days across 3-4 weeks.

## 4. Roles and responsibilities (Clause 9.2.2 d)

| Role | Responsibility | Independence requirement |
|---|---|---|
| Programme owner (Compliance Officer) | Maintains the audit programme + schedules audits + closes corrective actions in the register. | Cannot audit areas they personally own operationally (e.g., cannot audit own Compliance Officer documentation work). |
| Internal auditor (advisor pool member) | Executes monthly / quarterly / annual audits per scope. | Must be independent of the activity audited (Clause 9.2.2 c). Solo-founder phase: advisor pool member with CISA or equivalent. Post-headcount-growth: dedicated internal auditor role. |
| Auditee (control owner per SoA) | Provides evidence, makes systems / records available, signs off corrective actions. | — |
| Management review forum | Receives consolidated audit outputs at quarterly + annual review meetings per `ISO27001-MANAGEMENT-REVIEW-TEMPLATE.md`. | — |

## 5. Audit method (Clause 9.2.2 b)

For every audit (monthly / quarterly / annual):

1. **Plan** — define scope (controls × time window), criteria (SoA + procedures + runbooks), and sampling strategy.
2. **Notify auditee** — at least 5 business days in advance for monthly; 10 business days for quarterly; 20 business days for annual.
3. **Execute** — collect evidence via:
   - Drata continuous-compliance evidence stream (auto: 90.7% per `SOC2-EVIDENCE-ROLLUP-2026-05-15.md §4.1`).
   - Manual sampling per `RB-*.md` runbook checklists.
   - Interview with control owner (≥ 1 per quarterly + annual audit).
4. **Document findings** — per-finding record: control ID + observation + evidence + classification (Conformity / Minor non-conformity / Major non-conformity / Opportunity for improvement).
5. **Issue report** — within 5 business days of audit close.
6. **Track corrective actions** — log in corrective-action register; SLA per severity:
   - Major NC: 30 calendar days root-cause + corrective action.
   - Minor NC: 60 calendar days root-cause + corrective action.
   - Opportunity: tracked in backlog; no hard SLA.
7. **Verify closure** — auditor confirms evidence of corrective-action effectiveness before closing in register.

## 6. Reporting (Clause 9.2.2 e)

Internal audit results are reported to:

1. **Compliance Officer** — immediately upon report issuance.
2. **Quarterly management review** — consolidated quarterly digest of all audits in the period (per `ISO27001-MANAGEMENT-REVIEW-TEMPLATE.md §3`).
3. **Annual management review** — full ISMS audit report + corrective-action register status.
4. **External certification body** — on request, during Stage 1 / Stage 2 / surveillance / recertification audits.

## 7. Records retention (Clause 9.2.2 f)

| Record | Retention | Storage |
|---|---|---|
| Monthly process audit reports | 7 years | R2 Object Lock Governance Mode (per CTRL-AUDIT-005) |
| Quarterly control audit reports | 7 years | R2 Object Lock Governance Mode |
| Annual ISMS review reports | 7 years (rolling; 1 active + 6 historical) | R2 Object Lock Governance Mode |
| Corrective-action register | Active + 7y after closure | Drata register + R2 archive |
| Audit programme (this doc) | Active version + 7y after supersession | Git VCS (`specs/_compliance/`) |

## 8. First-cycle execution plan (feeds Stage 1 audit Q4-2026)

| Step | Window | Owner | Deliverable |
|---|---|---|---|
| 1. Programme finalization | T+0 (this doc) | Compliance Officer | This document signed off by Owner. |
| 2. Advisor-pool internal auditor engagement | T+0 .. T+1m | Compliance Officer + Legal | Engagement contract per `legal/legal-externo-engagement-contract.md` with CISA / CIPP-E advisor. |
| 3. First monthly process audit (June 2026 — asset management theme) | T+1m | Internal auditor | Audit report #1 (J2026-PRA-A.5.9/.5.10/.5.11/.5.12/.5.13/.5.32/.5.33). |
| 4. First monthly process audit (July — people + HR security) | T+2m | Internal auditor | Audit report #2. |
| 5. First quarterly deep audit (Q3-2026 — A.7 Physical in-scope + A.8 first half) | T+3m | Internal auditor | Audit report Q3-2026 (29 controls). |
| 6. Quarterly management review #1 | T+4m | Owner + Compliance Officer | Per `ISO27001-MANAGEMENT-REVIEW-TEMPLATE.md`. |
| 7. Annual ISMS review #0 (gap-mode; full readiness for Stage 1) | T+5m | Internal auditor + Compliance Officer | Annual report fed into Stage 1 documentation review (Q4-2026). |

## 9. Cross-references

- **SoA (93 Annex A rows + 3 exclusions):** `specs/_compliance/ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md`
- **Management review template (Clause 9.3):** `specs/_compliance/ISO27001-MANAGEMENT-REVIEW-TEMPLATE.md`
- **Crosswalk (control-by-control evidence pointers):** `specs/_compliance/ISO27001-CROSSWALK-2026-05-15.md`
- **Gap analysis (7 ISO-unique + 1 informational PIMS):** `specs/_compliance/ISO27001-GAP-ANALYSIS.md`
- **Roadmap to Q1-2027 cert (M2 + M3 align with this programme):** `specs/_compliance/ISO27001-ROADMAP.md`
- **SOC 2 evidence rollup (shared Drata streams):** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- **Advisor pool engagement template:** `legal/legal-externo-engagement-contract.md`

---

## 10. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7 DEBT-012 closure agent) | Initial internal audit programme satisfying ISO/IEC 27001:2022 Clause 9.2.2. Defines monthly process audits (12 themes covering all 90 in-scope Annex A controls per year) + quarterly control audits (deep evidence review) + annual ISMS review. Aligns with ISO 27007:2020 + ISO 19011:2018 guidance. First cycle T+1m .. T+5m feeds Stage 1 audit Q4-2026 per `ISO27001-ROADMAP.md` M2. |

---

**Fim ISO27001-INTERNAL-AUDIT-PROGRAM.**
