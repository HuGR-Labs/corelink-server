---
document_type: "capability_errata_draft"
status: "DRAFT — NOT FOR PUBLICATION"
version: "2026-09-08-draft"
created: "2026-09-08"
legal_review_status: "pending_owner_and_counsel"
effective_date: null
---

# Enterprise capability and case-study errata — draft

This errata records the engineering truth needed for B-154 without claiming
that an executed DPA/SLA has been corrected.

## Current launch boundary

- BYOK is not enabled or provisioned in the launched data plane; the BYOK
  admin route remains a fail-closed not-implemented path.
- The BYOK kill-switch p99 commitment in `legal/sla/v1.0.0.md` is therefore
  an executed-instrument mismatch requiring counsel/owner action.
- R2 Object Lock is not implemented in the launched audit path. The audit
  chain is tamper-evident/append-only, not Object Lock/WORM.
- The Enterprise case-study testimonial is a draft with placeholders and has
  no customer attribution or approval. It must not be presented as a customer
  statement until the drill is real and the customer approves the quote.

## Publication gate

This file is an internal, unsigned correction aid. It does not amend the DPA,
SLA, Terms, or a customer case study. Publish only after the corresponding
instrument is superseded or formally noticed and the case-study customer
evidence exists.
