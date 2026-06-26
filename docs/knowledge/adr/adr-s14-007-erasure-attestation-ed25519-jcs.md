---
type: "ADR"
title: "ADR-S14-007 — Erasure attestation: Ed25519 + RFC 8785 JCS + 30d key overlap"
description: "Why crypto-erasure proofs are Ed25519 (FIPS 186-5) signatures over RFC 8785 JCS-canonicalized payloads with a 30d signing-key overlap, retained 7 years."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s14", "erasure-attestation", "ed25519", "jcs", "gdpr"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-007 — Erasure attestation: Ed25519 + RFC 8785 JCS + 30d key overlap

When a BYOK tenant files a DSR erasure request, CoreLink must produce a cryptographic proof of erasure that the customer and external auditors can verify offline, that complies with NIST SP 800-88 Rev.1 §2.4 crypto-erase, and that survives a 7-year audit retention. This ADR (ACCEPTED, WI-S14-007 SEALED) records three simultaneous choices — signature scheme, JSON canonicalization, and signing-key overlap window — and why each well-audited standard was selected over its alternatives.

# Context

The attestation must be independently offline-verifiable by customer + auditors, compliant with NIST SP 800-88 Rev.1 §2.4 crypto-erase, retained 7 years (SOC 2 + GDPR Art. 17 + LGPD Art. 17/18), and signed with a standard scheme — forcing three decisions: signature scheme, JSON canonicalization, and key overlap window (`specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:22-36`).

# Decision

- **Ed25519 (FIPS 186-5 EdDSA) via `ed25519-dalek`**, not RSA-2048 or ECDSA P-256: FIPS 186-5 approves it, signatures are 64 bytes (cost-efficient for 7y R2 retention), verify is constant-time by default, it is faster than the alternatives, and it is the TUF/sigstore/OpenSSH industry pattern with non-deterministic OS-CSPRNG keygen (`specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:39-50`).
- **RFC 8785 JCS via `serde_jcs`**, not a custom canonicalizer: deterministic serialization is mandatory for stable signatures, RFC 8785 is an IETF standard already used in corelink-audit / corelink-privacy-erasure-worker / corelink-dual-approval, and it includes Unicode NFC normalization to block canonicalization-bypass attacks (a `prop_jcs_deterministic` 10k test verifies byte-identical output) (`specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:52-64`).
- **30d signing-key overlap**, not 7d or 24h: the attestation is a permanent forensic record (not an auth credential, no replay surface since `request_id` is UNIQUE and the payload is immutable), so a 24h/7d overlap would leave a customer who cached the old public key unable to verify after rotation; 30d matches the rotation period and the ADR-0018 hard upper bound (`specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:65-80`).

# Consequences

- Positive: customer + auditor verify offline with standard Ed25519 tooling (OpenSSL, ed25519-dalek, PyNaCl); FIPS 186-5 satisfies Compliance + AppSec; JCS is auditable/reproducible; 30d overlap eliminates rotation-boundary verification gaps (`specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:83-88`).
- Negative / mitigated: `ed25519-dalek` is a new workspace dependency (MIT, RUSTSEC-clean, widely audited), and old public keys stay published for 30d (read-only, verify-only; emergency rotation zeroizes + publishes a notice) (`specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:90-96`).

# Citations

1. `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:22-36` — the offline-verifiable / NIST 800-88 / 7y-retention requirements and the three decisions.
2. `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:39-50` — Ed25519 over RSA/ECDSA.
3. `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:52-64` — RFC 8785 JCS over a custom canonicalizer.
4. `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:65-80` — the 30d key-overlap decision.
5. `specs/03_architecture/adrs/ADR-S14-007-erasure-attestation-ed25519-jcs.md:83-96` — positive and mitigated-negative consequences.
