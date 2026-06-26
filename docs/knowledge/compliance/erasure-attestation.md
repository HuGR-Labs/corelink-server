---
type: "ComplianceControl"
title: "Ed25519/JCS erasure attestation"
description: "How a BYOK DSR erasure produces an Ed25519-signed, RFC 8785 JCS-canonicalized receipt that a data subject or auditor can verify offline."
source_files:
  - specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md
  - crates/corelink-erasure-attestation/src/lib.rs
  - crates/corelink-erasure-attestation/src/attestation.rs
  - crates/corelink-erasure-attestation/src/key.rs
  - crates/corelink-erasure-attestation/src/verify.rs
  - crates/corelink-erasure-attestation/src/evidence.rs
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["compliance", "erasure", "ed25519", "jcs", "rfc-8785", "nist-sp-800-88", "gdpr-art-17", "byok", "dsr"]
timestamp: "2026-06-26T00:00:00Z"
---

When a BYOK tenant files a DSR erasure request, CoreLink does not merely promise it deleted the data — it destroys CMK access through the KMS provider and emits a cryptographically signed receipt proving the destroy occurred. That receipt is an Ed25519 signature over an RFC 8785 JCS-canonicalized payload, persisted to an R2 audit bucket with 7-year retention and indexed in D1, so the customer and any external auditor can verify it offline with standard Ed25519 tooling and no access to CoreLink infrastructure. This is the control that turns GDPR Art. 17 / LGPD erasure and NIST SP 800-88 Rev.1 §2.4 crypto-erase from a claim into a portable, tamper-evident proof. The sibling control `compliance/dsr-erasure` covers the request orchestration that drives this attestation.

# Role

This crate is the pure-logic signing and verification surface for erasure attestations: it canonicalizes the erasure payload, signs it with a per-region Ed25519 key, derives the public key served to verifiers, and validates signatures offline. It owns the cryptographic shape of the proof — not the KMS destroy itself nor the request lifecycle.

# How it works

- A BYOK erasure destroys CMK access via the KMS provider API, then emits one Ed25519-signed attestation persisted to R2 (7y retention) and indexed in D1 for the public verify endpoint `crates/corelink-erasure-attestation/src/lib.rs:5-14`.
- The signed payload carries `tenant_id`, `request_id`, `destroyed_ts`, KMS provider/key id, `evidence_hash`, region, and `attestation_key_id` — every field is included in the canonical JSON `crates/corelink-erasure-attestation/src/attestation.rs:17-45`.
- Signing first JCS-canonicalizes the payload via `serde_jcs::to_string` (RFC 8785) `crates/corelink-erasure-attestation/src/attestation.rs:99`, then signs those canonical bytes with Ed25519 `crates/corelink-erasure-attestation/src/attestation.rs:103`, then base64-encodes the 64-byte signature `crates/corelink-erasure-attestation/src/attestation.rs:104`.
- The resulting `ErasureAttestation` stores the payload, the base64 signature, and the exact JCS byte string that was signed, which a verifier MUST verify against `crates/corelink-erasure-attestation/src/attestation.rs:52-63`.
- `evidence_hash` is a SHA-256 over the JCS-canonical serialization of an `EvidenceBundle` binding audit-chain segment IDs + KMS destroy timestamp + KMS key id + tenant id `crates/corelink-erasure-attestation/src/evidence.rs:52-61`.
- The bundle is validated before hashing — empty segment list, KMS key id, or tenant id are rejected so an incomplete evidence bundle cannot be attested `crates/corelink-erasure-attestation/src/evidence.rs:68-85`.
- Per-region signing keys are generated from the OS CSPRNG (`OsRng`) `crates/corelink-erasure-attestation/src/key.rs:72`, or reconstructed deterministically from a 32-byte secret seed so the served public key stays stable across Workers/container restarts `crates/corelink-erasure-attestation/src/key.rs:95-110`.
- `public_key()` derives the verifying key and a PEM SubjectPublicKeyInfo (RFC 8410 Ed25519 OID prefix) served at `GET /v1/public/keys/erasure/{region}.pub` `crates/corelink-erasure-attestation/src/key.rs:114-125`.
- Offline verification decodes the base64 signature, enforces exactly 64 bytes, and verifies it against the stored canonical JCS bytes using the public key `crates/corelink-erasure-attestation/src/verify.rs:46-60`.
- Ed25519 was chosen over RSA-2048/ECDSA P-256: 64-byte signatures (cheap for 7y R2 retention), FIPS 186-5 approved, constant-time verify `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:39-50`.
- Keys rotate every 30 days with a 30d overlap window during which both Active and Overlap public keys are served, so a verifier holding a pre-rotation key still verifies post-rotation attestations `crates/corelink-erasure-attestation/src/lib.rs:27-33`.

# Invariants

- INV-ERASURE-ATTESTATION-SIGNED (HIGH): every DSR erasure of a BYOK tenant MUST produce exactly one Ed25519-signed attestation persisted in R2 (7y) and indexed in D1 `crates/corelink-erasure-attestation/src/lib.rs:35-39`.
- The signature MUST be verified against the exact `canonical_payload_jcs` byte string, not a re-serialized payload `crates/corelink-erasure-attestation/src/attestation.rs:60-62`.
- Identical payloads always canonicalize byte-identically (RFC 8785 determinism), so a signature is stable and reproducible `crates/corelink-erasure-attestation/src/attestation.rs:98-100`.
- Signing key material is zeroized on drop and never appears in logs, traces, or `Debug` output — the `signing_key` field renders as `[REDACTED]` `crates/corelink-erasure-attestation/src/key.rs:41-52`.
- A signature that does not decode to exactly 64 bytes, or fails Ed25519 verification, is rejected as a verify error `crates/corelink-erasure-attestation/src/verify.rs:50-60`.
- An attestation cannot be produced over an incomplete evidence bundle — validation fails closed on any empty mandatory field `crates/corelink-erasure-attestation/src/evidence.rs:68-85`.
- 30d overlap is the hard upper bound and the chosen window for erasure keys: a customer verifying any time within 30d of rotation is always covered `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:65-79`.
- Attestations are retained 7 years to span SOC 2 + GDPR Art. 17 + LGPD audit windows `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:28`.

# Gotchas

- Verifiers must select the correct public key from the endpoint list using `attestation_key_id` — `verify_attestation_signature` does not auto-select; passing the wrong key yields a verify error, not a silent pass `crates/corelink-erasure-attestation/src/verify.rs:41-60`.
- RFC 8785 JCS includes Unicode NFC normalization, which is what prevents canonicalization-bypass attacks; a custom canonicalizer was explicitly rejected `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:52-63`.
- `EvidenceBundle::compute_hash` falls back to `serde_json` if `serde_jcs` fails, but that branch is treated as unreachable (a property test guards JCS); rely on `validated_hash` for the fail-closed checks `crates/corelink-erasure-attestation/src/evidence.rs:52-61`.
- `from_seed` reproduces the same keypair on every process start; the seed is the raw 32-byte Ed25519 secret scalar and MUST stay out of logs/Debug/errors — only `generate` is random/ephemeral `crates/corelink-erasure-attestation/src/key.rs:82-110`.
- Old public keys remain served for the 30d overlap but accept no new signatures; emergency rotation zeroizes the old key and publishes a security notice `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:93-96`.

# Citations

- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:28` — 7-year retention rationale.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:39-50` — Ed25519 over RSA/ECDSA decision.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:52-63` — RFC 8785 JCS choice + NFC.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:65-79` — 30d overlap decision.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:93-96` — old-key read-only / emergency rotation.
- `crates/corelink-erasure-attestation/src/lib.rs:5-14` — destroy → sign → R2 → D1 flow.
- `crates/corelink-erasure-attestation/src/lib.rs:27-33` — 30d key rotation / overlap serving.
- `crates/corelink-erasure-attestation/src/lib.rs:35-39` — INV-ERASURE-ATTESTATION-SIGNED.
- `crates/corelink-erasure-attestation/src/attestation.rs:17-45` — signed payload fields.
- `crates/corelink-erasure-attestation/src/attestation.rs:52-63` — `ErasureAttestation` (canonical bytes + signature).
- `crates/corelink-erasure-attestation/src/attestation.rs:98-104` — JCS canonicalize + Ed25519 sign + base64.
- `crates/corelink-erasure-attestation/src/key.rs:41-52` — Debug redaction.
- `crates/corelink-erasure-attestation/src/key.rs:72` — OsRng key generation.
- `crates/corelink-erasure-attestation/src/key.rs:82-110` — deterministic `from_seed`.
- `crates/corelink-erasure-attestation/src/key.rs:114-125` — public key / PEM derivation.
- `crates/corelink-erasure-attestation/src/verify.rs:41-60` — offline verify path.
- `crates/corelink-erasure-attestation/src/evidence.rs:52-61` — evidence SHA-256 over JCS.
- `crates/corelink-erasure-attestation/src/evidence.rs:68-85` — fail-closed bundle validation.
