---
id: "AUDIT-LIGHTHOUSE-CUSTOMER-DPA-SIGNED"
type: "audit_report"
doc_status: "PENDING"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
wi: "WI-S14-008"
observation_window: true
tags:
  - "lighthouse-customer"
  - "dpa"
  - "enterprise"
  - "s14"
  - "ga-evidence-gate"
supersedes: null
superseded_by: null
---

# Lighthouse Customer DPA Signed — Audit Report
## WI-S14-008 · S-14 · GA Evidence Gate

> **Status**: PENDING — to be completed when lighthouse enterprise customer DPA is signed.
> **Observation window**: D+30..D+60 from WI-S14-008 implementation start.

---

## Report Fields (fill on completion)

| Field | Value |
|---|---|
| **Customer ID** | `[CUSTOMER_ID — anonymised or redacted under NDA]` |
| **Region** | `[WNAM / ENAM / WEUR / SAM]` |
| **DPA signed date** | `[DATE]` |
| **DPA template version** | `[dpa-residency-amendment.md vN.N.N]` |
| **TIA template version** | `[tia-template.md vN.N.N]` |
| **Legal externo review completed** | `[YES / NO (WAIVER-S14-001 active)]` |
| **Customer plan** | `[Enterprise / Enterprise+]` |
| **Custom clauses** | `[YES (list) / NO]` |
| **CoreLink signatory** | Gustavo Schneiter |
| **Customer signatory** | `[NAME, TITLE — redacted]` |
| **Evidence committed** | `[this file]` |

---

## Signing Process Summary

- [ ] DPA package provided to customer under NDA.
- [ ] Customer Legal review cycle completed.
- [ ] Any custom clauses reviewed by Legal externo.
- [ ] DPA signed by both parties.
- [ ] Audit committed to `specs/_audits/`.
- [ ] CloudEvent emitted: `corelink.legal.lighthouse_customer.dpa_signed`.
- [ ] Metric updated: `corelink_legal_dpa_signed_total{region=[REGION], plan=[PLAN]}`.
- [ ] Lighthouse customer reference available for future prospects (under NDA, with consent).

---

## Notes

`[Fill on completion — any notable customer feedback, deviations from template, or process improvements identified during signing cycle]`

---

*PENDING — Observation window D+30..D+60 · WI-S14-008.*
