---
id: "ADR-S14-004"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
wi: "WI-S14-004"
sprint: "S-14"
tags: ["adr", "byok", "crypto", "kms", "envelope-encryption", "fips-140-3", "s14"]
supersedes: null
superseded_by: null
references:
  - "NIST SP 800-57 Pt 1 Rev 5 §5.6.2"
  - "NIST SP 800-130 §6.2"
  - "NIST FIPS 197 (AES)"
  - "NIST SP 800-38D (AES-GCM)"
  - "NIST SP 800-90A (DRBG)"
  - "FIPS 140-3"
  - "WI-S14-004 spec §9 (Design Decisions)"
  - "Lote 10.14 codex P0/P1 canonical fixes"
---

# ADR-S14-001: BYOK adapter trait + envelope encryption flow

## Status

**ACCEPTED** — ratified in WI-S14-004 implementation.

## Context

CoreLink S-14 introduces BYOK (Bring Your Own Key) enterprise tier.  The
design must simultaneously satisfy:

1. **Crypto sovereignty**: customer CMK revocation → cache inaccessible ≤ 5 min
   (INV-BYOK-CRYPTO-SOVEREIGNTY; SLA = 60s detection + 5min DEK TTL hard).
2. **Multi-cloud portability**: 4 KMS providers (AWS, GCP, Azure, Vault) with
   identical behaviour contract.
3. **FIPS 140-3 compliance**: AWS KMS L1 (default); others documented per provider.
4. **Security**: DEK must never be deterministic; nonce must never be reused;
   cross-blob key swap must be prevented.

Several design decisions were contentious and required explicit recording.

## Decisions

### D1 — DEK ephemeral random via CSPRNG (NOT BLAKE3-derived)

**Decision**: `getrandom::getrandom(&mut [0u8; 32])` — OS-backed CSPRNG
(NIST SP 800-90A approved DRBG: AES-CTR-DRBG or Hash-DRBG depending on OS).
NOT derived from `{tenant_id, blob_hash, salt}` via BLAKE3 or any KDF.

**Rationale**: deterministic DEK means compromise of one blob's DEK =
compromise of all blobs with the same hash-based input.  Random DEK isolates
the blast radius to the specific wrapped DEK.  NIST SP 800-57 Pt 1 Rev 5
§5.6.2 mandates that symmetric DEKs be generated from an approved random bit
generator.

**Rejected alternatives**:
- BLAKE3-derived deterministic DEK: cryptographically unsound for envelope
  encryption; violates NIST SP 800-57 §5.6.2; "reproducibility" is not
  needed — CMK rotation uses unwrap+rewrap preserving DEK identity.

**Codex**: Lote 10.14 P1 canonical fix.

---

### D2 — DEK cache TTL 5 min hard limit (no exception, no advisory mode)

**Decision**: `DekCache::new(ttl_seconds)` rejects `ttl_seconds > 300` with
`BYOKError::DekCacheTtlViolation`.  No runtime override; no advisory mode;
no operator escape hatch.

**Rationale**: customer kill switch SLA = 60s detection + 5min DEK cache TTL
hard = 6min p99 total.  TTL > 300s breaks the contract and the customer
"loses real control" of their data.  INV-BYOK-CRYPTO-SOVEREIGNTY is CRITICAL;
per `_spec_contract.md §19` TTL is non-waivable.

**Rejected alternatives**:
- TTL 10 min "for performance": breaks kill switch SLA; customer trust
  permanently lost.  Previously listed as waivable — this was a spec error
  (Lote 10.14 P0 canonical fix removes that waiver row).

**Codex**: Lote 10.14 P0 canonical fix.

---

### D3 — AES-256-GCM nonce 96-bit random per-write

**Decision**: each write generates a fresh 96-bit nonce via `getrandom`.
Nonce is stored alongside ciphertext (non-secret).

**Rationale**: AES-GCM nonce reuse with the same key is catastrophic
(NIST SP 800-38D).  96-bit random: birthday collision requires ~2^48 writes
(~280 trillion) which is beyond CoreLink per-tenant capacity for the
foreseeable future.  Counter-based nonces require per-tenant distributed
counter synchronisation — prohibitive in multi-region stateless Workers.

---

### D4 — AAD `encryption_context` binding `{tenant_id, blob_hash}` mandatory

**Decision**: every `wrap_dek` call includes `encryption_context =
{"tenant_id": "...", "blob_hash": "..."}` as AAD.  Missing context = error.
AWS KMS enforces on the server side; GCP/Azure/Vault adapters replicate this
in WI-S14-005.

**Rationale**: without AAD binding, an attacker who intercepts a wrapped DEK
for blob B1 can substitute it for blob B2's entry, allowing unauthorised
decryption (NIST SP 800-130 §6.2 cross-entity binding).

---

### D5 — `KmsProvider` async trait via `async-trait`

**Decision**: `KmsProvider` is an `#[async_trait]` Rust trait.  Each provider
is a struct implementing the trait.  `EnvelopeEncryptor<P: KmsProvider>`
is generic.

**Rationale**: clean adapter pattern; allows compile-time monomorphisation for
the production path and dynamic dispatch (`dyn KmsProvider`) for the matrix
test framework.  Avoids pinning the codebase to any single SDK.

---

### D6 — 16-combination matrix test framework

**Decision**: `tests/byok_matrix_framework.rs` drives 4 providers × 4 ops
= 16 cells.  WI-S14-004 provides 4 AWS cells; WI-S14-005 adds 12 more.
Pending cells report `MatrixCellStatus::Pending` (not fail) until the
adapter is implemented.

**Rationale**: any drift between provider behaviours is caught in CI before
merging.  Auto-fail on Fail cells; Pending cells are visible but non-blocking
until WI-S14-005 is sealed.

---

## Consequences

**Positive**:
- DEK compromise blast radius bounded per-write.
- Kill switch SLA contractually enforced in code.
- Cross-blob swap attacks structurally prevented.
- Multi-cloud extension is additive (implement trait; no core changes).
- Matrix test prevents silent provider divergence.

**Negative / trade-offs**:
- Random DEK requires KMS network call on every non-cached read (mitigated
  by 5-min DEK cache).
- `async-trait` adds a box allocation per call (acceptable overhead vs.
  KMS network latency).
- DEK cache introduces a 5-min window where revoked access is still served
  (contractually accepted; kill switch SLA is 6min p99).

## Reuse

This pattern is reused in:
- **WI-S14-005**: GCP / Azure / Vault adapters implement `KmsProvider`.
- **WI-S14-006**: kill switch consumes `DekCache::evict_all_for_key`.
- **WI-S14-007**: erasure attestation wraps the envelope path.
- **BYOE Fase 2**: per-blob CMK (different `KmsKeyId` per blob) is
  drop-in via the same trait.
