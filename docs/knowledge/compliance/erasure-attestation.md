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
  - crates/corelink-container/src/routes/dsr/attestation.rs
  - crates/corelink-container/src/routes/public_attestation.rs
checkpoint_sha: "8af9ed65caf286d3f800e91d3f823face3aefd31"
provenance: "AUTHORED"
tags: ["compliance", "erasure", "ed25519", "jcs", "rfc-8785", "nist-sp-800-88", "gdpr-art-17", "byok", "dsr"]
timestamp: "2026-06-26T00:00:00Z"
---

When a tenant files a DSR erasure request, CoreLink does not merely promise it deleted the data — it emits a cryptographically signed receipt proving the destroy occurred. **CoreLink's launch erasure mechanism is a real D1 / R2 / Stripe delete-set, NOT a BYOK KMS crypto-erase**: the attestation records this truthfully, with `kms_provider = "corelink_d1r2_erase"` hardcoded and `kms_key_id = "dsr:{dsr_id}"` (`crates/corelink-container/src/routes/dsr/attestation.rs:11-14`). The signature format is generic enough to ALSO cover a future BYOK CMK-destroy mechanism, which is why the crate and ADR are named for KMS/CMK erasure — but do not read that naming as a claim that CoreLink calls out to a KMS provider today. The receipt itself is an Ed25519 signature over an RFC 8785 JCS-canonicalized payload, persisted to an R2 audit bucket with 7-year retention and indexed in D1, so the customer and any external auditor can verify it offline with standard Ed25519 tooling and no access to CoreLink infrastructure. This is the control that turns GDPR Art. 17 / LGPD erasure and NIST SP 800-88 Rev.1 §2.4 crypto-erase language into a portable, tamper-evident proof of the ACTUAL delete-set mechanism CoreLink runs at launch. The sibling control `compliance/dsr-erasure` covers the request orchestration that drives this attestation.

> ✅ Reality check (brutal-review H1 CLOSED, Artifact 1): the signed + served attestation is now REAL, not deferred. The pure-logic `corelink-erasure-attestation` crate (signing, JCS canonicalization, key derivation, offline verification) is wired into the container: on a bound `VerifiedComplete`, `sign_and_persist` produces a REAL Ed25519 signature over the JCS-canonical payload and persists it ALL-OR-NOTHING fail-CLOSED — region-gated (no silent mis-attribution) → R2 PUT FIRST → `erasure_public_keys` upsert → a signed D1 index row carrying BOTH `signature_ed25519` AND `canonical_payload_jcs` (migration 0079). It is served by `GET /v1/public/attestation/{request_id}` + `GET /v1/public/keys/erasure/{region}.pub` (the unauth `/v1/public/*` verifier), so a customer/auditor verifies the proof offline. The prior UNSIGNED-`evidence_hash` theater row is gone; the gate still REFUSES (no row) on incomplete/unverified evidence. The flow described below is now a live served proof.

# Role

This crate is the pure-logic signing and verification surface for erasure attestations: it canonicalizes the erasure payload, signs it with a per-region Ed25519 key, derives the public key served to verifiers, and validates signatures offline. It owns the cryptographic shape of the proof — not the KMS destroy itself nor the request lifecycle.

# How it works

- CoreLink's launch erasure runs a real D1/R2/Stripe delete-set (`kms_provider = "corelink_d1r2_erase"`, NOT a BYOK KMS crypto-erase), then emits one Ed25519-signed attestation persisted to R2 (7y retention) and indexed in D1 for the public verify endpoint `crates/corelink-container/src/routes/dsr/attestation.rs:11-14`, `crates/corelink-erasure-attestation/src/lib.rs:5-14`. The signature format is designed to also cover a future BYOK CMK-destroy mechanism, but no such KMS-provider call exists today.
- LIVE in the container (Artifact 1): on a bound `VerifiedComplete`, `sign_and_persist` signs the JCS payload and persists STRICTLY ordered + fail-CLOSED — R2 PUT FIRST (the signed bundle JSON), then the `erasure_public_keys` upsert (a served cert MUST have a verifiable pubkey BEFORE the index row that flips it to "served"), then the `erasure_attestations` index row carrying BOTH `signature_ed25519` AND `canonical_payload_jcs` (migration 0079); ANY step failing aborts WITHOUT the index row (never a dangling `r2_key`, never a row missing the signature). The verifier treats a row with either column NULL as "not a verifiable signed attestation" `crates/corelink-container/src/routes/dsr/attestation.rs:307-460`. The SAME signer infra is now ALSO reused for DSR portability (Art.20): `sign_export_digest` detaches an Ed25519 signature over a portability export bundle's SHA-256 content digest using the SAME per-region key the erasure verifier serves, so a customer verifies a portability receipt offline with the identical public key — fail-CLOSED to `None` (an unsigned inline bundle, never a forgeable certificate) when the region/seed cannot be trustworthily resolved `crates/corelink-container/src/routes/dsr/attestation.rs:472-502`.
- The signed payload carries `tenant_id`, `request_id`, `destroyed_ts`, KMS provider/key id, `evidence_hash`, region, and `attestation_key_id` — every field is included in the canonical JSON `crates/corelink-erasure-attestation/src/attestation.rs:17-45`.
- Signing first JCS-canonicalizes the payload via `serde_jcs::to_string` (RFC 8785) `crates/corelink-erasure-attestation/src/attestation.rs:99`, then signs those canonical bytes with Ed25519 `crates/corelink-erasure-attestation/src/attestation.rs:103`, then base64-encodes the 64-byte signature `crates/corelink-erasure-attestation/src/attestation.rs:104`.
- The resulting `ErasureAttestation` stores the payload, the base64 signature, and the exact JCS byte string that was signed, which a verifier MUST verify against `crates/corelink-erasure-attestation/src/attestation.rs:52-63`.
- `evidence_hash` is a SHA-256 over the JCS-canonical serialization of an `EvidenceBundle` binding audit-chain segment IDs + KMS destroy timestamp + KMS key id + tenant id `crates/corelink-erasure-attestation/src/evidence.rs:52-61`.
- The bundle is validated before hashing — empty segment list, KMS key id, or tenant id are rejected so an incomplete evidence bundle cannot be attested `crates/corelink-erasure-attestation/src/evidence.rs:68-85`.
- Per-region signing keys are generated from the OS CSPRNG (`getrandom::SysRng`, wrapped in `rand_core`'s `UnwrapErr` to satisfy ed25519-dalek 3's `rand_core 0.10` `CryptoRng` bound) `crates/corelink-erasure-attestation/src/key.rs:77`, or reconstructed deterministically from a 32-byte secret seed so the served public key stays stable across Workers/container restarts `crates/corelink-erasure-attestation/src/key.rs:100-115`.
- `public_key()` derives the verifying key and a PEM SubjectPublicKeyInfo (RFC 8410 Ed25519 OID prefix) served at `GET /v1/public/keys/erasure/{region}.pub` `crates/corelink-erasure-attestation/src/key.rs:119-130`.
- Offline verification FIRST binds the typed `payload` to the signed bytes — it re-canonicalizes `attestation.payload` with the SAME canonicalizer the signer uses (`serde_jcs`, RFC 8785 JCS) and requires BYTE-EQUALITY with `canonical_payload_jcs`, so an attacker who keeps a validly-signed canonical+signature but mutates `payload.tenant_id`/`request_id`/`region` is rejected (the H2 payload-substitution fix) — then decodes the base64 signature, enforces exactly 64 bytes, and verifies against the canonical JCS bytes using the public key `crates/corelink-erasure-attestation/src/verify.rs:50-82`.
- Ed25519 was chosen over RSA-2048/ECDSA P-256: 64-byte signatures (cheap for 7y R2 retention), FIPS 186-5 approved, constant-time verify `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:39-50`.
- Keys rotate every 30 days with a 30d overlap window during which both Active and Overlap public keys are served, so a verifier holding a pre-rotation key still verifies post-rotation attestations `crates/corelink-erasure-attestation/src/lib.rs:27-33`.

# Invariants

- INV-ERASURE-ATTESTATION-SIGNED (HIGH): every DSR erasure that reaches a bound `VerifiedComplete` MUST produce exactly one Ed25519-signed attestation persisted in R2 (7y) and indexed in D1 — the executed container enforcer now SATISFIES it: `sign_and_persist` signs the JCS payload and persists STRICTLY ordered + fail-CLOSED (R2 PUT → pubkey upsert → signed index row), and REFUSES (no row, never an unsigned theater row) on incomplete/unverified evidence; `INSERT OR IGNORE` keyed on `request_id` makes a re-sweep idempotent `crates/corelink-container/src/routes/dsr/attestation.rs:249-298`.
- The signature MUST be verified against the exact `canonical_payload_jcs` byte string, not a re-serialized payload `crates/corelink-erasure-attestation/src/attestation.rs:60-62`.
- Identical payloads always canonicalize byte-identically (RFC 8785 determinism), so a signature is stable and reproducible `crates/corelink-erasure-attestation/src/attestation.rs:98-100`.
- Signing key material is zeroized on drop and never appears in logs, traces, or `Debug` output — the `signing_key` field renders as `[REDACTED]` `crates/corelink-erasure-attestation/src/key.rs:41-52`.
- A signature that does not decode to exactly 64 bytes, or fails Ed25519 verification, is rejected as a verify error `crates/corelink-erasure-attestation/src/verify.rs:67-82`.
- An attestation cannot be produced over an incomplete evidence bundle — validation fails closed on any empty mandatory field `crates/corelink-erasure-attestation/src/evidence.rs:68-85`.
- 30d overlap is the hard upper bound and the chosen window for erasure keys: a customer verifying any time within 30d of rotation is always covered `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:65-79`.
- Attestations are retained 7 years to span SOC 2 + GDPR Art. 17 + LGPD audit windows `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:28`.

# Gotchas

- Verifiers must select the correct public key from the endpoint list using `attestation_key_id` — `verify_attestation_signature` does not auto-select; passing the wrong key yields a verify error, not a silent pass `crates/corelink-erasure-attestation/src/verify.rs:22-24`.
- RFC 8785 JCS includes Unicode NFC normalization, which is what prevents canonicalization-bypass attacks; a custom canonicalizer was explicitly rejected `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:52-63`.
- `EvidenceBundle::compute_hash` falls back to `serde_json` if `serde_jcs` fails, but that branch is treated as unreachable (a property test guards JCS); rely on `validated_hash` for the fail-closed checks `crates/corelink-erasure-attestation/src/evidence.rs:52-61`.
- `from_seed` reproduces the same keypair on every process start; the seed is the raw 32-byte Ed25519 secret scalar and MUST stay out of logs/Debug/errors — only `generate` is random/ephemeral `crates/corelink-erasure-attestation/src/key.rs:87-115`.
- Old public keys remain served for the 30d overlap but accept no new signatures; emergency rotation zeroizes the old key and publishes a security notice `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:93-96`.
- M22(a): the unauth `/v1/public/*` verifier router mounts OUTSIDE the data-plane's `ratelimit_layer.rs`/residency/auth layers (no PAT, no per-tenant token bucket), so before this fix it carried NO container-side rate limiting at all despite being internet-reachable. It now wraps its two routes in a SCOPED per-IP token-bucket layer local to the router itself (never the shared data-plane limiter) — generous budget (20 req/s, burst 60) so a regulator/DPA/human verifier or shared-NAT/CI-egress caller is never falsely throttled; FAIL-OPEN when the trusted `x-corelink-client-ip` header is absent `crates/corelink-container/src/routes/public_attestation.rs:103-118`.

# Citations

- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:28` — 7-year retention rationale.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:39-50` — Ed25519 over RSA/ECDSA decision.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:52-63` — RFC 8785 JCS choice + NFC.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:65-79` — 30d overlap decision.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:93-96` — old-key read-only / emergency rotation.
- `crates/corelink-erasure-attestation/src/lib.rs:5-14` — destroy → sign → R2 → D1 flow.
- `crates/corelink-erasure-attestation/src/lib.rs:27-33` — 30d key rotation / overlap serving.
- `crates/corelink-container/src/routes/dsr/attestation.rs:249-298` — INV-ERASURE-ATTESTATION-SIGNED, the executed `sign_and_persist` enforcer (LIVE: signs + persists fail-CLOSED, refuses on incomplete evidence).
- `crates/corelink-erasure-attestation/src/attestation.rs:17-45` — signed payload fields.
- `crates/corelink-erasure-attestation/src/attestation.rs:52-63` — `ErasureAttestation` (canonical bytes + signature).
- `crates/corelink-erasure-attestation/src/attestation.rs:98-104` — JCS canonicalize + Ed25519 sign + base64.
- `crates/corelink-erasure-attestation/src/key.rs:41-52` — Debug redaction.
- `crates/corelink-erasure-attestation/src/key.rs:77` — OS-CSPRNG key generation (getrandom `SysRng` via ed25519-dalek 3 `rand_core` `UnwrapErr`).
- `crates/corelink-erasure-attestation/src/key.rs:87-115` — deterministic `from_seed`.
- `crates/corelink-erasure-attestation/src/key.rs:119-130` — public key / PEM derivation.
- `crates/corelink-erasure-attestation/src/verify.rs:46-82` — offline verify path (payload-binding + base64/64-byte + Ed25519).
- `crates/corelink-erasure-attestation/src/evidence.rs:52-61` — evidence SHA-256 over JCS.
- `crates/corelink-erasure-attestation/src/evidence.rs:68-85` — fail-closed bundle validation.
- `crates/corelink-container/src/routes/dsr/attestation.rs:1-9` — container-side signed + served attestation is LIVE: real Ed25519 certificate, served by the `/v1/public/*` verifier.
- `crates/corelink-container/src/routes/dsr/attestation.rs:55-79` — the STRICT all-or-nothing ordering (R2 PUT → pubkey → signed index row) that makes the served cert non-forgeable (anti-theater invariant).
- `crates/corelink-container/src/routes/public_attestation.rs:103-118` — the unauth public verifier router: `GET /v1/public/attestation/{request_id}` + `GET /v1/public/keys/erasure/{region}.pub` (axum-0.8 `{param}` capture syntax), wrapped in the M22(a) scoped per-IP rate-limit layer.
