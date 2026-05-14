---
id: "ADR-S11-005"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s11", "consent", "schema", "regulatory"]
---

# ADR-S11-005 — Consent Grant ↔ Revoke Symmetric Schema

## Status

Accepted — 2026-05-13.

## Context

GDPR Art. 7(3) + LGPD Art. 8 §5: "withdrawing consent shall be as easy
as giving it." Multiple interpretations exist for "as easy as":

- (a) Same UX surface (1-click symmetry).
- (b) Same proof-of-informed payload (cryptographic symmetry).
- (c) Both.

Interpretation (a) alone is insufficient under WP29 Op. 06/2014 §3.1
("consent must be specific and informed at withdrawal too").

## Decision

**Identical 6-field `ConsentProofPayload` struct used for grant AND
revoke**:

```rust
pub struct ConsentProofPayload {
    pub notice_text_hash: NoticeTextHash,
    pub notice_version: NoticeVersion,
    pub locale: NoticeLocale,
    pub wording_id: WordingId,
    pub ui_capture_ts: u64,
    pub submission_ts: u64,
}
```

Same fields. Same canonicalization. Same HMAC-SHA256 signature over the
canonical preimage. Same verify path (stateless). Same D1 schema slot
in `consent_ledger` and `consent_revocation` tables.

## Rationale

- **Cryptographic symmetry** = subject sees the same proof artifact at
  both moments; auditor verifies revoke-side with the same
  `verify_proof()` they verified the grant with.
- **Regulator-defensible**: ANPD / Irish DPC investigations can run a
  single property test against both tables.
- **Code reuse**: one `ConsentProofPayload` type, two table inserts.
  Single source of truth.
- **NO fail-open swap**: if a tenant disables a purpose, that does NOT
  transparently downgrade to `LegitimateInterest`. The revoke must be
  explicit. Corrects GPT P0-1 round-1 finding.

## Consequences

**Positive**: symmetric audit trail; one HMAC verify code path; tested
by `regression_symmetric_schema` 10k property tests; satisfies
INV-CONSENT-PROOF-VERIFIABLE CRITICAL §3.12.

**Negative**: revoke endpoint requires the same locale + wording_id
metadata as grant (slightly more UI friction than a 1-click). Mitigated
by pre-filling from the prior grant record in the response.

**Forbidden**: asymmetric schema between grant and revoke. Forbidden:
silent purpose-to-legitimate-interest swap on revoke.

## References

- WI-S11-003 §6 + AC-002 symmetric schema
- privacy_model.md §5.6 consent
- GDPR Art. 7(3), LGPD Art. 8 §5, WP29 Op. 06/2014
- INV-CONSENT-PROOF-VERIFIABLE (CRITICAL §3.12)
- Lote 9.4 Opus H-05 lift
