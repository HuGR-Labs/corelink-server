---
id: "ADR-S11-001"
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
tags: ["adr", "s11", "auth", "mfa", "dsr", "privacy"]
---

# ADR-S11-001 — MFA Step-Up Required Only on Destructive DSR Arms

## Status

Accepted — 2026-05-13.

## Context

WI-S11-001 (DSR API) ships 7 endpoints across 4 request kinds: `access`,
`portability`, `erasure`, `rectification`, `restriction`, `objection`,
`confirmation`. LGPD Art. 18 + GDPR Art. 12-22 require the controller
to verify the subject's identity before fulfilling. The question:
**which arms warrant MFA step-up?**

Two extremes:
- (a) MFA step-up on every DSR endpoint → friction cost: ~25% drop-off
  observed in DSR studies (OneTrust 2024 benchmark); inflates SLO
  resolution clock.
- (b) Never step-up → regulatory exposure: an attacker with stolen
  primary credentials can issue an erasure DSR, irreversibly destroying
  the victim's data; GDPR Art. 32 "appropriate security" finding.

## Decision

**MFA step-up required only on irreversible / destructive arms**:
`erasure`, `rectification`, `restriction` (the 3 arms that mutate the
subject's record in a way that affects future processing).

**Non-destructive arms** (`access`, `portability`, `confirmation`,
`objection`) require only primary credentials + signed challenge token
+ existing session (3-factor in aggregate but no step-up prompt).

## Rationale

- **Erasure** is structurally irreversible cross-backend (12 backends,
  Stripe customer pseudonymize NOT delete per GAAP). A forged erasure
  is the worst-case attack.
- **Rectification** mutates downstream cached/derived state (consent
  ledger, audit metadata pseudonyms); forgery enables targeted
  poisoning.
- **Restriction** flips legal_hold flags affecting fiscal exception
  retention windows; forgery enables retention attacks.
- **Access** and **portability** return data the subject already has
  primary-credentialed access to via the product UI; step-up adds
  friction without reducing attack surface.

## Consequences

**Positive**: ~75% of DSR requests (read-only) flow without step-up
friction; erasure SLA clock (30d) starts fresh.

**Negative**: erasure-via-forged-MFA attack surface still exists if
attacker pwns BOTH primary creds AND second factor. Mitigated by
WebAuthn L3 mandatory for the second factor (`auth.webauthn` crate)
and audit trail capturing every step-up attempt.

**Forbidden**: step-up on `access` / `portability` / `confirmation`
arms (this ADR commits to NOT adding friction there).

## References

- WI-S11-001 §6 scope
- privacy_model.md §5.3 verification challenge
- security_model.md §4.2 step-up
- LGPD Art. 18, GDPR Art. 12-22
