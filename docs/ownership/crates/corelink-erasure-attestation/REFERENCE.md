---
schema: corelink-ownership/1.1
document: reference
package: corelink-erasure-attestation
manifest: crates/corelink-erasure-attestation/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-erasure-attestation-structural-normalization-20260921
---

# corelink-erasure-attestation — reference

SOURCE/static reference for the recorded revision. It records Rust contracts,
declared dependencies, and observed relations; it does not prove erasure, key
lifecycle, persistence, endpoint, or deployment.

[Scope](#r01) · [Surface](#r02) · [Sign/verify](#r03) · [Evidence/keys/regions](#r04) · [Invariants](#r05) · [Graph](#r06) · [Boundaries](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Identity and evidence boundary

Record index: [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004)

Cargo.toml names package corelink-erasure-attestation. src/lib.rs forbids unsafe
code, exposes six modules, re-exports the public surface, and returns schema
version 1. Comments about Workers, R2, D1, KMS, routes, or rotation are not
operation evidence.

<a id="r02"></a>
## R02 — Public source surface

Exports are ErasureAttestation, ErasureAttestationPayload,
ErasureAttestationSigner, AttestationError, EvidenceBundle, ErasurePublicKey,
ErasureSigningKey, Region, and verify_attestation_signature. Payload fields are
tenant_id, request_id, destroyed_ts, kms_provider, kms_key_id, evidence_hash,
region, and attestation_key_id. An attestation contains payload,
signature_ed25519, and canonical_payload_jcs.

<a id="r03"></a>
## R03 — Signing and verification contracts

ErasureAttestationSigner.sign JCS-canonicalizes the given payload with
serde_jcs, signs those bytes with its Ed25519 key, and base64-encodes the
signature. Its key_id and region return held-key fields; signing does not verify
payload key ID or region agrees with them.

verify_attestation_signature re-canonicalizes typed payload and requires byte
equality with canonical_payload_jcs; it decodes an exactly 64-byte base64
signature and verifies the canonical bytes with caller-supplied public key.
Documentation assigns public-key selection by payload key ID to caller; function
does not compare payload key ID or region to supplied public key.

<a id="r04"></a>
## R04 — Evidence, key, and region contracts

EvidenceBundle contains audit segment IDs, KMS destroy timestamp, KMS key ID,
and tenant ID. compute_hash JCS-serializes, falls back to
serde_json serialization with default on JCS error, then returns SHA-256 hex.
validated_hash rejects empty segments, empty KMS key ID, and empty tenant ID,
but not zero destroy timestamp.

ErasureSigningKey has public ID, region, timestamps, and signing key fields; it
derives ZeroizeOnDrop and Debug prints [REDACTED] for signing key. generate uses
UnwrapErr(SysRng); from_seed takes [u8; 32]; public_key derives matching identity
fields, verifying key, and PEM. fingerprint returns SHA-256 hex.

Actual Region variants are Wnam, Enam, Weur, Sam, Apac, and Afr. as_str and
Display use lowercase, parse accepts only those strings, and audit_bucket returns
corelink-audit-{region}. This is naming only, not bucket/deployment proof.

<a id="r05"></a>
## R05 — Falsifiable invariants

<a id="inv-001"></a>
### INV-001 — Canonical signing relation

sign returns JCS bytes of supplied payload and base64 Ed25519 signature over
those bytes. Changing canonicalizer, signing input, or encoded construction in
attestation.rs falsifies it.

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Typed-payload binding

Verification rejects when re-canonicalized typed payload differs from
canonical_payload_jcs before signature verification. Weakening this equality in
verify.rs falsifies it.

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Evidence validation boundary

validated_hash rejects only the three R04 emptiness conditions before invoking
compute_hash. Altering checks/delegation in evidence.rs falsifies it.

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Six-region mapping

Every declared Region has a lowercase as_str value and parse(as_str()) mapping.
Changing region.rs mapping arms falsifies it.
[↩](#r01)

<a id="r06"></a>
## R06 — Declared dependencies and static consumers

Manifest dependencies are ed25519-dalek, getrandom, serde_jcs, base64, sha2,
serde, serde_json, thiserror, zeroize, hex, and tracing; no corelink-* dependency
is declared. Static manifests identify corelink-crypto and corelink-container as
direct consumers. corelink-crypto/src/ed25519.rs re-exports the whole API at
corelink_crypto::ed25519::attestation. Container source imports selected symbols
in routes/dsr/attestation.rs, dsr.rs, public_attestation.rs, and audit_drain.rs.
These relations do not prove features, route mounts, or execution.

<a id="r07"></a>
## R07 — Source boundaries

| Boundary | Source-defined observation | Not established |
|---|---|---|
| Signing | Local key plus payload produces local representation | Key sourcing, authorization, production use |
| Evidence | Bundle hashing and three validation checks exist | KMS destruction, audit delivery, completeness |
| Keys | Local generation/derivation and redacted Debug exist | Seed custody, rotation, active/overlap, key serving |
| Region | Six variants and bucket-name convention exist | Provisioning, residency, bucket/routing |
| Verification | Caller-key verification and payload binding exist | Key provenance/retrieval, endpoint, user verification |
| Errors | Storage, D1, audit, rotation variants are named | R2/D1/audit/rotation operation |

<a id="r08"></a>
## R08 — Explicit unknowns

Unknown: complete consumers, targets/features, compatibility, external-library
behavior, key/seed custody, KMS/audit events, R2 persistence/retention, D1
index operation, public-key/verification service, rotation, regional deployment,
endpoint mounting, secrets, runtime, deployment, publication, cold review.

[Relations](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
