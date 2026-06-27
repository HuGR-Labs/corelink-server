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
checkpoint_sha: "d0e4f8bd669cb1e982f7511de9a895c602a5ee45"
provenance: "AUTHORED"
tags: ["crates", "privacy", "dsr", "gdpr", "erasure", "compliance"]
timestamp: "2026-06-26T00:00:00Z"
---

# Privacy/compliance crate cluster

A multi-tenant cache that stores customer data carries regulatory obligations — right-to-erasure, data-subject requests, breach notification, consent, sub-processor disclosure — and this cluster is where CoreLink discharges them in code rather than in a policy PDF. It is grouped around a single posture distinction: privacy/DSR events are regulatory-grade and fail CLOSED (silent loss is never tolerated), unlike billing's fail-open arms. `corelink-privacy` is the aggregator over the 11 privacy primitives; `corelink-dsr` is the self-service rights orchestrator; `corelink-erasure-attestation` is the pure-logic Ed25519/JCS signing surface for an erasure proof — but note the durable, offline-verifiable proof is **DEFERRED, not live** (see the [erasure attestation](/compliance/erasure-attestation.md) concept): the deployed consumer signs in-process best-effort/fail-OPEN and writes only a D1 **metadata index row** — the signature itself is dropped, there is no R2 7y persistence, and no public serving endpoint.

# Role

The cluster backs the [DSR / erasure pipeline](/compliance/dsr-erasure.md) and the [erasure attestation](/compliance/erasure-attestation.md). It receives data-subject requests, gates the destructive ones behind step-up MFA, fans an erasure across the backend topology, and — on `VerifiedComplete` — signs an Ed25519 attestation in-process. The DESIGN is an independently-verifiable signed proof a customer can verify offline without trusting CoreLink's word, but that durable, retrievable proof is **DEFERRED, not live**: today only a D1 metadata index row is written (the signature is dropped, no R2 persistence, no serving endpoint — WI-S11-008), so the live artifact is a status record, not a downloadable offline-verifiable receipt.

# How it works

- `corelink-privacy` is an Option-A aggregator re-exporting the 11 privacy primitives (dsr, statuspage, breach, consent, erasure, notice, pseudonymize, residency, sub_processor, dpa::acceptance, dpa::versioning) at canonical `corelink_privacy::*` paths (`crates/corelink-privacy/src/lib.rs:1-20`, `crates/corelink-privacy/src/lib.rs:32-65`).
- `corelink-dsr` exposes the 6-arm rights taxonomy (Access / Portability / Rectification / Erasure / Restriction / Objection) with per-jurisdiction SLA mapping (LGPD/GDPR/CCPA) and a UUIDv7 request id (`crates/corelink-dsr/src/lib.rs:30-43`).
- It gates MFA only on the destructive arms — Erasure + Rectification require step-up; Access/Portability/Restriction/Objection do not — per the ADR-S11-001 friction-vs-security trade-off. The `MfaStepUpVerifier` contract treats a `None` token on a destructive arm as `Required` (`crates/corelink-dsr/src/mfa.rs:80-103`); the orchestrator enforces it by branching on `request.is_destructive()` and returning `DsrDecision::MfaRequired` before any store insert (`crates/corelink-dsr/src/endpoint.rs:354-367`).
- `corelink-erasure-attestation` signs a destroy proof with per-region Ed25519 (FIPS 186-5) over RFC 8785 JCS-canonical JSON (`crates/corelink-erasure-attestation/src/lib.rs:1-33`). The crate `//!` DESIGNS R2 7-year persistence + a public-key serving endpoint, but neither is wired: the live consumer writes only a D1 **metadata index row** (no signature column — `migrations/d1/0032_erasure_attestation.sql:15-27`), performs no R2 `PutObject`, and registers no public endpoint (the R2-7y persistence + serving are deferred WI-S11-008 — see `compliance/erasure-attestation`).

# Invariants

- DSR audit is fail-CLOSED: every `self.audit.emit(...)?` in the orchestrator `?`-propagates, so an emit failure aborts the arm before any store mutation — DSR is regulatory-grade and NEVER tolerates silent loss, distinct from billing's fail-open split-tier (`crates/corelink-dsr/src/endpoint.rs:312-318`); the `FailingDsrAuditSink` test fixture pins this fail-CLOSED contract in CI (`crates/corelink-dsr/src/audit.rs:279-295`).
- Audit emits before store mutation on the DSR run pipeline — the orchestrator emits `request_received` BEFORE the idempotency lookup or any insert (`crates/corelink-dsr/src/endpoint.rs:307-318`).
- `INV-ERASURE-ATTESTATION-SIGNED`: the crate `//!` frames a fail-CLOSED, BYOK-only "every BYOK-tenant erasure MUST produce exactly one Ed25519-signed attestation, persisted in R2 (7y) and indexed in D1" HIGH invariant (`crates/corelink-erasure-attestation/src/lib.rs:35-39`) — **but that durable, served proof is DEFERRED, not the deployed reality** (consistent with `compliance/erasure-attestation`). The doc-comment also names a `ErasureAttester::attest_erasure` enforcer that **does not exist**; the real enforcer is the container free function `attestation::sign_and_persist` (`crates/corelink-container/src/routes/dsr/attestation.rs:108-220`), and reading it line-by-line shows the SHIPPED contract differs three ways from the `//!`: (1) it signs **best-effort / fail-OPEN** on the `VerifiedComplete` 24h sweep, silently `return`ing with **no attestation and no D1 row** when the seed/region is absent (`crates/corelink-container/src/routes/dsr/attestation.rs:114-127`) — not the fail-CLOSED "MUST produce exactly one"; (2) it runs for **ordinary tenants — NOT BYOK-only** — recording a **non-BYOK D1/R2/Stripe delete-set** with `kms_provider="corelink_d1r2_erase"` and **no KMS CMK destroy** (`crates/corelink-container/src/routes/dsr/attestation.rs:10-14`, `:49-51`); (3) the signature is **NOT durably persisted** — the consumer writes only a D1 **metadata index row** (`crates/corelink-container/src/routes/dsr/attestation.rs:191-212`) and `erasure_attestations` has **no signature / no JCS-payload column** (`migrations/d1/0032_erasure_attestation.sql:15-27`); there is **no R2 `PutObject`** and **no public serving endpoint** (deferred WI-S11-008). So the "signed + persisted in R2 + served" invariant is enforced **neither inside this crate nor anywhere live**; the live artifact is an in-process signature that is dropped, leaving a non-retrievable D1 status index. Absence of an attestation does NOT mean the erasure failed — the erasure is already complete + audited; the attestation is an extra (currently non-retrievable) evidence artifact.
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
8. `crates/corelink-erasure-attestation/src/lib.rs:1-33` — Ed25519/JCS attestation purpose; the R2-7y persistence + public-key serving it describes are DESIGN, not live (deferred WI-S11-008).
9. `crates/corelink-container/src/routes/dsr/attestation.rs:108-220` — `sign_and_persist`: the **executed** enforcer for `INV-ERASURE-ATTESTATION-SIGNED`. Signs in-process best-effort then writes a metadata-only D1 index row; the `//!` HIGH invariant (`crates/corelink-erasure-attestation/src/lib.rs:35-39`) it implements is DEFERRED-not-live (no fail-CLOSED guarantee, no R2, no serving) and names a non-existent `ErasureAttester::attest_erasure`.
10. `crates/corelink-container/src/routes/dsr/attestation.rs:114-127` — `sign_and_persist` fail-OPEN: seed unset/malformed or no resolvable region → silent `return`, NO attestation and NO D1 row written (so the contract is NOT fail-CLOSED "every erasure MUST produce exactly one").
11. `crates/corelink-container/src/routes/dsr/attestation.rs:10-14`, `:49-51`, `:191-212` — the non-BYOK D1/R2/Stripe delete-set (`kms_provider="corelink_d1r2_erase"`, no CMK destroy) + the metadata-only `erasure_attestations` / `erasure_public_keys` D1 upserts (the signature is dropped — no R2 `PutObject`).
12. `migrations/d1/0032_erasure_attestation.sql:15-27` — `erasure_attestations` schema: metadata index only, NO signature / NO JCS-payload column; R2 7y deferred.
13. `crates/corelink-erasure-attestation/src/key.rs:21-49` — `ErasureSigningKey`: `ZeroizeOnDrop` derive + redacting `Debug` (no key material in logs).
