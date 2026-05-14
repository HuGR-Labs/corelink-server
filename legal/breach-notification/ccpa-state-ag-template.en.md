---
id: "BREACH-TMPL-CCPA-STATE-AG-EN"
type: "breach_notification_template"
doc_status: "DRAFT"
jurisdiction: "US-CA"
authority: "California Attorney General"
language: "en"
legal_basis: "CCPA §1798.82 + California Civil Code §1798.82 (SB-1386/AB-1950)"
version: "1.0.0"
created: "2026-05-13"
owner: "Privacy Officer (Gustavo Schneiter interim)"
legal_review_status: "PENDING"
legal_review_evt_044_path: "r2://evidence-legal/breach-notification/ccpa-state-ag-template-legal-review.pdf"
privacy_officer_review: false
template_variables:
  - "{{breach_id}}"
  - "{{breach_detected_at}}"
  - "{{data_categories_affected}}"
  - "{{count_subjects_affected}}"
  - "{{containment_actions}}"
  - "{{mitigation_offered}}"
  - "{{contact_email}}"
  - "{{breach_severity}}"
---

# Data Breach Notification — California Attorney General
## (Pursuant to California Civil Code §1798.82 / CCPA §1798.82)

**COMPLETION INSTRUCTIONS**: This is a pre-drafted template. Before sending:
1. Complete ALL `{{...}}` variables with actual incident data.
2. Remove these completion instructions.
3. Review with external Legal counsel specializing in California privacy law.
4. Submit via California AG breach reporting portal: https://oag.ca.gov/privacy/databreach/reporting
5. Provide simultaneous notice to affected California residents (§1798.82(d)).
6. Emit audit event `breach.notification_dispatched.v1` post-dispatch.

**NOTE**: Under Cal. Civ. Code §1798.82(a), notification must be made "in the most expedient time possible and without unreasonable delay" — California courts have interpreted this as ≤72h in the majority of cases. Consult Legal for any extension with documented justification.

---

**SUBJECT**: Data Breach Notification — Reference `{{breach_id}}`

---

**TO**: Office of the Attorney General of California
**FROM**: HuGR Labs
**DATE**: *(fill with dispatch date)*
**INCIDENT REFERENCE**: `{{breach_id}}`

---

## 1. Business Information

| Field | Value |
|---|---|
| **Business Name** | HuGR Labs |
| **Principal Business Address** | *(to be confirmed pre-GA)* |
| **Contact Name** | Gustavo Schneiter (Privacy Officer interim) |
| **Contact Email** | `{{contact_email}}` |
| **Contact Phone** | *(to be confirmed)* |
| **Nature of Business** | Software-as-a-Service (SaaS) — content-addressable build cache and CI/CD acceleration platform |

---

## 2. Date of Breach

*(Cal. Civ. Code §1798.82(d)(4))*

**Date breach occurred (estimated)**: *(fill)*

**Date breach was discovered**: `{{breach_detected_at}}` (UTC)

**Date of this notification**: *(fill)*

---

## 3. Description of the Breach

*(Cal. Civ. Code §1798.82(d)(1))*

On `{{breach_detected_at}}` (UTC), HuGR Labs discovered a data security incident. Incident classified as **`{{breach_severity}}`** per internal classification criteria.

**Description of what happened**: *(provide a clear, plain-language explanation of how the breach occurred and what data was affected — avoid technical jargon; California law requires notice in plain language under §1798.82(d))*

---

## 4. Types of Personal Information Involved

*(Cal. Civ. Code §1798.82(d)(3))*

The following categories of California residents' personal information were or may have been involved in this breach (as defined under Cal. Civ. Code §1798.82(h)):

`{{data_categories_affected}}`

**Specific California-defined categories affected** (check all that apply):
- [ ] First name / last name + Social Security Number
- [ ] First name / last name + driver's license / California ID card number
- [ ] First name / last name + account number / credit / debit card number (with security code / password)
- [ ] First name / last name + medical information
- [ ] First name / last name + health insurance information
- [ ] **Login credentials** (email address + password / security question — Cal. Civ. Code §1798.82(h)(4))
- [ ] First name / last name + DNA profile
- [ ] First name / last name + fingerprint/retina/biometric data
- [ ] First name / last name + unique biometric data

**Approximate number of California residents affected**: *(fill — AG notification required when ≥ 500 California residents per §1798.82(f))*

---

## 5. What HuGR Labs Has Done

*(Cal. Civ. Code §1798.82(d)(5))*

**Immediate containment actions taken**:

`{{containment_actions}}`

**Remediation and mitigation measures**:

`{{mitigation_offered}}`

---

## 6. What You Can Do

*(Cal. Civ. Code §1798.82(d)(6) — to be included in resident notification)*

We recommend that affected California residents:
- *(list recommended protective steps — e.g., change passwords, monitor accounts, place a fraud alert)*

---

## 7. Contact Information for Further Assistance

*(Cal. Civ. Code §1798.82(d)(7))*

For questions about this incident:

| Field | Value |
|---|---|
| **Contact** | Privacy Officer — HuGR Labs |
| **Email** | `{{contact_email}}` |
| **Availability** | Monday–Friday 9am–6pm PT; 24/7 for security matters |

---

## 8. Notification to California Residents

This notification is being sent to all California residents who were or may have been affected by this breach. Notification is being provided:
- **By email** (transactional email via Cloudflare Email — 3 locales: PT-BR / EN / ES per LGPD Art. 9)
- **By substitute notice** *(if applicable — check §1798.82(j)(2) threshold criteria)*

The content of the resident notification complies with Cal. Civ. Code §1798.82(d)(1)-(7).

---

*This notification is submitted pursuant to California Civil Code §1798.82. HuGR Labs is committed to the security of California residents' personal information.*

*Breach reference for audit purposes: `{{breach_id}}`*
