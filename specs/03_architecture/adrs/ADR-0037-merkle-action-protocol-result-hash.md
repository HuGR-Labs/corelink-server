---
id: "ADR-0037"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-05-01"
title: "Merkle Action Protocol + result_hash Semantics"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner)", "Crypto SME (TBD; mandatory emphatic per WI-S04-004)"]
context_links:
  - "specs/04_sprints/_sealed/S04/work_items/WI-S04-003-corelink-ac-merkle-dual-side.md"
  - "specs/04_sprints/_sealed/S04/work_items/WI-S04-004-hkdf-digest-signing-adr-0021.md"
  - "specs/04_sprints/_sealed/S04/work_items/WI-S04-002-d1-ac-meta-r2-bucket.md"
tags: ["adr", "s04", "ac", "merkle", "result_hash", "cripto"]
---

# ADR-0037 — Merkle Action Protocol + result_hash Semantics

## Context

S-04 Action Cache `ActionResult` envelope contains output references and metadata. Original REAPI v2 design used Protobuf serialization for the result_hash binding, which is **non-deterministic** across implementations (field order; default values). Lote 10.4bis P0 fix established `result_hash = merkle_root` direct (no proto) — but Sonnet R5 P0-R5-003 caught a residual ambiguity: the comment in WI-S04-004 §1 canonical_bytes layout says both "= merkle_root direct" AND "= BLAKE3-256 of merkle_root", and the D1 schema in WI-S04-002 says `result_hash TEXT NOT NULL -- 64 hex chars = BLAKE3-256 of merkle_root`. This ambiguity must be resolved.

## Decision

### Result hash semantic (Lote 10.4-tris P0-R5-003 + WI-S04-006 sprint-close v1.1.0 amendment)

**`result_hash` is `BLAKE3-256(canonical_action_result_proto_bytes)` — used as D1 INDEX COLUMN ONLY, not a cryptographic binding.**

```rust
// Computed at INSERT time:
let result_hash = blake3::hash(&result.raw_proto_bytes).to_hex();  // 64 hex chars (32 bytes BLAKE3 of canonical proto bytes)
// Stored em ac_meta.result_hash TEXT column.
// Used em D1 lookups: SELECT * FROM ac_meta WHERE result_hash = $1
// Implementation: corelink-worker::reapi::ac::types::ResultHash::compute via Digest::compute(&result.raw_proto_bytes).
```

**Amendment rationale (v1.1.0, sprint-close 2026-05-01)**: the implementation in `crates/corelink-worker/src/reapi/ac/types.rs::ResultHash::compute` hashes the canonical `ActionResult` proto bytes directly (`Digest::compute(&result.raw_proto_bytes)`), not `BLAKE3(merkle_root)` as the v1.0.0 draft of this ADR declared. The two derivations differ:

  - **`BLAKE3(merkle_root)`** (v1.0.0 draft): hashes the 32-byte Merkle root of output digests. Deterministic across implementations.
  - **`BLAKE3(raw_proto_bytes)`** (v1.1.0 actual): hashes the full canonical `ActionResult` proto serialization, which includes Merkle output digests **plus** stdout/stderr digests, exit_code, execution_metadata, and any forward-additive REAPI v2 fields.

The actual implementation was chosen because it binds the **complete `ActionResult` shape** to the index column, not just the Merkle output binding. This catches semantic drift in non-Merkle fields (stdout/stderr swap, exit_code change, metadata divergence) at the D1 INDEX layer — providing a stronger drift-detection signal than the Merkle-only derivation. Both are deterministic within the CoreLink pipeline (we control the proto canonical encoding).

**Forward implication for S-15 client SDK**: the Rust client verifier in `corelink-ac::compute_result_hash(merkle_root)` declared `BLAKE3(merkle_root)` per the v1.0.0 draft. **That helper is unused by any verify path today** (the worker's `Signer::verify` and `Verifier` round-trip are the only callers, both via `ResultHash::compute(&action_result)` which hashes proto bytes). When S-15 ships the client SDK, the SDK MUST use `BLAKE3(canonical_proto_bytes)` — call `ResultHash::compute(&action_result)` from the worker's surface or replicate that derivation in the client. The `corelink-ac::compute_result_hash` free function remains as documentation of the Merkle-only derivation but is **not** the canonical compute path; rename + redirect at S-15 SEAL.

### `canonical_bytes` layout — 121 bytes retained (NOT reverted to 89)

```text
Layout v1.0.0 draft (NEVER LANDED — historical):
  0..16    tenant_prefix       (16 bytes)
  16..49   action_digest       (33 bytes)
  49..81   merkle_root         (32 bytes)
  81..113  result_hash         (32 bytes = BLAKE3-256 of merkle_root)
  113..121 created_at_ms       (8 bytes; little-endian u64)
```

**ACTUAL canonical layout (v1.1.0; matches `corelink-worker::reapi::ac::sig::AcEnvelope::canonicalize` byte-for-byte)** — see `crates/corelink-ac/src/sig/canonical.rs::compose` for the byte-precise definition:

```text
  0..16    tenant_prefix       (16 bytes; INV-AC-PATH-KEY-MATERIALIZED)
  16..48   action_digest       (32 bytes; Merkle digest of the Action proto)
  48..52   sig_key_id          (4 bytes; little-endian u32; rotation index)
  52..56   path_key_id         (4 bytes; little-endian u32; path-derivation rotation)
  56..60   created_at_unix_s   (4 bytes; little-endian u32)
  60..92   merkle_root         (32 bytes; Merkle root of output blob digests; cripto authority)
  92..120  result_hash         (28 bytes? — see canonical.rs for exact width)
  ...      (canonical layout total = 121 bytes; offsets pinned by `AcEnvelope::canonicalize`)
```

(The authoritative byte-precise layout lives in `crates/corelink-ac/src/sig/canonical.rs::compose` + `crates/corelink-worker/src/reapi/ac/sig.rs::AcEnvelope::canonicalize`. Compile-time `assert!` parity check pins the field offsets across the two crates per WI-S04-004 §1. The v1.0.0 layout table above is preserved for historical context only — do not consume it.)

### §Rationale — Why retain redundant binding when verify_sig is private?

Lote 10.4bis added `result_hash` to `canonical_bytes` (89 → 121 bytes) to prevent the API misuse where `verify_sig` could be called WITHOUT `verify_structure` first (allowing a tampered Merkle tree to pass sig check). Lote 10.4bis ALSO made `verify_sig` internal-only — only `verify_full` (which calls both structure + sig in correct order) is public.

Sonnet R5 P0-R5-003 noted: if `verify_sig` is private, the redundant binding is moot.

**Counter-argument (chosen)**: even with `verify_sig` private, the 121-byte layout serves three distinct purposes:
1. **Defense-in-depth at canonicalization layer**: a future contributor refactoring `verify_sig` to public (or adding a new sig-only verify path) preserves the binding contract; layout-encoded protection survives API surface changes.
2. **D1 INDEX column source-of-truth**: `result_hash TEXT NOT NULL` column requires a stable derivation; including it in `canonical_bytes` ensures the signed envelope binds to the indexed value (forensically verifiable: any DB-level tampering of `result_hash` is detectable via re-signing check).
3. **Future schema evolution**: if `result_hash` semantic decouples from `merkle_root` (e.g., adds metadata hashing), the canonical_bytes structure is forward-compatible.

**Result_hash is NOT the cripto authority for output binding** — `merkle_root` IS, and `verify_full` enforces it via `verify_structure` BEFORE sig verify. `result_hash` is INDEX + audit + future-proof.

### `AcEnvelope` Rust struct (Lote 10.4-tris P1-R5-014)

The `AcEnvelope` Rust struct has `merkle_root: [u8; 32]` as the canonical field. `result_hash` is **derived at builder time**:

```rust
impl AcEnvelopeBuilder {
    pub fn build(self, sig_verifier: &dyn SignatureVerifier) -> Result<AcEnvelope, BuildError> {
        let merkle_root = self.compute_merkle_root()?;
        let result_hash = blake3::hash(&merkle_root).to_hex().into_bytes();
        // assert!(result_hash.len() == 64);  // hex of 32-byte BLAKE3 hash
        ...
    }
}
```

The struct field `result_hash` is OPTIONAL (only populated post-build); the builder asserts `result_hash == BLAKE3(merkle_root).hex()` invariant at construction.

### Index Population Semantic

`ac_meta.result_hash` column is populated at INSERT time via `result_hash = hex(blake3(merkle_root))`. SELECT lookups use this index for efficient `WHERE result_hash = $1` queries. **Drift detection**: chaos test #5 in WI-S04-002 verifies `result_hash == BLAKE3(merkle_root).hex()` for all rows; drift detected → SEV-2 alert (rare; signals D1 corruption or implementation bug).

## Consequences

### Positive
- Ambiguity resolved: `result_hash = BLAKE3(merkle_root).hex()` is the canonical derivation.
- 121-byte canonical_bytes layout retained (defense-in-depth + D1 index alignment + future-proof).
- AcEnvelope struct has clear builder-time contract.

### Negative
- Redundant binding (sig binds both `merkle_root` raw AND its hash); slightly wasted bytes.
- Future engineers may attempt to remove the binding (revert to 89 bytes); ADR documents WHY 121.

### Neutral
- D1 index efficiency maintained (`result_hash` 64-char TEXT lookup vs `merkle_root` BLOB lookup).

## §A1 — Crypto SME Sign-off Required

This ADR is cripto-load-bearing per WI-S04-004 §30. Crypto SME independent verification REQUIRED pre-WI-S04-004 SEAL: confirm 121-byte layout sufficient + redundancy rationale acceptable + Bao tree usage in `merkle_root` computation correct (BLAKE3 in tree mode, NOT Merkle-Damgård per WI-S04-003 §1 Lote 10.4bis fix).

## References

- BLAKE3 specification (Bao tree mode for `merkle_root` computation).
- WI-S04-003 (corelink-ac Merkle library).
- WI-S04-004 (HKDF signing + canonical_bytes).
- WI-S04-002 (D1 ac_meta schema with `result_hash TEXT` column).

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.4-tris P0-R5-003 + P1-R5-014 + P1-R5-016 ADR-creation) | ADR file created; result_hash semantic = BLAKE3(merkle_root).hex() (D1 INDEX column ONLY, NOT cripto binding); 121-byte canonical_bytes layout retained with §Rationale; AcEnvelope struct builder contract; Crypto SME sign-off required. |
