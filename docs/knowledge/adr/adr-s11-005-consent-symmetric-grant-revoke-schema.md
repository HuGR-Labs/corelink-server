---
type: "ADR"
title: "ADR-S11-005 — Consent grant ↔ revoke symmetric schema"
description: "Why grant and revoke use one identical 6-field ConsentProofPayload with the same canonicalization, HMAC signature, and verify path."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-005-consent-symmetric-grant-revoke-schema.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "consent", "schema", "regulatory", "privacy"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-005 — Consent grant ↔ revoke symmetric schema

GDPR Art. 7(3) + LGPD Art. 8 §5 require that "withdrawing consent shall be as easy as giving it," and
that phrase has competing readings — same UX surface, same proof-of-informed payload, or both. UX
symmetry alone is insufficient under WP29 Op. 06/2014 (consent must be specific and informed at
withdrawal too). This ADR commits to *cryptographic* symmetry: one identical proof payload, one
canonicalization, one signature, and one stateless verify path used for both grant and revoke — so an
auditor verifies the revoke side with the very same code they verified the grant with.

# Context

"As easy as" admits interpretations (a) same UX surface, (b) same cryptographic proof payload, or
(c) both. Interpretation (a) alone fails WP29 Op. 06/2014 §3.1, which requires consent to remain
specific and informed at withdrawal — so the proof artifact, not just the click, must be symmetric.

# Decision

**An identical 6-field `ConsentProofPayload` struct is used for grant AND revoke** —
`notice_text_hash`, `notice_version`, `locale`, `wording_id`, `ui_capture_ts`, `submission_ts` — with
the same canonicalization, the same HMAC-SHA256 signature over the canonical preimage, the same
stateless verify path, and the same D1 schema slot across the `consent_ledger` and `consent_revocation`
tables. Critically, disabling a purpose does NOT transparently downgrade to `LegitimateInterest`: the
revoke must be explicit (correcting a prior round-1 fail-open finding).

# Consequences

- Positive: a symmetric audit trail with one HMAC verify code path, one `ConsentProofPayload` type and
  two table inserts (single source of truth), regulator-defensible (a single property test runs against
  both tables), and tested by 10k `regression_symmetric_schema` property tests satisfying
  INV-CONSENT-PROOF-VERIFIABLE.
- Negative: the revoke endpoint requires the same locale + wording_id metadata as grant (slightly more
  friction than a 1-click), mitigated by pre-filling from the prior grant record in the response.
- Forbidden: an asymmetric schema between grant and revoke, and any silent
  purpose-to-legitimate-interest swap on revoke.

# Citations

1. `specs/03_architecture/adrs/ADR-S11-005-consent-symmetric-grant-revoke-schema.md:23-33` — the
   Context: the "as easy as" interpretations and why UX symmetry alone fails WP29 Op. 06/2014.
2. `specs/03_architecture/adrs/ADR-S11-005-consent-symmetric-grant-revoke-schema.md:35-53` — the
   Decision: the identical 6-field `ConsentProofPayload` and shared HMAC verify path.
3. `specs/03_architecture/adrs/ADR-S11-005-consent-symmetric-grant-revoke-schema.md:68-79` — the
   Consequences: the symmetric audit trail / property tests, the metadata friction, and the forbidden
   cases.
