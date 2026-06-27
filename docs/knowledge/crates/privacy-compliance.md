---
type: "CrateCluster"
title: "Privacy/compliance crate cluster"
description: "The GDPR/LGPD machinery — the DSR rights orchestrator with MFA-gated destructive arms, the multi-backend erasure worker, and the Ed25519/JCS erasure attestation."
source_files:
  - "crates/corelink-privacy/src/lib.rs"
  - "crates/corelink-dsr/src/lib.rs"
  - "crates/corelink-dsr/src/mfa.rs"
  - "crates/corelink-dsr/src/endpoint.rs"
  - "crates/corelink-dsr/src/audit.rs"
  - "crates/corelink-erasure-attestation/src/lib.rs"
  - "crates/corelink-erasure-attestation/src/key.rs"
  - "crates/corelink-container/src/routes/dsr/attestation.rs"
  - "migrations/d1/0032_erasure_attestation.sql"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["crates", "privacy", "dsr", "gdpr", "erasure", "compliance"]
timestamp: "2026-06-26T00:00:00Z"
---

# Privacy/compliance crate cluster

A multi-tenant cache that stores customer data carries regulatory obligations — right-to-erasure, data-subject requests, breach notification, consent, sub-processor disclosure — and this cluster is where CoreLink discharges them in code rather than in a policy PDF. It is grouped around a single posture distinction: privacy/DSR events are regulatory-grade and fail CLOSED (silent loss is never tolerated), unlike billing's fail-open arms. `corelink-privacy` is the aggregator over the 11 privacy primitives; `corelink-dsr` is the self-service rights orchestrator; `corelink-erasure-attestation` produces the cryptographic proof that a destroy actually happened.

# Role

The cluster backs the [DSR / erasure pipeline](/compliance/dsr-erasure.md) and the [erasure attestation](/compliance/erasure-attestation.md). It receives data-subject requests, gates the destructive ones behind step-up MFA, fans an erasure across the backend topology, and emits an independently-verifiable signed attestation so a customer can prove their data was destroyed without trusting CoreLink's word.

# How it works

- `corelink-privacy` is an Option-A aggregator re-exporting the 11 privacy primitives (dsr, statuspage, breach, consent, erasure, notice, pseudonymize, residency, sub_processor, dpa::acceptance, dpa::versioning) at canonical `corelink_privacy::*` paths (`crates/corelink-privacy/src/lib.rs:1-20`, `crates/corelink-privacy/src/lib.rs:32-65`).
- `corelink-dsr` exposes the 6-arm rights taxonomy (Access / Portability / Rectification / Erasure / Restriction / Objection) with per-jurisdiction SLA mapping (LGPD/GDPR/CCPA) and a UUIDv7 request id (`crates/corelink-dsr/src/lib.rs:30-43`).
- It gates MFA only on the destructive arms — Erasure + Rectification require step-up; Access/Portability/Restriction/Objection do not — per the ADR-S11-001 friction-vs-security trade-off. The `MfaStepUpVerifier` contract treats a `None` token on a destructive arm as `Required` (`crates/corelink-dsr/src/mfa.rs:80-103`); the orchestrator enforces it by branching on `request.is_destructive()` and returning `DsrDecision::MfaRequired` before any store insert (`crates/corelink-dsr/src/endpoint.rs:354-367`).
- `corelink-erasure-attestation` signs a destroy proof with per-region Ed25519 (FIPS 186-5) over RFC 8785 JCS-canonical JSON (`crates/corelink-erasure-attestation/src/lib.rs:1-33`). The crate `//!` DESIGNS R2 7-year persistence + a public-key serving endpoint, but neither is wired: the live consumer writes only a D1 **metadata index row** (no signature column — `migrations/d1/0032_erasure_attestation.sql:15-27`), performs no R2 `PutObject`, and registers no public endpoint (the R2-7y persistence + serving are deferred WI-S11-008 — see `compliance/erasure-attestation`).

# Invariants

- DSR audit is fail-CLOSED: every `self.audit.emit(...)?` in the orchestrator `?`-propagates, so an emit failure aborts the arm before any store mutation — DSR is regulatory-grade and NEVER tolerates silent loss, distinct from billing's fail-open split-tier (`crates/corelink-dsr/src/endpoint.rs:312-318`); the `FailingDsrAuditSink` test fixture pins this fail-CLOSED contract in CI (`crates/corelink-dsr/src/audit.rs:279-295`).
- Audit emits before store mutation on the DSR run pipeline — the orchestrator emits `request_received` BEFORE the idempotency lookup or any insert (`crates/corelink-dsr/src/endpoint.rs:307-318`).
- `INV-ERASURE-ATTESTATION-SIGNED`: this crate only provides the signer/payload surface (`crates/corelink-erasure-attestation/src/lib.rs:35-39`) — and the `//!` names a `ErasureAttester::attest_erasure` enforcer that **does not exist** (the real enforcer is the container free function `attestation::sign_and_persist`, `crates/corelink-container/src/routes/dsr/attestation.rs:108-220`). The binding is enforced **cross-crate** by that consumer, which signs on a `VerifiedComplete` erasure, fail-OPEN (skipping if the seed/region is absent). Correct the `//!` framing two ways: (1) it is a **non-BYOK D1/R2/Stripe delete-set for ANY tenant**, NOT "every BYOK-tenant erasure" — it records `kms_provider="corelink_d1r2_erase"` and does no KMS CMK destroy (`crates/corelink-container/src/routes/dsr/attestation.rs:10-14`, `:49-51`); (2) "persisted in R2 (7y) and indexed in D1" is only **half-live** — the D1 row is a **metadata index only** (no signature/JCS column — `migrations/d1/0032_erasure_attestation.sql:15-27`) and the R2 7-year persistence is unwired (deferred WI-S11-008). So the invariant is enforced neither inside this crate nor (for the R2/persistence half) anywhere live.
- The signing key zeroizes on drop and never appears in logs/traces/errors: `ErasureSigningKey` derives `ZeroizeOnDrop` and its `Debug` redacts the key bytes (`crates/corelink-erasure-attestation/src/key.rs:21-49`); verify is constant-time via `ed25519-dalek`.

# Gotchas

- The Ed25519 signing primitive lives at `corelink_crypto::ed25519::attestation`, not inside the privacy aggregator — `corelink-privacy` re-exports the erasure worker by reference and deliberately does NOT touch the crypto path.
- Per-region signing keys rotate every 30 days with a 30-day overlap window where both Active and Overlap public keys are served, so a verifier holding the pre-rotation key can still verify post-rotation signatures.
- These crates ship the pure-logic skeleton + in-memory fakes; production wiring (WI-S11-008) binds the Neon `dsr_tickets` mirror, the real RS256 JWT receipt issuer, and the 12-backend erasure transports.

# Citations

1. `crates/corelink-privacy/src/lib.rs:1-20` — the single-import aggregator over the 11 privacy primitives.
2. `crates/corelink-privacy/src/lib.rs:32-65` — the absorbed-crate list (dsr/breach/consent/erasure/residency/dpa…).
3. `crates/corelink-dsr/src/lib.rs:30-43` — the 6-arm DSR taxonomy + per-jurisdiction SLA + UUIDv7 id.
4. `crates/corelink-dsr/src/audit.rs:279-295` — `FailingDsrAuditSink`: the test fixture pinning the DSR-audit fail-CLOSED contract (the real enforcer is the `?` propagation at `crates/corelink-dsr/src/endpoint.rs:312-318`).
5. `crates/corelink-dsr/src/mfa.rs:80-103` — `MfaStepUpVerifier` contract: `None` token on a destructive arm → `Required`.
6. `crates/corelink-dsr/src/endpoint.rs:354-367` — MFA gate on destructive arms only (`is_destructive` → `MfaRequired`, no insert).
7. `crates/corelink-dsr/src/endpoint.rs:307-318` — `request_received` audit emitted BEFORE the idempotency lookup / store insert.
7. `crates/corelink-erasure-attestation/src/lib.rs:1-33` — Ed25519/JCS attestation purpose; the R2-7y persistence + public-key serving it describes are DESIGN, not live (deferred WI-S11-008).
8. `crates/corelink-erasure-attestation/src/lib.rs:35-39` — the signer/payload surface for `INV-ERASURE-ATTESTATION-SIGNED`; names a non-existent `ErasureAttester::attest_erasure` enforcer. The real (fail-OPEN, non-BYOK, metadata-index-only) enforcer is the consumer below.
10. `crates/corelink-container/src/routes/dsr/attestation.rs:10-14`, `:108-220` — `sign_and_persist`: the real cross-crate enforcer; D1/R2/Stripe delete-set (`kms_provider="corelink_d1r2_erase"`, no CMK destroy), writes a metadata-only D1 index row.
11. `migrations/d1/0032_erasure_attestation.sql:15-27` — `erasure_attestations` schema: metadata index only, NO signature / NO JCS-payload column; R2 7y deferred.
9. `crates/corelink-erasure-attestation/src/key.rs:21-49` — `ErasureSigningKey`: `ZeroizeOnDrop` derive + redacting `Debug` (no key material in logs).
