---
type: "ComplianceControl"
title: "Ed25519/JCS erasure attestation"
description: "How a DSR erasure SIGNS an Ed25519 / RFC 8785 JCS attestation in-process — and why the signed proof is NOT durably persisted today (only a D1 metadata index row is written; the signature + JCS payload are dropped; there is no R2 persistence and no public serving endpoint — those are deferred WI-S11-008)."
source_files:
  - specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md
  - crates/corelink-erasure-attestation/src/lib.rs
  - crates/corelink-erasure-attestation/src/attestation.rs
  - crates/corelink-erasure-attestation/src/key.rs
  - crates/corelink-erasure-attestation/src/verify.rs
  - crates/corelink-erasure-attestation/src/evidence.rs
  - crates/corelink-erasure-attestation/src/region.rs
  - crates/corelink-container/src/routes/dsr.rs
  - crates/corelink-container/src/routes/dsr/attestation.rs
  - crates/corelink-container/src/routes.rs
  - migrations/d1/0032_erasure_attestation.sql
checkpoint_sha: "e3ab218a549a161083a52327b34c6a04d524f179"
provenance: "AUTHORED"
tags: ["compliance", "erasure", "ed25519", "jcs", "rfc-8785", "nist-sp-800-88", "gdpr-art-17", "byok", "dsr"]
timestamp: "2026-06-26T00:00:00Z"
---

When a DSR erasure reaches `VerifiedComplete`, the live container **signs** an Ed25519 attestation over an RFC 8785 JCS-canonicalized payload. The **design** (crate `//!`) is that this becomes a portable, tamper-evident receipt a customer or external auditor can verify offline — turning GDPR Art. 17 / LGPD erasure and NIST SP 800-88 Rev.1 §2.4 evidence-binding from a claim into a downloadable proof. **That offline-verifiable proof is NOT live today**, for the reason below: the signature is computed in-process and then **dropped** — only a D1 metadata *index* row survives, and there is no public endpoint to fetch it from. So the in-process signing primitive is real and exercised, but "a portable proof a customer/auditor can verify offline" is **not** a running capability.

**Scope / wiring status (code-confirmed — the load-bearing correction).** The signed artifact is **not durably persisted**. The deployed `sign_and_persist` (`crates/corelink-container/src/routes/dsr/attestation.rs:108-220`) computes the `ErasureAttestation` (payload + Ed25519 signature + canonical JCS bytes) and then writes ONLY a **metadata index row** into `erasure_attestations` (`crates/corelink-container/src/routes/dsr/attestation.rs:191-212`) plus the public-key row into `erasure_public_keys` (`:166-182`). The `erasure_attestations` table schema (`migrations/d1/0032_erasure_attestation.sql:15-27`) has **NO column for the Ed25519 signature and NO column for the canonical JCS payload** — it stores `request_id`, `tenant_id`, `region`, `attestation_key_id`, an `r2_key` *string*, `signed_at_ms`, `kms_provider`, `kms_key_id`, `evidence_hash`. So the signature and the signed bytes are computed and then **discarded**; nothing reconstructible-and-verifiable is stored. There is **NO R2 `PutObject` anywhere** on this path — `region.audit_bucket()` (`crates/corelink-erasure-attestation/src/region.rs:57-59`) only *builds the bucket-name string* used to format the `r2_key` index column; no object is ever written to R2. The R2 7-year persistence of the signature and the public serving endpoints are **deferred (WI-S11-008)**.

**No serving surface.** The two endpoints the design relies on — `GET /v1/public/keys/erasure/{region}.pub` and `GET /v1/public/attestation/{request_id}` — have **ZERO route registrations**: they appear only in doc-comments / examples (`crates/corelink-container/src/routes/dsr/attestation.rs:6-8` module `//!`, the crate `//!` at `crates/corelink-erasure-attestation/src/lib.rs:12-14`), and grepping the container router (`crates/corelink-container/src/routes.rs`) and the Worker (`worker/src/index.ts`) finds no handler for either. There is no way for a customer or auditor to retrieve a key or an attestation today.

**What the live container actually signs (accurately scoped).** `sign_and_persist` runs best-effort, **fail-OPEN**, on a `VerifiedComplete` 24h sweep, and for **ordinary tenants — NOT BYOK-only**. The recorded mechanism is **truthful and is NOT a BYOK KMS crypto-erase**: the shipped flow is a D1 / R2 / Stripe **delete-set**, so it records `kms_provider = "corelink_d1r2_erase"` and `kms_key_id = "dsr:{dsr_id}"` and performs **NO CMK destroy** via any KMS provider (`crates/corelink-container/src/routes/dsr/attestation.rs:10-14`, `:49-51`, `:138-149`). The signing IS gated on a fail-CLOSED cross-backend erase — `VerifiedComplete` is only reached when every backend's erase succeeded and re-fingerprints canonical-empty (see the sibling `compliance/dsr-erasure`), so surviving subject bytes block the signature. The signer constructs a **synthetic, self-referential** `EvidenceBundle` whose `audit_chain_segment_ids` is the single literal `dsr.erasure.completed.v1:{dsr_id}` (`crates/corelink-container/src/routes/dsr/attestation.rs:139-144`); there is **no live producer** that emits/persists such an audit-chain segment, so the bundle binds a *self-describing record of the DSR id* rather than an *independently-retrievable* segment. The `resolve_region` fallback is correct by design: at `VerifiedComplete` the tenant row is already deleted, so the D1 `primary_region` lookup misses and the region falls back to the `ERASURE_ATTESTATION_REGION` default (`crates/corelink-container/src/routes/dsr/attestation.rs:82-102`); the deployment is US-only, so the env default is the tenant's region.

# Role

This crate is the pure-logic signing and verification surface for erasure attestations: it canonicalizes the erasure payload, signs it with a per-region Ed25519 key, derives the public key, and validates signatures offline. It owns the cryptographic shape of the proof — not the KMS destroy, not the durable persistence of the signed artifact, and not the request lifecycle. The container consumer (`routes/dsr/attestation.rs`) invokes the signer and writes a metadata-only D1 index row (no signature, no R2 object, no serving endpoint).

# How it works

- The crate `//!` DESCRIBES a BYOK flow that destroys CMK access via the KMS provider then persists an Ed25519 attestation to R2 (7y) and indexes it in D1 for a public verify endpoint `crates/corelink-erasure-attestation/src/lib.rs:5-14` — but the SHIPPED path does none of the destroy/R2/serving parts: it signs in-process and writes a D1 **metadata index row only** (`erasure_attestations` has no signature/JCS column — `migrations/d1/0032_erasure_attestation.sql:15-27`), records `kms_provider="corelink_d1r2_erase"` with NO CMK destroy, and exposes no endpoint (see Scope above).
- The signed payload carries `tenant_id`, `request_id`, `destroyed_ts`, KMS provider/key id, `evidence_hash`, region, and `attestation_key_id` — every field is included in the canonical JSON `crates/corelink-erasure-attestation/src/attestation.rs:17-45`.
- Signing first JCS-canonicalizes the payload via `serde_jcs::to_string` (RFC 8785) `crates/corelink-erasure-attestation/src/attestation.rs:99`, then signs those canonical bytes with Ed25519 `crates/corelink-erasure-attestation/src/attestation.rs:103`, then base64-encodes the 64-byte signature `crates/corelink-erasure-attestation/src/attestation.rs:104`.
- The resulting in-memory `ErasureAttestation` holds the payload, the base64 signature, and the exact JCS byte string that was signed `crates/corelink-erasure-attestation/src/attestation.rs:52-63` — but on the live path the caller persists ONLY the metadata index columns, NOT the signature or the JCS bytes, so a verifier has no stored bytes to verify against (the `//!` "Persisted to R2 … and indexed in D1" at `:50` is the DESIGN, not the live path).
- `evidence_hash` is a SHA-256 over the JCS-canonical serialization of an `EvidenceBundle` binding audit-chain segment IDs + KMS destroy timestamp + KMS key id + tenant id `crates/corelink-erasure-attestation/src/evidence.rs:52-61`.
- `EvidenceBundle::validated_hash` exists and DOES fail closed on an empty segment list / KMS key id / tenant id `crates/corelink-erasure-attestation/src/evidence.rs:68-85` — but the signer never calls it: `ErasureAttestationSigner::sign` accepts a *precomputed* `evidence_hash` and never validates the bundle `crates/corelink-erasure-attestation/src/attestation.rs:94-118`, and the live caller computes the hash with `EvidenceBundle::compute_hash` (NO empty-field check) not `validated_hash` `crates/corelink-container/src/routes/dsr/attestation.rs:139-151`. The fail-closed guard is therefore NOT on the live path; the bundle is non-empty only because the caller hardcodes the three fields by construction — a caller convention, not a structural guard at the signer.
- Per-region signing keys are generated from the OS CSPRNG (`OsRng`) `crates/corelink-erasure-attestation/src/key.rs:72`, or reconstructed deterministically from a 32-byte secret seed so the served public key stays stable across Workers/container restarts `crates/corelink-erasure-attestation/src/key.rs:95-110`.
- `public_key()` derives the verifying key and a PEM SubjectPublicKeyInfo (RFC 8410 Ed25519 OID prefix) DESIGNED to be served at `GET /v1/public/keys/erasure/{region}.pub` — but that endpoint has ZERO route registrations (see No-serving-surface above); only the PEM derivation is live `crates/corelink-erasure-attestation/src/key.rs:114-125`. The container does upsert the PEM into the `erasure_public_keys` D1 table `crates/corelink-container/src/routes/dsr/attestation.rs:166-182`, but nothing serves that table.
- Offline verification decodes the base64 signature, enforces exactly 64 bytes, and verifies it against the canonical JCS bytes using the public key `crates/corelink-erasure-attestation/src/verify.rs:46-60` — a real primitive, but with no persisted signature and no serving endpoint there is no live producer of the inputs a third party would verify.
- Ed25519 was chosen over RSA-2048/ECDSA P-256: 64-byte signatures (cheap for 7y R2 retention), FIPS 186-5 approved, constant-time verify `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:39-50`.
- Keys rotate every 30 days with a 30d overlap window during which both Active and Overlap public keys are served, so a verifier holding a pre-rotation key still verifies post-rotation attestations `crates/corelink-erasure-attestation/src/lib.rs:27-33`.

# Invariants

- INV-ERASURE-ATTESTATION-SIGNED (HIGH): the crate `//!` frames a fail-CLOSED, BYOK-only "every erasure MUST be attested, persisted in R2 (7y) and indexed in D1" contract `crates/corelink-erasure-attestation/src/lib.rs:35-39` — but **reconcile that against the SHIPPED path** (sibling `compliance/dsr-erasure`): (a) the live container signs **best-effort / fail-OPEN** on the `VerifiedComplete` 24h sweep, gated by the executed `if matches!(decision, ErasureDecision::VerifiedComplete { .. })` check, for **ordinary tenants — NOT BYOK-only** (`crates/corelink-container/src/routes/dsr.rs:503`); (b) **the signature is NOT durably persisted** — only a metadata index row is written and `erasure_attestations` has no signature/JCS column (`migrations/d1/0032_erasure_attestation.sql:15-27`); (c) **there is no R2 persistence and no serving endpoint** (deferred WI-S11-008). Absence of an attestation does NOT mean the erasure failed — the erasure is already complete + audited; the attestation is an extra (currently non-retrievable) evidence artifact.
- The crate `//!` cites a `ErasureAttester::attest_erasure` "surface in the consumer crate" as the enforcer `crates/corelink-erasure-attestation/src/lib.rs:39` — **that type/method does NOT exist** (no `ErasureAttester` struct or `attest_erasure` method in any crate src; it appears only in this doc-comment and in the `WI-S14-007` spec). The real shipped enforcer is the container free function `attestation::sign_and_persist` `crates/corelink-container/src/routes/dsr/attestation.rs:108-220`.
- The signature MUST be verified against the exact `canonical_payload_jcs` byte string, not a re-serialized payload `crates/corelink-erasure-attestation/src/attestation.rs:60-62` — but that byte string is NOT persisted today, so an offline verifier has nothing to verify against on the live path.
- Identical payloads always canonicalize byte-identically (RFC 8785 determinism), so a signature is stable and reproducible `crates/corelink-erasure-attestation/src/attestation.rs:98-100`.
- Signing key material is zeroized on drop and never appears in logs, traces, or `Debug` output — the `signing_key` field renders as `[REDACTED]` `crates/corelink-erasure-attestation/src/key.rs:41-52`.
- A signature that does not decode to exactly 64 bytes, or fails Ed25519 verification, is rejected as a verify error `crates/corelink-erasure-attestation/src/verify.rs:50-60`.
- The crate's `validated_hash` fails closed on any empty mandatory field `crates/corelink-erasure-attestation/src/evidence.rs:68-85`, but the live signer never calls it — `ErasureAttestationSigner::sign` takes a precomputed `evidence_hash` and validates nothing `crates/corelink-erasure-attestation/src/attestation.rs:94-118`, and the caller uses `EvidenceBundle::compute_hash` (no empty-field check), not `validated_hash` `crates/corelink-container/src/routes/dsr/attestation.rs:139-151`. So fail-closed-on-empty-field holds only by the CALLER's convention of hardcoding the three fields non-empty by construction, NOT structurally at the signer (the guard is simply off the live path). Separately, the audit-chain segment id is synthetic and has no live producer, so the signature attests a self-describing DSR-id record, not an independently-retrievable deletion-evidence segment; and the signing is gated on the fail-CLOSED cross-backend erase (sibling `compliance/dsr-erasure`), so a signature is never produced while subject bytes survive.
- 30d overlap is the hard upper bound and the chosen window for erasure keys: a customer verifying any time within 30d of rotation is always covered `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:65-79`.
- Attestations are retained 7 years to span SOC 2 + GDPR Art. 17 + LGPD audit windows `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:28`.

# Gotchas

- Verifiers must select the correct public key from the endpoint list using `attestation_key_id` — `verify_attestation_signature` does not auto-select; passing the wrong key yields a verify error, not a silent pass `crates/corelink-erasure-attestation/src/verify.rs:41-60`.
- RFC 8785 JCS includes Unicode NFC normalization, which is what prevents canonicalization-bypass attacks; a custom canonicalizer was explicitly rejected `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:52-63`.
- `EvidenceBundle::compute_hash` falls back to `serde_json` if `serde_jcs` fails, but that branch is treated as unreachable (a property test guards JCS); `compute_hash` does NOT do the empty-field checks — those live in `validated_hash`, which the live path does not call `crates/corelink-erasure-attestation/src/evidence.rs:52-61`.
- `from_seed` reproduces the same keypair on every process start; the seed is the raw 32-byte Ed25519 secret scalar and MUST stay out of logs/Debug/errors — only `generate` is random/ephemeral `crates/corelink-erasure-attestation/src/key.rs:82-110`.
- Old public keys remain served for the 30d overlap but accept no new signatures; emergency rotation zeroizes the old key and publishes a security notice `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:93-96`.

# Citations

- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:28` — 7-year retention rationale.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:39-50` — Ed25519 over RSA/ECDSA decision.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:52-63` — RFC 8785 JCS choice + NFC.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:65-79` — 30d overlap decision.
- `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:93-96` — old-key read-only / emergency rotation.
- `crates/corelink-erasure-attestation/src/lib.rs:5-14` — the DESIGNED destroy → sign → R2 → D1 flow (not the live path).
- `crates/corelink-erasure-attestation/src/lib.rs:27-33` — 30d key rotation / overlap serving.
- `crates/corelink-erasure-attestation/src/lib.rs:35-39` — INV-ERASURE-ATTESTATION-SIGNED + the fabricated `ErasureAttester::attest_erasure` enforcer reference.
- `crates/corelink-erasure-attestation/src/attestation.rs:17-45` — signed payload fields.
- `crates/corelink-erasure-attestation/src/attestation.rs:50` — the `//!` "Persisted to R2 (7y) and indexed in D1" DESIGN comment (not live).
- `crates/corelink-erasure-attestation/src/attestation.rs:52-63` — `ErasureAttestation` (payload + signature + canonical bytes, in memory).
- `crates/corelink-erasure-attestation/src/attestation.rs:94-118` — `sign` takes a precomputed `evidence_hash`; never calls `validated_hash`.
- `crates/corelink-erasure-attestation/src/attestation.rs:98-104` — JCS canonicalize + Ed25519 sign + base64.
- `crates/corelink-erasure-attestation/src/key.rs:41-52` — Debug redaction.
- `crates/corelink-erasure-attestation/src/key.rs:72` — OsRng key generation.
- `crates/corelink-erasure-attestation/src/key.rs:82-110` — deterministic `from_seed`.
- `crates/corelink-erasure-attestation/src/key.rs:114-125` — public key / PEM derivation.
- `crates/corelink-erasure-attestation/src/region.rs:57-59` — `audit_bucket()` builds the bucket-NAME string only (no R2 write).
- `crates/corelink-erasure-attestation/src/verify.rs:41-60` — offline verify path.
- `crates/corelink-erasure-attestation/src/evidence.rs:52-61` — evidence SHA-256 over JCS (`compute_hash`, no empty-field check).
- `crates/corelink-erasure-attestation/src/evidence.rs:68-85` — `validated_hash` fail-closed bundle validation (NOT called on the live path).
- `crates/corelink-container/src/routes/dsr/attestation.rs:10-14` — `kms_provider="corelink_d1r2_erase"`, explicitly NOT a BYOK KMS crypto-erase.
- `crates/corelink-container/src/routes/dsr/attestation.rs:108-220` — `sign_and_persist`: the real enforcer; signs then writes a metadata-only D1 index row.
- `crates/corelink-container/src/routes/dsr/attestation.rs:139-151` — synthetic `EvidenceBundle` + `compute_hash` (not `validated_hash`).
- `crates/corelink-container/src/routes/dsr/attestation.rs:166-212` — D1 upserts: `erasure_public_keys` PEM + `erasure_attestations` metadata index (no signature/JCS columns).
- `crates/corelink-container/src/routes/dsr.rs:503` — `VerifiedComplete` gate; fail-OPEN, ordinary tenants.
- `migrations/d1/0032_erasure_attestation.sql:15-27` — `erasure_attestations` schema: NO signature / NO JCS-payload column.
- `crates/corelink-container/src/routes.rs:608-617` — the router `.merge(...)` assembly: no erasure-key / attestation public router is merged in (no serving surface for either endpoint).
