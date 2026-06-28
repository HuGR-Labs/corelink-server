---
type: "ADR"
title: "ADR-S11-001 — MFA step-up required only on destructive DSR arms"
description: "Why DSR MFA step-up is required only on the irreversible arms (erasure, rectification, restriction) and forbidden on the read-only arms."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-001-mfa-step-up-destructive-arms-only.md"
checkpoint_sha: "0aad76e1d132cd98d35c814a5bb23008c226d08e"
provenance: "AUTHORED"
tags: ["adr", "s11", "auth", "mfa", "dsr", "privacy"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-001 — MFA step-up required only on destructive DSR arms

The DSR API (WI-S11-001) ships seven endpoints across the LGPD/GDPR request kinds, and the controller
must verify the subject's identity before fulfilling. This ADR settles *which* arms warrant an MFA
step-up by rejecting both extremes — step-up on every endpoint (≈25% drop-off friction, inflates the
SLO clock) and never stepping up (an attacker with stolen primary creds could issue an irreversible
erasure). It draws the line at destructiveness: step-up only where the operation mutates the subject's
record in a way that affects future processing.

# Context

LGPD Art. 18 + GDPR Art. 12–22 require identity verification before fulfilling a DSR. Step-up on every
arm imposes a documented ~25% drop-off and inflates the resolution clock; never stepping up leaves an
erasure-via-stolen-credentials path that is a GDPR Art. 32 "appropriate security" finding.

# Decision

**MFA step-up is required only on the irreversible / destructive arms** — `erasure` and `rectification`
(the two that mutate the subject's record in a way affecting future processing).
Erasure is structurally irreversible cross-backend (e.g. Stripe customer pseudonymize, not delete) and
rectification mutates downstream cached/derived state — so a forged one is the worst case. The
**non-destructive arms** (`access`, `portability`, `confirmation`, `objection`, `restriction`) do not
trigger a step-up prompt, because they return or policy-flag data without irreversibly mutating the
subject's stored record; step-up there would add friction without reducing attack surface.

> **Status vs shipped code (2026-06-28):** the spec text above originally listed `restriction` among
> the destructive arms; the shipped enforcement does NOT. The live `DsrRequestKind::is_destructive()`
> predicate that gates the MFA step-up is `matches!(self, Self::Erasure | Self::Rectification)` — exactly
> **two** arms (event.rs lines 90-92, in the `corelink-dsr` crate). `Restriction` is treated as
> policy-only and SKIPS the MFA step-up; this ADR has been reconciled to that 2-arm reality.

# Consequences

- Positive: ~75% of DSR requests (read-only) flow without step-up friction and the 30-day erasure SLA
  clock starts fresh.
- Negative: an erasure-via-forged-MFA surface remains if an attacker owns *both* primary creds and the
  second factor; it is mitigated by mandatory WebAuthn L3 for the second factor and an audit trail
  capturing every step-up attempt.
- Forbidden by this ADR: adding step-up on the `access` / `portability` / `confirmation` arms — it
  commits to NOT adding friction there.

# Citations

1. `specs/03_architecture/adrs/ADR-S11-001-mfa-step-up-destructive-arms-only.md:23-37` — the Context:
   the verification requirement and the two rejected extremes (always-on vs never).
2. `specs/03_architecture/adrs/ADR-S11-001-mfa-step-up-destructive-arms-only.md:39-47` — the Decision:
   step-up only on erasure/rectification/restriction; read-only arms need no prompt.
3. `specs/03_architecture/adrs/ADR-S11-001-mfa-step-up-destructive-arms-only.md:63-74` — the
   Consequences: the friction win, the residual two-factor-compromise surface, and the forbidden cases.
