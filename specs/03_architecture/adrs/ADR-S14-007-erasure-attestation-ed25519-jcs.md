---
id: "ADR-S14-007"
type: "adr"
doc_status: "ACCEPTED"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "S-14"
work_item: "WI-S14-007"
owner: "Gustavo Schneiter"
tags: ["adr", "s14", "ed25519", "fips-186-5", "erasure-attestation", "jcs", "nist-sp-800-88", "7y-retention"]
---

# ADR-S14-007 — Erasure Attestation: Ed25519 (FIPS 186-5) + JCS + Per-Region 30d Overlap + NIST SP 800-88 Rev.1 §2.4

## Context

WI-S14-007 requires CoreLink to provide cryptographic proof of erasure when a BYOK
tenant files a DSR erasure request (S-11). The attestation must be:

- Independently verifiable by the customer and external auditors offline.
- Compliant with NIST SP 800-88 Rev.1 §2.4 crypto-erase mode.
- Retained for 7 years (SOC 2 + GDPR Art. 17 + LGPD Art. 17/18 audit windows).
- Signed using a standard, well-audited digital signature scheme.

Three decisions were made simultaneously:

1. **Signature scheme**: Ed25519 vs RSA-2048 vs ECDSA P-256.
2. **JSON canonicalization**: RFC 8785 JCS vs custom vs no canonicalization.
3. **Key overlap window**: 30d vs 7d vs 24h.

## Decision

### 1. Ed25519 (FIPS 186-5 EdDSA) — NOT RSA / ECDSA

**Chosen**: Ed25519 via `ed25519-dalek` crate.

Reasons:
- FIPS 186-5 (NIST 2023) formally approves EdDSA Ed25519.
- Signature is 64 bytes (vs RSA-2048: 256 bytes); cost-efficient for 7y R2 retention.
- `ed25519-dalek` verify is constant-time by default (side-channel resistant).
- Faster sign + verify than ECDSA P-256 and RSA at equivalent security level.
- Industry-standard pattern: TUF (The Update Framework), sigstore, OpenSSH.
- Key generation from OS CSPRNG (`OsRng`); not deterministic — compromise of one key
  does not compromise others.

### 2. RFC 8785 JCS (serde_jcs) for canonicalization — NOT custom

**Chosen**: `serde_jcs = "0.2"` (RFC 8785 JSON Canonicalization Scheme).

Reasons:
- Deterministic JSON serialization mandatory for stable signatures.
- RFC 8785 is an IETF standard (used in DSSE, sigstore, COSE, JOSE).
- `serde_jcs` Rust crate is well-maintained and already used in corelink-audit,
  corelink-privacy-erasure-worker, and corelink-dual-approval.
- Includes Unicode NFC normalization — prevents canonicalization bypass attacks.
- Property test `prop_jcs_deterministic` verifies 10k random payloads serialize
  byte-identically across two calls.

### 3. 30d overlap canonical — NOT 7d or 24h

**Chosen**: 30d overlap via `AssetClass::ErasureAttestationKey.overlap_seconds()`.

Reasons:
- Erasure attestation is a permanent forensic record, NOT an authentication credential.
  Long overlap does not introduce elevated risk (no replay attack surface: request_id
  is UNIQUE; payload is immutable post-sign).
- 24h overlap: customer who receives an attestation just before rotation cannot verify
  it the next day if they cached the old public key.
- 7d overlap: same problem for customers running weekly verification.
- 30d overlap: matches the rotation period; a customer verifying any time during the
  30d after rotation is always covered.
- Per `key_management.md §3.2.1` + ADR-0018: 30d is the hard upper bound for ALL
  asset classes. `ErasureAttestationKey` uses the full 30d.

## Consequences

**Positive:**
- Customer + auditor can verify offline without CoreLink infrastructure using standard
  Ed25519 tooling (OpenSSL, ed25519-dalek, PyNaCl).
- FIPS 186-5 attestation satisfies Compliance Officer + AppSec requirements.
- RFC 8785 JCS is auditable and reproducible.
- 30d overlap eliminates verification gaps at rotation boundaries.

**Negative / mitigated:**
- `ed25519-dalek` is a new workspace dependency (added to `[workspace.dependencies]`).
  Mitigated: MIT-licensed, RUSTSEC clean at time of writing, widely audited.
- 30d overlap means old public keys remain on the public-key endpoint for 30d.
  Mitigated: old keys are read-only (no new signatures accepted); serve only to
  verify pre-rotation attestations. Emergency rotation procedure zeroizes old key
  and publishes a security notice.

## Compliance mapping

| Standard | Satisfied by |
|---|---|
| FIPS 186-5 EdDSA | Ed25519 via ed25519-dalek |
| NIST SP 800-88 Rev.1 §2.4 crypto-erase | `evidence_hash` binds audit chain + KMS destroy |
| RFC 8785 JCS | `serde_jcs` canonicalization |
| GDPR Art. 17 / LGPD Art. 17-18 erasure right | 7y R2 retention + verify endpoint |
| SOC 2 CC6.1 | Per-region signing key + 30d rotation + D1 audit trail |

## ADR status

ACCEPTED — WI-S14-007 SEALED with this ADR ratified.

## References

- `crates/corelink-erasure-attestation/` — implementation.
- `crates/corelink-rotation-adapters/src/erasure_attestation.rs` — rotation adapter.
- `migrations/d1/0032_erasure_attestation.sql` — D1 schema.
- `key_management.md §3.2.1` — overlap canonical per asset class.
- ADR-0018 — rotation overlap hard upper bound 30d.
- WI-S14-007 spec — full narrative + acceptance criteria.
