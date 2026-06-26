---
type: "ADR"
title: "ADR-S20 — rsa 0.9.x Marvin timing-sidechannel decision (waiver + mitigations)"
description: "Records the RUSTSEC-2023-0071 Marvin decision: waiver + operational mitigations, because CoreLink's rsa usage is signing-only with no chosen-ciphertext oracle, with auto-promotion triggers to a full crate replacement."
source_files:
  - "specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "security", "supply-chain", "rsa", "marvin-attack", "rustsec-2023-0071", "s20"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S20 — rsa 0.9.x Marvin timing-sidechannel decision (waiver + mitigations)

RUSTSEC-2023-0071 (the Marvin timing attack) flags the `rsa` crate for potential key recovery via decryption-latency sidechannels, with no upstream fix available. This DRAFT ADR reasons from the threat model — CoreLink uses `rsa` only on the signing side (receipt + audit-chain anchoring), never for attacker-chosen-ciphertext decryption — to recommend a waiver plus operational mitigations over a costly crate replacement, while binding that recommendation to explicit auto-promotion triggers. It exists so the `cargo audit` finding is a documented, conditional, monitored accepted-risk rather than an unexplained suppression. Related: [BYOK envelope encryption](/storage/byok-envelope-encryption.md) uses AES-GCM, not RSA.

# Context

The Marvin attack extends Bleichenbacher/Manger oracles to recover RSA keys when an attacker can measure decryption latency over many handshakes (CVSS 5.9, no upstream fix as of filing); CoreLink's three `rsa` usages are all signing-side (test fixtures, DPA-acceptance receipts, worker consent/audit-chain receipts) with RSA decryption absent from every production codepath — KMS-wrap is AES-GCM, TLS uses rustls/aws-lc-rs, and PAT/JWT verify is Ed25519 + HMAC — so the attack's chosen-ciphertext-oracle precondition does not hold, as analysed at `specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md:31-84`.

# Decision

The recommended path (pending crypto-SME sign-off) is Option B — waiver + ADR + operational mitigations — because the Marvin preconditions do not hold for signing-only usage and Option A's rewrite cost is not justified by the residual risk; the decision binds four auto-promotion triggers that void Option B and make the `aws-lc-rs`/`ring` replacement mandatory within 30 days (adding an RSA decryption path, an advisory bump to High, a public sub-24h PoC, or a FIPS 140-3 constant-time requirement), recorded at `specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md:99-126`. Four mitigations are in force: 90-day automated RS256 key rotation, a signing-latency anomaly SLO/page, single-tenant signing-process isolation, and quarterly review, per `specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md:127-141`.

# Consequences

`cargo audit` continues to flag RUSTSEC-2023-0071 until `rsa` 0.10 ships or Option A executes, carried in the `deny.toml` ignore list and the CycloneDX SBOM VEX as "not affected — vulnerable-code-not-in-execute-path", with no code change required in this PR (mitigations ship as follow-on work items), per `specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md:143-154`.

# Citations

1. `specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md:31-84` — the advisory, the three signing-only usages, and the threat-model exclusion (Context).
2. `specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md:99-126` — the Option B recommendation and the four conditional auto-promotion triggers.
3. `specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md:127-141` — the four in-force mitigations (rotation, latency alert, process isolation, quarterly review).
4. `specs/03_architecture/adrs/ADR-S20-RSA-MARVIN-MITIGATION.md:143-154` — consequences: continued audit flag, deny.toml waiver, SBOM VEX, no code change.
