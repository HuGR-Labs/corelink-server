# Vendor Legal-Review Record — &lt;Vendor Name&gt;

> STATUS: TEMPLATE — pending the actual legal review record (owner/counsel to complete).

This is the canonical template for a sub-processor legal-review evidence record.
Copy it to the exact path declared in the disclosure's `legal_review_evidence:`
field, then have Legal Counsel complete every field with the **real** review
outcome. Do not fabricate outcomes; leave fields as `TBD` until verified.

| Field | Value |
|---|---|
| Vendor | `<legal entity name>` |
| Sub-processor id | `<id as in legal/sub-processors.md>` |
| Review date | `TBD (YYYY-MM-DD)` |
| Reviewer | `TBD (named Legal Counsel / Privacy Officer)` |
| DPA reference | `<dpa_url>` |
| DPA status | `TBD (executed / pending / N/A)` |
| SCC / transfer mechanism | `TBD (e.g. EU SCCs 2021/914 Module 3; UK IDTA)` |
| Schrems II TIA | `TBD (completed / not required / pending)` |
| Data categories processed | `<list — must match the disclosure>` |
| Data residency / region | `<region — must match the disclosure>` |
| Sub-processor flow-down | `TBD (confirmed materially-equivalent flow-down per GDPR Art. 28(4))` |
| Certifications verified | `TBD (e.g. SOC 2 Type II, ISO 27001)` |
| Review outcome | `TBD (approved / approved-with-conditions / rejected)` |
| Conditions / follow-ups | `TBD` |
| Next review due | `TBD (YYYY-MM-DD)` |

## Notes

`<Free-form reviewer notes: scope of processing, residual risk, supplementary
measures applied, government-access regime, anything that informed the
outcome. Replace this section with the real review narrative.>`

---

*This record is referenced by `legal/sub-processors.md` and/or
`legal/dpa/SUB-PROCESSOR-COMMITMENTS.md`. Its existence is enforced by
`scripts/validate_sub_processors.py`.*
