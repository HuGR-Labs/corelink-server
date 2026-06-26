---
type: "ADR"
title: "ADR-0037 — Merkle action protocol + result_hash semantics"
description: "Resolves the result_hash ambiguity: it is BLAKE3-256 of the canonical ActionResult proto bytes, used as a D1 index column, NOT the cryptographic output binding (that is merkle_root)."
source_files:
  - "specs/03_architecture/adrs/ADR-0037-merkle-action-protocol-result-hash.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s04", "ac", "merkle", "result_hash", "crypto"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0037 — Merkle action protocol + result_hash semantics

The Action Cache stores a signed `ActionResult` envelope, and an early ambiguity left two
incompatible readings of `result_hash` floating in the specs — "= merkle_root direct" versus
"= BLAKE3-256 of merkle_root" — which would have produced divergent implementations and a
non-verifiable cache. This ADR is the tie-breaking decision record that pins what `result_hash`
actually means and why it is decoupled from the cryptographic output binding, which matters to anyone
reasoning about the [Action Cache surface](/surfaces/action-cache.md) or the
[CAS/AC core crate cluster](/crates/cas-ac-core.md).

# Context

The S-04 `ActionResult` envelope originally leaned on Protobuf serialization for the result_hash
binding, which is non-deterministic across implementations (field order, default values); a residual
ambiguity then remained because the canonical_bytes comment and the D1 schema disagreed on whether
`result_hash` was the merkle_root directly or a BLAKE3 hash of it.

# Decision

`result_hash` is `BLAKE3-256(canonical_action_result_proto_bytes)` and is used as a **D1 INDEX
COLUMN ONLY, not a cryptographic binding** — the v1.1.0 amendment chose to hash the full canonical
`ActionResult` proto bytes (Merkle output digests plus stdout/stderr, exit_code, metadata) rather
than `BLAKE3(merkle_root)`, because that binds the complete result shape to the index and catches
drift in non-Merkle fields at the D1 layer. The cryptographic output authority stays `merkle_root`,
enforced by `verify_full` running structure-verify before sig-verify; the 121-byte canonical_bytes
layout is retained for defense-in-depth, D1-index alignment, and forward-compatibility.

# Consequences

The ambiguity is resolved and the AC envelope's builder-time contract is clear, with D1 index
efficiency preserved; the accepted cost is a redundant binding (the signature covers both
`merkle_root` and a hash of it) that future engineers may be tempted to remove — the ADR exists to
document why the 121 bytes stay.

# Citations

1. `specs/03_architecture/adrs/ADR-0037-merkle-action-protocol-result-hash.md:26-27` — Context: proto serialization is non-deterministic + the unresolved result_hash ambiguity.
2. `specs/03_architecture/adrs/ADR-0037-merkle-action-protocol-result-hash.md:33-33` — Decision: `result_hash` = BLAKE3-256 of canonical proto bytes, D1 INDEX column only, not a crypto binding.
3. `specs/03_architecture/adrs/ADR-0037-merkle-action-protocol-result-hash.md:43-50` — the v1.1.0 amendment rationale (hash full proto bytes, not `BLAKE3(merkle_root)`).
4. `specs/03_architecture/adrs/ADR-0037-merkle-action-protocol-result-hash.md:84-89` — `merkle_root` remains the crypto authority; result_hash is index + audit + future-proof.
5. `specs/03_architecture/adrs/ADR-0037-merkle-action-protocol-result-hash.md:114-124` — Consequences: ambiguity resolved + 121-byte layout retained vs the redundant-binding cost.
