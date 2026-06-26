---
type: "ADR"
title: "ADR-0032 — WebAuthn Level 3 admin step-up + AAGUID + OTP recovery"
description: "Why admin operations are gated by phishing-resistant WebAuthn L3 via webauthn-rs, with a closed-default AAGUID allowlist, a bound step-up token, and 6-digit OTP (not magic-link) recovery."
source_files:
  - "specs/03_architecture/adrs/ADR-0032-webauthn-level3.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "webauthn", "passkey", "mfa", "step-up", "auth", "s03"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0032 — WebAuthn Level 3 admin step-up + AAGUID + OTP recovery

Every destructive admin operation — mass revoke, billing change, tenant delete, admin user management — must be gated by phishing-resistant MFA. This ADR records the WebAuthn Level 3 strategy: lean on the audited `webauthn-rs` crate rather than hand-rolling CBOR/COSE, enforce a closed-default authenticator policy, reuse a short-lived step-up token across an admin session, and make magic-link recovery type-level unrepresentable.

# Context

The auth model mandates phishing-resistant MFA on every admin op, constrained by five CRITICAL/HIGH invariants — UV-required-for-admin (user-presence alone is insufficient), attestation-verified registration, monotonic sign-count, exact-match origin allowlist, and a canonical eTLD+1 RP-ID (ADR-0032:34-47).

# Decision

The canonical questions are resolved by using `webauthn-rs = 0.5` behind a trait surface (hand-rolling the L3 ceremony would forfeit security posture), a closed-default explicit AAGUID allowlist with a precedence denylist (FIDO MDS enriches the trace but does not gate accept/reject), a 5-minute step-up token bound to `(user_id, op_class, credential_id)` so one ceremony covers a session, and a lost-device recovery flow that is Clerk-SSO email + a 6-digit OTP — magic-link recovery is rejected as phishing-prone and the `RecoveryChannel` enum has exactly one variant so it is unrepresentable at the type level (ADR-0032:50-91). The OTP is Argon2id-hashed, single-use, rate-limited and TTL-bounded, and the COSE algorithm policy accepts only ES256/EdDSA/RS256, rejecting the `alg: 0` "none" attack (ADR-0032:154-188).

# Consequences

Every WebAuthn invariant is enforced algorithmically by the in-memory engine and adversarial regressions, the trait surface freezes the production-shim contract, and the type system blocks any future drift toward magic-link recovery, traded against Argon2id mint+verify being the wall-clock bottleneck of the recovery-OTP property tests (~200 ms/cycle, bounding that proptest to 100 iterations) (ADR-0032:190-219). It governs the [D1 PAT store](/auth/d1-pat-store.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0032-webauthn-level3.md:34-47` — the five WebAuthn load-bearing invariants (Context).
2. `specs/03_architecture/adrs/ADR-0032-webauthn-level3.md:50-91` — webauthn-rs over hand-roll, closed-default AAGUID, bound step-up token, OTP-not-magic-link recovery (Decision).
3. `specs/03_architecture/adrs/ADR-0032-webauthn-level3.md:154-188` — Argon2id OTP params, single-use/rate-limit, COSE alg-none rejection (Decision).
4. `specs/03_architecture/adrs/ADR-0032-webauthn-level3.md:190-219` — invariants enforced + frozen trait contract vs Argon2id proptest cost (Consequences).
