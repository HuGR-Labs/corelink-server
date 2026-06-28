---
type: "CrateCluster"
title: "Privacy/compliance crate cluster"
description: "The GDPR/LGPD machinery — the DSR rights orchestrator with MFA-gated destructive arms, the multi-backend erasure worker, and the Ed25519/JCS erasure attestation signing primitive (produce/persist/serve DEFERRED)."
source_files:
  - "crates/corelink-privacy/src/lib.rs"
  - "crates/corelink-dsr/src/lib.rs"
  - "crates/corelink-erasure-attestation/src/lib.rs"
  - "crates/corelink-erasure-attestation/src/attestation.rs"
  - "crates/corelink-erasure-attestation/src/key.rs"
checkpoint_sha: "30ec21dc78d79c85f4d7e1e19e13118c66025c9e"
provenance: "AUTHORED"
tags: ["crates", "privacy", "dsr", "gdpr", "erasure", "compliance"]
timestamp: "2026-06-26T00:00:00Z"
---

# Privacy/compliance crate cluster

A multi-tenant cache that stores customer data carries regulatory obligations — right-to-erasure, data-subject requests, breach notification, consent, sub-processor disclosure — and this cluster is where CoreLink discharges them in code rather than in a policy PDF. It is grouped around a single posture distinction: privacy/DSR events are regulatory-grade and fail CLOSED (silent loss is never tolerated), unlike billing's fail-open arms. `corelink-privacy` is the aggregator over the 11 privacy primitives; `corelink-dsr` is the self-service rights orchestrator; `corelink-erasure-attestation` is the offline Ed25519 signing primitive for a destroy proof — STATUS: a crate-level primitive only (no production caller produces, persists, or serves an attestation yet; WI-S11-008).

# Role

The cluster backs the [DSR / erasure pipeline](/compliance/dsr-erasure.md) and the [erasure attestation](/compliance/erasure-attestation.md). It receives data-subject requests, gates the destructive ones behind step-up MFA, and fans an erasure across the backend topology. The independently-verifiable signed attestation that would let a customer prove destruction without trusting CoreLink's word is DEFERRED (WI-S11-008): the signing primitive exists in-crate but is not yet produced, persisted, or served on any live path.

# How it works

- `corelink-privacy` is an Option-A aggregator re-exporting the 11 privacy primitives (dsr, statuspage, breach, consent, erasure, notice, pseudonymize, residency, sub_processor, dpa::acceptance, dpa::versioning) at canonical `corelink_privacy::*` paths (`crates/corelink-privacy/src/lib.rs:1-20`, `crates/corelink-privacy/src/lib.rs:32-65`).
- `corelink-dsr` exposes the 6-arm rights taxonomy (Access / Portability / Rectification / Erasure / Restriction / Objection) with per-jurisdiction SLA mapping (LGPD/GDPR/CCPA) and a UUIDv7 request id (`crates/corelink-dsr/src/lib.rs:30-43`).
- It gates MFA only on the destructive arms — Erasure + Rectification require step-up; Access/Portability/Restriction/Objection do not — per the ADR-S11-001 friction-vs-security trade-off (`crates/corelink-dsr/src/lib.rs:62-66`).
- `corelink-erasure-attestation` provides the per-region Ed25519 (FIPS 186-5) **signing primitive**: `ErasureAttestationSigner::sign` JCS-canonicalizes (RFC 8785) the payload and returns an in-memory `ErasureAttestation { signature_ed25519, canonical_payload_jcs }` (`crates/corelink-erasure-attestation/src/attestation.rs:94-130`). STATUS: this is an offline crypto primitive only — the crate has NO production caller (its sole user is the container's test-only module under `routes/dsr/`), writes NO R2 object, indexes NO D1 row, and serves NO public key. The R2-7y / D1-index / public-key-serving wiring described in the crate's module doc-comment (`crates/corelink-erasure-attestation/src/lib.rs:1-33`) is the DEFERRED design target (WI-S11-008), not shipped code.

# Invariants

- DSR audit is fail-CLOSED: the `FailingDsrAuditSink` envelope models that DSR is regulatory-grade and NEVER tolerates silent loss, distinct from billing's fail-open split-tier (`crates/corelink-dsr/src/lib.rs:44-53`).
- Audit emits before store mutation on the DSR run pipeline — `request_received` is recorded before any insert (`crates/corelink-dsr/src/lib.rs:73-75`).
- `INV-ERASURE-ATTESTATION-SIGNED` (DEFERRED — not yet enforced anywhere). The design TARGET is "every BYOK-tenant erasure produces exactly one Ed25519-signed attestation, persisted in R2 (7y) and indexed in D1". STATUS: only the signing primitive is shipped — `ErasureAttestationSigner::sign` deterministically (JCS) signs a payload in memory and is covered by the crate's tests (`crates/corelink-erasure-attestation/src/attestation.rs:94-130`). NO production path produces / persists / indexes / serves an attestation (WI-S11-008). The invariant doc-comment (`crates/corelink-erasure-attestation/src/lib.rs:35-39`) names `ErasureAttester::attest_erasure` as its enforcer — that symbol does NOT exist anywhere in the tree (it is aspirational doc text, not an executed enforcer).
- The signing key zeroizes on drop and never appears in logs/traces/errors; verify is constant-time via `ed25519-dalek` — the real `#[derive(ZeroizeOnDrop)] pub struct ErasureSigningKey` is in `crates/corelink-erasure-attestation/src/key.rs:21-22`.

# Gotchas

- The Ed25519 attestation types are also re-exported at `corelink_crypto::ed25519::attestation` (`crates/corelink-erasure-attestation/src/lib.rs:51-62`); `corelink-privacy` re-exports the erasure worker by reference and deliberately does NOT touch the crypto path.
- Per-region signing keys are DESIGNED to rotate every 30 days with a 30-day overlap window where both Active and Overlap public keys are served (so a verifier holding the pre-rotation key can still verify post-rotation signatures) — this rotation + public-key-serving wiring is part of the DEFERRED served-attestation path, not live today.
- These crates ship the pure-logic skeleton + in-memory fakes; production wiring (WI-S11-008) binds the Neon `dsr_tickets` mirror, the real RS256 JWT receipt issuer, the 12-backend erasure transports, and the produce/persist/serve path for the erasure attestation.

# Citations

1. `crates/corelink-privacy/src/lib.rs:1-20` — the single-import aggregator over the 11 privacy primitives.
2. `crates/corelink-privacy/src/lib.rs:32-65` — the absorbed-crate list (dsr/breach/consent/erasure/residency/dpa…).
3. `crates/corelink-dsr/src/lib.rs:30-43` — the 6-arm DSR taxonomy + per-jurisdiction SLA + UUIDv7 id.
4. `crates/corelink-dsr/src/lib.rs:44-53` — DSR audit fail-CLOSED (regulatory-grade, no silent loss).
5. `crates/corelink-dsr/src/lib.rs:62-66` — MFA gate on destructive arms only (Erasure + Rectification).
6. `crates/corelink-dsr/src/lib.rs:73-75` — audit-emit-before-store-insert in the run pipeline.
7. `crates/corelink-erasure-attestation/src/attestation.rs:94-130` — the EXECUTED `ErasureAttestationSigner::sign` (JCS canonicalize → Ed25519 sign → in-memory `ErasureAttestation`); the in-prose R2-7y / public-key-serve / 30d-rotation described in the `crates/corelink-erasure-attestation/src/lib.rs:1-33` doc-comment is DEFERRED.
8. `crates/corelink-erasure-attestation/src/lib.rs:35-39` — the `INV-ERASURE-ATTESTATION-SIGNED` doc-comment (design target); names the FABRICATED `ErasureAttester::attest_erasure` enforcer — no such symbol exists, no production produce/persist path.
9. `crates/corelink-erasure-attestation/src/key.rs:21-22` — the real `#[derive(ZeroizeOnDrop)] ErasureSigningKey` (key zeroize-on-drop; no key material in logs); verify is constant-time via `ed25519-dalek`.
