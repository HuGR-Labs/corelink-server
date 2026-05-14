---
id: "DPIA-TEMPLATE"
type: "template"
doc_status: "FROZEN"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Privacy Officer"
tags: ["dpia", "gdpr-art-35", "lgpd-art-38", "template"]
evidence_event: "EVT-045"
evidence_retain: "7y"
---

# Data Protection Impact Assessment (DPIA) Template
### GDPR Art. 35 / LGPD Art. 38 (RIPD) Unified

> **How to use:** Copy this file to `legal/dpia/<feature-slug>.md`. Fill every
> section marked `[FILL]`. Leave no section blank — write "N/A — rationale" if
> genuinely not applicable. Privacy Officer signs off before PR merge.

---

## Metadata

| Field | Value |
|---|---|
| DPIA ID | `[FILL: e.g. DPIA-S12-001]` |
| Feature / Sprint | `[FILL: e.g. S-12 WI-S12-003 — rate-limit PII logging]` |
| Data Controller | HuGR Labs Ltda (LGPD) / HuGR Labs Ltd (GDPR) |
| Encarregado / DPO | `[FILL: name + contact]` |
| Privacy Officer | `[FILL: name]` |
| Processing basis | `[FILL: consent / contract / legal_obligation / legitimate_interest / vital_interest / public_task]` |
| Date initiated | `[FILL: YYYY-MM-DD]` |
| Target review date | `[FILL: YYYY-MM-DD — must be before feature PR merge]` |
| Legal frameworks | GDPR Art. 35 + LGPD Art. 38 + WP29 WP248rev01 (2017) endorsed by EDPB + ANPD Res. CD/ANPD nº 4/2023 |
| EVT evidence path | `evidence-legal/dpia-<slug>.md` (R2 retain 7y per EVT-045) |

---

## Section 1 — Description of Processing

### 1.1 Processing purpose(s)
`[FILL: State clearly and specifically the purpose(s) for which personal data are processed. Reference the canonical purpose enum from privacy_model.md §5.6.1 where applicable.]`

### 1.2 Data subjects affected
`[FILL: Categories of data subjects (e.g., registered users, tenant admins, end-consumers of tenants). Estimate scale — number of subjects, geographic spread.]`

### 1.3 Personal data categories processed
`[FILL: List each data category with reference to the data model (data_model.md). Include: email, IP address, usage telemetry, payment data, etc. Mark each as ordinary / special-category (Art. 9 GDPR / Art. 11 LGPD).]`

### 1.4 Data flows
`[FILL: Describe data flows — where data originates, where it is stored (backends: Neon, R2, D1, KV, Stripe, Loki, etc.), cross-border transfers if any. Reference sub-processors.md for third-party recipients.]`

### 1.5 Retention periods
`[FILL: Per data category, state retention period and legal basis for retention. Reference retention_schedules.md or privacy_model.md §2.]`

### 1.6 WP29 WP248rev01 high-risk criteria checklist

Per WP29 Guidelines WP248rev01 (2017) endorsed by EDPB — DPIA is mandatory if ≥ 2 criteria are met:

| Criterion | Met? | Justification |
|---|---|---|
| 1. Evaluation/scoring (profiling) | `[YES/NO]` | `[FILL]` |
| 2. Automated decision-making with legal/significant effect | `[YES/NO]` | `[FILL]` |
| 3. Systematic monitoring | `[YES/NO]` | `[FILL]` |
| 4. Sensitive data (special categories) | `[YES/NO]` | `[FILL]` |
| 5. Large-scale processing | `[YES/NO]` | `[FILL]` |
| 6. Matching or combining datasets | `[YES/NO]` | `[FILL]` |
| 7. Data on vulnerable subjects | `[YES/NO]` | `[FILL]` |
| 8. Innovative use or new technology | `[YES/NO]` | `[FILL]` |
| 9. Data transfer outside EU/EEA or BR | `[YES/NO]` | `[FILL]` |

**Criteria count:** `[FILL: X/9]` — DPIA `[FILL: mandatory / precautionary]`

---

## Section 2 — Necessity and Proportionality Assessment

### 2.1 Legal basis
`[FILL: State the legal basis (GDPR Art. 6 + LGPD Art. 7/10). If legitimate interest, attach or reference LIA document at legal/lia/<slug>.md.]`

### 2.2 Necessity test
`[FILL: Is the processing necessary to achieve the stated purpose? Is there a less intrusive alternative? Demonstrate that data minimization principles (GDPR Art. 5(1)(c) / LGPD Art. 6 III) are applied.]`

### 2.3 Proportionality assessment
`[FILL: Is the scale and nature of processing proportionate to the purpose? Address: data minimization, storage limitation, purpose limitation, accuracy.]`

### 2.4 Data protection by design measures (GDPR Art. 25 / LGPD Art. 46)
`[FILL: List technical and organizational measures: pseudonymization, encryption at rest/transit, access controls, audit logging, consent capture, etc. Reference relevant WIs/controls.]`

### 2.5 Sub-processor obligations
`[FILL: List sub-processors involved (reference sub-processors.md). Confirm DPA / DPAs in place. For cross-border (esp. US), confirm SCC + TIA per Schrems II (CJEU C-311/18).]`

---

## Section 3 — Risk Identification

> Risk scoring: Likelihood ∈ {Low, Medium, High} × Severity ∈ {Low, Medium, High, Critical}.
> Per LINDDUN threat model (privacy_model.md §4) + ANPD Res. CD/ANPD nº 4/2023 §4.

### 3.1 Risk register

| ID | Threat (LINDDUN) | Description | Likelihood | Severity | Risk Level | Mitigation |
|---|---|---|---|---|---|---|
| R-001 | `[FILL]` | `[FILL]` | `[L/M/H]` | `[L/M/H/C]` | `[FILL]` | `[FILL]` |
| R-002 | `[FILL]` | `[FILL]` | `[L/M/H]` | `[L/M/H/C]` | `[FILL]` | `[FILL]` |
| R-003 | `[FILL]` | `[FILL]` | `[L/M/H]` | `[L/M/H/C]` | `[FILL]` | `[FILL]` |

> **Add rows as needed.** Reference failure modes from failure_modes.md where applicable.

---

## Section 4 — Mitigation Measures

`[FILL: For each HIGH/CRITICAL risk in §3, describe the mitigation measure(s) in place or planned. Include: technical controls (controls_catalogue.md references), organizational controls, contractual safeguards, and testing evidence (EVT-* references).]`

### 4.1 Technical mitigations
`[FILL]`

### 4.2 Organizational mitigations
`[FILL]`

### 4.3 Contractual / legal mitigations
`[FILL: SCC references, DPA terms, TIA for cross-border transfers.]`

### 4.4 Residual risk after mitigation

| Risk ID | Residual Likelihood | Residual Severity | Residual Level | Accepted by |
|---|---|---|---|---|
| R-001 | `[FILL]` | `[FILL]` | `[FILL]` | `[FILL]` |
| R-002 | `[FILL]` | `[FILL]` | `[FILL]` | `[FILL]` |

---

## Section 5 — Residual Risk Assessment + Sign-off

### 5.1 Overall residual risk assessment
`[FILL: Summarize the overall residual risk level. If any CRITICAL residual risk remains, processing MUST NOT commence without DPO + Encarregado endorsement and Privacy Officer escalation.]`

### 5.2 Consultation (GDPR Art. 36 / LGPD Art. 38 §3)
`[FILL: If residual risk remains HIGH after mitigation, prior consultation with the supervisory authority (DPC for GDPR / ANPD for LGPD) may be required. Document outcome here.]`

### 5.3 Sign-off

| Role | Name | Date | Status |
|---|---|---|---|
| Privacy Officer | `[FILL]` | `[FILL]` | `[ ] Approved / [ ] Pending` |
| DPO / Encarregado | `[FILL]` | `[FILL]` | `[ ] Approved / [ ] Pending` |
| Legal (external) | `[FILL]` | `[FILL]` | `[ ] Approved / [ ] Pending` |
| Compliance | `[FILL]` | `[FILL]` | `[ ] Approved / [ ] Pending` |

> **EVT-045 evidence:** Once signed, upload this document to R2 `evidence-legal/dpia-<slug>.md` with 7-year retention lock. Record event `EVT-045` in the audit log.

> **EVT-044 LEGAL_REVIEW:** Record legal review event `EVT-044` in the audit log.

---

## Section 6 — Quarterly Review Schedule

| Review cycle | Target date | Trigger condition | Reviewer |
|---|---|---|---|
| Initial | `[FILL: date of sign-off]` | Pre-merge | Privacy Officer |
| Q1 review | `[FILL]` | Quarterly | Privacy Officer |
| Q2 review | `[FILL]` | Quarterly | Privacy Officer |
| Q3 review | `[FILL]` | Quarterly | Privacy Officer |
| Q4 review | `[FILL]` | Quarterly | Privacy Officer |
| Change-triggered | On PR changing PII handling for this feature | CI hook detect | Privacy Officer |

> **Quarterly review SLA:** 30 days from trigger. Missed review triggers SEV-3 alert (`corelink_dpia_quarterly_review_completion_total`).

> **LGPD Art. 38 §2 (ANPD Res. 4/2023):** RIPD must be updated whenever there is a significant change to the processing. Document any update in the changelog below.

### 6.1 DPIA changelog

| Version | Date | Author | Change summary |
|---|---|---|---|
| 1.0 | `[FILL]` | `[FILL]` | Initial DPIA |

---

*Template version 1.0.0 — WI-S11-008 — GDPR Art. 35 + LGPD Art. 38 (RIPD) unified.*
*Legal citations: WP29 WP248rev01 (2017) endorsed by EDPB; ANPD Res. CD/ANPD nº 4/2023; CJEU C-311/18 (Schrems II); ICO DPIA guidance.*
