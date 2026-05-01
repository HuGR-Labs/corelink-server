---
id: "ADR-0021"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-05-01"
title: "HKDF-SHA256 + BLAKE3-keyed MAC vs Ed25519 for Action Cache Digest Signing"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner)", "Crypto SME (TBD; mandatory emphatic per WI-S04-004 §30)"]
status_history: [{"date": "2026-04-25", "status": "FROZEN", "by": "Gustavo Schneiter (Lote 10.4-tris P1-R5-016 fix — file created from inline content in WI-S04-004 §1)"}]
context_links:
  - "specs/04_sprints/S04/work_items/WI-S04-004-hkdf-digest-signing-adr-0021.md"
  - "specs/04_sprints/S04/work_items/WI-S04-001-reapi-actioncache-handlers.md"
  - "specs/04_sprints/S04/_spec_contract.md"
tags: ["adr", "s04", "ac", "hkdf", "signing", "cripto-load-bearing"]
---

# ADR-0021 — HKDF-SHA256 + BLAKE3-keyed MAC vs Ed25519 for AC Digest Signing

## Context

S-04 Action Cache stores `ActionResult` envelopes per (tenant_id, action_digest). To prevent tampering and provide tenant-scoped integrity, each envelope must be signed. Decision: symmetric MAC (HKDF-derived per-tenant key + BLAKE3-keyed) vs asymmetric signature (Ed25519 with per-tenant keypair).

## Decision

**HKDF-SHA256 + BLAKE3-keyed MAC** is canonical for S-04 AC signing.

```rust
// Sign path:
let salt = sig_key_id.to_le_bytes();
let hk = Hkdf::<Sha256>::new(Some(&salt), &TDK);
let mut sig_key = [0u8; 32];
hk.expand(b"ac-sig", &mut sig_key)?;
let sig = blake3::Hasher::new_keyed(&sig_key).update(canonical_bytes).finalize();
// 32-byte sig; sig_key_id persisted alongside.
```

## Decision matrix

| Criterion | HKDF + BLAKE3-keyed (chosen) | Ed25519 |
|---|---|---|
| Performance (sign + verify p99) | ≤ 0.5ms native; ≤ 1ms WASM (CF Workers) | 3-5ms native; 8-15ms WASM |
| Key management | Per-tenant TDK (already exists S-01) | New per-tenant keypair (op overhead) |
| Tamper resistance | Symmetric — tenant cannot impersonate other tenants | Asymmetric — public key publication enables 3rd-party verify |
| Cripto baseline | NIST SP 800-108 KDF + RFC 5869 HKDF + BLAKE3-keyed (256-bit security) | NIST FIPS 186-5 Ed25519 (~128-bit security) |
| Threat model fit | Tenant-scoped integrity (we sign for ourselves; we verify for ourselves) | Public-verifier scenarios (we don't have these) |
| Constant-time discipline | Fully covered by `subtle::ct_eq` | Ed25519-dalek constant-time but key fetching cost |

**Winner: HKDF + BLAKE3-keyed** — faster, simpler key management, sufficient security for tenant-scoped integrity. Asymmetric Ed25519 advantages (3rd-party verification, non-repudiation) are NOT load-bearing in CoreLink's threat model.

## Consequences

### Positive
- Sign + verify ≤ 0.5ms p99 (Mann-Whitney 3-prong cripto-grade gate per WI-S04-004 §6.1.10).
- TDK reused (S-01 corelink-tenant-path); no new KMS key class.
- HKDF info `b"ac-sig"` provides domain separation from path-HMAC use of TDK (S-01) AND from manifest sig in S-05 (`b"manifest-sig"`, `b"meta-manifest-sig"`).

### Negative
- Tenant cannot prove ownership to 3rd party (acceptable; out-of-scope for CoreLink GA).
- Same TDK used for path-HMAC + HKDF-Extract (composition concern — see §Risks).

### Neutral
- 32-byte signature size (same as Ed25519 sig).
- Key rotation procedure documented separately for `sig_key_id` and `path_key_id` (see WI-S04-004 §30.1).

## §Risks (Lote 10.4-tris P0-R5-002 explicit acknowledgement)

**Risk: TDK multi-derivation without unified HKDF-Extract step.**

The TDK is used for:
1. `tenant_prefix = HMAC-SHA256(TDK, tenant_id)` truncated at the serialization boundary to 16 ASCII chars via `b64url_no_pad(...)[..16]` (raw HMAC bytes inside `corelink-tenant-path`, base64-URL-truncated for path use; canonical algorithm registered in **ADR-0043**).
2. `sig_key = HKDF-Expand(HKDF-Extract(salt=sig_key_id, IKM=TDK), info=b"ac-sig")` (HKDF chain; this WI).

These two usages share the same root secret without a unified HKDF-Extract step. In the formal security model, all uses of an IKM should pass through a single HKDF-Extract before any derivation.

**Mitigation analysis** (Crypto SME ruling required pre-WI-S04-004 SEAL):
- Option A (SOTA): refactor S-01 path-HMAC to also pass through HKDF-Extract(TDK, salt=tenant_id) → PRK; use HKDF-Expand with `info=b"path-prefix-v1"`. Breaking change to S-01; requires full re-derivation of all `tenant_prefix` materialized columns.
- **Option B (chosen — acceptable risk)**: keep raw HMAC for path; document that path-HMAC and HKDF-sig analysis assumes HMAC security (PRF property of HMAC-SHA256); attack cost bounded by 2^128 (HMAC-SHA256 PRF distinguishability bound). Accepted under standard cryptographic assumptions; compositional security not formally proven but practically secure.

**Crypto SME advisory ruling**: Option B accepted for S-04 GA. Option A tracked as S-07+ improvement (TDK rotation + key hierarchy redesign).

## §Sentinel and Edge Cases (Lote 10.4-tris P0-R5-001)

- `sig_key_id = 0` is reserved sentinel ("never-issued"). Rotation starts from `key_id = 1`.
- `HkdfVerifier` rejects `sig_key_id == 0` explicitly via `SigError::KeyIdReserved`.
- `accepted_key_ids.iter().min().unwrap_or(&1)` (NOT `&0`) avoids leaking sentinel via error response.
- `debug_assert!(!accepted_key_ids.is_empty())` in production verifier init path.
- 4-byte all-zero salt for `key_id = 1` is distinct from RFC 5869 default (32 zero bytes); intentional and documented.

## §RotationProcedures (Lote 10.4-tris P0-R5-006)

See WI-S04-004 §30.1 for canonical procedure. Summary:
- `sig_key_id`: bump current_key_id; add to accepted_key_ids; 30d grace; remove old.
- `path_key_id`: bump current_key_id for new INSERTs only; old rows retain old TDK indefinitely (NEVER delete retired path TDKs).
- `sig_key_id` and `path_key_id` MAY diverge per row (valid state); INV-AC-PATH-SIG-KEY-VERSION-INDEPENDENT.

## References

- RFC 5869 — HMAC-based Extract-and-Expand Key Derivation Function (HKDF).
- NIST SP 800-108 — Recommendation for Key Derivation Using Pseudorandom Functions.
- BLAKE3 Specification (Connor et al., 2020) — keyed MAC mode.
- WI-S04-004 §1 (canonical implementation).
- WI-S01-001 (corelink-tenant-path; raw HMAC path derivation).

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.4-tris P1-R5-016 ADR-creation) | ADR file created from inline content em WI-S04-004 §1; §Risks (P0-R5-002 HKDF composition); §Sentinel (P0-R5-001 key_id=0); §RotationProcedures (P0-R5-006). Architect + Crypto SME independent re-verification REQUIRED pre-WI-S04-004 SEAL. |
| 1.1.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7 1M) | ADR ratified — WI-S04-004 SEALED. Implementation lives at `crates/corelink-ac/src/sig/` (HKDF-SHA256 + BLAKE3-keyed) and matches the canonical decision matrix byte-for-byte: `salt = sig_key_id.to_le_bytes()`, `info = b"ac-sig"`, 32-byte tag, `subtle::ConstantTimeEq` verify, `accepted_key_ids` whitelist for rotation grace, `sig_key_id == 0` reserved sentinel rejected on every entry point. §Risks Option B accepted under standard HMAC security assumptions (path-HMAC and HKDF-Extract share the TDK; attack cost bounded by 2^128). Test corpus pins the algorithm: 28 lib unit + 11 canonical vectors + 8 key rotation + 6 property × 10 000 iter + Mann-Whitney 3-prong cripto-grade timing gate (10/80/10 trimmed mean; 0.5 ms `\|Δ\|` upper-CI threshold; release-mode-only). HKDF info CI gate (`b"ac-sig"` byte-equal) prevents typo-driven drift. |
