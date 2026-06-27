---
type: "ADR"
title: "ADR-S11-001 — MFA step-up required only on destructive DSR arms"
description: "Why DSR MFA step-up is required only on the irreversible arms (erasure, rectification, restriction) and forbidden on the read-only arms."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-001-mfa-step-up-destructive-arms-only.md"
  - "crates/corelink-container/src/main.rs"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
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

**MFA step-up is required only on the irreversible / destructive arms** — `erasure`, `rectification`,
`restriction` (the three that mutate the subject's record in a way affecting future processing).
Erasure is structurally irreversible cross-backend (e.g. Stripe customer pseudonymize, not delete),
rectification mutates downstream cached/derived state, and restriction flips legal-hold flags affecting
retention windows — so a forged one is the worst case. The **non-destructive arms** (`access`,
`portability`, `confirmation`, `objection`) require only primary credentials + a signed challenge token
+ an existing session (three factors in aggregate, no step-up prompt), because they return data the
subject already has primary-credentialed UI access to and step-up there would add friction without
reducing attack surface.

# Consequences

- Positive: ~75% of DSR requests (read-only) flow without step-up friction and the 30-day erasure SLA
  clock starts fresh.
- Negative: an erasure-via-forged-MFA surface remains if an attacker owns *both* primary creds and the
  second factor; it is mitigated by mandatory WebAuthn L3 for the second factor and an audit trail
  capturing every step-up attempt.
- Forbidden by this ADR: adding step-up on the `access` / `portability` / `confirmation` arms — it
  commits to NOT adding friction there.

# Status vs shipped code

The "seven-endpoint DSR API" this ADR gates is **designed, not shipped**. The deployed container exposes
the erasure path as a single internal route — `POST /_internal/dsr/erase`, gated by
`CORELINK_INTERNAL_AUTH_KEY` and driving the 12-backend erasure orchestrator
(`crates/corelink-container/src/main.rs:525-532`) — plus a per-hash CAS-erase route and the
Clerk-`user.deleted` account-delete enqueue; there is **no seven-arm public DSR API and no WebAuthn
MFA step-up wiring** in the live request path. The destructive-arms-only step-up rule is the
forward-looking policy for when that API is built; today the irreversible erasure path is guarded by an
internal-auth key, not by subject MFA step-up.

# Citations

1. `specs/03_architecture/adrs/ADR-S11-001-mfa-step-up-destructive-arms-only.md:23-37` — the Context:
   the verification requirement and the two rejected extremes (always-on vs never).
2. `specs/03_architecture/adrs/ADR-S11-001-mfa-step-up-destructive-arms-only.md:39-47` — the Decision:
   step-up only on erasure/rectification/restriction; read-only arms need no prompt.
3. `specs/03_architecture/adrs/ADR-S11-001-mfa-step-up-destructive-arms-only.md:63-74` — the
   Consequences: the friction win, the residual two-factor-compromise surface, and the forbidden cases.
