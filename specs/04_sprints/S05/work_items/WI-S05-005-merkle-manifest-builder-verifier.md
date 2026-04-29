---
id: "WI-S05-005"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
parent: "S-05"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "STORAGE-SEMANTICS"
  - "CAS-PROFILE"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s05", "merkle", "manifest", "builder", "verifier", "dual-side", "high-risk", "crypto"]
---

# WI-S05-005 — Merkle Manifest Builder + Verifier Dual-Side (Server Pre-persist + Client Post-download) + Streaming Progressive Verify + INV-CAS-INTEGRITY Tree Enforcement + Property Test 100k Invalid Trees + Reuse Pattern WI-S04-003

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-05](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S05-005 |
| Título | Manifest tree builder/verifier; dual-side verify (server pre-persist + client post-download via SDK); streaming progressive verify (fail-fast em mid-stream invalid chunk); INV-CAS-INTEGRITY tree enforcement; HKDF sig info=`b"manifest-sig"` separated domain (lesson WI-S04-004 ADR-0021); bounded parser MAX_CHUNKS_PER_BLOB 81920; cycle detection (re-uses pattern WI-S04-003); test vectors Annex A + B; cargo-fuzz 1h CI nightly; Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 1ms |
| Sprint | S-05 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (manifest forge → cross-tenant chunk leak via reassembled blob), FF-HR-005 (CTRL-CAS-001 distribuído em tree; verify boundary), FF-HR-009 (defense-in-depth — server + client verify) |

## 1. Intent

Implementar `crates/corelink-manifest/` — biblioteca Merkle tree manifest:

```rust
#![forbid(unsafe_code)]

pub use crate::builder::{ManifestBuilder, Manifest};
pub use crate::verifier::{ManifestVerifier, VerifyError};
pub use crate::sig::{ManifestSigner, ManifestVerifierSig};
pub use crate::error::ManifestError;

pub mod builder;
pub mod verifier;
pub mod sig;       // HKDF info=b"manifest-sig" reuse pattern WI-S04-004
pub mod bounds;    // constants

pub struct Manifest {
    pub version: u8,                       // = 1
    pub tenant_id: TenantId,
    pub blob_digest: BlobDigest,           // 32 bytes BLAKE3-256 of total reassembled bytes
    pub merkle_root: [u8; 32],             // BLAKE3-256 of chunk tree
    pub chunk_count: u32,                  // ≤ 81920 (Lote 10.5bis P0 fix off-by-one)
    pub total_size_bytes: u64,             // sum of chunk sizes
    pub chunks: Vec<ChunkRef>,             // ordered chunk references
    pub created_at_ms: u64,
    pub sig: Vec<u8>,                      // HKDF-keyed sig 32 bytes
    pub sig_key_id: u32,                   // HKDF tenant_key version
    pub chunker_algo: ChunkerAlgorithm,    // Fixed2MiB | FastCDC2MiB (from WI-S05-002)
}

pub struct ChunkRef {
    pub index: u32,                        // 0..chunk_count
    pub digest: [u8; 32],                  // BLAKE3-256 of chunk bytes
    pub size_bytes: u32,                   // 1..=4 MiB
}

pub trait ManifestBuilder {
    /// Server-side: builds Manifest from streaming chunks (consumes WI-S05-002 chunker output).
    /// Sig delegated to ManifestSigner (WI-S04-004 pattern reuse).
    async fn build(
        &self,
        tenant_id: &TenantId,
        blob_digest: &BlobDigest,
        chunks: impl Stream<Item = (ChunkDigest, u32)>,
        chunker_algo: ChunkerAlgorithm,
    ) -> Result<Manifest, ManifestError>;
}

pub trait ManifestVerifier {
    /// Server pre-persist: verify Merkle structure + bounds (no sig check; that's separate).
    /// **Lote 10.5bis P0 fix**: pub(crate); NOT public — caller deve usar verify_full
    /// to prevent API misuse (verify_sig sem verify_structure first).
    pub(crate) fn verify_structure(&self, manifest: &Manifest) -> Result<(), VerifyError>;

    /// Client-side post-download: verify structure THEN sig (pinned ordering; lesson WI-S04-003 chaos #1).
    /// Returns Ok(()) only if structure + sig both valid.
    /// **Public API**: this is the only way to verify a manifest from external code.
    fn verify_full(
        &self,
        manifest: &Manifest,
        sig_verifier: &dyn SignatureVerifier,    // WI-S04-004 trait reuse via wrapper (see §6.1.X)
    ) -> Result<(), VerifyError>;

    /// Streaming progressive verify: as chunks arrive, verify each against manifest's chunk_digests.
    /// Fail-fast em mid-stream invalid chunk; signal handler to abort streaming via cancel_token.
    /// **Lote 10.5bis P0 fix**: cancel_token mandatory — caller drops/cancels token to abort upstream
    /// stream when verifier signals mismatch; previously abort mechanism was unspecified (returning
    /// Result alone doesn't propagate cancellation back to producer).
    async fn verify_streaming<'a>(
        &'a self,
        manifest: &'a Manifest,
        chunk_stream: impl Stream<Item = (u32, Bytes)> + 'a,
        cancel_token: tokio_util::sync::CancellationToken,  // Lote 10.5bis P0 fix
    ) -> Result<(), VerifyError>;
}

/// Lote 10.5bis P0 fix: ManifestSignatureVerifier wraps WI-S04-004 SignatureVerifier
/// trait with HKDF info=`b"manifest-sig"` domain separation hard-wired internally.
/// The WI-S04-004 SignatureVerifier hard-codes `info=b"ac-sig"`; this wrapper provides
/// the manifest-domain equivalent without modifying upstream WI-S04-004 (which may
/// already be SEALED).
pub struct ManifestSignatureVerifier {
    tdk_handle: Arc<dyn TdkHandle>,
    accepted_key_ids: Vec<u32>,
}

impl ManifestSignatureVerifier {
    /// Sign manifest envelope with HKDF info=`b"manifest-sig"`.
    pub fn sign_manifest(&self, canonical_bytes: &[u8]) -> Result<(Vec<u8>, u32), SigError> {
        // ... (impl mirrors WI-S04-004 HkdfSigner but with info=b"manifest-sig" + salt=current_key_id)
    }

    /// Verify manifest sig with HKDF info=`b"manifest-sig"`.
    /// CI byte-equal test asserts info string distinct from `b"ac-sig"` and `b"meta-manifest-sig"`.
    pub fn verify_manifest_sig(&self, canonical_bytes: &[u8], sig: &[u8], sig_key_id: u32) -> Result<(), SigError> {
        // ... (impl mirrors WI-S04-004 HkdfVerifier but with info=b"manifest-sig")
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ManifestError {
    #[error("chunk count {found} exceeds bound {bound}")]
    ChunkCountExceeded { found: u32, bound: u32 },          // bound=81920

    #[error("total size {found} exceeds bound {bound}")]
    TotalSizeExceeded { found: u64, bound: u64 },           // bound=160 GiB

    #[error("merkle root mismatch: computed={computed:x?}, claimed={claimed:x?}")]
    RootMismatch { computed: [u8; 32], claimed: [u8; 32] },

    #[error("chunk digest format invalid at index {0}")]
    InvalidChunkDigest(u32),

    #[error("chunk index {0} out of order (expected {1})")]
    ChunkIndexOutOfOrder(u32, u32),

    #[error("manifest version unsupported: {0}")]
    VersionUnsupported(u8),

    #[error("sig delegation error: {0}")]
    SigError(String),
}

pub enum VerifyError {
    Structure(ManifestError),
    Sig(SigError),                                          // delegate WI-S04-004
    StreamingChunkMismatch { index: u32 },
}
```

**Cripto-driven invariants enforced**:

1. **Merkle hash = BLAKE3-256** (consistent com WI-S04-003 Merkle pattern).
2. **Determinism**: tree construction byte-stable for same `(tenant_id, blob_digest, ordered chunks)` — no timestamps in tree; protobuf-free encoding (lesson Lote 10.4bis WI-S04-003 P0 fix: result_hash = merkle_root direct).
3. **Domain separation**: HKDF info=`b"manifest-sig"` (different from `b"ac-sig"` in WI-S04-004 + `b"meta-manifest-sig"` in WI-S05-006 stitched flow); prevents cross-domain replay; CI byte-equal test asserts all 3 info strings present, distinct, non-prefix.
4. **Bounded parser**: MAX_CHUNK_COUNT 81920 (= 160 GiB / 2 MiB exato; **Lote 10.5bis P0 fix**: was 80000 off-by-one with 160 GiB / 2 MiB = 81920); MAX_TOTAL_SIZE 160 GiB; reject early at decode.
5. **Streaming progressive verify**: fail-fast pattern; never trust client-supplied bytes mid-stream; abort signal via `CancellationToken` parameter (Lote 10.5bis P0 fix: previously unspecified).
6. **canonical_bytes() binding** (Lote 10.5bis P0 fix: highest-leverage; previously unpublished): sig MUST commit to all bound-relevant fields, not merely merkle_root.

**Tree shape** (Lote 10.5bis P0 fix: pinned ordering by chunk_index ascending; **NOT** lex-sort-by-digest like WI-S04-003 — that pattern was for ActionResult unordered file paths; for multipart, chunks are an ORDERED sequence whose concatenation is the blob):

- Leaves are `leaf_hash[i] = blake3(b"\x00" || chunks[i].digest)` for `i in 0..chunk_count` (domain separation per RFC 6962-style).
- Inner nodes are `inner_hash = blake3(b"\x01" || left || right)` (prevents 2nd-preimage tree-shape attack).
- Tree is balanced binary over leaves **in chunk_index ascending order; NO sorting** — order is semantic (concatenation order produces the blob).
- Root: top of recursion.
- Property test `prop_manifest_chunk_order_preserved`: shuffle input chunks → manifest_root differs → reject (asserts order matters).

**`Manifest::canonical_bytes()` layout** (Lote 10.5bis P0 fix; explicit per WI-S04-004 §6.1.5 layout style; the sig signs over canonical_bytes, NOT merely merkle_root):

```
canonical_bytes_v1 = 
    version_le_u8                              (1 byte)
 || tenant_id_uuidv7                           (16 bytes; raw UUIDv7)
 || blob_digest                                (32 bytes; BLAKE3-256)
 || merkle_root                                (32 bytes; BLAKE3-256)
 || chunk_count_le_u32                         (4 bytes)
 || total_size_bytes_le_u64                    (8 bytes)
 || created_at_ms_le_u64                       (8 bytes)
 || chunker_algo_le_u8                         (1 byte; Fixed2MiB=1, FastCDC2MiB=2)
                                               ─── 102 bytes total

pub fn canonical_bytes(manifest: &Manifest) -> [u8; 102] {
    let mut out = [0u8; 102];
    out[0] = manifest.version;
    out[1..17].copy_from_slice(manifest.tenant_id.as_bytes());
    out[17..49].copy_from_slice(&manifest.blob_digest);
    out[49..81].copy_from_slice(&manifest.merkle_root);
    out[81..85].copy_from_slice(&manifest.chunk_count.to_le_bytes());
    out[85..93].copy_from_slice(&manifest.total_size_bytes.to_le_bytes());
    out[93..101].copy_from_slice(&manifest.created_at_ms.to_le_bytes());
    out[101] = manifest.chunker_algo as u8;
    out
}
```

**Why all 8 fields signed** (defense against bound-bypass forge):
- An attacker who captures a valid manifest with `chunk_count=25, total_size_bytes=50_MiB` could otherwise forge a manifest with same `merkle_root` but `chunk_count=80000, total_size_bytes=160_GiB` — bounded parser would accept (within bounds) and sig would still verify. By committing chunk_count + total_size_bytes into canonical_bytes, the sig binds the bounds-relevant metadata.
- `chunks: Vec<ChunkRef>` is NOT in canonical_bytes — `merkle_root` already commits to chunks via tree binding.
- `sig`, `sig_key_id` are NOT in canonical_bytes — those are the sig output, not input.

**Pattern reuse from WI-S04-003** (with corrections):
- BLAKE3 + RFC 6962 domain separation: same primitive.
- Bounded parser + cycle detection: not applicable here (manifest tree is flat list of chunks; no nested directories).
- Test vectors Annex pattern: 70 valid + 70 invalid manifests (Lote 10.5bis P0 fix; was 50+50 — 50 inadequate for 7 error variants).
- cargo-fuzz harness 1h: 4 targets (decode, verify, sig, build/encode — Lote 10.5bis P0 fix: was 3 missing builder).
- **NOT** reused: lex-sort-by-digest (WI-S04-003 pattern only valid for unordered ActionResult outputs; multipart chunks are ordered sequence).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Manifest dual-side verify é **única defense efetiva contra cache poisoning em multipart**. Sem verify cliente-side (post-download), atacante que comprometa R2 manifest envelope ou intermediate proxy injeta tampered chunks; cliente Bazel reassembles bytes; build "succeeds" com binário comprometido.

HIGH_RISK em N dimensões:

1. **Manifest forge via cripto break (BLAKE3-256)**: collision-resistant 2^128; computacionalmente intratável. Mitigação: BLAKE3 paper §6 NIST-compatible analysis; ADR fallback SHA-256 forward if needed.

2. **Determinism violation**: two server builds produce different manifest_root for same input chunks. Mitigação: lex sort by chunk_digest (consistente WI-S04-003); deterministic encoding (no protobuf for canonical bytes); property test 1000 random blobs × 100 builds = byte-identical.

3. **Streaming progressive verify bypass**: handler skips per-chunk verify mid-stream → tampered chunk reaches client. Mitigação: `verify_streaming()` mandatory em SpliceBlob (WI-S05-001 §6.1.4 step [5]); integration test asserts call.

4. **Sig domain separation drift**: dev refactors HKDF info from `b"manifest-sig"` to `b"ac-sig"` — manifest sig becomes valid em AC envelope domain (cross-domain replay). Mitigação: CI byte-equal test asserts info string fixed; ADR-0021/0038 documents domain separation.

5. **MAX_CHUNK_COUNT exceeded**: crafted manifest com 1B chunks → D1 manifest_chunks INSERT exhaustion. Mitigação: bounded MAX_CHUNK_COUNT 81920; `ChunkCountExceeded` error em `build()`; verify_structure rejects.

6. **Chunk index out-of-order attack**: malicious manifest claims chunks [0, 5, 1, 2, 3, 4] (out of order); reassembly produces wrong bytes. Mitigação: builder enforces sorted by index 0..N-1 sequential; verify checks; integration test asserts.

7. **Streaming verify partial fail handling**: chunk N tampered; verify catches; handler aborts mid-stream; client receives partial bytes + error trailer. Mitigação: gRPC error trailer mechanism; HTTP chunked transfer with Trailer header; client SDK propagates error.

8. **Cargo-fuzz crash on adversarial manifest**: 1h CI nightly; assert no panic; bounded inputs.

**Atacante adversarial scenarios**:

- **Cache poisoning via R2 manifest tampering** (insider with R2 access; no HKDF key): sig verify catches; partial compromise scenario (lesson Lote 10.4bis WI-S04-003 framing).

- **Full HKDF compromise (chave leak)**: attacker recomputes Merkle root + re-signs; both layers fail; mitigated externally via TDK rotation + S-09 audit chain (lesson Lote 10.4bis honest framing).

- **Replay attack** (capture manifest from one tenant; replay onto another): manifest sig binds tenant_id em canonical_bytes; cross-tenant replay rejected.

- **DoS via crafted oversized manifest**: bounded parser rejects.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: manifest forge → cross-tenant chunk leak via reassembled blob.
- **FF-HR-005**: CTRL-CAS-001 distribuído em tree; verify boundary.
- **FF-HR-009**: defense-in-depth dual-side; effective em partial compromise.
- **Reversibility**: cache poisoning detected via INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY pattern (S-06 forward reconcile).
- **Customer impact**: build with poisoned binary = supply chain incident.

11 sign-offs canonical incl. **Architect with Crypto SME specialization mandatory emphatic** (BLAKE3 + Merkle protocol + sig domain separation review).

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev**: post-Splice, client SDK (Rust v1.0 corelink-manifest) verifies dual-side; fail-fast if mismatch; build aborts; customer notified.

**Persona 2 — Security auditor (SLSA L3)**: reviews dual-side verify discipline; reviews test vectors Annex (50 valid + 50 invalid); reviews threat model em `manifest_protocol.md`.

**Persona 3 — Compliance reviewer**: SLSA L3 partial alignment via Merkle + audit chain.

**SLA addendum**:
- Manifest verify p99 ≤ 50ms for trees ≤ 80k chunks (criterion benchmark).
- Builder p99 ≤ 100ms for 80k chunks (streaming).
- Streaming verify per-chunk p99 ≤ 5ms (BLAKE3 SIMD).
- Test vectors Annex em `corelink-manifest/spec/test_vectors_manifest.md`.
- Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 1ms (between valid vs tampered manifest verify).

## 4. Capability Mapping

- **CAP-CAS-009** (Merkle chunking) — IMPLEMENTA verifier side; chunker em WI-S05-002.
- Trace: `cas_profile.md §4 (Merkle decomposition)` + `security_model.md CTRL-CAS-001` + ADR-0022.

## 5. Tipo

Cripto library; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo (compact)

### 6.1 In-scope

1. Crate `corelink-manifest`: builder + verifier + sig modules.
2. Manifest tree builder: BLAKE3 + RFC 6962 domain sep (lesson WI-S04-003).
3. Verifier dual-side: server pre-persist + client post-download.
4. Streaming progressive verify: fail-fast mid-stream.
5. HKDF sig info=`b"manifest-sig"` (domain separated from `b"ac-sig"`); reuses `SignatureVerifier` trait from WI-S04-004.
6. Bounded parser: MAX_CHUNK_COUNT 81920; MAX_TOTAL_SIZE 160 GiB.
7. Property tests (10k iter PR; 100k nightly):
   - `prop_manifest_determinism`: 1000 random × 100 builds = byte-identical.
   - `prop_manifest_round_trip`: encode → decode → re-encode = identical.
   - `prop_manifest_tampering_detected`: 1000 random; flip 1 byte in chunks; 100% caught.
   - `prop_manifest_bounds_enforcement`: oversized manifests rejected.
   - `prop_streaming_verify_fail_fast`: tampered chunk N; verify catches before chunk N+1.
8. Mann-Whitney 3-prong cripto-grade test (valid vs tampered manifest verify timing); |Δmedian| ≤ 1ms.
9. Criterion benchmarks: verify ≤ 50ms p99 @ 80k chunks; build ≤ 100ms p99.
10. Cargo-fuzz harness 1h CI nightly (decode + verify + sig).
11. Test vectors Annex A (50 valid manifests) + B (50 invalid each error variant).
12. Documentation: `crates/corelink-manifest/README.md` + `spec/manifest_protocol.md` + threat model.
13. Public API stability: `#[non_exhaustive]` on `Manifest` struct; semver discipline; ADR-0041 forward (manifest API stability).

### 6.2 Out-of-scope (deferred)

- Client SDK Java/C++ impl (S-15 forward; spec doc + reference vectors enable).
- Online schema migration v1 → v2 (post-GA via ADR).
- Property test 1M iter (overkill for S-05).

## 7. Anti-Scope

- ❌ Skip `verify_structure()` em build (always invoked internally).
- ❌ Variable hash function (BLAKE3-256 fixed).
- ❌ Non-deterministic build (lex sort by index enforced).
- ❌ Server-only verify (dual-side defense-in-depth).
- ❌ HKDF info=`b"ac-sig"` reuse (cross-domain replay).
- ❌ Skip streaming progressive verify em SpliceBlob (security control bypass).
- ❌ Cargo-fuzz < 1h.
- ❌ Mann-Whitney p>0.05 sozinho (3-prong gate).
- ❌ Test vectors absent.
- ❌ Public API breaking changes post-v1.0 sem ADR.

## 8. Acceptance Criteria (Gherkin) (compact 9 scenarios)

```gherkin
Feature: Merkle manifest builder + verifier dual-side

  Scenario: Build manifest from valid chunks
    Given 25 chunks (50 MiB blob; Fixed2MiB)
    When ManifestBuilder::build(tenant_id=A, blob_digest=D, chunks)
    Then manifest created with computed merkle_root + sig + 25 ChunkRef ordered
    And p99 build latency ≤ 100ms

  Scenario: verify_structure detects tampering
    Given valid manifest M
    When 1 byte flipped in M.chunks[0].digest
    And verify_structure(M)
    Then error VerifyError::Structure(RootMismatch)
    And p99 verify latency ≤ 50ms @ 25 chunks

  Scenario: Determinism — 1000 manifests × 100 builds = byte-identical
    Given 1000 random ordered chunks lists
    When each built 100 times
    Then 100 builds produce IDENTICAL manifest bytes (sig + tree)
    And property test prop_manifest_determinism green

  Scenario: Bounded parser rejects oversized manifest (chunk_count)
    Given manifest claiming 100000 chunks (> 81920 max)
    When verify_structure
    Then error ManifestError::ChunkCountExceeded { found: 100000, bound: 81920 }
    And p99 reject ≤ 1ms (fail-fast at decode)

  Scenario: Bounded parser rejects oversized manifest (total_size)
    Given manifest claiming total_size = 200 GiB
    When verify_structure
    Then error ManifestError::TotalSizeExceeded

  Scenario: Streaming progressive verify fail-fast mid-stream
    Given valid manifest M (25 chunks)
    Given chunk 5 tampered (mid-stream)
    When verify_streaming(M, chunks_stream)
    Then fail at chunk 5; chunks 6..25 NOT processed
    And error VerifyError::StreamingChunkMismatch { index: 5 }
    And handler signals abort to client

  Scenario: verify_full (client-side) — sig + structure
    Given valid manifest M with valid sig
    When verify_full(M, sig_verifier)
    Then OK

  Scenario: verify_full detects sig forge (chave compromise simulated; partial)
    Given manifest M with structure valid; sig forged with wrong key
    When verify_full(M, sig_verifier_with_correct_key)
    Then error VerifyError::Sig(SigInvalid)

  Scenario: Mann-Whitney verify timing — valid vs tampered indistinguishable
    Given 10k valid + 10k tampered manifests
    When verify timing measured
    Then p > 0.05 com Šidák 3-trial; |Δmedian| ≤ 1ms (cripto-grade)
```

## 9. Design Decisions (compact)

- **9.1** BLAKE3-256 (consistent WI-S04-003 + WI-S05-002 chunker; SIMD performance).
- **9.2** RFC 6962 domain separation (\x00 leaf / \x01 inner) — prevents tree-shape attack.
- **9.3** HKDF info=`b"manifest-sig"` separated from `b"ac-sig"` (cross-domain replay defense).
- **9.4** Streaming progressive verify mandatory em SpliceBlob (defense-in-depth).
- **9.5** Bounded MAX_CHUNK_COUNT 81920 = 160 GiB / 2 MiB (consistent ADR-0022).
- **9.6** result_hash = merkle_root direct (lesson Lote 10.4bis WI-S04-003 P0 fix; eliminates protobuf-determinism dependency).
- **9.7** SignatureVerifier trait reuse from WI-S04-004 (no duplicate cripto stack).
- **9.8** Test vectors Annex pattern reuse from WI-S04-003.
- **9.9** Cargo-fuzz 1h pattern reuse.
- **9.10** ADR forward: ADR-0041 — manifest public API stability + semver + sig domain separation policy.

## 10. Completeness Criteria SOTA

- [ ] **10.s05.005.1** Property tests 5 × 10k iter PR + 100k nightly green.
- [ ] **10.s05.005.2** Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 1ms (valid vs tampered verify).
- [ ] **10.s05.005.3** Criterion benchmarks: verify ≤ 50ms p99 @ 80k chunks; build ≤ 100ms p99.
- [ ] **10.s05.005.4** Cargo-fuzz 1h CI nightly (3 targets: decode + verify + sig) → 0 panics.
- [ ] **10.s05.005.5** Test vectors Annex A (50 valid) + B (50 invalid; each error variant).
- [ ] **10.s05.005.6** Determinism property: 1000 × 100 builds = byte-identical.
- [ ] **10.s05.005.7** Streaming progressive verify fail-fast test green.
- [ ] **10.s05.005.8** ADR-0041 published forward (manifest API stability).
- [ ] **10.s05.005.9** rustdoc 100% public API + 4 examples + threat model README + spec/manifest_protocol.md.
- [ ] **10.s05.005.10** Public API stability: `#[non_exhaustive]`; semver discipline.

## 11. DoD

- [ ] Crate compila + integration tests green.
- [ ] All Gherkin scenarios green.
- [ ] Property tests 10k green; 100k nightly.
- [ ] Mann-Whitney 3-prong cripto-grade green.
- [ ] Criterion benchmarks green.
- [ ] Cargo-fuzz 1h CI nightly green.
- [ ] Test vectors Annex published + integrated CI.
- [ ] ADR-0041 published forward.
- [ ] rustdoc + 4 examples + threat model README.
- [ ] `crates/corelink-manifest/spec/manifest_protocol.md` published.
- [ ] Architect + AppSec + Crypto SME (mandatory emphatic) + Security Lead reviews.
- [ ] PRR Architect + Crypto SME mini sign-off.
- [ ] Cost regression gate green.

## 12. Invariants Validated

- **INV-CAS-INTEGRITY** (CRITICAL, registry §3.3): manifest tree binds chunks; verify mandatory pre-persist + post-download.
- **INV-CAS-IDEMPOTENCY** (CRITICAL, registry §3.3): same chunks order → same manifest_root byte-identical.
- **INV-MULTIPART-MANIFEST-SIGNED** (HIGH, registry §3.16): HKDF sig info=`b"manifest-sig"` separated domain.
- **INV-MULTIPART-MANIFEST-VALID** (CRITICAL, NEW promovida §3.16 Lote 10.5bis): verify_structure rejects 100% tampered manifests.
- **INV-MULTIPART-DUAL-SIDE-VERIFY** (HIGH, NEW): server + client both invokeable; structure independent of sig.
- **INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST** (HIGH, NEW): mid-stream tampered chunk catches before next chunk processed.
- **INV-MULTIPART-BOUNDED-PARSER** (HIGH, registry §3.16): MAX_CHUNK_COUNT 81920; MAX_TOTAL_SIZE 160 GiB.

TLA+ alignment: `cas_integrity.tla` chunked variant (forward S-09 TLA+ work).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate `corelink-manifest` | `crates/corelink-manifest/` | Rust |
| Builder module | `crates/corelink-manifest/src/builder/` | Rust |
| Verifier module | `crates/corelink-manifest/src/verifier/` | Rust |
| Sig module | `crates/corelink-manifest/src/sig/` (HKDF info reuse pattern WI-S04-004) | Rust |
| Bounds + Error | `crates/corelink-manifest/src/{bounds,error}.rs` | Rust |
| Property tests | `crates/corelink-manifest/tests/prop_manifest.rs` | Rust |
| Mann-Whitney timing tests | `crates/corelink-manifest/tests/timing_manifest.rs` | Rust |
| Criterion benchmarks | `crates/corelink-manifest/benches/` | Rust |
| Cargo-fuzz harness | `crates/corelink-manifest/fuzz/fuzz_targets/` (3 targets) | Rust |
| Test vectors Annex | `crates/corelink-manifest/spec/test_vectors_manifest.md` + `tests/fixtures/` | Markdown + JSON |
| Spec doc | `crates/corelink-manifest/spec/manifest_protocol.md` | Markdown |
| README + threat model | `crates/corelink-manifest/README.md` | Markdown |
| Examples | `crates/corelink-manifest/examples/` (4 examples) | Rust |
| ADR-0041 | `specs/03_architecture/adrs/ADR-0041-manifest-api-stability.md` | Markdown |

## 14. Quality Standards SOTA (compact)

- 14.s05.005.1: `#![forbid(unsafe_code)]`; zero `unwrap`.
- 14.s05.005.2: rustdoc 100% public API + 4 examples + threat model.
- 14.s05.005.3: Test coverage ≥ 95% (cripto boundary).
- 14.s05.005.4: Latência: verify ≤ 50ms p99 @ 80k chunks; build ≤ 100ms p99; streaming verify per-chunk ≤ 5ms.
- 14.s05.005.5: SAST: cargo-audit + cargo-deny + clippy `-D warnings`; cargo-fuzz 1h × 3 targets CI nightly.
- 14.s05.005.6: Métricas: corelink.manifest.{verify_duration_us, build_duration_us, bounds_exceeded_total{kind}}.
- 14.s05.005.7: Public API stability `#[non_exhaustive]`; semver post v1.0; ADR-0041.
- 14.s05.005.8: REAPI v2 spec compliance (manifest format aligned).
- 14.s05.005.9: Memory bounded (Lote 10.5-tris P0-SR5-003 fix — math corrected; was claimed ≤ 32 KiB stack but Vec<ChunkRef> at 81920 × 40 bytes = 3.28 MB heap):
  - **`verify_full` / `verify_structure`**: ≤ 4 MiB heap (Manifest worst-case 81920 ChunkRefs ~3.28 MB + envelope fields ~32 KB) + ≤ 32 KiB stack.
  - **`verify_streaming`** (Lote 10.5-tris P0-SR5-003 (c) redesign): does NOT hold full `Vec<ChunkRef>` in memory; verifier reads from D1 `manifest_chunks` table per-chunk (one row at a time) → **O(1) memory per chunk** (~64 bytes); total streaming verify memory ~32 KiB stack only. Architectural cleanup: aligns with D1 manifest_chunks table's existing purpose (Lote 10.5bis WI-S05-004 batch INSERT em step [10] of SplitBlob).
  - **Per-request peak combined** (chunker buffer + manifest in-memory): chunker 2 MiB + manifest 3.5 MB = ~5.5 MB total during SplitBlob. CF Worker 128 MiB / 5.5 MB ≈ 23 max concurrent split ops per Worker isolate; semaphore 4 per tenant caps tenant concurrency.
  - **INV-MULTIPART-STREAMING-MEMORY** (registry §3.16): "verify_streaming O(1) per-chunk; build/verify_full bounded by 81920-chunk worst case ~3.5 MB heap". Updated by Lote 10.5-tris.
- 14.s05.005.10: Cost regression gate: per-op cost ≤ $0.000002 (verify) + $0.000005 (build).

## 15. Chaos Experiments (5)

1. Tampering detection (R2 manifest envelope byte flip): client GET; verify_full catches via Merkle root mismatch.
2. Determinism regression (chaos PR introduces non-determinism): property test catches.
3. Streaming verify bypass attempt (handler skips verify_streaming): integration test asserts call.
4. Sig domain separation drift (chaos PR changes info to `b"ac-sig"`): CI byte-equal test catches.
5. Cargo-fuzz crash detected: 1h fuzz nightly; immediate fix.

## 16. PRR

PRR mini Architect + **Crypto SME mandatory emphatic**.

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | Crate skeleton + Cargo.toml + module structure | 1.5 |
| ST-002 | ManifestBuilder impl (BLAKE3 + RFC 6962 domain sep) | 4 |
| ST-003 | ManifestVerifier impl (verify_structure + verify_full) | 3 |
| ST-004 | Streaming progressive verify | 3 |
| ST-005 | Sig module (HKDF info=`b"manifest-sig"` reuse pattern WI-S04-004) | 2 |
| ST-006 | Bounded parser + bounds | 2 |
| ST-007 | Property tests (5 × 10k) | 4 |
| ST-008 | Mann-Whitney 3-prong cripto-grade | 3 |
| ST-009 | Criterion benchmarks (3 benches) | 3 |
| ST-010 | Cargo-fuzz harness (3 targets) | 3 |
| ST-011 | Test vectors Annex A + B (50 each) | 4 |
| ST-012 | rustdoc + 4 examples + threat model README | 3 |
| ST-013 | spec/manifest_protocol.md doc | 3 |
| ST-014 | ADR-0041 redação | 2 |
| ST-015 | Crypto SME review iter (BLAKE3 + Merkle protocol + sig domain sep) | 4 |
| ST-016 | Architect + AppSec review iter | 3 |
| ST-017 | Métricas emit hooks (consumed by handler) | 1.5 |
| ST-018 | Public API stability review | 1 |

**Total Optimistic**: ~50h. **PERT** (O=44h, M=52h, P=80h): **~56h**.

## 18. Dependencies

- Hard: `blake3`, `subtle`, `hkdf` crates; WI-S04-004 SignatureVerifier trait reused; WI-S05-002 chunker output consumed.
- Soft: WI-S05-001 handler consumes builder/verifier; WI-S05-006 conformance.

## 19. Effort PERT: 56h. ## 20. Time-boxing: 64h hard limit.

## 21. Observability

Métricas: `corelink.manifest.verify_duration_us`, `build_duration_us`, `bounds_exceeded_total{kind}`, `streaming_verify_fail_fast_total`. Trace span `manifest.{verify_structure, verify_full, build, verify_streaming}`.

## 22. Cost Analysis

**Per-verify cost** (consumed by SpliceBlob handler):
- Worker CPU (BLAKE3 verify ~2ms @ 25 chunks): negligible.
- Per-verify: ~$0.000001.

**Per-build cost** (consumed by SplitBlob handler):
- Worker CPU (BLAKE3 build + sig sign ~10ms @ 25 chunks): ~$0.0000005.
- D1 manifest_chunks INSERT batch (delegated WI-S05-001): ~$0.000004.
- Per-build: ~$0.000005.

**TCO 12m projection**:
- 1M Split/dia × $0.000005 = $5/dia × 365 = **~$1.8k/yr**.
- 10M Splice/dia × $0.000001 = $10/dia × 365 = **~$3.6k/yr**.
- Total ~$5.4k/yr.

**Cost regression gate**: build ≤ $0.000010; verify ≤ $0.000002.

## 23. API Contract (compact)

Public traits documented §1; semver post v1.0; `#[non_exhaustive]`. Versioning: manifest schema v1; v2+ via ADR.

## 24. Post-mortem Hooks

- Manifest forge sustained > 5/h → CRITICAL post-mortem.
- Determinism regression → CRITICAL (revert).
- Streaming verify bypass detected → CRITICAL post-mortem.
- Sig domain separation drift → CRITICAL ADR review.
- Cargo-fuzz crash → SEV-1.

## 25. Rollback / Recovery

Crate version pin via Cargo.lock; revert via cargo update. RTO ≤ 10 min; RPO 0.

## 26. Security & Privacy (compact)

**STRIDE**: tenant_id binding em sig; BLAKE3 256-bit; bounded parser; per-tenant TDK isolation. **LINDDUN**: tenant_id pseudonymous; chunk_digest content-hash; manifest sig binds.

## 27. Knowledge Transfer

- Tech talk (1.5h): "Manifest Merkle Dual-Side Verify + Streaming Progressive + Domain Separation".
- Doc spec/manifest_protocol.md.
- Workshop com Crypto SME + Architect + handler authors.
- Onboarding test (5 questions): BLAKE3 rationale, RFC 6962 domain sep, dual-side verify, streaming fail-fast, sig domain separation.

## 28. Risk Register (12-row)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Manifest forge via cripto break (BLAKE3 collision) | L | L | CRITICAL | L | LOW | BLAKE3 256-bit collision-resistant; ADR-0041 |
| R-002 | Determinism regression (HashMap order) | M | L | CRITICAL | L | LOW | Property test 100k; CI gate |
| R-003 | Streaming verify bypass (handler skips call) | L | M | CRITICAL | L | LOW | Integration test asserts; clippy lint |
| R-004 | Sig domain separation drift | L | L | CRITICAL | L | LOW | CI byte-equal test asserts info |
| R-005 | Bounded parser bypass (decoder accepts oversize) | L | M | HIGH | L | LOW | Bounds at decode; cargo-fuzz validates |
| R-006 | Sig delegate trait misuse (wrong-key SignatureVerifier accepted) | L | M | HIGH | L | LOW | verify_structure independent (defense-in-depth) |
| R-007 | Mann-Whitney CI flake | M | H | LOW | M | LOW | Šidák 3-trial gate |
| R-008 | Cost regression > 10% | M | L | MEDIUM | L | LOW | §14.10 cost gate |
| R-009 | Cargo-fuzz crash detected | L | M | HIGH | L | LOW | 1h CI nightly; immediate fix |
| R-010 | Public API breaking change post-v1.0 sem ADR | L | L | HIGH | L | LOW | Semver discipline; #[non_exhaustive]; ADR-0041 |
| R-011 | Test vectors Annex regression | L | L | MEDIUM | L | LOW | CI integrates Annex A + B |
| R-012 | BLAKE3 crate vulnerability (zero-day) | L | M | HIGH | L | LOW | Cargo-audit weekly; rapid patch |

## 29. Review Checkpoints

D+0 design (Architect + Crypto SME); D+2 AppSec; D+5 code review; D+6 Crypto SME independent review (BLAKE3 + Merkle protocol + sig domain sep); D+8 adversarial; D+9 cargo-fuzz; D+10 PRR mini.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034_ |
| 4 | Security Lead | _TBD; **mandatory** — dual-side verify + tampering detection_ |
| 5-6 | Engineer × 2 | _TBD_ |
| 7 | QA | _TBD_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; SLSA L3 alignment_ |
| 10 | Privacy | _TBD; ActionResult metadata redaction_ |
| 11 | Architect | _TBD; **mandatory** — bounded parser + public API + ADR-0041_ |
| 12 | AppSec | _TBD; **mandatory** — Merkle protocol + bounds_ |
| 13 | Crypto SME | _**MANDATORY EMPHATIC** — BLAKE3 + RFC 6962 domain sep + sig domain separation `b"manifest-sig"` + Mann-Whitney cripto-grade + test vectors_ |

## 31. Change Log

1.0.0 / 2026-04-25 / Gustavo: Criação WI-S05-005 (Lote 10.5; SOTA pós-Lote 10.4bis lessons applied: result_hash = merkle_root direct; sig domain separation; cargo-fuzz 1h × 3 targets; test vectors Annex; Crypto SME MANDATORY EMPHATIC).

## 32. Anti-patterns evitados

- ❌ Skip verify_structure() em build; ❌ Variable hash function; ❌ Non-deterministic build; ❌ Server-only verify; ❌ HKDF info `b"ac-sig"` reuse; ❌ Skip streaming progressive verify; ❌ Cargo-fuzz < 1h; ❌ Test vectors absent; ❌ Public API breaking changes post-v1.0; ❌ Mann-Whitney p>0.05 sozinho; ❌ Buffered manifest (streaming Iterator); ❌ Skip RFC 6962 domain sep.

---

**Fim WI-S05-005.** Próximo: WI-S05-006 (Sweeper cron DO + RB-FM-060 dry-run + PRR ship gate + 160 GiB stitched flow).
