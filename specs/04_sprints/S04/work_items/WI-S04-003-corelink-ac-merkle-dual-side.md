---
id: "WI-S04-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
parent: "S-04"
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
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s04", "ac", "merkle", "codec", "verify", "dual-side", "high-risk", "crypto"]
---

# WI-S04-003 — Crate `corelink-ac` (Merkle Tree Codec + Dual-Side Verifier + INV-AC-OUTPUTS-VALID Enforcement + ActionResult Proto Adapter + Bounded Parser)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-04](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S04-003 |
| Título | Crate `corelink-ac` — ActionResult proto codec; Merkle tree builder + verifier dual-side (server pre-persist + client post-download); bounded parser (depth ≤ 32, fanout ≤ 4096, payload ≤ 1 MiB); INV-AC-OUTPUTS-VALID strict enforcement at builder layer; criterion benchmarks p99 ≤ 10ms verify @ 100 nodes |
| Sprint | S-04 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (Merkle invalid persisted = cache poisoning vector → cross-tenant catastrophic), FF-HR-005 (CTRL-AC-001 enforcement primary), FF-HR-009 (defense-in-depth — server + client verify) |

## 1. Intent

Implementar `crates/corelink-ac/` — biblioteca cripto-coordenada que:

1. **Codec ActionResult proto** ↔ envelope JSON (REAPI v2 → CoreLink wire format).
2. **Merkle tree builder** (server-side, em UpdateActionResult): constrói tree determinística; embeds digest raiz; assina (delegate WI-S04-004).
3. **Verifier dual-side** (CTRL-AC-001 PRIMARY enforcement):
   - **Server pre-persist** (UpdateActionResult): rejeita árvores inválidas ANTES de R2 PUT + D1 INSERT.
   - **Client post-download** (Bazel/Buck2 client side; fornecemos crate cliente OR documentation): valida árvore pós-GET; detecta envelope tampering.
4. **Bounded parser**: depth ≤ 32, fanout ≤ 4096, total nodes ≤ 100k, payload total ≤ 1 MiB (prevent DoS via giant tree).
5. **INV-AC-OUTPUTS-VALID strict enforcement em builder**: lookup output_files + output_directories digests in `blob_meta(tenant_id, digest)` WHERE deleted_at IS NULL; reject if any missing.

```rust
// File: crates/corelink-ac/src/lib.rs

#![forbid(unsafe_code)]

pub use crate::codec::{ActionResult, OutputFile, OutputDirectory, ExecutedActionMetadata};
pub use crate::merkle::{MerkleTree, MerkleNode, MerkleVerifier};
pub use crate::error::{AcCodecError, MerkleError, OutputsCheckError};

// Public API surface
pub mod codec;     // proto ↔ envelope
pub mod merkle;    // builder + verifier
pub mod outputs;   // INV-AC-OUTPUTS-VALID enforcement
pub mod bounds;    // depth/fanout/size constants

// Public types
pub struct AcEnvelope {
    pub version: u8,                          // = 1
    pub tenant_id: TenantId,
    pub action_digest: ActionDigest,
    pub result: ActionResult,                 // REAPI v2 ActionResult proto
    pub merkle_root: [u8; 32],                // BLAKE3-256 of tree
    pub created_at_ms: u64,
    pub sig: Vec<u8>,                         // delegated WI-S04-004 (HKDF-SHA256 sig)
    pub sig_key_id: u32,                      // HKDF tenant_key version
    pub sig_alg: SigAlg,                      // = HkdfSha256 in v1
}

pub trait ActionResultBuilder {
    /// Server-side: build envelope with Merkle root + outputs check.
    /// Caller must subsequently call `sig::sign` (WI-S04-004) before R2 put.
    fn build(
        &self,
        tenant_id: &TenantId,
        action_digest: &ActionDigest,
        result: ActionResult,
    ) -> Result<AcEnvelope, BuildError>;
}

pub trait ActionResultVerifier {
    /// Server-side pre-persist: verify Merkle + bounds (no sig check; that's WI-S04-004).
    fn verify_structure(&self, envelope: &AcEnvelope) -> Result<(), MerkleError>;

    /// Client-side post-download: verify Merkle + bounds + sig delegate.
    /// Returns Ok(()) only if structure + sig both valid.
    fn verify_full(
        &self,
        envelope: &AcEnvelope,
        sig_verifier: &dyn SignatureVerifier,
    ) -> Result<(), VerifyError>;
}

pub trait OutputsValidator {
    /// Strict server-side: lookup ALL output_file + output_directory digests in blob_meta.
    /// Reject if any tombstoned (deleted_at IS NOT NULL) OR missing.
    /// Used in UpdateActionResult; NOT in GetActionResult (warn-only there).
    async fn validate_outputs(
        &self,
        tenant_id: &TenantId,
        result: &ActionResult,
        blob_meta: &dyn BlobMetaReader,
    ) -> Result<(), OutputsCheckError>;
}

#[derive(thiserror::Error, Debug)]
pub enum MerkleError {
    #[error("tree depth {found} exceeds bound {bound}")]
    DepthExceeded { found: usize, bound: usize },          // bound=32

    #[error("tree fanout {found} exceeds bound {bound}")]
    FanoutExceeded { found: usize, bound: usize },         // bound=4096

    #[error("tree total nodes {found} exceeds bound {bound}")]
    NodeCountExceeded { found: usize, bound: usize },      // bound=100_000

    #[error("envelope payload {found_bytes} exceeds bound {bound}")]
    PayloadExceeded { found_bytes: usize, bound: usize },  // bound=1_048_576

    #[error("merkle root mismatch: computed={computed:x?}, claimed={claimed:x?}")]
    RootMismatch { computed: [u8; 32], claimed: [u8; 32] },

    #[error("cycle detected in output_directories at digest {0:x?}")]
    CycleDetected([u8; 32]),                               // Lote 10.4bis P0 fix: variant added; was referenced em §2.5 + §8 Gherkin mas missing from enum

    #[error("digest format invalid in node: {0}")]
    InvalidDigestFormat(String),

    #[error("envelope version unsupported: {0}")]
    VersionUnsupported(u8),

    #[error("decode error: {0}")]
    DecodeError(String),
}

#[derive(thiserror::Error, Debug)]
pub enum OutputsCheckError {
    #[error("output blob {digest:?} missing or tombstoned")]
    BlobMissing { digest: String },

    #[error("output directory contains {count} blobs, exceeds bound {bound}")]
    DirectoryFanoutExceeded { count: usize, bound: usize },  // bound=4096

    #[error("blob_meta backend error: {0}")]
    BackendError(String),
}
```

**Constraint cripto-driven**:

1. **Merkle hash function = BLAKE3-256** (matches CAS S-01; perf > SHA-256 4× via SIMD; collision-resistant 2^128).
2. **Determinismo**: tree construction is byte-stable for same `(tenant_id, action_digest, result)` — no timestamps, no random, no reordering of inputs.
3. **Envelope schema versioned**: `version: u8` field; v1 mandatory; future v2+ via migration ADR.
4. **Signature delegated to WI-S04-004**: this WI builds + verifies STRUCTURE; HKDF sig is separate concern; trait `SignatureVerifier` consumed.
5. **Outputs validation REQUIRES `&dyn BlobMetaReader`** trait — concrete impl supplied by handler context (Worker D1 binding); test impl uses fixture map.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Merkle dual-side verify é o **único defense efetivo contra cache poisoning** em remote AC. Sem verify cliente-side (post-download), atacante que comprometa R2 envelope (insider, supply chain attack on R2 binding, etc.) injeta ActionResult com output_files apontando para blobs maliciosos; cliente Bazel busca os bytes; build "succeeds" com binário comprometido. Server-side verify pré-persist é o single line of defense em REAPI v2 spec; **CoreLink innovation = client-side verify também**, expondo cripto-suficient envelope schema para validation reproduzível.

HIGH_RISK justificado em N dimensões:

1. **Merkle bypass via missing verifier call**: handler chama `build()` em UpdateActionResult mas esquece `verify_structure()`; árvore inválida (e.g., output_file digest format errado, depth excessive) escreve em R2; subsequent GET serve corrompido. Mitigação: `build()` retorna `Result<AcEnvelope, BuildError>`; build() **chama internamente** verify_structure() OR retorna erro; impossível bypass via API misuse.

2. **Tampering em R2 envelope (rest)** — **partial compromise scenarios** (Lote 10.4bis P0 fix: prior text overstated dual-side defense under FULL chave compromise):
   - **Storage-tier insider** (R2 write access mas no HKDF key): modifies result_hash em envelope; HKDF sig fails (mismatch); detected by sig::verify alone. Merkle dual-side é redundância adicional aqui — útil se o storage path é compromised mid-pipeline (sig was generated before tamper, now stored bytes don't match canonical_bytes). Mitigação primary: HKDF sig.
   - **Wire-tampering between handler and R2** (during PUT or GET): same as storage-tier; sig catches.
   - **Compromised writer-side mid-pipeline** (sig was generated for envelope_v1; envelope_v2 stored with tampered fields; sig still valid for v1 but v2's content differs): Merkle binding catches via canonical_bytes mismatch (envelope.merkle_root signed alongside).
   - **Full HKDF key compromise** (atacante has TDK access): atacante modifies result, recomputes Merkle root, sets envelope.merkle_root = recomputed root, **re-signs envelope with compromised key**. Both verify_structure (matching root) and verify_sig (valid sig) return OK. **Defense-in-depth FAILS in full chave compromise**; mitigated externally via TDK rotation + audit chain detection (S-09) + tenant_id binding limits blast radius.

   Honest formulation: dual-side verify catches storage-tier tampering even when writer-side has been compromised mid-pipeline OR sig delegate is misused (e.g., wrong-key SignatureVerifier accepted). Both fail together in full chave compromise. Threat model in `merkle_protocol.md` enumerates which adversary capabilities each layer defends against.

3. **DoS via giant tree** (depth=10000 OR fanout=1M): parser allocates 1 MiB per node × 1M nodes = 1 TB; Worker memory exhaustion (CF cap 128 MiB) → OOM crash. Mitigação: bounded parser (depth ≤ 32 = 4M leaves max; fanout ≤ 4096 per node; total nodes ≤ 100k; payload ≤ 1 MiB); reject early at decode time before alloc.

4. **Determinism violation**: two clients build same (tenant, action_digest, result) but different Merkle root (e.g., HashMap iteration order in protobuf). REAPI v2 idempotency violated; UPDATE same digest twice with different root → COR_AC_RESULT_HASH_MISMATCH false-positive. **Lote 10.4bis P0 fix per ADR-0037 ratificação**: `prost` does NOT guarantee deterministic encoding for messages with map/repeated fields; "protobuf canonical serialization (deterministic)" claim was wrong. **Decision (ADR-0037 Lote 10.4bis)**: `result_hash = merkle_root` direct (eliminates protobuf-bytes dependency); Merkle tree is built from lex-sorted `(output_files + output_directories)` digests; the tree itself is the canonical artifact, not the proto encoding. Builder iterates outputs in **lexicographic order by digest**; tree shape determined by sorted leaf digests + balanced binary structure (Bao-style). Test 1000 random ActionResults each built 100 times → all 100 builds produce identical merkle_root bytes (= identical result_hash bytes).

5. **Output_directories nested Merkle abuse**: REAPI Directory proto contains nested Directory references; cycle detection mandatory (else infinite recursion). Mitigação: bounded parser tracks visited node digests; cycle → `MerkleError::CycleDetected` (added to error enum); test 100 crafted cyclic protos rejected.

6. **INV-AC-OUTPUTS-VALID race**: build calls `outputs::validate` at T+0; S-06 GC tombstones blob_X at T+1; envelope already constructed references blob_X. **Lote 10.4bis P0 fix**: SQLite/D1 has **NO row-level locking primitives** (`SELECT ... FOR SHARE` é Postgres-only); transactions são serializable-by-engine but cannot re-claim a row. Honest formulation: **INV-AC-OUTPUTS-VALID is point-in-time best-effort**. We accept TOCTOU between handler outputs check and S-06 GC tombstone window; reconcile diário (S-06 forward) é the **only** mechanism for drift detection + correction. Race window is bounded by GC mark-phase + S-06 outbox emission window. INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY tier explicitly documented (registry §3.15 + chaos test); orphan_rate metric monitors.

7. **Codec adapter drift (REAPI v2 spec evolution)**: Bazel 8 adds new ActionResult fields; codec drops or mishandles. Mitigação: `prost` codegen from vendored .proto (WI-S04-001); upgrade via migration ADR; conformance suite catches.

8. **Client-side verify availability**: para client verify, cliente needs (a) crate corelink-ac OR (b) language-native impl using documented schema. Decisão: shipping `corelink-ac` é Rust crate; client SDKs (S-15) embed; for non-Rust clients (Java Bazel, C++ Buck2 native), document schema + provide reference vectors; clients implement validator. **Coverage incomplete em S-04 GA (Rust client only); S-15 expands**.

**Atacante adversarial scenarios**:

- **Cache poisoning via R2 envelope tampering**: insider (CF employee, contractor) modifies envelope to point to blob D' (attacker's malicious binary). Mitigação primary: HKDF sig (WI-S04-004) detects (insider sem HKDF key = sig fails). **Lote 10.4bis P0 fix**: prior text claimed Merkle catches "even if sig forged" — that is true ONLY for **partial compromise** (insider has R2 access but not HKDF key; sig forge requires both). Under **full HKDF key compromise**, attacker recomputes Merkle root + re-signs; both layers fail. Realistic dual-side defense: 2 cripto layers requiring partial compromise (one path) — full compromise mitigated externally via TDK rotation + S-09 audit chain anomaly detection.

- **Merkle root collision attempt**: attacker crafts ActionResult with same Merkle root as legitimate but different output_files. BLAKE3-256 collision-resistant 2^128; attempt computationally infeasible (10^29 years brute-force).

- **DoS via crafted oversized tree**: 100 MB ActionResult body via output_directories deep-nest. Mitigação: bounds parser rejects at decode time (depth/fanout/payload caps).

- **Cycle bomb**: Directory_A references Directory_B references Directory_A. Stack overflow OR infinite recursion. Mitigação: visited set; bound depth + cycle detection.

- **Non-deterministic build poison**: legitimate non-deterministic build (compiler with hidden timestamp dep) produces different result_hash on each build; REAPI client retries; storms UPDATE; CoreLink populates many entries with same action_digest. Mitigação: result_hash mismatch on idempotent UPDATE → 409 (WI-S04-001); customer investigates determinism; metric `corelink.ac.update.result_mismatch_total` alert.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: Merkle invalid persisted = cache poisoning = cross-tenant if poisoned blob is shared (CAS dedup + AC reference). FM-303-adjacent severity.
- **FF-HR-005**: CTRL-AC-001 IS Merkle verify; this WI is the implementation. Bypass = security control disabled.
- **FF-HR-009**: defense-in-depth dual-side; server pre-persist + client post-download = 2 cripto checks; either one catches integrity violation.
- **Reversibility**: cache poisoning detected only when customer build fails AT LATER TIME (or ext audit); INV-AC-OUTPUTS-VALID daily reconcile partial coverage.
- **Customer impact**: build with poisoned binary = supply chain incident; could cascade to customer's customers (CoreLink customer's customer).

13 sign-offs incl. Architect, AppSec, Crypto SME (BLAKE3 review + Merkle protocol), Security Lead.

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev**:
- After UpdateActionResult, downstream GET returns ActionResult; if customer SDK has client-side verify (Rust SDK from S-15), validates Merkle root before consuming output digests.
- Build fails fast (≤ 10ms verify) if tampering detected; no compromised binary fetched.
- Customer-visible: `corelink_remote_cache_integrity_check_failed: <reason>` log line.

**Persona 2 — Security auditor reviewing AC integrity**:
- Reviews `corelink-ac::verify` test vectors (Annex A reference vectors); confirms reproducibility.
- Validates dual-side coverage: server pre-persist test in CI; client post-download test in client SDK CI.
- Reviews bounded parser fuzz harness: 1h cargo-fuzz run = 0 crashes, 0 OOM.

**Persona 3 — Compliance reviewer (SLSA Level 3 — supply chain integrity)**:
- AC envelope acts as build provenance attestation (lightweight); customer can re-verify ActionResult ↔ output blobs binding.
- ADR-0035 (handler) + ADR-0037 (Merkle protocol) document integrity claims.

**SLA addendum**:
- Merkle verify p99 ≤ 10ms for trees ≤ 100 nodes (criterion benchmark gate).
- Builder p99 ≤ 15ms (build + verify_structure inline).
- Outputs validation (D1 lookup) p99 ≤ 50ms for ≤ 100 outputs (batch SELECT).
- Bounded parser fail-fast: invalid input rejected ≤ 1ms (no full parse).
- Test vectors: Annex A in `corelink-ac/spec/test_vectors.md` for reproducibility.

## 4. Capability Mapping

- **CAP-AC-003** (Merkle verification dual-side) — IMPLEMENTA primary.
- **CAP-AC-001** + **CAP-AC-002** — IMPLEMENTA partial (Merkle build/verify provided to handler WI-S04-001).
- Trace: `security_model.md CTRL-AC-001 (Merkle verify primary)` + `cas_profile.md §4 (Merkle decomposition reference)` + `remote_cache_product_profile.md §6 (REAPI conformance)`.

## 5. Tipo

Cripto library; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-ac`** structure:
   - `crates/corelink-ac/Cargo.toml`.
   - `src/lib.rs` (public API surface).
   - `src/codec/`: ActionResult proto ↔ envelope JSON.
   - `src/merkle/`: builder + verifier + bounds.
   - `src/outputs/`: INV-AC-OUTPUTS-VALID enforcement.
   - `src/bounds.rs`: constants (depth, fanout, node_count, payload).
   - `src/error.rs`: error enums.

2. **ActionResult proto codec**:
   - prost-generated types from REAPI v2 .proto (vendored WI-S04-001).
   - JSON envelope wire format: `serde_json` + canonical key ordering (lex sorted).
   - Roundtrip determinism: encode → decode → re-encode = identical bytes (test 1000 random).

3. **Merkle tree builder** (`src/merkle/builder.rs`):
   - Hash function: BLAKE3-256 (delegate `blake3` crate).
   - Tree shape: balanced binary tree over output_files + output_directories digests, sorted lex.
   - Inner node: `node_hash = blake3(left || right)`.
   - Leaf: `leaf_hash = blake3(b"\x00" || digest_bytes)` (domain separation per RFC 6962-style).
   - Inner: `inner_hash = blake3(b"\x01" || left || right)` (domain separation prevents 2nd-preimage tree-shape attack).
   - Root: top of recursion.
   - Determinism: lex sort outputs; binary tree balanced; no timestamps. **Lote 10.4bis P0 fix per ADR-0037**: `result_hash = merkle_root` direct (eliminates protobuf encoding dependency; `prost` does not guarantee deterministic encoding for map/repeated fields).

4. **Merkle verifier** (`src/merkle/verifier.rs`):
   - `verify_structure(envelope)`: re-build tree from envelope.result; compare computed root to envelope.merkle_root; mismatch → `MerkleError::RootMismatch`.
   - Bounds check: depth, fanout, node_count, payload — fail-fast at decode time.
   - `verify_full(envelope, sig_verifier)`: structure + sig (delegate WI-S04-004 trait).

5. **Bounded parser** (`src/bounds.rs`):
   - Constants:
     ```rust
     pub const MAX_TREE_DEPTH: usize = 32;             // 2^32 leaves max; well above 4M typical
     pub const MAX_FANOUT: usize = 4096;               // per node
     pub const MAX_NODE_COUNT: usize = 100_000;
     pub const MAX_PAYLOAD_BYTES: usize = 1_048_576;   // 1 MiB
     pub const MAX_OUTPUT_FILES: usize = 4096;
     pub const MAX_OUTPUT_DIRECTORIES: usize = 4096;
     ```
   - Decoder enforces at parse time (not post-parse); reject with `MerkleError::PayloadExceeded` etc.

6. **Outputs validator** (`src/outputs/`):
   - `BlobMetaReader` trait:
     ```rust
     #[async_trait::async_trait]
     pub trait BlobMetaReader: Send + Sync {
         async fn batch_check_alive(
             &self,
             tenant_id: &TenantId,
             digests: &[Digest],
         ) -> Result<Vec<bool>, OutputsCheckError>;  // true = alive (deleted_at NULL)
     }
     ```
   - Concrete impl: `D1BlobMetaReader` using sqlx (handler context).
   - Test impl: `MockBlobMetaReader` with HashMap fixture.
   - `validate_outputs(tenant_id, result, reader)`:
     - Extract digests from result.output_files + recursive output_directories.
     - Cycle detection (visited set).
     - Single batch SELECT (`WHERE tenant_id = ? AND digest IN (...) AND deleted_at IS NULL`).
     - Reject if any tombstoned/missing → `OutputsCheckError::BlobMissing { digest }`.

7. **Envelope JSON serialization**:
   - `serde_json` with sorted keys; `serde_jcs` (RFC 8785 JCS) NOT used here (envelope is internal binding, not inter-service event); `serde_json` with sorted is sufficient.
   - Forward-compat via `version` field; v1 fixed.
   - Test vectors Annex A in spec doc for reproducibility.

8. **Cycle detection** (output_directories):
   - REAPI Directory proto contains nested Directory references (Directory.directories field).
   - Visited set: `HashSet<[u8; 32]>` of digest bytes; recursion bounded.
   - Cycle → `MerkleError::CycleDetected` (added to error enum).

9. **Public API stability**:
   - `#[non_exhaustive]` on `AcEnvelope` struct (forward-compat fields).
   - Public traits `ActionResultBuilder`, `ActionResultVerifier`, `OutputsValidator`, `BlobMetaReader`.
   - Semver: v0.x during S-04; v1.0 at S-04 SEALED.

10. **Property tests** (10k iter PR; 100k nightly):
    - `prop_merkle_determinism`: 1000 random ActionResults; build 100× each; all 100 produce identical envelope bytes.
    - `prop_merkle_round_trip`: encode → decode → re-encode = identical (1000 random).
    - `prop_merkle_tampering_detected`: random ActionResults; flip 1 byte in result; verify_structure detects 100%.
    - `prop_bounds_enforcement`: 1000 random sizes (depth, fanout, payload); reject at decode if exceeds bounds.
    - `prop_outputs_missing_rejected`: 100 random ActionResults; 1 output tombstoned; reject 100%.
    - `prop_cycle_detection`: 100 crafted Directory cycle protos; reject 100%.

11. **Mann-Whitney timing test** (3-prong, extending S-03 methodology):
    - Goal: cliente cannot distinguish "valid envelope" vs "tampered envelope" via verify timing.
    - 10k samples per arm; valid envelopes vs envelopes with 1-byte flip in result_hash field.
    - Power 1−β ≥ 0.80 com Cohen's d = 0.2.
    - Šidák 3-trial; combined α ≈ 0.000125.
    - |Δmedian| ≤ 1ms (verify is fast; target 1ms not 5ms).
    - Bootstrap 95% CI sobre |Δmedian| cruzando 0.

12. **Criterion benchmarks** (`benches/`):
    - `bench_merkle_verify_100_nodes`: target p99 ≤ 10ms.
    - `bench_merkle_build_100_nodes`: target p99 ≤ 15ms.
    - `bench_outputs_validate_100`: target p99 ≤ 50ms (D1 batch SELECT).
    - Cost regression gate: PR > 10% regression bloqueia.

13. **Cargo-fuzz harness**:
    - `fuzz/fuzz_targets/decode_envelope.rs`: 1h CI nightly; arbitrary bytes input; assert never panics + always returns Result.
    - Bound enforcement: assert no OOM regardless of input size.

14. **Integration tests**:
    - Test vectors Annex A: 50 known-good envelopes; assert verify_structure ok.
    - Test vectors Annex B: 50 known-bad envelopes (each invariant violated); assert verify_structure rejects with correct error variant.

15. **Documentation**:
    - `crates/corelink-ac/README.md`: API overview + threat model.
    - `corelink-ac/spec/merkle_protocol.md`: tree shape, hash function, domain separation, test vectors.
    - rustdoc 100% public API + 4 examples (build, verify_structure, verify_full, batch outputs check).

### 6.2 Out-of-scope (deferred)

- **Signature impl** (HKDF tenant_key signing/verify): WI-S04-004.
- **D1 BlobMetaReader concrete impl**: handler context (WI-S04-001 supplies via dyn Trait).
- **Client SDK Java/C++ impl** (for Bazel/Buck2 non-Rust clients): S-15 forward.
- **Online schema migration v1 → v2**: post-GA via ADR.
- **Property test 1M iter** (overkill for S-04): forward S-09 chaos sprint.
- **Hardware acceleration** (BLAKE3 SIMD detection, AVX-512 dispatch): blake3 crate already does; no work here.

## 7. Anti-Scope

- ❌ Skip verify_structure() em build() (always invoked internally).
- ❌ Variable-length tree depth (bounded 32).
- ❌ Custom hash function (BLAKE3-256 fixed).
- ❌ Non-deterministic build (lex sort + canonical proto enforced).
- ❌ Skip cycle detection (visited set mandatory).
- ❌ JSON canonical via `serde_json` only (without sorted keys); use `serde_json` with sorted wrapper OR document schema rigid.
- ❌ Allow envelope.version 0 (start at 1; future-compat).
- ❌ Output digest format unvalidated (must be hex-32-bytes BLAKE3 OR SHA-256 per REAPI multi-hash; reject malformed).
- ❌ Synchronous outputs check via single SELECT-each (batch SELECT mandatory).
- ❌ Public API breaking changes post v1.0 without ADR.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: corelink-ac Merkle codec + verifier dual-side

  Background:
    Given crate corelink-ac built
    Given test vectors Annex A (50 valid envelopes) + Annex B (50 invalid)
    Given mock BlobMetaReader with 1000 alive blobs

  Scenario: Build envelope from valid ActionResult
    Given ActionResult R with 10 output_files all alive in blob_meta
    When ActionResultBuilder::build(tenant_id=A, action_digest=D, R)
    Then envelope created with computed merkle_root
    And verify_structure() OK
    And outputs::validate_outputs() OK
    And p99 build latency ≤ 15ms

  Scenario: Verify_structure detects tampering
    Given valid envelope E
    When 1 byte flipped in E.result.output_files[0].digest
    And verify_structure(E)
    Then error MerkleError::RootMismatch
    And p99 verify latency ≤ 10ms

  Scenario: Determinism — 100 build cycles produce identical bytes
    Given 1000 random ActionResults
    When each built 100 times
    Then 100 builds per result produce IDENTICAL envelope bytes
    And property test prop_merkle_determinism green

  Scenario: Round-trip codec stable
    Given ActionResult R
    When encode → decode → re-encode
    Then result is byte-identical to original encode
    And property test prop_merkle_round_trip green

  Scenario: Bounded parser rejects oversized tree (depth)
    Given crafted ActionResult with output_directories nested 50 levels deep
    When ActionResultVerifier::verify_structure
    Then error MerkleError::DepthExceeded { found: 50, bound: 32 }
    And NO recursion past depth 32 (memory bounded)
    And p99 reject latency ≤ 1ms (fail-fast at decode)

  Scenario: Bounded parser rejects oversized fanout
    Given crafted ActionResult with output_directories[0].files containing 5000 entries
    When verify_structure
    Then error MerkleError::FanoutExceeded { found: 5000, bound: 4096 }

  Scenario: Bounded parser rejects oversized payload
    Given crafted ActionResult with stderr_raw 2 MiB blob
    When verify_structure
    Then error MerkleError::PayloadExceeded { found_bytes: ~2 MiB, bound: 1 MiB }

  Scenario: Cycle detection — Directory_A → Directory_B → Directory_A
    Given crafted ActionResult with cyclic output_directories
    When verify_structure
    Then error MerkleError::CycleDetected
    And visited set bounded; no infinite recursion

  Scenario: Outputs validation — all alive
    Given ActionResult with 4 output_files digests all alive
    When OutputsValidator::validate_outputs(tenant_id, result, reader)
    Then OK

  Scenario: Outputs validation — 1 tombstoned (rejection)
    Given ActionResult with 4 output_files; 1 has deleted_at NOT NULL in mock reader
    When validate_outputs
    Then error OutputsCheckError::BlobMissing { digest }
    And property test prop_outputs_missing_rejected green

  Scenario: Outputs validation — batch SELECT (efficient)
    Given ActionResult with 100 output_files
    When validate_outputs
    Then ONE D1 batch SELECT with IN clause (not 100 individual)
    And p99 latency ≤ 50ms

  Scenario: verify_full (client-side) — sig + structure
    Given valid envelope E with valid HKDF sig (from WI-S04-004 fixture)
    Given SignatureVerifier impl from WI-S04-004
    When verify_full(E, sig_verifier)
    Then OK (structure + sig both valid)

  Scenario: verify_full detects sig forge (chave compromise simulated)
    Given envelope E with structure valid but sig forged (signed with wrong key)
    When verify_full(E, sig_verifier)
    Then error VerifyError::SigInvalid
    And client-side defense-in-depth catches forge

  Scenario: verify_full detects structure tamper even if sig valid (insider scenario)
    Given envelope E with byte flip in result; sig invalidated
    AND attacker also forges sig (chave compromise scenario)
    When verify_full(E, sig_verifier_with_wrong_key_accepted)
    Then verify_structure path catches MerkleError::RootMismatch
    AND defense-in-depth: even if sig forged, Merkle independent check catches
    (provided attacker doesn't recompute Merkle root binding)

  Scenario: Mann-Whitney verify timing — valid vs tampered indistinguishable
    Given 10k valid envelopes
    Given 10k tampered envelopes (byte flip)
    When verify_structure latency measured per arm
    And Mann-Whitney U + power analysis applied
    Then p > 0.05 com Šidák 3-trial
    And |Δmedian| ≤ 1ms com 95% CI cruzando 0

  Scenario: Criterion benchmark verify ≤ 10ms p99 @ 100 nodes
    Given 100-node Merkle tree (typical Bazel build target)
    When criterion benchmark runs 1000 iter
    Then p99 ≤ 10ms
    And cost regression gate green (no > 10% slowdown vs baseline)

  Scenario: Cargo-fuzz harness 1h CI — no panics
    Given fuzz target decode_envelope
    When 1h cargo-fuzz run with arbitrary bytes
    Then 0 panics, 0 OOM, all return Result<_, _>
    And bounds enforced regardless of input
```

## 9. Design Decisions

### 9.1 Why BLAKE3-256 (not SHA-256)

- BLAKE3 4× faster via SIMD (AVX2/AVX-512); critical for verify p99 ≤ 10ms.
- Same security level (256-bit collision-resistant).
- CAS S-01 already uses BLAKE3; consistency across cripto stack.
- REAPI v2 multi-hash supports BLAKE3 (Bazel 7+ adopted).

### 9.2 Why domain separation prefix `\x00` leaf vs `\x01` inner

- RFC 6962 (Certificate Transparency) Merkle tree convention.
- Prevents 2nd-preimage attacks where attacker constructs leaf that hashes to inner node's hash (tree-shape attack).
- Cost: 1 byte per hash; negligible.

### 9.3 Why bounded parser at decode time (not post-parse validate)

- Allocation bomb scenario: 100M element array allocates Vec<u8> first; OOM. Post-validate too late.
- Decoder reads incrementally; reject when limit exceeded (e.g., array length > 4096); no full alloc.
- prost has recursion limit feature (default 100); set to 32 for output_directories.

### 9.4 Why dual-side verify (server + client)

- Server-only verify: insider with R2 access can tamper post-persist; client trusts envelope blindly.
- Client-only verify: client can lie about validation; server has no defense against client-side bypass.
- Dual: defense-in-depth; even if one layer compromised (chave leak server-side, malicious client SDK), other catches.

### 9.5 Why determinism mandatory (lex sort + canonical proto)

- REAPI v2 idempotency contract: same Action proto → same action_digest → same ActionResult.
- Non-determinism violates contract; UPDATE retry lots → 409 result_hash_mismatch storm; customer pain.
- Lex sort + canonical proto = byte-stable; testable via 1000× build identical.

### 9.6 Why outputs validate strict in builder (not warn-only)

- Server pre-persist is the LAST chance to reject before storage; warn-only here = INV-AC-OUTPUTS-VALID violation persisted.
- GET path is warn-only (S-04 WI-S04-001 §9.4 rationale: customer build resilience).
- Builder strict + GET warn-only = balanced policy; reconcile diário catches drift.

### 9.7 Why batch SELECT outputs (not per-output)

- 100 output_files × 1 SELECT each = 100 round-trips × 5ms = 500ms; over p99 budget.
- Single batch SELECT WHERE digest IN (...) = 1 round-trip × 50ms; 10× faster.
- D1 batch limit ~100 IN-clause args; handler caps output_files at 4096 but batch in chunks of 100.

### 9.8 Why test vectors Annex A + B

- Reproducibility: external auditors validate impl matches spec.
- Regression detection: refactor accidentally changes Merkle root computation → annex A test vectors fail.
- Compliance: SLSA Level 3 supply chain provenance; documented expected behavior.

### 9.9 Why cargo-fuzz 1h (not 10s)

- Decoder is attack surface; arbitrary bytes input.
- 10s = ~100k iters; 1h = ~30M iters; bounded inputs more thoroughly explored.
- CI nightly budget: 1h × $0.05/h = $0.05/nightly = negligible.

### 9.10 ADR potencial?

Sim — **ADR-0037**: "AC Merkle protocol — BLAKE3 + RFC 6962-style domain separation + bounded parser + deterministic build + dual-side verify + JCS envelope". Whitelist em validate_references.py.

## 10. Completeness Criteria SOTA

- [ ] **10.s04.003.1** Property tests 10k iter (PR) + 100k nightly → 0 panics, 0 false-accepts (EVT-002):
  - prop_merkle_determinism, prop_merkle_round_trip, prop_merkle_tampering_detected, prop_bounds_enforcement, prop_outputs_missing_rejected, prop_cycle_detection.
- [ ] **10.s04.003.2** Mann-Whitney U + power analysis 3-prong em verify timing (valid vs tampered) (EVT-002):
  - N ≥ 10000 samples per arm.
  - Power 1−β ≥ 0.80 com Cohen's d = 0.2 via statrs.
  - Šidák 3-trial gate (combined α ≈ 0.000125).
  - |Δmedian| ≤ 1ms com 95% CI cruzando 0.
- [ ] **10.s04.003.3** Criterion benchmarks: verify p99 ≤ 10ms @ 100 nodes; build p99 ≤ 15ms; outputs validate p99 ≤ 50ms @ 100 outputs (EVT-021).
- [ ] **10.s04.003.4** Cargo-fuzz harness 1h CI nightly → 0 panics, 0 OOM (EVT-002).
- [ ] **10.s04.003.5** Test vectors Annex A (50 valid) + Annex B (50 invalid) — verify_structure ok/rejected per spec (EVT-002).
- [ ] **10.s04.003.6** Determinism — 1000 ActionResults × 100 builds = 100% byte-identical (EVT-002).
- [ ] **10.s04.003.7** Round-trip codec stable: 1000 random encode→decode→re-encode = identical (EVT-002).
- [ ] **10.s04.003.8** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s04.003.9** Cost regression gate: per-op cost ≤ $0.000002 (verify) + $0.000005 (build incl outputs check) (Lote 9.4 §14.10).
- [ ] **10.s04.003.10** rustdoc 100% public API + 4 examples + threat model README + spec doc `merkle_protocol.md`.
- [ ] **10.s04.003.11** Public API stability: `#[non_exhaustive]` on AcEnvelope; semver discipline.
- [ ] **10.s04.003.12** Bound enforcement: depth ≤ 32, fanout ≤ 4096, node_count ≤ 100k, payload ≤ 1 MiB validated by chaos test.

## 11. DoD

- [ ] Crate `corelink-ac` compila + integration tests green.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em CI; 100k nightly green.
- [ ] Mann-Whitney 3-prong test green (verify timing valid vs tampered).
- [ ] Criterion benchmarks green (verify ≤ 10ms p99 @ 100 nodes).
- [ ] Cargo-fuzz 1h CI nightly green.
- [ ] Test vectors Annex A + B published + integrated into CI.
- [ ] Determinism + round-trip property tests green.
- [ ] rustdoc 100% public API + 4 examples + threat model README.
- [ ] `corelink-ac/spec/merkle_protocol.md` published.
- [ ] ADR-0037 published.
- [ ] Architect + AppSec + Crypto SME + Security Lead reviews.
- [ ] PRR Architect + Crypto SME mini sign-off.
- [ ] Cost regression gate green.
- [ ] Cargo-audit + cargo-deny clean.

## 12. Invariants Validated

- **INV-AC-MERKLE-VALID** (CRITICAL, NEW — promovida em registry §3.15 Lote 10.4bis): envelope.merkle_root binds tree structure; verify_structure rejects tampering 100%.
- **INV-AC-MERKLE-DETERMINISTIC** (CRITICAL, NEW): same input → same Merkle root byte-identical; lex sort + canonical proto enforced.
- **INV-AC-OUTPUTS-VALID** (HIGH, registry §3.3): builder strict-fail; reject if any output tombstoned; reconcile diário cross-check.
- **INV-AC-BOUNDED-PARSER** (HIGH, NEW): depth ≤ 32, fanout ≤ 4096, node_count ≤ 100k, payload ≤ 1 MiB enforced at decode time.
- **INV-AC-CYCLE-FREE** (HIGH, NEW): visited set rejects cycles; bounded recursion.
- **INV-AC-DUAL-SIDE-VERIFY** (HIGH, NEW): server + client both invokeable; verify_structure independent of sig (defense-in-depth).

TLA+ alignment: cas_integrity.tla extension for AC variant; INV-AC-MERKLE-VALID derives from CAS integrity invariant (extension TBD em S-09 forward TLA+ work).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate `corelink-ac` | `crates/corelink-ac/` | Rust |
| Codec module | `crates/corelink-ac/src/codec/` | Rust |
| Merkle module | `crates/corelink-ac/src/merkle/` | Rust |
| Outputs module | `crates/corelink-ac/src/outputs/` | Rust |
| Bounds constants | `crates/corelink-ac/src/bounds.rs` | Rust |
| Error enums | `crates/corelink-ac/src/error.rs` | Rust |
| Property tests | `crates/corelink-ac/tests/prop_merkle.rs` | Rust |
| Mann-Whitney timing tests | `crates/corelink-ac/tests/timing_verify.rs` | Rust |
| Criterion benchmarks | `crates/corelink-ac/benches/merkle_bench.rs` | Rust |
| Cargo-fuzz harness | `crates/corelink-ac/fuzz/fuzz_targets/decode_envelope.rs` | Rust |
| Test vectors Annex A + B | `crates/corelink-ac/spec/test_vectors.md` + `tests/fixtures/annex_*/` | Markdown + JSON |
| Spec doc | `crates/corelink-ac/spec/merkle_protocol.md` | Markdown |
| README | `crates/corelink-ac/README.md` | Markdown |
| Examples | `crates/corelink-ac/examples/` (4 examples) | Rust |
| ADR-0037 | `specs/02_governance/decisions/ADR-0037-ac-merkle-protocol.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s04.003.1** `#![forbid(unsafe_code)]`; zero `unwrap` em src/.
- **14.s04.003.2** rustdoc 100% public API + 4 examples + threat model README + merkle_protocol.md spec.
- **14.s04.003.3** Test coverage ≥ 95% (cripto boundary).
- **14.s04.003.4** Latência: verify p99 ≤ 10ms @ 100 nodes; build p99 ≤ 15ms; outputs validate p99 ≤ 50ms.
- **14.s04.003.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings`; cargo-fuzz target 1h CI nightly.
- **14.s04.003.6** Métricas: `corelink.ac.merkle.verify_duration_us`, `corelink.ac.merkle.build_duration_us`, `corelink.ac.outputs.validate_duration_ms`, `corelink.ac.bounds.exceeded_total{kind}` (alert), `corelink.ac.cycle_detected_total` (alert).
- **14.s04.003.7** Public API stability: `#[non_exhaustive]`; semver post v1.0.
- **14.s04.003.8** REAPI v2 spec compliance: vendored proto pinned commit (WI-S04-001 shared); test vectors aligned.
- **14.s04.003.9** Memory bounded: per-verify ≤ 32 KiB stack + ≤ 1 MiB heap; bound parser enforced.
- **14.s04.003.10** Cost regression gate: per-op cost ≤ $0.000002 verify + $0.000005 build.

## 15. Chaos Experiments

1. **Tampering detection (R2 envelope byte flip)**: simulate insider modifies envelope post-persist; client GET; verify_full detects via Merkle root mismatch. Hypothesis: 100% detection ≤ 10ms; alert fired.

2. **Determinism regression**: chaos PR introduces non-determinism (HashMap iteration in builder); property test prop_merkle_determinism catches; CI red.

3. **Cycle bomb (Directory_A → Directory_B → Directory_A)**: synthetic crafted protobuf; verify_structure rejects via cycle detection; no stack overflow.

4. **DoS via crafted depth=10000**: synthetic; bounded parser rejects at decode (depth > 32); ≤ 1ms reject; no OOM.

5. **DoS via crafted fanout=1M**: synthetic; bounded parser rejects (fanout > 4096); ≤ 1ms reject.

6. **DoS via crafted payload=100MB**: synthetic; bounded parser rejects (payload > 1 MiB); ≤ 1ms reject.

7. **Determinism boundary (concurrency)**: 1000 concurrent build calls of same input; assert all produce identical envelope bytes (no shared mutable state).

8. **INV-AC-OUTPUTS-VALID race (S-06 GC tombstones mid-flight)**: simulate; builder fails; handler 422 (INV held).

9. **Sig-forge insider (WI-S04-004 trait misuse)**: chaos test passes wrong-key SignatureVerifier to verify_full; structure check still catches Merkle tampering even if sig accepted (defense-in-depth).

10. **REAPI v2 spec drift (Bazel 8 new field)**: chaos PR upgrades vendored proto with breaking schema change; build fails compilation OR roundtrip drops field; test catches.

11. **Cargo-fuzz crashes**: 1h fuzz nightly; if crash detected, regress + fix; CI fails.

## 16. PRR

PRR HIGH_RISK 13 sign-offs gated em WI-S04-006. Este WI mini-PRR Architect + Crypto SME (mandatory).

- [ ] All Gherkin green.
- [ ] Property + Mann-Whitney + chaos green.
- [ ] Criterion benchmarks green.
- [ ] Cargo-fuzz 1h CI nightly green.
- [ ] Test vectors Annex A + B published.
- [ ] Determinism + round-trip green.
- [ ] ADR-0037 published.
- [ ] Crypto SME independent BLAKE3 + Merkle protocol review.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate skeleton + Cargo.toml + module structure | 1.5h |
| ST-002 | ActionResult proto codec (prost integration) | 2h |
| ST-003 | JSON envelope serde with sorted keys | 2h |
| ST-004 | Merkle builder (lex sort + balanced binary tree) | 4h |
| ST-005 | Merkle verifier (re-build + compare root) | 2.5h |
| ST-006 | Bounded parser (depth/fanout/node_count/payload) | 3h |
| ST-007 | Cycle detection (visited set; recursive Directory) | 2h |
| ST-008 | Outputs validator + BlobMetaReader trait | 3h |
| ST-009 | Mock BlobMetaReader (HashMap fixture) | 1h |
| ST-010 | Property tests (6 properties × 10k iter) | 5h |
| ST-011 | Mann-Whitney 3-prong test (verify timing) | 3h |
| ST-012 | Criterion benchmarks (3 benches) | 3h |
| ST-013 | Cargo-fuzz harness | 2h |
| ST-014 | Test vectors Annex A + B (50 each) | 5h |
| ST-015 | rustdoc + 4 examples + threat model README | 4h |
| ST-016 | spec/merkle_protocol.md doc | 3h |
| ST-017 | ADR-0037 redação | 2h |
| ST-018 | Architect + AppSec + Crypto SME review iteration | 4h |
| ST-019 | Métricas emit hooks (5 metrics in lib + handler integration) | 2h |
| ST-020 | Public API stability review (semver discipline) | 1h |
| ST-021 | Cost regression bench setup | 1.5h |

**Total Optimistic**: ~56h. **PERT** (O=50h, M=58h, P=88h): **~62h**.

## 18. Dependencies

### Hard blockers

- WI-S04-001 (REAPI proto vendoring) shared.
- `blake3` crate (stable; widely adopted).
- `prost` crate (REAPI proto codegen).
- `subtle` crate (constant-time compare).
- Crypto SME availability (review BLAKE3 + Merkle protocol independently).

### Soft blockers

- WI-S04-004 (SignatureVerifier trait) — soft; this WI defines trait, WI-004 implements.
- WI-S04-002 (D1 schema) — soft; outputs validator uses BlobMetaReader trait; concrete impl in handler (WI-001).

### Outbound

- WI-S04-001 (handler) consumes builder + verifier + outputs validator.
- WI-S04-004 (HKDF signing) implements SignatureVerifier trait.
- WI-S04-005 (TTL worker) does not consume directly.
- WI-S04-006 (conformance + PRR) consumes for verification cross-checks.
- S-15 (CLI/SDK) embeds for client-side verify.

## 19. Effort PERT

O: 50h, M: 58h, P: 88h → PERT **62h**.

## 20. Time-boxing

**72h hard limit**. Se exceder → escalation: split em "Merkle codec" + "outputs validator" sub-WIs.

## 21. Observability

5 métricas listadas §14.s04.003.6. Trace spans (consumed by handler):
- `ac.merkle.build` — depth, fanout, node_count, duration.
- `ac.merkle.verify` — depth, fanout, result (ok|root_mismatch|bounds_exceeded|cycle).
- `ac.outputs.validate` — count, duration, result (ok|missing|backend_error).

Logs structured JSON (when running in handler context):
- INFO em verify ok.
- WARN em bounds exceeded (potential abuse).
- ERROR em cycle detected (alert PD).
- CRITICAL em root mismatch sustained > 5/h (suspected tampering campaign).

Dashboard widget (partial; full em DASH-AC WI-S04-006):
- Verify rate per result.
- Bounds exceeded rate (alert if > 0/h sustained).
- Cycle detected counter.
- p99 latency build/verify.

## 22. Cost Analysis

**Per-verify cost** (warm path; AC GET):
- Worker CPU (BLAKE3 verify ~5ms @ 100 nodes): negligible.
- Memory: ~10 KiB heap per verify.
- Per-verify: ~$0.000002.

**Per-build cost** (UPDATE path):
- Worker CPU (BLAKE3 build + structure verify ~10ms): ~$0.000003.
- Outputs validate (D1 batch SELECT 100 IDs): ~$0.000002.
- Per-build: ~$0.000005.

**TCO 12m projection** (10M GET/dia + 1M UPDATE/dia):
- Verify: 10M × $0.000002 = $20/dia.
- Build: 1M × $0.000005 = $5/dia.
- Total: ~$25/dia × 365 = **~$9.1k/yr**.

**Cost regression gate**:
- Verify ≤ $0.000002 per-op.
- Build ≤ $0.000005 per-op.
- CI bench fails if exceed.

**Comparison vs alternatives**:
- SHA-256 (Bazel default): ~4× slower; verify ~20ms; cost ~$30k/yr at 10M/dia.
- BLAKE3 (this impl): **$9.1k/yr** at 10M/dia.

## 23. API Contract

Public crate API (semver post v1.0):

```rust
// Builders
pub trait ActionResultBuilder { fn build(...) -> Result<AcEnvelope, BuildError>; }

// Verifiers
pub trait ActionResultVerifier {
    fn verify_structure(...) -> Result<(), MerkleError>;
    fn verify_full(...) -> Result<(), VerifyError>;
}

// Outputs validation
pub trait OutputsValidator { async fn validate_outputs(...) -> Result<(), OutputsCheckError>; }
pub trait BlobMetaReader { async fn batch_check_alive(...) -> Result<Vec<bool>, _>; }

// Sig delegate (impl by WI-S04-004)
pub trait SignatureVerifier { fn verify_sig(...) -> Result<(), SigError>; }

// Envelope (immutable post-build; #[non_exhaustive])
pub struct AcEnvelope { ... }

// Errors
pub enum MerkleError { ... }
pub enum OutputsCheckError { ... }
pub enum VerifyError { Structure(MerkleError), Sig(SigError) }
```

**Stability**: post-v1.0, `#[non_exhaustive]` allows additive fields in AcEnvelope; trait methods stable.

**Versioning**: envelope.version u8; v1 fixed; v2+ via migration ADR.

## 24. Post-mortem Hooks

- Merkle root mismatch sustained > 5/h → CRITICAL post-mortem (suspected tampering campaign).
- Cycle detected sustained > 1/h → CRITICAL post-mortem (suspected DoS attempt OR malformed Bazel client).
- Bounds exceeded sustained > 100/h → SEV-2 (DoS attempt; rate limit S-08 forward).
- INV-AC-OUTPUTS-VALID race detected via reconcile drift → SEV-2 (S-06 GC race tuning).
- Determinism regression detected by property test → SEV-1 (refactor regression; revert).
- Cargo-fuzz crash → SEV-1 (decoder bug; fix + regression test).
- Test vectors Annex regression (verify rejection unexpected) → SEV-2 (compat regression).

## 25. Rollback / Recovery

- Crate version pin via Cargo.lock; rollback via git revert + cargo update.
- Envelope version backward-compat: v1 readers continue to work post-v2 deploy (handler routes by envelope.version).
- RTO: ≤ 10 min (deploy revert).
- RPO: 0 (stateless library; no data loss).

Fallback: if corelink-ac crash detected, handler returns 503 (graceful); no envelope creation; AC writes pause until fix.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: tenant_id binding in envelope; attacker cannot forge envelope for another tenant (tenant_id field signed with WI-004 HKDF).
- **Tampering**: dual-side Merkle verify catches byte-flips post-persist; defense-in-depth even with chave compromise.
- **Repudiation**: envelope.created_at_ms + signing chain via WI-S03-007 audit; immutable record.
- **Information disclosure**: envelope JSON contains tenant_id (UUID v7 pseudonymous) + action_digest (content hash; non-PII) + result fields (PII risk customer-side).
- **DoS**: bounded parser rejects oversized inputs ≤ 1ms; cargo-fuzz 1h validates.
- **Elevation of privilege**: trait boundary explicit; OutputsValidator requires BlobMetaReader; impl scoped to handler context (TenantCtx-backed).

**LINDDUN delta**:
- **Linkability**: tenant_id pseudonymous (UUID v7); action_digest content-hash.
- **Identifiability**: ActionResult fields (command_line, env_vars) PII risk customer-side; redact policy via S-09 macro.
- **Non-repudiation**: append-only envelope + audit chain.
- **Detectability**: Merkle root mismatch alert; cycle detected alert; bounds exceeded alert.
- **Disclosure of information**: envelope encrypted at rest (R2 SSE-S3); sig verify catches tampering.
- **Unawareness**: spec doc + ADR-0037 + test vectors public to customer auditors.
- **Non-compliance**: SLSA Level 3 supply chain provenance partial alignment via Merkle binding.

## 27. Knowledge Transfer

- **Tech talk** (1.5h): "Merkle Dual-Side Verify + BLAKE3 + RFC 6962-style Domain Separation + Bounded Parser Discipline".
- **Doc** `crates/corelink-ac/spec/merkle_protocol.md` — canonical spec.
- **Doc** `crates/corelink-ac/README.md` — API overview + threat model.
- **ADR-0037** — design rationale.
- **Workshop** (2h): com Architect + AppSec + Crypto SME + downstream WI authors (WI-S04-001/004/006).
- **Onboarding test** (5 questions): BLAKE3 vs SHA-256 rationale, domain separation purpose, dual-side verify rationale, bounded parser bounds, cycle detection necessity.
- **External-facing**: blog post post-S-04 SEALED — "How CoreLink prevents AC cache poisoning via dual-side Merkle verify".

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Merkle root forge via cripto break (BLAKE3-256 collision) | L | L | CRITICAL | L | LOW | BLAKE3-256 collision-resistant 2^128; computacionalmente intratável; ADR-0037 documents threat model |
| R-002 | Determinism regression via HashMap order | M | L | HIGH | L | LOW | Property test prop_merkle_determinism 10k iter; CI gate; lex sort enforced |
| R-003 | Bounded parser bypass (decoder accepts 100MB before checking) | L | M | HIGH | L | LOW | Bounds at decode time (incremental); cargo-fuzz validates 1h; ADR-0037 documents |
| R-004 | Cycle in output_directories causes stack overflow | L | M | HIGH | L | LOW | Visited set + bounded depth; chaos test 100 cyclic inputs |
| R-005 | INV-AC-OUTPUTS-VALID race (GC mid-flight) | M | M | HIGH | M | LOW | Strict server pre-persist; reconcile diário; INV registry §3.15 promotion |
| R-006 | Sig delegate (WI-S04-004) trait misuse — wrong-key SignatureVerifier accepted | L | M | HIGH | L | LOW | verify_structure independent (defense-in-depth); ADR-0037; test scenario chaos |
| R-007 | Mann-Whitney CI flake | M | H | LOW | M | LOW | Šidák 3-trial gate; combined α ≈ 0.000125 |
| R-008 | REAPI v2 spec drift (Bazel 8 schema delta) | M | M | MEDIUM | M | LOW | Vendored proto pinned; conformance suite (WI-S04-006); ADR upgrade policy |
| R-009 | Cost regression > 10% per-op | M | L | MEDIUM | L | LOW | §14.10 cost gate; criterion benchmark CI |
| R-010 | Cargo-fuzz crash detected | L | M | HIGH | L | LOW | 1h CI nightly; immediate fix + regression test; SEV-1 |
| R-011 | Public API breaking change post-v1.0 | L | L | HIGH | L | LOW | Semver discipline; #[non_exhaustive]; ADR for major bump |
| R-012 | Test vectors Annex regression (compat regression) | L | L | MEDIUM | L | LOW | CI integrates Annex A + B; PR gate |
| R-013 | BLAKE3 crate vulnerability (zero-day) | L | M | HIGH | L | LOW | Cargo-audit weekly; cargo-deny crates allowlist; rapid patch cycle |
| R-014 | Outputs batch SELECT exceeds D1 IN-clause limit | M | L | LOW | L | LOW | Chunk in 100s; documented in spec; integration test |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME review Merkle protocol (BLAKE3 + RFC 6962 domain sep) + bounded parser bounds.
2. **AppSec (D+2)**: AppSec review tampering detection + dual-side coverage + sig integration boundary.
3. **Code (D+5)**: peer review (2 engineers).
4. **Crypto (D+6)**: Crypto SME independent review — BLAKE3 usage, domain separation, determinism, cycle detection, test vectors.
5. **Adversarial (pre-merge D+8)**: red team — cycle bomb, depth bomb, fanout bomb, sig forge insider.
6. **Cargo-fuzz (D+9)**: 1h fuzz validates 0 panics.
7. **PRR (D+10)**: Architect + Crypto SME mini sign-off (full ship em WI-S04-006).

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; **mandatory** — dual-side verify + tampering detection_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD; SLSA Level 3 alignment review_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD; ActionResult metadata redaction policy_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; **mandatory** — bounded parser + public API stability_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; **mandatory** — Merkle protocol + cycle detection + bounds_ | _pending_ | _pending_ |
| 13 | Crypto SME | _**mandatory emphatic** — BLAKE3 + RFC 6962 domain sep + determinism + dual-side defense-in-depth + test vectors_ | _pending_ | _pending_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S04-003 (Lote 10.4); SOTA pós-Lote 10.3bis (32 seções; 13-row sign-off; 14-row risk; Mann-Whitney 3-prong; cost TCO 12m; 11 chaos experiments; STRIDE+LINDDUN delta full; Crypto SME emphatic mandatory). |

## 32. Anti-patterns evitados

- ❌ Skip verify_structure() em build (always invoked internally).
- ❌ Variable-length tree depth (bounded 32).
- ❌ Custom hash function (BLAKE3-256 fixed).
- ❌ Non-deterministic build (lex sort + canonical proto enforced).
- ❌ Skip cycle detection (visited set mandatory).
- ❌ Server-only verify (dual-side defense-in-depth).
- ❌ Output digest format unvalidated.
- ❌ Synchronous outputs check single SELECT (batch SELECT mandatory).
- ❌ Public API breaking changes post v1.0 without ADR.
- ❌ Mann-Whitney p>0.05 sozinho (3-prong gate).
- ❌ Cargo-fuzz < 1h (insufficient coverage).
- ❌ Test vectors absent (no compat regression detection).

---

**Fim WI-S04-003.** Próximo: WI-S04-004 (CTRL-AC-002 HKDF-SHA256 digest signing + ADR-0021 + property test 10k + Mann-Whitney 3-prong cripto-grade).
