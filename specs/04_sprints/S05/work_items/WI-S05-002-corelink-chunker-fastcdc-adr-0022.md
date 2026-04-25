---
id: "WI-S05-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-009"]
parent: "S-05"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "STORAGE-SEMANTICS"
  - "CAS-PROFILE"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s05", "chunker", "fastcdc", "blake3", "streaming", "adr-0022", "high-risk", "crypto"]
---

# WI-S05-002 — Crate `corelink-chunker` (Fixed-Size 2 MiB Default + FastCDC Opt-in + ADR-0022 Ratificada Chunk-vs-Part Decoupling) + Streaming Iterator + Determinism Property Test 100k

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-05](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S05-002 |
| Título | Crate `corelink-chunker` — fixed-size 2 MiB chunks (default) + FastCDC content-defined chunking opt-in (Xia 2016); zero-allocation streaming Iterator API; BLAKE3 SIMD hash inline; INV-CAS-IDEMPOTENCY enforce (same input → same chunks byte-identical); ADR-0022 ratificada (chunk size vs multipart part size decoupling); criterion benchmarks ≥ 500 MB/s native + ≥ 200 MB/s WASM (Lote 10.5bis recalibration; was 2 GB/s desktop AVX-512 ceiling); cargo-fuzz harness 1h CI nightly |
| Sprint | S-05 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (chunker determinism é INV-CAS-IDEMPOTENCY load-bearing), FF-HR-009 (defense-in-depth — chunker is single source of truth for content-addressing) |

## 1. Intent

Implementar `crates/corelink-chunker/` — biblioteca cripto-coordenada que:

1. **Codec chunker**: streaming Iterator API; zero-allocation per chunk.
2. **Fixed-size chunker** (default): 2 MiB chunks + final partial.
3. **FastCDC chunker** (opt-in via flag): content-defined chunking via rolling hash (Xia et al., USENIX ATC 2016).
4. **BLAKE3 hash inline**: per-chunk hash computed during streaming (SIMD-optimized; ≥ 500 MB/s native + ≥ 200 MB/s WASM (Lote 10.5bis recalibration; was 2 GB/s desktop AVX-512 ceiling)).
5. **INV-CAS-IDEMPOTENCY enforce**: determinism property — same input bytes → same chunk boundaries → same chunk digests byte-identical.

```rust
// File: crates/corelink-chunker/src/lib.rs

#![forbid(unsafe_code)]

pub use crate::chunker::{Chunker, ChunkerAlgorithm, ChunkerConfig, Chunk};
pub use crate::error::ChunkerError;

pub mod chunker;
pub mod fixed;     // fixed-size 2 MiB
pub mod fastcdc;   // FastCDC content-defined
pub mod error;
pub mod bounds;    // constants

// Public types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkerAlgorithm {
    /// Default: fixed-size 2 MiB chunks + final partial.
    /// Stable across all client versions; deterministic.
    Fixed2MiB,

    /// Opt-in: FastCDC content-defined chunking (Xia 2016).
    /// Better dedup ratio em payloads similar (e.g., Docker layers, ML model deltas).
    /// Rolling-hash anchor; deterministic per spec.
    /// Avg chunk 2 MiB; min 1 MiB; max 4 MiB.
    FastCDC2MiB,
}

#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    pub algorithm: ChunkerAlgorithm,
    /// Fixed chunk size (FixedNMiB only; ignored for FastCDC).
    pub fixed_chunk_size: usize,
    /// FastCDC bounds (min/avg/max) em bytes (FastCDC only).
    pub fastcdc_min: usize,        // default 1 MiB
    pub fastcdc_avg: usize,        // default 2 MiB (target)
    pub fastcdc_max: usize,        // default 4 MiB
    /// FastCDC mask seeds (deterministic; per spec).
    pub fastcdc_mask_s: u64,
    pub fastcdc_mask_l: u64,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        ChunkerConfig {
            algorithm: ChunkerAlgorithm::Fixed2MiB,
            fixed_chunk_size: 2 * 1024 * 1024,           // 2 MiB
            fastcdc_min: 1 * 1024 * 1024,                // 1 MiB
            fastcdc_avg: 2 * 1024 * 1024,                // 2 MiB
            fastcdc_max: 4 * 1024 * 1024,                // 4 MiB
            fastcdc_mask_s: 0x0000d9f003530000,          // FastCDC paper Table 2
            fastcdc_mask_l: 0x0000d90003530000,          // FastCDC paper Table 2
        }
    }
}

/// Chunk produced by streaming chunker.
/// Lifetime tied to internal buffer; consume immediately or copy bytes out.
pub struct Chunk<'a> {
    pub bytes: &'a [u8],
    pub digest: [u8; 32],          // BLAKE3-256 hash inline
    pub offset_in_blob: u64,
    pub size_bytes: usize,
}

pub trait Chunker {
    /// **Lote 10.5bis P0 fix**: API redesigned for genuine zero-allocation.
    /// Pull-based: caller invokes `next_chunk(input)` repeatedly; each call returns
    /// `Some(Chunk)` if a boundary is detected mid-input, else `None` (more input needed).
    /// `Chunk<'a>` borrows internal buffer; consumer must consume before next call OR copy.
    /// Was `Box<dyn Iterator<...>>` which DID heap-allocate per `feed()` call (one trait-object box
    /// per call; ~24 bytes × N calls per blob); contradicted "zero-allocation" claim throughout WI.
    fn next_chunk<'a>(&'a mut self, input: &'a [u8]) -> Option<Chunk<'a>>;

    /// Feed remaining bytes after `next_chunk` returned None (input fully consumed).
    fn feed_more(&mut self, input: &[u8]) -> usize;  // returns bytes consumed

    /// Finalize: emit any remaining buffered bytes as final partial chunk.
    /// Must be called after last `feed_more` / `next_chunk`.
    fn finalize<'a>(&'a mut self) -> Option<Chunk<'a>>;

    /// Reset chunker state (for reuse).
    fn reset(&mut self);
}

#[derive(thiserror::Error, Debug)]
pub enum ChunkerError {
    #[error("input exceeds maximum blob size: {found} > {max}")]
    BlobTooLarge { found: u64, max: u64 },          // max = 160 GiB single multipart

    #[error("FastCDC config invalid: min={min} > avg={avg} OR avg > max={max}")]
    FastCDCConfigInvalid { min: usize, avg: usize, max: usize },

    #[error("internal error: {0}")]
    Internal(String),
}
```

**Cripto-driven invariants**:

1. **INV-CAS-IDEMPOTENCY**: same input bytes + same `ChunkerConfig` → same chunk boundaries + same digests byte-identical. Property test 1000 random blobs × 100 chunkings = 100% byte-equal.
2. **INV-MULTIPART-CHUNK-DETERMINISTIC** (NEW): FastCDC rolling-hash deterministic per spec; mask seeds fixed em `ChunkerConfig::default()`; documented em ADR-0022.
3. **BLAKE3 hash inline**: each chunk's digest computed during streaming (`blake3::Hasher::update` on chunk bytes); finalize on chunk boundary; SIMD-optimized.

**ADR-0022 RATIFICAÇÃO** (este WI promotes from "forward-looking" to "ratified"):

> **ADR-0022 — Multipart Chunk Size vs R2 Part Size Decoupling**
>
> **Status:** ACCEPTED (S-05 WI-S05-002 implementation).
>
> **Context:** Codex finding S-05:55 vs S-05:94 contradiction (Lote 9.4) — chunk size and multipart part size were conflated em previous WI drafts. Sprint contract amended Lote 9.4 to clarify; this ADR ratifies.
>
> **Two distinct concepts**:
>
> | Concept | Purpose | Default S-05 | Tunable |
> |---|---|---|---|
> | **Chunk size** | Content-addressable unit; dedup granularity | **2 MiB** (Fixed) OR avg 2 MiB (FastCDC) | Per-tenant config S-13 forward |
> | **R2 multipart part size** | R2 SDK upload-part API unit | **16 MiB** (8 chunks per part) | R2 minimum 5 MiB; max 5 GiB per part |
> | **R2 max parts** | R2 hard limit per multipart session | 10,000 | Beyond → stitched flow WI-S05-006 |
> | **Max single multipart blob** | 10,000 parts × 16 MiB | 160 GiB | Stitched > 160 GiB |
>
> **Decision: Decoupled.**
>
> **Rationale:**
> 1. **Dedup granularity** independent of R2 API constraints — chunk size é semantic decision; R2 part size é infrastructure.
> 2. **R2 cost optimization** — 16 MiB parts batch 8 × 2 MiB chunks; reduces multipart API ops (10k parts/blob vs 80k chunks/blob).
> 3. **Buildbarn precedent** — Buildbarn decouples; CoreLink alignment.
> 4. **FastCDC opt-in** — content-defined chunking better dedup em Docker layers; rolling-hash anchor deterministic.
>
> **Future**: Customer-tunable chunk size per-tenant via S-13 admin plane (post-GA); mandates ChunkerConfig serialization stability.

**Constraint cripto-driven**:

1. **Default Fixed2MiB**: stable across all client versions; trivially deterministic.
2. **FastCDC opt-in via flag**: enable via `ChunkerConfig::FastCDC2MiB`; default disabled (S-04 GA); customer-controlled per-blob.
3. **FastCDC determinism**: mask seeds + min/avg/max bounds fixed em config; rolling-hash anchor independent of memory layout.
4. **Streaming Iterator zero-allocation**: per-chunk `&[u8]` slice; no Vec allocation per chunk; backpressure native.
5. **BLAKE3 SIMD**: `blake3::Hasher` SIMD-optimized; throughput ≥ 500 MB/s native + ≥ 200 MB/s WASM (Lote 10.5bis recalibration; was 2 GB/s desktop AVX-512 ceiling) (criterion benchmark gate).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Chunker é **single source of truth** for content-addressing em multipart blobs. Bug em determinism = INV-CAS-IDEMPOTENCY violated globally (same blob → different chunks → different manifest_digest → different cache hit semantics → multipart cache effectively useless). HIGH_RISK em N dimensões:

1. **Determinism violation via memory layout dependency**: chunker uses `HashMap` iteration OR pointer addresses → non-deterministic. Mitigação: streaming pure-functional via Iterator; no stateful map iteration; FastCDC rolling-hash uses byte-level operations only; property test 1000 random blobs × 100 chunkings = byte-equal.

2. **FastCDC anchor drift**: rolling-hash mask seeds change between client versions → same blob → different chunk boundaries → cache miss explosion. Mitigação: mask seeds **fixed em `ChunkerConfig::default()`**; ADR-0022 documents; CI byte-equal test asserts seeds stable across crate versions; semver discipline.

3. **Streaming buffer overflow**: client sends 200 GiB blob → bounded parser must reject early. Mitigação: `BlobTooLarge` error em `feed()` if cumulative size > 160 GiB (single multipart cap per ADR-0022); stitched flow WI-S05-006 for > 160 GiB.

4. **BLAKE3 SIMD panic on misaligned buffers**: x86 AVX-512 requires alignment. Mitigação: `blake3` crate handles internally (Bao tree implementation); cargo-fuzz harness 1h validates no panic on arbitrary input alignment.

5. **Iterator lifetime issue**: `Chunk<'a>` borrows internal buffer; if caller stores Chunk past next feed() → dangling reference. Mitigação: `'a` lifetime ties Chunk to borrow on chunker; borrow checker enforces compile-time; rustdoc warns "consume immediately or copy bytes out".

6. **Final partial chunk handling**: blob size NOT multiple of 2 MiB → final partial chunk < 2 MiB. Mitigação: `finalize()` emits remaining buffer; integration test 5 MiB blob = 2 chunks of 2 MiB + 1 chunk of 1 MiB; INV-CAS-IDEMPOTENCY preserved.

7. **FastCDC bounds violation**: if min > avg OR avg > max → infinite loop OR exponential boundary search. Mitigação: `FastCDCConfigInvalid` error em config construction; `Default::default()` always valid; runtime check em `Chunker::feed`.

8. **Cargo-fuzz crash on adversarial input**: pathological inputs (all 0xFF, all 0x00, repeating patterns) may trigger edge cases. Mitigação: 1h cargo-fuzz CI nightly; assert never panics; bounded recursion (FastCDC depth bounded by max chunk size).

**Atacante adversarial scenarios**:

- **Determinism poison via crafted FastCDC input**: attacker constructs blob that hits FastCDC anchor at every byte → 100k+ tiny chunks → D1 manifest_chunks bloat. Mitigação: bounded MAX_CHUNKS_PER_BLOB = 80000 (per WI-S05-005 INV); chunker rejects via `feed()` before inserting; FastCDC min bound = 1 MiB prevents tiny chunks anyway.

- **Memory exhaustion via crafted large input**: 1 TiB stream. Mitigação: BlobTooLarge error at 160 GiB; streaming pipeline never accumulates full blob.

- **Hash collision attempt**: BLAKE3 256-bit collision-resistant 2^128; computacionalmente intratável.

- **Cripto break of BLAKE3**: if BLAKE3 broken (theoretical), attacker forges chunk with matching digest. Mitigação: BLAKE3 paper §6 NIST-compatible analysis; fallback SHA-256 via ADR forward if needed.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: chunker determinism é INV-CAS-IDEMPOTENCY load-bearing; bug = global cache invalidation.
- **FF-HR-009**: defense-in-depth — chunker is single source of truth for content-addressing; bypass = security control disabled.
- **Reversibility**: chunker bug post-deploy = customer cache invalidation cascade; rollback via Wrangler version revert.
- **Customer impact**: dedup ratio drop = R2 cost spike; bug em determinism = cache effectively useless.

13 sign-offs incl. **Crypto SME mandatory emphatic** (BLAKE3 + FastCDC determinism review).

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev pushing Docker layer (50 MiB)**:
- Chunker fixed 2 MiB → 25 chunks; manifest references each.
- Subsequent build with 1 MiB delta in middle of layer:
  - Fixed-size: shifts all subsequent chunks → all 25 chunks new (no dedup; cache miss).
  - FastCDC: anchor reuse → only 1-2 chunks affected; 23-24 chunks dedup'd; cache hit ratio jump.
- Customer-visible: FastCDC enables Docker layer dedup that fixed-size cannot.

**Persona 2 — ML CI dev pushing model file (5 GiB)**:
- Fixed-size 2 MiB → 2500 chunks.
- FastCDC opt-in: similar count but better dedup on model fine-tuning deltas.

**Persona 3 — Performance reviewer**:
- Criterion benchmark `chunker_throughput`: ≥ 500 MB/s native + ≥ 200 MB/s WASM (CF Workers realistic; Lote 10.5bis P0 fix recalibration: was 2 GB/s desktop AVX-512 ceiling unachievable em deploy target).
- Cargo-fuzz: 1h CI nightly; 0 panics on arbitrary input.
- Property test: 1000 blobs × 100 chunkings = 100% byte-identical (determinism).

**SLA addendum**:
- Chunker throughput ≥ 500 MB/s native; ≥ 200 MB/s CF Workers WASM (Lote 10.5bis P0 fix).
- Streaming memory bound ≤ 4 MiB stack per invocation.
- FastCDC determinism: mask seeds fixed; same input → same boundaries 100%.
- FastCDC opt-in: enable via `ChunkerConfig::algorithm = FastCDC2MiB`; default disabled (S-05 GA).

## 4. Capability Mapping

- **CAP-CAS-009** (Merkle chunking) — IMPLEMENTA primary chunker side; manifest builder em WI-S05-005.
- **CAP-CAS-013** (Chunk size vs part size separation) — IMPLEMENTA via ADR-0022.
- Trace: `cas_profile.md §4 (Merkle decomposition)` + `storage_semantics.md §5 (multipart layout)` + ADR-0022.

## 5. Tipo

Cripto library; HIGH_RISK; FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-chunker`** structure:
   - `crates/corelink-chunker/Cargo.toml`.
   - `src/lib.rs` (public API surface).
   - `src/chunker/`: trait + dispatch.
   - `src/fixed/`: Fixed2MiB impl.
   - `src/fastcdc/`: FastCDC impl.
   - `src/bounds.rs`: constants (MAX_BLOB_SIZE = 160 GiB; MAX_CHUNKS_PER_BLOB = 80000).
   - `src/error.rs`: ChunkerError enum.

2. **Fixed2MiB chunker** (`src/fixed/`):
   - Stateless: track offset; emit chunk every 2 MiB.
   - Final partial chunk via `finalize()`.
   - BLAKE3 hash computed during emit (`blake3::Hasher::update` em chunk bytes; finalize on boundary).
   - Streaming Iterator zero-allocation.

3. **FastCDC chunker** (`src/fastcdc/`):
   - Per FastCDC paper (Xia et al., USENIX ATC 2016).
   - Rolling hash via Gear hash (paper §3.3); mask seeds fixed.
   - Anchor detection: `(hash & mask_l == 0)` for chunk boundary.
   - Bounds: min 1 MiB; avg 2 MiB; max 4 MiB.
   - Streaming Iterator zero-allocation.
   - BLAKE3 hash inline per emitted chunk.

4. **Streaming Iterator API**:
   - `chunker.feed(bytes) -> impl Iterator<Item = Chunk>` — lazy emission.
   - `chunker.finalize() -> Option<Chunk>` — final partial.
   - Zero-allocation: Chunk borrows internal buffer; consumer copies bytes if storing past next feed.
   - Backpressure: chunker pauses if downstream slower (R2 PUT in WI-S05-001 handler).

5. **Determinism property tests** (10k iter PR; 100k nightly):
   - `prop_fixed_determinism`: 1000 random blobs × 100 chunkings = byte-identical chunks (digest + bytes).
   - `prop_fastcdc_determinism`: 1000 random blobs × 100 chunkings = byte-identical (mask seeds fixed).
   - `prop_chunk_size_bounds`: 1000 random blobs; Fixed = 2 MiB except final; FastCDC = [1 MiB, 4 MiB].
   - `prop_total_size_invariant`: sum of chunk sizes == input blob size.
   - `prop_blob_too_large_rejected`: 1000 inputs > 160 GiB rejected with BlobTooLarge.

6. **Mann-Whitney timing test** (3-prong cripto-grade):
   - Goal: cliente cannot distinguish "Fixed2MiB" vs "FastCDC2MiB" chunking via timing alone (algorithm flag is public; este test mais para regression detection).
   - Goal alt: timing of chunker invariant across input distributions (uniform random vs all-zero); Mann-Whitney detect anomaly via statistical test.
   - 10k samples per arm; power 1−β ≥ 0.80; |Δmedian| ≤ 5ms (middleware-grade).

7. **Criterion benchmarks** (`benches/`):
   - `bench_fixed_throughput`: target ≥ 500 MB/s native + ≥ 200 MB/s WASM (CF Workers realistic; Lote 10.5bis P0 fix recalibration: was 2 GB/s desktop AVX-512 ceiling unachievable em deploy target).
   - `bench_fastcdc_throughput`: target ≥ 1.5 GB/s single core (rolling-hash overhead).
   - `bench_streaming_pipeline`: target ≥ 100 MB/s end-to-end (chunker + R2 PUT mock).
   - Cost regression gate: PR > 10% regression bloqueia.

8. **Cargo-fuzz harness**:
   - `fuzz/fuzz_targets/fuzz_chunker_fixed.rs`: 1h CI nightly; arbitrary bytes input; assert no panic.
   - `fuzz/fuzz_targets/fuzz_chunker_fastcdc.rs`: 1h CI nightly; FastCDC adversarial patterns.
   - Bound enforcement: assert no OOM regardless of input size.

9. **Test vectors Annex** (`spec/test_vectors_chunker.md`):
   - 50 known input + expected chunks (Fixed and FastCDC).
   - Boundary cases: blob size = 1 byte, 2 MiB - 1, 2 MiB, 2 MiB + 1, 160 GiB.
   - Adversarial: all 0xFF, all 0x00, alternating patterns.
   - External SLSA L3 reviewer can reproduce.

10. **ADR-0022 ratificação**:
    - Update `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md` from DRAFT to ACCEPTED.
    - Document rationale, mitigations, future tunable per-tenant.
    - Whitelist em validate_references.py (já forward-looking; promote).

11. **Public API stability**:
    - `#[non_exhaustive]` on `ChunkerAlgorithm` enum (forward-compat new algorithms).
    - `#[non_exhaustive]` on `ChunkerConfig` struct.
    - Public traits: `Chunker`.
    - Semver: v0.x during S-05; v1.0 at S-05 SEALED.

12. **Métricas observability** (consumed by handler WI-S05-001):
    - `corelink.chunker.throughput_bytes_per_sec` (histogram).
    - `corelink.chunker.chunk_count_per_blob{algo}` (histogram).
    - `corelink.chunker.chunk_size_bytes{algo}` (histogram).
    - `corelink.chunker.dedup_ratio{algo}` (gauge; for FastCDC ratio claim).

13. **Documentation**:
    - `crates/corelink-chunker/README.md`: API + threat model + algo selection guide.
    - `crates/corelink-chunker/spec/chunker_protocol.md`: FastCDC spec adaptation, mask seeds, test vectors.
    - rustdoc 100% public API + 4 examples (Fixed default, FastCDC opt-in, streaming, custom config).

### 6.2 Out-of-scope (deferred)

- **Customer-tunable chunk size per-tenant via S-13 admin plane** — post-GA.
- **Compression (Zstd) integration**: anti-scope sprint contract §10.
- **Encryption-at-write per chunk**: BYOK S-14.
- **Variable chunk size > 16 MiB**: anti-scope.
- **Erasure coding chunks (Reed-Solomon)**: anti-scope.
- **Resumable chunker** (mid-stream restart): anti-scope; restart full multipart.

## 7. Anti-Scope

- ❌ Stateful map iteration in chunker (HashMap iteration order non-deterministic).
- ❌ Variable mask seeds per-invocation (FastCDC determinism requires fixed).
- ❌ Skip BLAKE3 hash inline (must be computed during streaming).
- ❌ Buffered chunker (zero-allocation Iterator mandatory).
- ❌ Unbounded recursion em FastCDC (max chunk size enforced).
- ❌ Skip cargo-fuzz harness (cripto path requires).
- ❌ Public API breaking changes post-v1.0 sem ADR.
- ❌ Custom hash function (BLAKE3-256 fixed).
- ❌ Mann-Whitney p>0.05 sozinho (3-prong gate).
- ❌ Skip determinism property test (INV-CAS-IDEMPOTENCY load-bearing).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: corelink-chunker fixed-size + FastCDC

  Background:
    Given crate corelink-chunker built
    Given test vectors Annex (50 known input + expected chunks for Fixed and FastCDC)

  Scenario: Fixed2MiB happy path (10 MiB blob)
    Given input 10 MiB random bytes
    When ChunkerConfig::default() (Fixed2MiB) feed input + finalize
    Then 5 chunks of 2 MiB each emitted
    And total size = 10 MiB; sum of chunk sizes == input
    And each chunk has BLAKE3-256 digest computed inline

  Scenario: Fixed2MiB partial final chunk
    Given input 5 MiB
    When chunker feed + finalize
    Then chunks: [2 MiB, 2 MiB, 1 MiB] emitted
    And total size = 5 MiB

  Scenario: FastCDC2MiB content-defined chunking
    Given ChunkerConfig::default() with algorithm = FastCDC2MiB
    Given input 10 MiB random bytes
    When feed + finalize
    Then chunks emitted with sizes em [1 MiB, 4 MiB] range (FastCDC bounds)
    And chunk count varies based on rolling-hash anchor detection
    And BLAKE3 digest computed inline per chunk

  Scenario: Determinism — Fixed2MiB
    Given 1000 random blobs × 100 chunkings each (Fixed2MiB)
    When property test runs
    Then 100 chunkings per blob produce IDENTICAL chunks (digest + bytes)
    And property test prop_fixed_determinism green

  Scenario: Determinism — FastCDC2MiB
    Given 1000 random blobs × 100 chunkings each (FastCDC2MiB; mask seeds fixed default)
    When property test runs
    Then 100 chunkings per blob produce IDENTICAL chunks
    And property test prop_fastcdc_determinism green

  Scenario: Blob too large rejection
    Given input claiming > 160 GiB
    When chunker feed exceeds bound
    Then error ChunkerError::BlobTooLarge { found: 200 GiB, max: 160 GiB }
    And NO panic
    And property test prop_blob_too_large_rejected green

  Scenario: FastCDC dedup ratio improvement vs Fixed
    Given Docker layer L1 (50 MiB)
    Given Docker layer L2 (50 MiB; 1 MiB delta in middle of L1)
    When chunker Fixed2MiB on L1 + L2
    Then 25 chunks each; 0 chunks shared (delta shifts all subsequent)
    When chunker FastCDC2MiB on L1 + L2
    Then ~25 chunks each; 23-24 chunks shared (anchor reuse around delta)
    And dedup ratio improvement ≥ 1.5× FastCDC vs Fixed

  Scenario: Streaming Iterator zero-allocation
    Given input 1 GiB stream
    When chunker.feed iterator drives
    Then per-request stack ≤ 4 MiB throughout (chunker buffer 2 MiB + headroom)
    And NO OOM on CF Workers 128 MiB cap
    And throughput ≥ 100 MB/s end-to-end

  Scenario: BLAKE3 throughput benchmark
    Given criterion benchmark bench_fixed_throughput
    When 1 GiB random data chunked
    Then ≥ 500 MB/s native + ≥ 200 MB/s WASM (Lote 10.5bis recalibration; was 2 GB/s desktop AVX-512 ceiling) (BLAKE3 SIMD AVX2/AVX-512 dispatch)

  Scenario: Cargo-fuzz harness 1h — no panics
    Given fuzz target fuzz_chunker_fixed + fuzz_chunker_fastcdc
    When 1h cargo-fuzz run with arbitrary bytes
    Then 0 panics, 0 OOM
    And bounds enforced regardless of input

  Scenario: ADR-0022 ratificada
    Given ADR-0022 file content
    Then doc_status = ACCEPTED (not DRAFT)
    And rationale documents chunk vs part decoupling
    And whitelist em validate_references.py
    And test vectors Annex referenced

  Scenario: Test vectors Annex reproducibility
    Given test vectors `spec/test_vectors_chunker.md` (50 known inputs + expected chunks)
    When chunker run on each input
    Then output matches expected (Fixed and FastCDC)
    And external SLSA L3 reviewer can reproduce
```

## 9. Design Decisions

### 9.1 Why Fixed2MiB default (não FastCDC default)

Stable across all client versions; trivially deterministic; no rolling-hash complexity. FastCDC opt-in for advanced workloads (Docker layers, ML models). Customer SDK (S-15) exposes flag; default Fixed.

### 9.2 Why 2 MiB chunk size (sprint contract §5.1)

Trade-off: 1 MiB more dedup but 2× R2 PUT overhead; 4 MiB less dedup, less ops. 2 MiB balance per Buildbarn precedent + FastCDC paper recommendations.

### 9.3 Why FastCDC mask seeds fixed em Default

INV-CAS-IDEMPOTENCY requires same input → same chunks. Mask seeds are part of the algorithm spec; changing them breaks determinism globally. ADR-0022 documents stability commitment; semver discipline.

### 9.4 Why BLAKE3 SIMD (não SHA-256)

BLAKE3 4× faster on AVX-512; critical for ≥ 500 MB/s native (recalibrated Lote 10.5bis) throughput. Same security level (256-bit collision-resistant). REAPI v2 multi-hash supports BLAKE3 (Bazel 7+ adopted).

### 9.5 Why streaming Iterator (não buffered)

160 GiB blob requires streaming; CF Workers memory cap 128 MiB; buffered impossible. Iterator<Chunk> zero-allocation; backpressure native.

### 9.6 Why bounded MAX_BLOB_SIZE = 160 GiB (per ADR-0022)

R2 multipart hard limit: 10000 parts × 16 MiB part = 160 GiB single multipart session. Beyond → stitched flow WI-S05-006.

### 9.7 Why MAX_CHUNKS_PER_BLOB = 80000

160 GiB / 2 MiB chunk = 80000 chunks. WI-S05-005 manifest builder enforces; bounded parser rejects > 80000.

### 9.8 Why cargo-fuzz 1h (não 10s)

Chunker is attack surface; arbitrary bytes input; FastCDC rolling-hash adversarial patterns possible. 1h = ~30M iters; bounded inputs explored. CI nightly $0.05/h.

### 9.9 Why test vectors Annex

Reproducibility: external SLSA L3 reviewers validate impl matches spec. Regression detection: refactor accidentally changes chunk boundaries → annex test vectors fail.

### 9.10 ADR potencial?

ADR-0022 já forward; este WI ratifies (DRAFT → ACCEPTED). New related ADR potencial: **ADR-0039**: "Chunker public API stability + semver discipline + mask seeds versioning policy" — small registry-extension; ratificada em WI-S05-006 ship gate.

## 10. Completeness Criteria SOTA

- [ ] **10.s05.002.1** Property tests 10k iter (PR) + 100k nightly → 0 panics, 0 false-accepts (EVT-002):
  - prop_fixed_determinism, prop_fastcdc_determinism, prop_chunk_size_bounds, prop_total_size_invariant, prop_blob_too_large_rejected.
- [ ] **10.s05.002.2** Mann-Whitney U + power analysis 3-prong em chunker timing across distributions (EVT-002):
  - N ≥ 10000 samples per arm; power 1−β ≥ 0.80 com Cohen's d = 0.2; Šidák 3-trial; |Δmedian| ≤ 5ms.
- [ ] **10.s05.002.3** Criterion benchmarks: Fixed throughput ≥ 500 MB/s native + ≥ 200 MB/s WASM (Lote 10.5bis recalibration; was 2 GB/s desktop AVX-512 ceiling); FastCDC ≥ 1.5 GB/s; streaming pipeline ≥ 100 MB/s (EVT-021).
- [ ] **10.s05.002.4** Cargo-fuzz harness 1h CI nightly (Fixed + FastCDC targets) → 0 panics, 0 OOM (EVT-002).
- [ ] **10.s05.002.5** Test vectors Annex (50 known input + expected chunks Fixed + FastCDC) — chunker output matches per spec (EVT-002).
- [ ] **10.s05.002.6** Determinism — 1000 blobs × 100 chunkings = 100% byte-identical (Fixed and FastCDC) (EVT-002).
- [ ] **10.s05.002.7** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s05.002.8** Cost regression gate: per-op cost ≤ $0.000001 (chunker hot path; mostly BLAKE3 compute) (Lote 9.4 §14.10).
- [ ] **10.s05.002.9** rustdoc 100% public API + 4 examples + threat model README + spec doc `chunker_protocol.md`.
- [ ] **10.s05.002.10** Public API stability: `#[non_exhaustive]` on enum + struct; semver discipline.
- [ ] **10.s05.002.11** ADR-0022 ratificada (DRAFT → ACCEPTED) + ADR-0039 forward (chunker public API stability + mask seeds versioning).
- [ ] **10.s05.002.12** Bound enforcement: MAX_BLOB_SIZE 160 GiB, MAX_CHUNKS_PER_BLOB 80000 validated by chaos test.

## 11. DoD

- [ ] Crate `corelink-chunker` compila + integration tests green.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em CI; 100k nightly green.
- [ ] Mann-Whitney 3-prong test green.
- [ ] Criterion benchmarks green (≥ 500 MB/s native (recalibrated Lote 10.5bis) Fixed; ≥ 1.5 GB/s FastCDC).
- [ ] Cargo-fuzz 1h CI nightly green.
- [ ] Test vectors Annex published + integrated CI.
- [ ] Determinism property tests green (Fixed and FastCDC).
- [ ] rustdoc 100% public API + 4 examples + threat model README.
- [ ] `corelink-chunker/spec/chunker_protocol.md` published.
- [ ] ADR-0022 ratificada (ACCEPTED).
- [ ] ADR-0039 published (forward).
- [ ] Architect + AppSec + Crypto SME (mandatory emphatic) + Security Lead reviews.
- [ ] PRR Architect + Crypto SME mini sign-off.
- [ ] Cost regression gate green.
- [ ] Cargo-audit + cargo-deny clean.

## 12. Invariants Validated

- **INV-CAS-IDEMPOTENCY** (CRITICAL, registry §3.3): same input bytes + same `ChunkerConfig` → same chunk boundaries + same digests byte-identical; property test 100k.
- **INV-MULTIPART-CHUNK-DETERMINISTIC** (CRITICAL, NEW — promovida registry §3.16 Lote 10.5bis): FastCDC mask seeds fixed em `ChunkerConfig::default()`; ADR-0022 stability commitment.
- **INV-MULTIPART-BOUNDED-PARSER** (HIGH, NEW): MAX_BLOB_SIZE 160 GiB, MAX_CHUNKS_PER_BLOB 80000 enforced at decode time.
- **INV-MULTIPART-STREAMING-MEMORY** (HIGH, NEW): per-request stack ≤ 4 MiB (chunker buffer 2 MiB + headroom); zero-allocation Iterator.
- **INV-CAS-INTEGRITY** (CRITICAL, registry §3.3): chunker BLAKE3 hash inline; integrity binding per chunk; manifest builder em WI-S05-005 binds tree.

TLA+ alignment: `cas_integrity.tla` chunked variant (forward S-09 TLA+ work).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate `corelink-chunker` | `crates/corelink-chunker/` | Rust |
| Chunker trait + dispatch | `crates/corelink-chunker/src/chunker/` | Rust |
| Fixed2MiB impl | `crates/corelink-chunker/src/fixed/` | Rust |
| FastCDC impl | `crates/corelink-chunker/src/fastcdc/` | Rust |
| Bounds constants | `crates/corelink-chunker/src/bounds.rs` | Rust |
| Error enum | `crates/corelink-chunker/src/error.rs` | Rust |
| Property tests | `crates/corelink-chunker/tests/prop_chunker.rs` | Rust |
| Mann-Whitney timing tests | `crates/corelink-chunker/tests/timing_chunker.rs` | Rust |
| Criterion benchmarks | `crates/corelink-chunker/benches/chunker_bench.rs` | Rust |
| Cargo-fuzz harness | `crates/corelink-chunker/fuzz/fuzz_targets/` | Rust |
| Test vectors Annex | `crates/corelink-chunker/spec/test_vectors_chunker.md` + `tests/fixtures/` | Markdown + JSON |
| Spec doc | `crates/corelink-chunker/spec/chunker_protocol.md` | Markdown |
| README | `crates/corelink-chunker/README.md` | Markdown |
| Examples | `crates/corelink-chunker/examples/` (4 examples) | Rust |
| ADR-0022 ratificada | `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md` | Markdown |
| ADR-0039 | `specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s05.002.1** `#![forbid(unsafe_code)]`; zero `unwrap` em src/.
- **14.s05.002.2** rustdoc 100% public API + 4 examples + threat model README + spec/chunker_protocol.md.
- **14.s05.002.3** Test coverage ≥ 95% (cripto boundary).
- **14.s05.002.4** Latência: chunker p99 throughput ≥ 500 MB/s native (recalibrated Lote 10.5bis) (Fixed); ≥ 1.5 GB/s (FastCDC).
- **14.s05.002.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings`; cargo-fuzz 1h CI nightly (2 targets).
- **14.s05.002.6** Métricas: 4 listadas §6.1.12; alert if dedup_ratio < 1.2× sustained 7d.
- **14.s05.002.7** Public API stability: `#[non_exhaustive]`; semver post v1.0; ADR-0039.
- **14.s05.002.8** REAPI v2 spec compliance: chunker output integrates com REAPI manifest.
- **14.s05.002.9** Memory bounded: per-request stack ≤ 4 MiB; zero-allocation Iterator.
- **14.s05.002.10** Cost regression gate: per-op ≤ $0.000001.

## 15. Chaos Experiments

1. **Determinism regression** (chaos PR introduces non-determinism): property test prop_fixed_determinism + prop_fastcdc_determinism catch; CI red.

2. **FastCDC mask seeds drift** (chaos PR changes seeds): test vectors Annex assert byte-equal; CI red.

3. **DoS via crafted FastCDC input** (anchor every byte): bounded MAX_CHUNKS_PER_BLOB rejects; chaos test 100 crafted inputs.

4. **Streaming memory exhaustion** (1 TiB synthetic input): BlobTooLarge error at 160 GiB; integration test 1 GiB no OOM.

5. **BLAKE3 SIMD panic on misaligned buffer**: cargo-fuzz harness 1h validates 0 panics; chaos test pathological alignments.

6. **Iterator lifetime issue** (consumer stores Chunk past next feed): borrow checker compile-time enforce; chaos test asserts compile failure.

7. **Throughput regression** (chaos PR optimizes wrong path): criterion bench detects; CI gate.

8. **FastCDC bounds violation** (config min > avg): FastCDCConfigInvalid error; integration test.

9. **Cargo-fuzz crash detected**: 1h fuzz nightly; immediate fix + regression test.

10. **Final partial chunk handling**: 1 byte blob → 1 chunk of 1 byte via finalize(); INV-CAS-IDEMPOTENCY preserved.

11. **Public API breaking change** (non-additive enum variant): cargo-semver-checks CI gate detects; PR red sem ADR.

## 16. PRR

PRR HIGH_RISK 13 sign-offs gated em WI-S05-006. Este WI mini-PRR Architect + **Crypto SME mandatory emphatic**.

- [ ] All Gherkin green.
- [ ] Property + Mann-Whitney + chaos green.
- [ ] Criterion benchmarks green.
- [ ] Cargo-fuzz 1h CI nightly green.
- [ ] Test vectors Annex published.
- [ ] Determinism property tests green (both algos).
- [ ] ADR-0022 ratificada; ADR-0039 published.
- [ ] Crypto SME independent BLAKE3 + FastCDC determinism review.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate skeleton + Cargo.toml + module structure | 1.5h |
| ST-002 | Fixed2MiB chunker impl (streaming Iterator + BLAKE3 inline) | 4h |
| ST-003 | FastCDC chunker impl (Gear hash + mask seeds + bounds) | 6h |
| ST-004 | Bounds + ChunkerConfig validation | 1.5h |
| ST-005 | Streaming Iterator zero-allocation (lifetime + backpressure) | 3h |
| ST-006 | Property tests (5 properties × 10k iter) | 4h |
| ST-007 | Mann-Whitney 3-prong test (timing across distributions) | 3h |
| ST-008 | Criterion benchmarks (3 benches) | 3h |
| ST-009 | Cargo-fuzz harness (2 targets) | 2.5h |
| ST-010 | Test vectors Annex (50 inputs Fixed + 50 FastCDC) | 5h |
| ST-011 | rustdoc + 4 examples + threat model README | 4h |
| ST-012 | spec/chunker_protocol.md doc | 3h |
| ST-013 | ADR-0022 ratificação (DRAFT → ACCEPTED) + rationale | 3h |
| ST-014 | ADR-0039 redação (chunker API stability + mask seeds versioning) | 2h |
| ST-015 | Crypto SME review iteration (BLAKE3 + FastCDC determinism) | 4h |
| ST-016 | Architect + AppSec review iteration | 3h |
| ST-017 | Métricas emit hooks (consumed by handler WI-S05-001) | 1.5h |
| ST-018 | Public API stability review (semver discipline) | 1.5h |
| ST-019 | Cost regression bench setup | 1h |

**Total Optimistic**: ~57h. **PERT** (O=50h, M=58h, P=88h): **~63h**.

## 18. Dependencies

### Hard blockers

- `blake3` crate (RustCrypto / BLAKE3 official; stable).
- `subtle` crate (constant-time compare; not directly used here but for any future sig).
- Crypto SME availability (mandatory emphatic; BLAKE3 + FastCDC determinism review).
- FastCDC paper (Xia 2016) reference implementation studied.

### Soft blockers

- WI-S04-003 (corelink-ac Merkle codec) consumed pattern (BLAKE3 keyed-hash em bytes) — reuse pattern.
- WI-S05-005 (manifest builder) — outbound; consumes chunker output.

### Outbound

- WI-S05-001 (handler) consumes Chunker trait.
- WI-S05-005 (manifest builder) consumes chunk digests.
- S-15 (CLI/SDK) embeds chunker for client-side dedup hints.

## 19. Effort PERT

O: 50h, M: 58h, P: 88h → PERT **63h**.

## 20. Time-boxing

**72h hard limit**. Se exceder → escalation: split em "Fixed chunker" + "FastCDC chunker" sub-WIs.

## 21. Observability

4 métricas listadas §6.1.12. Trace span (consumed by handler):
- `chunker.feed` — algo, bytes_in, chunks_emitted, duration.

Logs structured JSON (when running in handler context):
- INFO em normal chunking.
- WARN em bounds exceeded.
- ERROR em config invalid.

## 22. Cost Analysis

**Per-chunk cost** (consumed by SplitBlob handler):
- Worker CPU (BLAKE3 SIMD ~5ms per 2 MiB chunk): negligible per-chunk.
- Memory: per-request stack ≤ 4 MiB.
- Per-chunk: ~$0.0000005.

**TCO 12m projection** (1M Split/dia × ~5 chunks each = 5M chunks/dia):
- Chunker compute: 5M × $0.0000005 = $2.5/dia × 365 = **~$900/yr**.
- Effectively free; chunker is BLAKE3 hash + slicing.

**Cost regression gate**: per-op chunker ≤ $0.000001 (with 100% headroom).

**Comparison vs alternatives**:
- SHA-256 (4× slower): $3.6k/yr at same workload.
- BLAKE3 (this impl): **$900/yr**.

## 23. API Contract

Public crate API (semver post v1.0):

```rust
pub trait Chunker {
    fn feed<'a>(&'a mut self, bytes: &'a [u8]) -> Box<dyn Iterator<Item = Chunk<'a>> + 'a>;
    fn finalize<'a>(&'a mut self) -> Option<Chunk<'a>>;
    fn reset(&mut self);
}

pub struct Chunk<'a> { ... }       // borrows internal buffer; lifetime bounded
pub enum ChunkerAlgorithm { Fixed2MiB, FastCDC2MiB }    // #[non_exhaustive]
pub struct ChunkerConfig { ... }   // #[non_exhaustive]
pub enum ChunkerError { ... }
```

**Stability**: post-v1.0, traits stable; new ChunkerAlgorithm variants additive (non-breaking via `#[non_exhaustive]`).

**Versioning**: chunker protocol v1 (Fixed2MiB + FastCDC2MiB com mask seeds default); v2+ via ADR migration.

## 24. Post-mortem Hooks

- INV-CAS-IDEMPOTENCY violation (determinism regression) → CRITICAL post-mortem + revert to last-good chunker version.
- FastCDC mask seeds drift → CRITICAL post-mortem + ADR review.
- Throughput regression > 10% sustained → SEV-2 (BLAKE3 SIMD review).
- Cargo-fuzz crash → SEV-1 (decoder bug; immediate fix).
- Test vectors Annex regression → SEV-2 (compat regression).
- Public API breaking change post-v1.0 sem ADR → SEV-1 (semver discipline failure).

## 25. Rollback / Recovery

- Crate version pin via Cargo.lock; rollback via git revert + cargo update.
- Chunker protocol v1 → v2 migration: dual-chunker period (handler routes by config flag).
- RTO: ≤ 10 min.
- RPO: 0 (stateless library; no data loss).

Fallback: if chunker crash detected, handler returns 503; multipart writes pause until fix.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: not applicable (chunker is internal lib; no auth boundary).
- **Tampering**: BLAKE3-256 hash inline catches per-chunk tampering; manifest builder em WI-S05-005 binds tree.
- **Repudiation**: chunker output deterministic; same input → same chunks; auditable via test vectors Annex.
- **Information disclosure**: chunker is content-pure; no PII handling beyond passing bytes through.
- **DoS**: bounded parser MAX_BLOB_SIZE 160 GiB + MAX_CHUNKS_PER_BLOB 80000; cargo-fuzz validates.
- **Elevation of privilege**: not applicable (lib).

**LINDDUN delta**:
- **Linkability**: chunk_digest é content-hash; non-PII unless blob content has PII.
- **Identifiability**: blob content may have PII (customer responsibility); chunker is content-agnostic.
- **Non-repudiation**: deterministic chunking; reproducible.
- **Detectability**: throughput metrics; bounds exceeded counter.
- **Disclosure of information**: chunker content-pure; no logging of bytes.
- **Unawareness**: spec doc + ADR-0022 + test vectors public.
- **Non-compliance**: SLSA L3 alignment via deterministic chunking + BLAKE3 integrity.

## 27. Knowledge Transfer

- **Tech talk** (1.5h): "Streaming Chunker + FastCDC + BLAKE3 SIMD + Determinism Discipline".
- **Doc** `crates/corelink-chunker/spec/chunker_protocol.md` — canonical spec.
- **Doc** `crates/corelink-chunker/README.md` — API + threat model + algo selection guide.
- **ADR-0022** ratificada — chunk vs part decoupling.
- **ADR-0039** — chunker public API stability.
- **Workshop** (2h): com Architect + AppSec + Crypto SME + downstream WI authors.
- **Onboarding test** (5 questions): Fixed vs FastCDC trade-off, determinism rationale, mask seeds stability, BLAKE3 SIMD throughput, streaming Iterator zero-allocation.
- **External-facing**: blog post — "How CoreLink chunks blobs deterministically: BLAKE3 + FastCDC + zero-allocation streaming".

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Determinism regression (HashMap iteration) | M | L | CRITICAL | L | LOW | Property test 100k iter; CI gate; pure-functional Iterator |
| R-002 | FastCDC mask seeds drift between versions | L | L | CRITICAL | L | LOW | Fixed em Default; ADR-0022 stability; test vectors Annex |
| R-003 | DoS via crafted FastCDC input | L | M | HIGH | L | LOW | Bounded MAX_CHUNKS_PER_BLOB; bounded MAX_BLOB_SIZE; cargo-fuzz |
| R-004 | BLAKE3 SIMD panic on misaligned buffers | L | M | HIGH | L | LOW | blake3 crate handles internally; cargo-fuzz validates |
| R-005 | Streaming buffer overflow > 160 GiB | L | M | MEDIUM | L | LOW | BlobTooLarge error em feed(); integration test |
| R-006 | Iterator lifetime misuse (storing Chunk past feed) | L | L | LOW | L | LOW | Borrow checker compile-time; rustdoc warns |
| R-007 | Throughput regression > 10% | M | L | MEDIUM | L | LOW | Criterion bench CI gate |
| R-008 | Mann-Whitney CI flake | M | H | LOW | M | LOW | Šidák 3-trial gate |
| R-009 | FastCDC bounds violation (min > avg) | L | L | LOW | L | LOW | FastCDCConfigInvalid error em construction |
| R-010 | Cargo-fuzz crash detected | L | M | HIGH | L | LOW | 1h CI nightly; immediate fix |
| R-011 | Public API breaking change post-v1.0 sem ADR | L | L | HIGH | L | LOW | Semver discipline; #[non_exhaustive]; ADR-0039 |
| R-012 | BLAKE3 crate vulnerability (zero-day) | L | M | HIGH | L | LOW | Cargo-audit weekly; rapid patch cycle |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME review BLAKE3 SIMD + FastCDC algorithm + bounded parser bounds.
2. **AppSec (D+2)**: AppSec review tampering detection + bounds + memory bounds.
3. **Code (D+5)**: peer review (2 engineers).
4. **Crypto (D+6)**: Crypto SME independent review — BLAKE3 usage, FastCDC determinism, mask seeds stability, test vectors.
5. **Adversarial (pre-merge D+8)**: red team — determinism poison, DoS via crafted input, FastCDC anchor abuse.
6. **Cargo-fuzz (D+9)**: 1h fuzz validates 0 panics.
7. **PRR (D+10)**: Architect + Crypto SME mini sign-off.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; **mandatory** — bounded parser + memory bounds_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD; ideally cripto-experienced_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD; SLSA L3 alignment_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; **mandatory** — public API stability + ADR-0022/0039 ratificação_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; **mandatory** — bounded parser + cargo-fuzz harness_ | _pending_ | _pending_ |
| 13 | Crypto SME | _**MANDATORY EMPHATIC** — BLAKE3 SIMD + FastCDC determinism + mask seeds stability + test vectors review_ | _pending_ | _pending_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S05-002 (Lote 10.5); SOTA pós-Lote 10.4bis (32 seções; 13-row sign-off; 12-row risk; Mann-Whitney 3-prong; cost TCO 12m; 11 chaos experiments; STRIDE+LINDDUN delta; aplicada lições Lote 10.4bis: ADR ratificação plan, test vectors Annex, cargo-fuzz 1h, Crypto SME mandatory emphatic). |

## 32. Anti-patterns evitados

- ❌ Stateful map iteration (HashMap order non-deterministic).
- ❌ Variable mask seeds per-invocation.
- ❌ Skip BLAKE3 hash inline.
- ❌ Buffered chunker (zero-allocation Iterator).
- ❌ Unbounded recursion em FastCDC.
- ❌ Skip cargo-fuzz harness.
- ❌ Public API breaking changes post-v1.0 sem ADR.
- ❌ Custom hash function (BLAKE3-256 fixed).
- ❌ Mann-Whitney p>0.05 sozinho (3-prong gate).
- ❌ Skip determinism property test.
- ❌ Cargo-fuzz < 1h.
- ❌ Test vectors absent.

---

**Fim WI-S05-002.** Próximo: WI-S05-003 (R2 multipart adapter — init/upload-parts/complete/abort).
