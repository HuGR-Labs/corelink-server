---
id: "ADR-S11-003"
title: "Erasure salt management interim — per-tenant random salt em D1 vault encrypted (S-11 a S-13); BYOK KMS final solution deferred to S-14"
status: "ACCEPTED"
date: "2026-04-29"
tags: ["adr", "s-11", "privacy", "erasure", "byok", "kms", "interim"]
deciders:
  - "Gustavo Schneiter (owner / final approver)"
  - "Privacy Officer (interim Gustavo até hire)"
  - "Compliance Officer"
sup​ersedes: null
superseded_by: null
parent: "WI-S11-002"
---

# ADR-S11-003 — Erasure salt management interim

## Context

The CoreLink DSR erasure pipeline pseudonymizes retained PII fields
(legal_hold WORM regulatory immutability per GDPR Recital 26 + Art. 11
+ WP29 Op. 05/2014 endorsed by EDPB anonymization techniques) via the
canonical rule:

```
pseudonym(subject_id, erasure_salt) = sha256(subject_id || erasure_salt)
```

Per WI-S11-002 §9.3 DD-003 the canonical `erasure_salt` is **per-tenant
+ per-DSR scope** (forward secrecy: even if a single salt leaks, prior
+ future DSRs remain unrecoverable). The customer-controlled
erasure_salt vault (BYOK KMS surface) is the canonical S-14 deliverable
— the canonical
`corelink-privacy-pseudonymize` crate exposes `verify_pseudonym` so a
court-ordered production hold can re-correlate audit rows under the
customer-held key.

**Tension**: S-14 BYOK ships at sprint S-14 PRR ship gate (canonical
`autonomous_state.json` queue position 23); S-11 (this WI) cannot
block on a downstream sprint. The Sprint contract S-11 v1.4.0 freezes
the canonical 12-backend canonical pipeline pós Lote 10.11.0-bis (8
effective + 4 pseudonymized) so the canonical erasure_salt surface
must be wired BEFORE S-14 KMS lands.

## Decision

**Interim (S-11 → S-13)**: per-tenant random salt stored in D1 vault
**encrypted at rest** via AES-256-GCM (CTRL-CRYPTO-002) with a
tenant-key-derivation chain rooted in the canonical KMS master key
(security_model.md §7.2). The salt is generated fresh per-DSR (32
bytes from the canonical CSPRNG) and persisted in the encrypted D1
vault at the time the canonical `dsr.queued.v1` event is enqueued.

**Final (S-14)**: customer-controlled erasure_salt vault via BYOK KMS
integration. Customer holds the canonical 256-bit master key in their
own KMS (AWS KMS / Google Cloud KMS / Azure Key Vault); the canonical
HKDF info=`corelink/v1/erasure-salt` derives per-DSR salts; the
customer can revoke their KMS key without CoreLink involvement
(NIST SP 800-88 Rev.1 crypto-erase compliant).

## Consequences

### Positive

- **Regulatory baseline**: LGPD Art. 18 IV + GDPR Art. 17 + CCPA
  §1798.105 + GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 endorsed by
  EDPB (anonymization techniques) requirements satisfied at S-11
  cutover (canonical 12-backend pipeline operational).
- **Customer onboarding**: Forge customer zero (HuGR) onboards at
  S-11 cutover without waiting for S-14 KMS integration.
- **Forward secrecy preserved**: per-DSR salt scope (vs global)
  prevents cross-DSR correlation by attacker even under interim
  storage path.

### Negative

- **Interim attack surface**: per-tenant D1 vault is a centralised
  storage path; compromise of the KMS-rooted derivation chain (or the
  CoreLink-managed master key) exposes every interim salt across every
  tenant. **Mitigation**: AES-256-GCM at-rest + tenant key rotation
  S-19 + quarterly KMS key rotation per security_model.md §7.2; no
  raw plaintext salts in any backup. **Residual risk**: ACCEPTED until
  S-14 ships (5 sprints cap; max 5 months from S-11 SEAL).
- **Customer trust delta**: interim solution is "we hold the key";
  full solution is "you hold the key". DPA + privacy notice document
  the interim scope explicitly + commit to S-14 BYOK migration.

### Neutral

- Code path change at S-14: the canonical
  [`corelink_privacy_pseudonymize::pseudonymize_subject_id`] surface
  is unchanged; only the `erasure_salt` source binding swaps from D1
  vault to KMS HKDF.

## Sign-off

- **Privacy Officer (interim)**: ✅ accepts interim per regulatory
  baseline urgency.
- **Compliance Officer**: ✅ accepts subject to DPA documentation +
  S-14 ship commitment.
- **Architect**: ✅ accepts subject to canonical
  CTRL-CRYPTO-002 AES-256-GCM at-rest verified at PRR ship gate.
- **DPO interim (Gustavo até hire)**: ✅ accepts subject to S-14 ship
  date in roadmap.

## References

- WI-S11-002 §9.2 ADR-S11-003 trigger.
- security_model.md §7.2 KMS master key + key rotation cadence.
- key_management.md §2 HKDF info=`corelink/v1/erasure-salt`.
- privacy_model.md §6.2 12-backend canonical pipeline.
- GDPR Recital 26 + Art. 11 + WP29 Op. 05/2014 (anonymization
  techniques) — pseudonymization escape valve regulatório.
- ADR-S11-004 — cross-backend eventual consistency rationale.
