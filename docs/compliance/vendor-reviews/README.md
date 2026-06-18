# Vendor Legal-Review Evidence Store

This directory is the **evidence store for the legal review of CoreLink's
sub-processors**. Each file here is the recorded outcome of Legal Counsel's
review of a vendor's Data Processing Agreement (DPA), Standard Contractual
Clauses (SCCs), transfer mechanism, and flow-down obligations as part of
vendor onboarding.

## Why this directory exists

The contractual sub-processor disclosures point at files here via their
`legal_review_evidence:` paths:

- `legal/sub-processors.md` — YAML frontmatter, one
  `legal_review_evidence:` path per active sub-processor.
- `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md` — `Vendor review evidence` row
  per sub-processor, and the flow-down note referencing
  `docs/compliance/vendor-reviews/*.md`.

These references **MUST resolve to a real file**. `scripts/validate_sub_processors.py`
now enforces (a) the path format **and** (b) that the target file **exists** —
so a missing evidence record can no longer silently pass the gate.

## File naming

`<vendor-id>-<dpa|review>-<YYYY-MM>.md`, matching the exact path declared in
`legal_review_evidence:` (and the `Vendor review evidence` rows in
`SUB-PROCESSOR-COMMITMENTS.md`).

## Status of the current files

Every file currently in this directory is a **TEMPLATE** (see `_TEMPLATE.md`),
marked at the top with:

> STATUS: TEMPLATE — pending the actual legal review record (owner/counsel to complete).

The templates close the *structural* gap (the referenced paths now exist and
are schema-checked). The **review content itself is owner/legal work** — a
template MUST be replaced with the real, dated, signed-off review record by
Legal Counsel before any compliance claim relies on it. Do **not** fabricate
review outcomes in these files.

## Adding / completing a record

1. Copy `_TEMPLATE.md` to the exact path the disclosure references.
2. Have Legal Counsel complete every field with the real review outcome.
3. Remove the `STATUS: TEMPLATE` banner once the record is genuine, dated, and
   attributed to a named reviewer.
