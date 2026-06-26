---
type: "ComplianceControl"
title: "Sub-processor / vendor DPA reviews"
description: "The evidence store and governance for Legal Counsel's review of each CoreLink sub-processor's Data Processing Agreement, transfer mechanism, and GDPR Art. 28 flow-down obligations."
source_files:
  - docs/compliance/vendor-reviews/README.md
  - docs/compliance/vendor-reviews/_TEMPLATE.md
  - docs/compliance/vendor-reviews/cloudflare-dpa-review-2026-04.md
  - legal/sub-processors.md
  - specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["compliance", "gdpr", "sub-processors", "dpa", "vendor-review", "legal"]
timestamp: "2026-06-26T00:00:00Z"
---

CoreLink discloses the third parties that process customer data, and each disclosure must point at a recorded outcome of Legal Counsel's review of that vendor's Data Processing Agreement (DPA), Standard Contractual Clauses (SCCs), transfer mechanism, and flow-down obligations. The machine-validated contractual register `legal/sub-processors.md` lists **SEVEN** active sub-processors — Cloudflare, Neon, Grafana, Stripe, GitHub, Sigstore, PagerDuty (`legal/sub-processors.md:5-115`). **Clerk (auth) is NOT in that validated register**: treat any "8 sub-processors including Clerk" count as a DISCLOSURE GAP to reconcile (add Clerk to the register or confirm Clerk is not a customer-personal-data sub-processor), NOT as a validated fact. The `docs/compliance/vendor-reviews/` directory is that evidence store: one file per active sub-processor, named to match the exact `legal_review_evidence:` path declared in the contractual disclosures. The references MUST resolve to a real file, and a validator enforces both path format and file existence so a missing evidence record cannot silently pass. As of this checkpoint every file is still a TEMPLATE — the structural gap is closed, but the actual signed-off review content is owner/legal work and must not be fabricated.

# Role

This directory is the evidence store for the legal review of CoreLink's sub-processors, holding the recorded outcome of Legal Counsel's DPA/SCC/transfer-mechanism review per vendor as part of vendor onboarding `docs/compliance/vendor-reviews/README.md:3-7`. It is the resolution target for the `legal_review_evidence:` paths declared in the contractual sub-processor disclosures `docs/compliance/vendor-reviews/README.md:11-12`.

# How it works

- The contractual disclosures reference this store: `legal/sub-processors.md` carries one `legal_review_evidence:` path per active sub-processor, and `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md` has a `Vendor review evidence` row per sub-processor plus a flow-down note pointing at `docs/compliance/vendor-reviews/*.md` `docs/compliance/vendor-reviews/README.md:14-18`.
- `scripts/validate_sub_processors.py` enforces both (a) the path format and (b) that the target file actually exists, so a missing evidence record can no longer silently pass the gate `docs/compliance/vendor-reviews/README.md:20-22`.
- Files are named `<vendor-id>-<dpa|review>-<YYYY-MM>.md`, matching the exact path declared in `legal_review_evidence:` and the `Vendor review evidence` rows `docs/compliance/vendor-reviews/README.md:24-28`.
- Each record is a table of structured fields — vendor legal entity, sub-processor id, review date, named reviewer, DPA reference and status, SCC/transfer mechanism, Schrems II TIA, data categories, residency/region, flow-down confirmation, certifications, outcome, conditions, and next-review date `docs/compliance/vendor-reviews/_TEMPLATE.md:10-26`.
- The Cloudflare record illustrates a populated stub: vendor `Cloudflare, Inc.`, id `cloudflare`, the customer-DPA URL, data categories (account_metadata; blob_content; audit_logs; telemetry), and a region field of "multi-region (tenant-pinned per `tenant.primary_region`)" `docs/compliance/vendor-reviews/cloudflare-dpa-review-2026-04.md:7-16`. That "tenant-pinned per `tenant.primary_region`" wording is the disclosure's framing — for native CAS bytes the deployed reality is a **global** `R2_CAS_REGION` tag on a single bucket, NOT per-tenant `tenant.primary_region` pinning (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:27-33`), so the residency field overstates per-tenant pinning.
- Adding/completing a record is a 3-step process: copy `_TEMPLATE.md` to the exact referenced path, have Legal Counsel complete every field with the real outcome, then remove the `STATUS: TEMPLATE` banner once the record is genuine, dated, and attributed `docs/compliance/vendor-reviews/README.md:43-48`.

# Invariants

- Every `legal_review_evidence:` reference MUST resolve to a real file — enforced by `scripts/validate_sub_processors.py` on both path format and existence `docs/compliance/vendor-reviews/README.md:20-22`.
- A file name MUST follow `<vendor-id>-<dpa|review>-<YYYY-MM>.md` and match the exact path declared in the disclosure `docs/compliance/vendor-reviews/README.md:24-28`.
- A template MUST be replaced with the real, dated, signed-off review record by Legal Counsel before any compliance claim relies on it, and review outcomes MUST NOT be fabricated `docs/compliance/vendor-reviews/README.md:38-41`.
- A record's data categories and residency/region MUST match the disclosure they back `docs/compliance/vendor-reviews/_TEMPLATE.md:20-21`.
- The `STATUS: TEMPLATE` banner is removed only once the record is genuine, dated, and attributed to a named reviewer `docs/compliance/vendor-reviews/README.md:47-48`.

# Gotchas

- Every file currently in the directory is a TEMPLATE marked with a `STATUS: TEMPLATE` banner — they close only the structural gap (paths exist and are schema-checked), not the substantive legal review `docs/compliance/vendor-reviews/README.md:30-37`.
- Even a vendor-specific stub like the Cloudflare record still carries the TEMPLATE banner and leaves review-outcome fields as `TBD` — its presence does not mean the review happened `docs/compliance/vendor-reviews/cloudflare-dpa-review-2026-04.md:3` `docs/compliance/vendor-reviews/cloudflare-dpa-review-2026-04.md:19-25`.
- The template instructs leaving fields as `TBD` until verified rather than guessing — do not fabricate outcomes `docs/compliance/vendor-reviews/_TEMPLATE.md:8`.
- **Clerk disclosure gap:** the validated contractual register lists 7 sub-processors and Clerk is absent (`legal/sub-processors.md:5-115`), yet Clerk is the live auth provider processing customer account/identity data. Do not state "8 sub-processors incl. Clerk" as validated — it is an open reconcile item (add Clerk to both the register and `specs/_compliance/VENDOR-RISK-REGISTER.md`, or document why Clerk is out of scope).

# Citations

- `docs/compliance/vendor-reviews/README.md:3-22` — the store's purpose, the disclosures that reference it, and the existence-enforcing validator.
- `docs/compliance/vendor-reviews/README.md:30-48` — TEMPLATE status, the no-fabrication rule, and the add/complete workflow.
- `docs/compliance/vendor-reviews/_TEMPLATE.md:10-26` — the canonical per-record field schema.
- `docs/compliance/vendor-reviews/cloudflare-dpa-review-2026-04.md:7-25` — a worked vendor stub still pending real review.
- `legal/sub-processors.md:5-115` — the machine-validated contractual register: SEVEN sub-processors (Clerk absent).
- `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:27-33` — native-CAS region is a global `R2_CAS_REGION` tag, not per-tenant `tenant.primary_region` pinning.
