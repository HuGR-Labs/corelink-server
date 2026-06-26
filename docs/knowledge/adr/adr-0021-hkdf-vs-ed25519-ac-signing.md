---
type: "ADR"
title: "ADR-0021 — HKDF-SHA256 + BLAKE3-keyed MAC vs Ed25519 for AC signing"
description: "Chooses a symmetric HKDF-derived per-tenant BLAKE3-keyed MAC over Ed25519 for signing Action Cache envelopes, optimizing for WASM speed and reuse of the existing tenant derivation key."
source_files:
  - "specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s04", "ac", "hkdf", "signing", "crypto"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0021 — HKDF-SHA256 + BLAKE3-keyed MAC vs Ed25519 for AC signing

Every Action Cache `ActionResult` envelope must be signed so a tenant cannot tamper with or forge another tenant's cached result. This ADR chooses a symmetric MAC — a per-tenant signing key derived via HKDF-SHA256 from the existing tenant derivation key (TDK), then a BLAKE3-keyed tag — over an asymmetric Ed25519 signature. It matters because CoreLink's threat model is tenant-scoped integrity (we sign for ourselves and verify for ourselves), where Ed25519's public-verifier advantage is not load-bearing, and the symmetric path is far faster on Cloudflare Workers WASM. It signs the envelopes of the [Action Cache](/surfaces/action-cache.md) surface.

# Context

S-04 stores `ActionResult` envelopes per `(tenant_id, action_digest)` and each must be signed to prevent tampering and provide tenant-scoped integrity, posing a choice between a symmetric HKDF-derived BLAKE3-keyed MAC and an asymmetric Ed25519 per-tenant keypair (`specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:26-28`).

# Decision

HKDF-SHA256 + BLAKE3-keyed MAC is canonical for S-04 AC signing — salt = `sig_key_id`, HKDF-expand info `b"ac-sig"`, producing a 32-byte tag with the `sig_key_id` persisted alongside (`specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:32-42`). The decision matrix favored it on every load-bearing axis: ≤0.5ms native / ≤1ms WASM (vs 3–5ms / 8–15ms for Ed25519), reuse of the existing per-tenant TDK with no new keypair, and symmetric tenant-scoped integrity — the asymmetric advantages of third-party verification and non-repudiation are not present in CoreLink's threat model (`specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:46-55`).

# Consequences

- Sub-millisecond sign+verify, TDK reuse with no new KMS key class, and HKDF info-string domain separation from path-HMAC and manifest signing (`specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:59-62`).
- A tenant cannot prove ownership to a third party (out of scope for GA), and the TDK is shared between path-HMAC and HKDF-Extract — a composition concern (`specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:64-67`).
- That risk is acknowledged: the TDK is multi-derived without a unified HKDF-Extract step, and Option B (keep raw HMAC for the path, rely on HMAC-SHA256's PRF property bounding attack cost at 2^128) is the chosen, Crypto-SME-accepted posture for S-04 GA (`specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:72-86`).

# Citations

1. `specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:26-28` — the AC envelope signing requirement and the two options.
2. `specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:32-42` — the decision: HKDF-SHA256 + BLAKE3-keyed MAC and its construction.
3. `specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:46-55` — the decision matrix favoring the symmetric MAC.
4. `specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:59-67` — positive and negative consequences.
5. `specs/03_architecture/adrs/ADR-0021-hkdf-vs-ed25519-ac-signing.md:72-86` — the TDK multi-derivation risk and the accepted Option B mitigation.
