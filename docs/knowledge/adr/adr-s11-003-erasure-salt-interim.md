---
type: "ADR"
title: "ADR-S11-003 — Erasure salt management interim (D1-encrypted vault, BYOK KMS deferred to S-14)"
description: "Why the per-DSR erasure salt is stored interim in an AES-256-GCM-encrypted D1 vault for S-11→S-13, with customer-held BYOK KMS as the final S-14 solution."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-003-erasure-salt-interim.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "privacy", "erasure", "byok", "kms", "interim"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-003 — Erasure salt management interim (D1-encrypted vault, BYOK KMS deferred to S-14)

The DSR erasure pipeline pseudonymizes WORM-retained PII via `sha256(subject_id || erasure_salt)`, with
a per-tenant + per-DSR salt scope for forward secrecy. The canonical customer-controlled salt vault is
a BYOK KMS surface, but that is the S-14 deliverable and S-11 cannot block on a downstream sprint. This
ADR (doc_status SEALED) resolves the tension with an interim: store the salt in a CoreLink-managed,
AES-256-GCM-encrypted D1 vault for S-11→S-13, then swap the salt *source binding* to customer-held KMS
HKDF at S-14 without changing the pseudonymize surface. It is grounded in the cross-backend
consistency rationale of ADR-S11-004.

# Context

Per WI-S11-002 §9.3 DD-003 the canonical `erasure_salt` is per-tenant + per-DSR (so a leaked salt
doesn't compromise prior or future DSRs). The customer-controlled BYOK-KMS vault is the canonical S-14
deliverable, but the S-11 12-backend pipeline must wire a working salt surface before S-14 lands.

# Decision

**Interim (S-11 → S-13):** a per-tenant random salt stored in a D1 vault encrypted at rest via
AES-256-GCM (CTRL-CRYPTO-002), with a tenant-key-derivation chain rooted in the canonical KMS master
key; the salt is generated fresh per-DSR (32 bytes from the CSPRNG) and persisted at the time the
`dsr.queued.v1` event is enqueued. **Final (S-14):** a customer-controlled erasure_salt vault via BYOK
KMS — the customer holds the 256-bit master key in their own KMS, HKDF `info=corelink/v1/erasure-salt`
derives per-DSR salts, and the customer can crypto-erase by revoking their KMS key without CoreLink
involvement.

# Consequences

- Positive: the LGPD/GDPR/CCPA + Recital-26 pseudonymization regulatory baseline is satisfied at S-11
  cutover, customer-zero (HuGR) onboards without waiting for S-14, and per-DSR salt scope preserves
  forward secrecy even under the interim storage path.
- Negative: the interim D1 vault is a centralized path — compromise of the KMS-rooted derivation chain
  exposes every interim salt; mitigated by AES-256-GCM at rest + key rotation, with the residual risk
  explicitly ACCEPTED until S-14 (capped at 5 sprints / max 5 months) and documented in the DPA.
- Neutral: the code change at S-14 is binding-only — the `pseudonymize_subject_id` surface is
  unchanged; only the salt source swaps from D1 vault to KMS HKDF. Signed off by Privacy, Compliance,
  Architect, and interim DPO subject to the S-14 ship commitment.

# Citations

1. `specs/03_architecture/adrs/ADR-S11-003-erasure-salt-interim.md:22-47` — the Context: the
   per-DSR-scoped salt rule and the S-11-cannot-block-on-S-14 tension.
2. `specs/03_architecture/adrs/ADR-S11-003-erasure-salt-interim.md:49-63` — the Decision: the interim
   AES-256-GCM D1 vault and the final customer-held BYOK-KMS HKDF design.
3. `specs/03_architecture/adrs/ADR-S11-003-erasure-salt-interim.md:65-97` — the Consequences: the
   regulatory baseline, the centralized-vault residual risk accepted until S-14, and the binding-only
   swap.
