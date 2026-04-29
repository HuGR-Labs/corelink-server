---
id: "WI-S05-001"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.2.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
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
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s05", "reapi", "splitblob", "spliceblob", "multipart", "chunking", "high-risk"]
---

# WI-S05-001 — REAPI v2 `SplitBlob` + `SpliceBlob` Handlers (gRPC + REST) + Tenant Context Propagation + Manifest Tree Integration + Streaming + Bounded Concurrency

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-05](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S05-001 |
| Título | REAPI `ContentAddressableStorage::SplitBlob` (server splits blob into chunks; returns manifest) + `SpliceBlob` (server reassembles chunks for legacy clients); gRPC tonic + REST axum surface; TenantCtx-only propagation; bounded concurrency per-tenant; manifest tree integration (delegate WI-S05-005); REAPI v2.3+ conformance |
| Sprint | S-05 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (handler bug → cross-tenant chunk leak via manifest = FM-303 catastrophic), FF-HR-005 (CTRL-CAS-001 distributed em tree; verify boundary) |

## 1. Intent

Implementar `crates/corelink-worker/src/reapi/cas/split_splice.rs` — handlers REAPI v2.3+ que:

```text
SplitBlob flow:
  Request gRPC (Bazel/Buck2 client) → Tower auth_stack (S-03 WI-S03-003) → TenantCtx ext →
    [1] scope_check::cache_w → 403 if missing
    [2] body decode → SplitBlobRequest (digest, size_bytes, chunking_algorithm hint)
    [3] cas::lookup_blob(tenant_id, blob_digest) → D1 SELECT cas_blobs WHERE …
    [4] r2::stream_get(tenant_prefix, blob_digest) → R2 GetObject streaming
    [5] chunker::feed(stream, algo) → corelink-chunker (WI-S05-002) Iterator<Chunk>
    [6] FOR EACH chunk:
        a. chunk_digest = BLAKE3(chunk.bytes)
        b. chunks::upsert(tenant_id, chunk_digest, size_bytes) → D1 INSERT ON CONFLICT (refcount += 1)
        c. r2::put(chunk-<region>/<tenant_prefix>/<chunk_digest>) → R2 PUT (small object) → returns ChunkPutReceipt
    [6.5] **AWAIT ALL ChunkPutReceipts** via `futures::future::try_join_all(receipts)` — Lote 10.5-tris P0-SR5-001 fix:
          - if ANY chunk PUT returns Err → abort SplitBlob with `MultipartError::ChunkPutFailed { chunk_index, source }` → 503 + Retry-After
          - manifest::build + sign NOT invoked until all receipts accepted (compile-time enforcement via ChunkPutReceipt type)
          - ChunkPutReceipt is unit struct returned by r2::put on success ONLY; absent receipt = uncommitted PUT
    [6.6] **CAPTURE created_at_ms ONCE** (Lote 10.5-tris P1-SR5-002 fix): `let created_at_ms = unix_ms_now();` BEFORE step [7]; reused on retry of step [9] without re-signing.
    [7] manifest::build(chunks_in_order, all_receipts_received) → Merkle root (delegate WI-S05-005)
    [8] manifest::sign(envelope, created_at_ms) → HKDF (delegate WI-S04-004 sig pattern); signed exactly ONCE; canonical_bytes computed once and stored em local variable
    [9] r2::put(manifest-<region>/<tenant_prefix>/<blob_digest>.json) → R2 PUT manifest envelope; on retry, reuses same envelope bytes + signature (no re-sign)
    [10] manifest_chunks::insert_batch(blob_digest, chunk_index, chunk_digest) → D1
    [11] cas_blobs::set_chunked(blob_digest, true) → D1 UPDATE
    [12] audit emit cas.split.ok (outbox; WI-S01-004 reuse)
  Response: SplitBlobResponse { manifest_digest, chunk_digests[], ... }

SpliceBlob flow (Lote 10.5-tris P0-SR5-002 fix — explicit per-chunk pipeline; sequential verify-then-write within chunk; NO unverified bytes ever reach client):
  Request gRPC → Tower auth_stack → TenantCtx ext →
    [1] scope_check::cache_r → 403 if missing
    [2] body decode → SpliceBlobRequest (manifest_digest)
    [3] manifest::lookup(tenant_id, manifest_digest) → D1 SELECT manifest_chunks ordered
    [4] manifest::verify_signature → delegate WI-S04-004-style HKDF sig (verify_full: structure + sig)
    [5] **PER-CHUNK PIPELINE (sequential within chunk; pipelined across chunks)** — Lote 10.5-tris P0-SR5-002:
        FOR i in 0..chunk_count:
          5.1) chunk_bytes_i = r2::stream_get(chunk-<region>/<tenant_prefix>/<manifest.chunks[i].digest>)  // R2 GET
          5.2) verifier.verify_chunk(i, chunk_bytes_i, manifest.chunks[i].digest)  // BLAKE3(chunk_bytes_i) == claimed_digest
               → on mismatch: cancel_token.cancel(); abort entire SpliceBlob with gRPC ABORTED OR HTTP 499; client receives exactly the i prior verified chunks + error; NO partial chunk i bytes delivered
          5.3) IF verify Ok → write chunk_bytes_i to client sink (gRPC ByteStream OR REST chunked transfer)
        Pipeline parallelism: while writing chunk N to client (5.3), chunk N+1 can be R2-GETting (5.1) AND chunk N+1 verified (5.2);
        but write to client sink for chunk i ONLY after verify(i) returns Ok. 1-chunk lookahead buffer = 2 MiB; total memory ≤ 4 MiB stack budget preserved.
    [6] response stream complete → client receives reassembled blob
    [7] audit emit cas.splice.ok
  Response: streaming bytes (REAPI v2.3+ ByteStream)
```

```rust
// Public types (corelink-worker reapi cas module)

pub trait SplitSpliceHandler {
    /// SplitBlob: server-side chunking; returns manifest digest + chunk list.
    /// Idempotent: if blob already chunked, returns existing manifest_digest.
    async fn split_blob(
        &self,
        ctx: &TenantCtx,
        req: SplitBlobRequest,
    ) -> Result<SplitBlobResponse, SplitError>;

    /// SpliceBlob: server-side reassembly; streams reassembled bytes to client.
    /// For legacy clients that don't support chunked download.
    async fn splice_blob(
        &self,
        ctx: &TenantCtx,
        req: SpliceBlobRequest,
        sink: impl Sink<Bytes>,
    ) -> Result<(), SpliceError>;
}

#[derive(thiserror::Error, Debug)]
pub enum SplitError {
    #[error("blob not found")]
    BlobNotFound,                                         // → 404 + COR_CAS_BLOB_NOT_FOUND
    #[error("blob already chunked: existing manifest {0}")]
    AlreadyChunked(ManifestDigest),                       // → 200 echo (idempotent)
    #[error("chunking algorithm unsupported: {0}")]
    UnsupportedAlgorithm(String),                         // → 422 + COR_MULTIPART_ALGO_UNSUPPORTED
    #[error("manifest verification failed")]
    ManifestInvalid,                                      // → 422 + COR_MULTIPART_MERKLE_INVALID
    #[error("blob too large for chunking: {size} > {max}")]
    BlobTooLarge { size: u64, max: u64 },                 // → 413 + COR_MULTIPART_BLOB_TOO_LARGE
    #[error("scope insufficient: required {required:?}")]
    ScopeInsufficient { required: PatScope },             // → 403 + COR_AUTH_SCOPE_INSUFFICIENT
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),                           // → 503 + COR_MULTIPART_BACKEND_UNAVAILABLE
    #[error("concurrency limit reached for tenant")]
    ConcurrencyLimitReached,                              // → 429 + COR_MULTIPART_CONCURRENCY_LIMITED
}

#[derive(thiserror::Error, Debug)]
pub enum SpliceError {
    #[error("manifest not found")]
    ManifestNotFound,                                     // → 404 + COR_MULTIPART_MANIFEST_NOT_FOUND
    #[error("manifest signature invalid")]
    SignatureInvalid,                                     // → 422 + COR_MULTIPART_SIG_INVALID
    #[error("chunk {0} not found (referenced by manifest but missing in R2)")]
    ChunkMissing(ChunkDigest),                            // → 422 + COR_MULTIPART_CHUNK_MISSING
    #[error("scope insufficient: required {required:?}")]
    ScopeInsufficient { required: PatScope },             // → 403
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),                           // → 503
}
```

**Cripto-driven invariants enforced em cada handler call**:

1. **Tenant-scoped lookup**: `cas_blobs::lookup` SQL é `WHERE tenant_id = $1 AND digest = $2` — `tenant_id` vem **EXCLUSIVAMENTE** de `TenantCtx.tenant_id` (não query param, não header, não body). Lessons learned from S-04 Lote 10.4bis: D1/SQLite has no RLS, no GUCs, no `SET LOCAL` semantic; enforcement via sqlx prepared statement compile-time tenant_id parameter binding + handler-level mandatory `WHERE tenant_id = ctx.tenant_id` clause + clippy custom lint forbidding `&str` SQL literals.
2. **Tenant-prefix derivation**: `tenant_prefix = HMAC(TDK, tenant_id)[:16]` reused via `corelink-tenant-path` (S-01 WI-S01-001); R2 paths são `chunk-<region>/<tenant_prefix>/<chunk_digest>` + `manifest-<region>/<tenant_prefix>/<blob_digest>.json`. Layer 4 da 5-Layer Defense.
3. **Idempotência**: `SplitBlob` em mesmo `(tenant_id, blob_digest)` é no-op pós-verify (returns existing manifest_digest); `cas_blobs.is_chunked = true` flag previne re-chunking.
4. **Bounded concurrency**: per-tenant semaphore limits concurrent SplitBlob calls (default 4; configurable per-tier); avoid R2 PUT storm.

**Surface dual gRPC + REST**:

- **gRPC** (primary REAPI v2.3+): `tonic::transport::Server` + `tonic-build` codegen.
- **REST** (companion): `axum::Router` `POST /v2/{instance}/blobs/{digest}/split` + `GET /v2/{instance}/manifests/{digest}/splice`.
- **Single source of truth**: handler trait impl shared (lesson learned WI-S04-001 §9.1).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

`SplitBlob` + `SpliceBlob` são REAPI v2.3+ operations que enabling **Docker layers, ML model files, binários grandes** — qualquer artefato > 5 MiB que excede single-PUT R2 limit. Multipart sem chunking = blob is opaque on R2 (no dedup); chunking + Merkle = dedup intra-tenant + integrity binding.

HIGH_RISK em N dimensões:

1. **Cross-tenant chunk leak via manifest forge**: handler reads manifest via `r2::get(manifest-<region>/<tenant_prefix>/<blob_digest>.json)`. Bug em SpliceBlob: handler usa body's `manifest_digest` para fetch sem tenant_prefix scoping → fetches manifest of OUTRO TENANT que coincidentally tem same blob_digest. **Catastrófico FM-303**. Mitigação: path scoping is **always tenant_prefix-derived from TenantCtx**; never trusts client-provided manifest_digest as-is; D1 SELECT manifest_chunks WHERE blob_digest = $1 AND tenant_id = $2 (lookup-time tenant filter).

2. **Manifest verify ausente em SpliceBlob**: cliente request SpliceBlob com `manifest_digest`; handler streams chunks BACK sem re-verifying manifest signature. If R2 envelope was tampered post-persist (insider scenario from WI-S04-003 dual-side analysis), client receives compromised reassembled blob. Mitigação: step [4] verify signature mandatory; step [5] streaming verify each chunk's hash matches manifest_chunks[i].chunk_digest (delegate WI-S05-005 verifier API).

3. **Race em SplitBlob concurrent calls** (mesma `(tenant_id, blob_digest)`): two clients trigger Split simultaneously; both fetch full blob, chunk independently; first to complete wins manifest write; second hits ON CONFLICT → idempotent return existing. BUT: chunks in R2 may have been doubly-PUT (no harm; content-addressable + ON CONFLICT em chunks table makes refcount += 1 idempotent). Mitigação: ON CONFLICT idempotent semantics; integration test concurrent.

4. **Streaming memory exhaustion**: 160 GiB blob streaming through chunker. CF Workers memory cap 128 MiB per request; blob > 128 MiB requires streaming via `Iterator<Chunk>` (zero-allocation per WI-S05-002). Mitigação: handler uses streaming pipeline (R2 GetObject streaming → chunker.feed iterator → R2 PUT chunks); never accumulates full blob in memory; integration test 1 GiB blob successful.

5. **Bounded concurrency vs DoS**: malicious client fires 10000 SplitBlob calls; each triggers full blob streaming + N R2 PUTs; R2 rate limit hit; legitimate ops queue. Mitigação: per-tenant semaphore (default 4 concurrent SplitBlob; configurable per-tier S-13 forward); 429 `COR_MULTIPART_CONCURRENCY_LIMITED` if exhausted; metric alert.

6. **REAPI v2.3+ spec compliance**: SplitBlob/SpliceBlob são REAPI v2.3 additions; older Bazel clients (< v6.5) don't speak this. Mitigação: capabilities response (`Capabilities::GetCapabilities`) declares chunking_algorithms only on v2.3+ instance; older clients fall back to direct CAS singular blob ops; documented em ADR-0022.

7. **Audit emission asymmetry**: SplitBlob emits `cas.split.ok` post-handler; SpliceBlob emits `cas.splice.ok` post-stream-complete. If client disconnects mid-splice, audit emits incomplete. Mitigação: outbox pattern; pre-stream audit `cas.splice.start`; post-complete `cas.splice.ok`; mismatch detected via correlation request_id.

8. **Chunk vs manifest race com S-06 GC**: GC runs concurrent; tombstones unreferenced chunk; SpliceBlob references that chunk via manifest. Mitigação: refcount-aware GC (S-06 forward); INV-DEDUP-CONSISTENCY (S-07); chunks marked deletion only if refcount == 0; this WI's INSERT ON CONFLICT increments refcount atomically.

**Atacante adversarial scenarios**:

- **Manifest digest collision attempt**: BLAKE3 256-bit collision-resistant 2^128; intra-tenant collision computacionalmente intratável; cross-tenant blocked by Layer 4 path scoping.

- **SpliceBlob replay** (capture manifest from one request; replay later): manifest is content-addressable; replay is idempotent (returns same blob); audit chain captures both; no security implication.

- **DoS via SplitBlob storm of small blobs**: 10000 × 5 MiB blobs → 10000 × 3 chunks each = 30000 R2 PUTs. R2 rate limit. Mitigação: per-tenant concurrency cap + S-08 forward rate limit.

- **Crafted oversize manifest** (manifest claims 1B chunks): D1 manifest_chunks INSERT batch = OOM. Mitigação: bounded MAX_CHUNKS_PER_MANIFEST = 80000 (160 GiB / 2 MiB) constant; reject if exceed; CHECK constraint em D1 (WI-S05-004 schema).

**Risk justification HIGH_RISK**:

- **FF-HR-002**: handler bug em manifest path scoping = cross-tenant chunk leak FM-303 catastrophic.
- **FF-HR-005**: CTRL-CAS-001 distributed em tree; verify boundary; bug = security control bypass.
- **Reversibility**: chunked blobs cannot trivially un-chunk; rollback via manifest invalidation + re-upload required.
- **Customer impact**: Bazel/Buck2 v2.3+ clients require SplitBlob for > 5 MiB blobs; broken handler = customer build fails.

11 sign-offs canonical incl. Architect (REAPI conformance + Crypto SME specialization for Merkle BLAKE3), AppSec (5-layer scoping for chunks).

## 3. Customer Impact & Journey

**Persona 1 — Bazel CI dev pushing Docker layers (50 MiB each)**:
- `bazel build //:image` produces image; `--remote_cache=corelink://...` triggers SplitBlob.
- Handler chunks layer (50 MiB / 2 MiB = 25 chunks); manifest stored.
- Cache hit ratio improves: similar layer changes = chunk dedup; only delta uploaded.
- Customer-visible: builds 50%+ faster on iterative Docker layer changes.

**Persona 2 — ML CI dev pushing model files (5 GiB)**:
- `bazel build //:model` produces large model file; SplitBlob splits into 2500 chunks.
- Streaming upload + verify; 5 GiB takes ~50s at 100 MB/s baseline.
- Customer-visible: cost savings via chunk dedup across model versions.

**Persona 3 — Compliance reviewer (SLSA Level 3 supply chain)**:
- Reviews manifest's signed envelope; chunks individually content-addressed; manifest binds order.
- Reviews dual-side verify discipline (server pre-persist + client post-download via SDK).

**SLA addendum**:
- SplitBlob throughput ≥ 100 MB/s steady (sprint contract §14).
- Manifest verify p99 ≤ 50ms for trees ≤ 80k chunks.
- Per-tenant concurrency: 4 default; tunable per-tier S-13 forward.
- 5xx backend (R2 down) → 503 `COR_MULTIPART_BACKEND_UNAVAILABLE` + Retry-After 5s.
- 160 GiB single multipart limit; > 160 GiB requires stitched flow (WI-S05-006).

## 4. Capability Mapping

- **CAP-CAS-008** (Multipart upload) — IMPLEMENTA primary (handler orchestration).
- **CAP-CAS-009** (Merkle chunking) — IMPLEMENTA partial (delegates WI-S05-002 chunker + WI-S05-005 verifier).
- **CAP-CAS-010** (Intra-tenant dedup) — IMPLEMENTA via chunks UPSERT refcount (delegates WI-S05-004 schema).
- **CAP-CAS-011** (REAPI SplitBlob + SpliceBlob) — IMPLEMENTA primary.
- Trace: `remote_cache_product_profile.md §6` + `cas_profile.md §4 (Merkle decomposition)` + `auth_model.md §8.1 (Layer 4 path scoping)`.

## 5. Tipo

REAPI handler (gRPC + REST); HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-worker/src/reapi/cas/split_splice.rs`** — handler module:
   - `SplitSpliceHandler` trait + `SplitSpliceHandlerImpl` struct.
   - gRPC tonic service impl (REAPI v2.3+ `ContentAddressableStorage::SplitBlob/SpliceBlob`).
   - REST axum routes:
     - `POST /v2/{instance}/blobs/{digest}/split` → SplitBlob.
     - `GET /v2/{instance}/manifests/{digest}/splice` → SpliceBlob (streaming response).
   - Single trait impl shared between gRPC + REST.
   - **TenantCtx propagation**: `req.extensions().get::<TenantCtx>()` mandatory.

2. **REAPI v2.3+ proto codegen**:
   - `build.rs` invoca `tonic_build::compile_protos` em vendored proto (shared com WI-S04-001 build.rs).
   - SplitBlob + SpliceBlob proto definitions.
   - Bazel client version compat: 7+ (REAPI v2.3+ adopted).

3. **SplitBlob flow** (steps [0]..[12] vide §1):
   - Step [0] **scope_check**: `PatScope::CacheW` mandatory.
   - Step [3] **cas::lookup_blob**: D1 SELECT cas_blobs WHERE tenant_id = $1 AND digest = $2; missing → 404.
   - Step [3.5] **idempotency check**: if `cas_blobs.is_chunked = true`, return existing manifest_digest (D1 manifest_chunks lookup); 200 echo.
   - Step [4] **r2::stream_get**: R2 GetObject streaming via `aws-sdk-s3-compatible` async stream.
   - Step [5] **chunker::feed** (delegate WI-S05-002): Iterator<Chunk> zero-allocation.
   - Step [6] **chunks UPSERT**:
     - For each chunk: BLAKE3 hash → upsert `chunks(tenant_id, chunk_digest, r2_object_key, size_bytes, refcount)` ON CONFLICT (tenant_id, chunk_digest) DO UPDATE SET refcount = refcount + 1.
     - Parallel R2 PUT chunks (bounded concurrency 8 per blob).
   - Step [7] **manifest::build** (delegate WI-S05-005): Merkle root from chunk digests in order.
   - Step [8] **manifest::sign** (Lote 10.5bis P0 fix: explicit API; was hand-wave "delegate WI-S04-004 sig pattern" without API surface): handler invokes WI-S05-005 `ManifestSignatureVerifier::sign_manifest(canonical_bytes(envelope))` which wraps WI-S04-004 HKDF primitives with `info=b"manifest-sig"` domain-separated from `b"ac-sig"` (WI-S04-004 hard-codes `b"ac-sig"`; manifest-sig wrapper provides separate domain without modifying upstream). canonical_bytes layout published em WI-S05-005 §1 (102 bytes; binds tenant_id + blob_digest + merkle_root + chunk_count + total_size_bytes + created_at_ms + chunker_algo).
   - Step [9] **r2::put manifest**: envelope JSON at `manifest-<region>/<tenant_prefix>/<blob_digest>.json`.
   - Step [10] **manifest_chunks INSERT batch**: D1 INSERT INTO manifest_chunks (blob_digest, chunk_index, chunk_digest, tenant_id) batch (capped 250 rows/batch per Lote 10.4bis D1 100KB limit lesson).
   - Step [11] **cas_blobs UPDATE**: SET is_chunked = true.
   - Step [12] **audit emit**: `cas.split.ok` event into outbox (WI-S01-004 audit_outbox table reuse).
   - Response: SplitBlobResponse { manifest_digest, chunk_digests[], chunk_count, total_size }.

4. **SpliceBlob flow** (streaming):
   - Step [0] **scope_check**: `PatScope::CacheR` mandatory.
   - Step [3] **manifest::lookup**: D1 SELECT manifest_chunks WHERE blob_digest = $1 AND tenant_id = $2 ORDER BY chunk_index ASC.
   - Step [4] **ManifestSignatureVerifier::verify_full** (Lote 10.5bis P0 fix): handler invokes WI-S05-005 `ManifestVerifier::verify_full(manifest, &manifest_sig_verifier)` which ALWAYS calls `verify_structure()` BEFORE `verify_sig()` (forced ordering by API design; `verify_structure` is `pub(crate)` not exposed). Mismatch (structure OR sig) → 422 + audit emit.
   - Step [5] **streaming reassembly with progressive verify** (Lote 10.5bis P0 fix: pinned position; previously contradictory across §1, §6.1.4, §6.2):
     - Handler creates `tokio_util::sync::CancellationToken`.
     - Handler invokes WI-S05-005 `ManifestVerifier::verify_streaming(manifest, chunk_stream, cancel_token)` em parallel with R2 chunk streaming.
     - Verifier per-chunk hash verify in stream (BLAKE3 incremental); on mismatch chunk N → cancels token → upstream R2 stream aborted before chunk N+1 read; verifier returns `Err(StreamingChunkMismatch { index: N })`.
     - Forward bytes to client sink via gRPC `Stream<ByteStream>` OR REST chunked transfer encoding only after per-chunk verify passes.
     - **Verifier impl** lives em WI-S05-005 (handler invokes); **invocation site + cancel_token wiring** lives em this WI.
     - WI-S05-001 ↔ WI-S05-005 SEAL coupling: both must SEAL together for INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST.
   - Step [6] **audit emit**: `cas.splice.ok` post-stream-complete; `cas.splice.error` if mid-stream fail.
   - Response: streaming bytes total = `cas_blobs.size_bytes`.

5. **Bounded concurrency per-tenant**:
   - `tokio::sync::Semaphore::new(4)` per-tenant SplitBlob concurrency (default; tunable).
   - Failure: 429 `COR_MULTIPART_CONCURRENCY_LIMITED` + Retry-After.
   - Métrica `corelink.multipart.concurrency.{used, max}{tenant_id}`.

6. **Streaming pipeline correctness**:
   - R2 GetObject → chunker.feed → R2 PUT chunks: zero-allocation via `Iterator<Chunk>`.
   - Backpressure: chunker drains slower than R2 GET → R2 stream paused.
   - Memory bound: per-request stack ≤ 4 MiB (chunker buffer 2 MiB + headroom).

7. **Métricas** (RED + multipart-specific):
   - `corelink.multipart.split.requests_total{result}` (counter; result ∈ ok|already_chunked|not_found|too_large|backend_unavailable|concurrency_limited|merkle_invalid).
   - `corelink.multipart.splice.requests_total{result}`.
   - `corelink.multipart.split.duration_ms_bucket{path}` (histogram; path ∈ stream|chunk|manifest_build|d1_batch).
   - `corelink.multipart.splice.duration_ms_bucket{path}`.
   - `corelink.multipart.throughput_bytes_per_sec` (gauge; rolling 1-min average).
   - `corelink.multipart.dedup_ratio{type=chunk}` (gauge; for CAP-CAS-010 business metric).
   - `corelink.multipart.bounds_exceeded_total{kind}` (alert if > 0; max chunks per manifest, etc.).

8. **Tracing**:
   - Span `multipart.split` com attributes: `tenant_id` (UUIDv7), `blob_digest_prefix` (16 hex), `result`, `chunks.count`, `total_size_bytes`, `manifest_digest_prefix`.
   - Span `multipart.splice` com attributes: `manifest_digest_prefix`, `chunks.count`, `bytes_streamed`, `verify_ok`.

9. **Property tests** (10k iter PR; 100k nightly):
   - `prop_split_tenant_isolation`: 1000 random (tenant_a, blob_digest) + 1000 (tenant_b, ...); SplitBlob tenant_b never references tenant_a's chunks (INV-AC-TENANT-SCOPED analog).
   - `prop_split_idempotent`: 1000 random blobs; Split + Split = same manifest_digest; `is_chunked` flag idempotent.
   - `prop_splice_verify_match`: 1000 random manifests; Splice reassembles bytes-equal to original blob.
   - `prop_chunk_refcount_consistent`: 1000 ops; refcount in `chunks` table matches actual references in `manifest_chunks`.
   - `prop_splice_chunk_missing_rejected`: 100 manifests with 1 chunk tombstoned mid-flight; reject 100% with COR_MULTIPART_CHUNK_MISSING.

10. **Mann-Whitney timing test** (3-prong, extending S-04 methodology):
    - Goal: cliente cannot distinguish "blob_not_found" vs "manifest_invalid" via timing.
    - Both paths return errors; 10k samples per arm; power 1−β ≥ 0.80 com Cohen's d = 0.2.
    - Šidák 3-trial gate; combined α ≈ 0.000125.
    - |Δmedian| ≤ 5ms (middleware-grade, not cripto-grade tighter 0.5ms; rationale: handler response time variability above sig path).

11. **Conformance test integration**:
    - `tests/conformance/reapi_v2_split_splice.rs` → invoke bazelbuild/remote-apis test suite SplitBlob/SpliceBlob subset.
    - CI gate em nightly.

12. **Integration test E2E**:
    - Real Bazel client (v7+) + real PAT + staging.
    - Flow: 1 GiB blob upload via SplitBlob → manifest stored → SpliceBlob downloads back; bytes-equal.
    - Flow: 50 MiB Docker layer × 3 versions (delta 10%); chunk dedup ratio ≥ 1.5×.

13. **REST surface details**:
    - `POST /v2/{instance}/blobs/{digest}/split` → JSON body `SplitBlobRequest`; 200 JSON `SplitBlobResponse`.
    - `GET /v2/{instance}/manifests/{digest}/splice` → response: chunked transfer encoding bytes.
    - `Content-Type: application/octet-stream` for splice response.
    - REST mainly for dashboard/CLI; gRPC primary for Bazel/Buck2 v2.3+.

### 6.2 Out-of-scope (deferred)

- **Chunker impl** (fixed 2 MiB + FastCDC opt-in): WI-S05-002.
- **R2 multipart adapter** (init/upload-parts/complete/abort): WI-S05-003.
- **D1 schema** (chunks + manifest_chunks + multipart_sessions): WI-S05-004.
- **Manifest builder/verifier**: WI-S05-005.
- **Sweeper cron DO + RB-FM-060**: WI-S05-006.
- (Lote 10.5bis P0 fix: removido contradição "partial in this WI; full em WI-S05-005"). Streaming progressive verify: **invocation site + cancel_token wiring** lives em this WI; **verifier impl + per-chunk BLAKE3 verify** lives em WI-S05-005. Both WIs SEAL together for INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST.
- **Pre-fetch chunk hot tier** (cache popular chunks): pós-GA.
- **Resumable upload**: anti-scope (sprint contract §10).

## 7. Anti-Scope

- ❌ `tenant_id` from request body/query/header (TenantCtx-only; lesson WI-S04-001).
- ❌ Skip manifest verify in SpliceBlob (fail-closed mandatory).
- ❌ Block GET on chunk drift (warn-only via reconcile S-06; lesson WI-S04-001 §9.4).
- ❌ Sync emit audit em hot path (outbox pattern via WI-S01-004).
- ❌ Variable-time tenant compare (subtle::ConstantTimeEq).
- ❌ Multi-tenant manifest_digest namespace (UNIQUE per `(tenant_id, blob_digest)`).
- ❌ Custom proto schema (use REAPI v2.3+ vendored proto).
- ❌ gRPC + REST duplicate impl (single trait).
- ❌ Drop SpliceBlob streaming verify (lesson WI-S04-003 §9.4 dual-side).
- ❌ Skip bounded concurrency per-tenant (DoS surface).
- ❌ `with_tenant_ctx!` claims em D1 (Postgres-only; lesson Lote 10.4bis).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: REAPI SplitBlob + SpliceBlob handlers

  Background:
    Given Tenant A has active PAT corelink_pat_xyz with scopes cache_rw
    Given Tenant A has TenantCtx propagated through Tower auth_stack
    Given chunks + manifest_chunks + multipart_sessions D1 schema deployed (WI-S05-004)
    Given R2 buckets chunk-sam + manifest-sam exist
    Given corelink-chunker (WI-S05-002) + manifest builder (WI-S05-005) available

  Scenario: SplitBlob happy path (10 MiB blob)
    Given Tenant A has CAS blob D (10 MiB) at cas-<region>/<tenant_prefix>/<digest>
    When client gRPC SplitBlob(digest=D)
    Then handler: scope_check → lookup → stream_get → chunker.feed (5 chunks of 2 MiB) → chunks UPSERT (refcount tracking) → manifest build → sig sign → r2 put manifest → manifest_chunks insert → cas_blobs is_chunked = true → audit emit
    And response 200 SplitBlobResponse with manifest_digest, chunk_digests[5]
    And metric corelink.multipart.split.requests_total{result="ok"} incremented
    And p99 latency ≤ 1s for 10 MiB blob

  Scenario: SplitBlob idempotent (already chunked)
    Given Tenant A has D with is_chunked = true and existing manifest M
    When client SplitBlob(digest=D) again
    Then handler short-circuits at step [3.5]
    And response 200 SplitBlobResponse with manifest_digest = M (echo)
    And NO re-chunking
    And metric corelink.multipart.split.requests_total{result="already_chunked"} incremented

  Scenario: SplitBlob tenant isolation
    Given Tenant A has D
    Given Tenant B authenticated; same digest D (different content)
    When Tenant B calls SplitBlob(digest=D)
    Then handler lookup is WHERE tenant_id = TenantCtx_B AND digest = D
    And lookup returns NULL (Tenant A's D invisible)
    And response 404 (existence not leaked)
    And property test prop_split_tenant_isolation green

  Scenario: SplitBlob blob too large (> 160 GiB)
    Given blob D size 200 GiB
    When client SplitBlob(digest=D)
    Then response 413 + COR_MULTIPART_BLOB_TOO_LARGE
    And error_body { size: 200 GiB, max: 160 GiB }
    And next_action: "Use stitched multipart flow (WI-S05-006)"

  Scenario: SplitBlob concurrency limit reached
    Given 4 ongoing SplitBlob calls for Tenant A (semaphore exhausted)
    When 5th SplitBlob arrives
    Then response 429 + COR_MULTIPART_CONCURRENCY_LIMITED + Retry-After
    And metric corelink.multipart.concurrency.exhausted_total{tenant_id} incremented

  Scenario: SpliceBlob happy path
    Given Tenant A has manifest M referencing 5 chunks
    When client gRPC SpliceBlob(manifest_digest=M)
    Then handler: scope_check → manifest lookup → sig verify → stream chunks in order → per-chunk hash verify → response stream
    And response streams bytes back; total = original blob size
    And audit emit cas.splice.ok

  Scenario: SpliceBlob signature invalid
    Given manifest M with tampered envelope (sig verify fails)
    When SpliceBlob(M)
    Then response 422 + COR_MULTIPART_SIG_INVALID
    And audit emit cas.splice.sig_invalid (CRITICAL)
    And NO chunks streamed back

  Scenario: SpliceBlob chunk missing (S-06 GC race)
    Given manifest M references chunk C; C tombstoned by S-06 GC
    When SpliceBlob(M); R2 GET chunk C returns 404
    Then handler fail-fast at chunk N
    And response 422 + COR_MULTIPART_CHUNK_MISSING
    And audit emit cas.splice.chunk_missing
    And metric corelink.multipart.bounds_exceeded_total incremented

  Scenario: Chunk dedup intra-tenant
    Given Tenant A SplitBlob D1 (10 MiB; 5 chunks: c1..c5)
    Given Tenant A SplitBlob D2 (12 MiB; chunks: c1, c2, c6, c7, c8, c9) — first 4 MiB identical to D1
    When SplitBlob D2 completes
    Then chunks c1, c2 refcount = 2 (incremented via ON CONFLICT)
    And chunks c6, c7, c8, c9 refcount = 1 (new)
    And cas_blobs(D2).is_chunked = true with manifest M2
    And dedup_ratio = (chunks unique used) / (chunks distinct) = 9 / 9 — OR refined ratio (chunks reused / chunks total) = 2/(5+6) ≈ 0.18
    And metric corelink.multipart.dedup_ratio{type=chunk} reflects

  Scenario: Streaming throughput 100 MB/s
    Given 1 GiB blob ready
    When SplitBlob streaming
    Then total time ≤ 10s (100 MB/s)
    And memory bound ≤ 4 MiB stack throughout

  Scenario: Mann-Whitney timing — blob_not_found vs manifest_invalid indistinguishable
    Given 10k requests blob_digest absent
    Given 10k requests manifest_digest invalid sig
    When latencies collected per arm
    And Mann-Whitney U applied with power analysis
    Then p > 0.05 com Šidák 3-trial
    And |Δmedian| ≤ 5ms com 95% CI cruzando 0

  Scenario: Backend unavailable (R2 down)
    Given R2 GetObject returns 5xx
    When client SplitBlob(D)
    Then response 503 + COR_MULTIPART_BACKEND_UNAVAILABLE + Retry-After 5s
    And metric corelink.multipart.split.requests_total{result="backend_unavailable"} incremented

  Scenario: gRPC + REST surface parity
    Given same TenantCtx + same digest
    When request via gRPC SplitBlob vs REST POST /v2/.../split
    Then both surfaces return identical SplitBlobResponse (semantically equivalent)
    And both surfaces hit same handler trait impl

  Scenario: REAPI conformance suite passes
    Given staging environment with WI-S05-001..006 deployed
    When bazelbuild/remote-apis test suite SplitBlob/SpliceBlob subset runs
    Then 100% pass
    And conformance report in CI artifact
```

## 9. Design Decisions

### 9.1 Why SplitBlob server-side (não client-side)

REAPI v2.3+ defines SplitBlob as **server-side operation**: client uploads full blob via standard PUT; server splits + manifest. Alternative (client-side chunking + ManifestPut) would require client SDK; REAPI v2.3+ chose server-side for backward-compat.

### 9.2 Why TenantCtx-only tenant_id source (lesson WI-S04-001)

Body/query/header é attacker-controlled; using as auth fonte = TOCTOU. TenantCtx é built post-auth-verify; immutable; única source authentified. Lessons learned from S-04 Lote 10.4bis: this pattern is canonical for all WIs.

### 9.3 Why chunk size 2 MiB (default; ADR-0022)

Trade-off:
- 1 MiB: more dedup granularity but 2× R2 PUT overhead.
- 4 MiB: less dedup, less overhead.
- 2 MiB: balance dedup vs ops cost (Buildbarn precedent; FastCDC paper).
- Configurable post-S-15 customer SDK.

### 9.4 Why bounded concurrency 4 per-tenant (default)

Avoid R2 PUT storm; per-tenant fairness; tunable per-tier S-13 forward (free=2, business=8, enterprise=16).

### 9.5 Why manifest sig HKDF info=`b"manifest-sig"` (different from `b"ac-sig"`)

Domain separation: prevents sig of manifest from being valid as AC envelope sig (cross-domain replay). Lesson learned from WI-S04-004 ADR-0021.

### 9.6 Why streaming pipeline (não buffered)

160 GiB blob requires streaming; CF Workers memory cap 128 MiB; buffered impossible. Iterator<Chunk> zero-allocation; backpressure native.

### 9.7 Why D1 manifest_chunks INSERT batch capped 250 rows (lesson Lote 10.4bis)

D1 batch 100KB limit; each manifest_chunks row ~80 bytes; 250 rows × 80 = 20KB safe. Larger manifests inserted via multiple batches.

### 9.8 Why audit outbox via WI-S01-004 (lesson Lote 10.4bis citation fix)

audit_outbox table is in WI-S01-004 schema (NOT WI-S01-005 BatchUpdateBlobs handler). Single source of truth.

### 9.9 Why no `with_tenant_ctx!` claim (lesson Lote 10.4bis)

D1/SQLite has no RLS, no GUCs, no `SET LOCAL`. Enforcement via sqlx prepared statement + handler discipline + Layer 4 HMAC path scoping.

### 9.10 ADR potencial?

Sim — **ADR-0038**: "SplitBlob/SpliceBlob handler invariants: TenantCtx-only; bounded concurrency 4/tenant default; streaming pipeline; manifest sig HKDF info=`manifest-sig` separate domain; idempotency via is_chunked flag." Whitelist em validate_references.py; ratificada em WI-S05-006 ship gate.

## 10. Completeness Criteria SOTA

- [ ] **10.s05.001.1** Property tests 10k iter (PR) + 100k nightly → 0 panics, 0 false-accepts (EVT-002):
  - prop_split_tenant_isolation, prop_split_idempotent, prop_splice_verify_match, prop_chunk_refcount_consistent, prop_splice_chunk_missing_rejected.
- [ ] **10.s05.001.2** Mann-Whitney U + power analysis 3-prong em not_found vs manifest_invalid timing (EVT-002):
  - N ≥ 10000 samples per arm; power 1−β ≥ 0.80 com Cohen's d = 0.2; Šidák 3-trial; |Δmedian| ≤ 5ms.
- [ ] **10.s05.001.3** REAPI v2.3+ conformance suite (bazelbuild/remote-apis) — SplitBlob/SpliceBlob subset 100% green nightly (EVT-002 + EVT-018).
- [ ] **10.s05.001.4** Throughput SplitBlob ≥ 100 MB/s steady em staging (sprint contract §14.s05.1) (EVT-021).
- [ ] **10.s05.001.5** TenantCtx propagation type-system enforced (handler signature `&TenantCtx`; clippy lint forbids `tenant_id: TenantId` parameter em handler module) (EVT-002).
- [ ] **10.s05.001.6** Streaming memory bound: per-request stack ≤ 4 MiB; integration test 1 GiB blob no OOM (EVT-021).
- [ ] **10.s05.001.7** gRPC + REST surface parity test: 100 random blobs via both surfaces; identical responses.
- [ ] **10.s05.001.8** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.s05.001.9** Cost regression gate: per-op cost ≤ $0.000020 (Split p99) + $0.000010 (Splice p99) (Lote 9.4 §14.10).
- [ ] **10.s05.001.10** Integration E2E: Bazel v7+ client + real PAT + staging green em CI nightly.
- [ ] **10.s05.001.11** OWASP API Security Top 10 checklist pass.
- [ ] **10.s05.001.12** Bounded concurrency 4/tenant test: 5th request → 429 + Retry-After.

## 11. DoD

- [ ] `crates/corelink-worker/src/reapi/cas/split_splice.rs` compila + integration tests green.
- [ ] gRPC tonic service + REST axum routes both functional.
- [ ] Single `SplitSpliceHandlerImpl` trait shared.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k iter green em CI; 100k nightly green.
- [ ] Mann-Whitney 3-prong test green.
- [ ] REAPI conformance suite 100% SplitBlob/SpliceBlob subset green em CI nightly.
- [ ] gRPC + REST parity test green.
- [ ] Throughput benchmark ≥ 100 MB/s em staging.
- [ ] Métricas (7 listadas §6.1.7) emitted; dashboard widget.
- [ ] Trace span `multipart.split` + `multipart.splice` em OTel pipeline.
- [ ] rustdoc 100% public API + 4 examples (basic Split, basic Splice, dedup demo, idempotency demo).
- [ ] ADR-0038 published (forward; ratificada em WI-S05-006).
- [ ] Architect + AppSec + Security Lead reviews.
- [ ] PRR Architect mini sign-off.
- [ ] Cost regression gate green.

## 12. Invariants Validated

- **INV-CAS-INTEGRITY** (CRITICAL, registry §3.3): manifest_digest binds chunk tree; verify mandatory pre-persist + post-download.
- **INV-CAS-IDEMPOTENCY** (CRITICAL, registry §3.3): same body + same chunker config → same manifest_digest.
- **INV-TENANT-ISOLATION** (CRITICAL, registry §3.3): `chunks` UNIQUE `(tenant_id, chunk_digest)`; manifest_chunks tenant-scoped via blob_digest lookup.
- **INV-MULTIPART-IDEMPOTENT** (HIGH, NEW — promovida registry §3.16 Lote 10.5bis): SplitBlob mesma `(tenant_id, blob_digest)` retorna existing manifest; `is_chunked` flag previne re-chunking.
- **INV-MULTIPART-MANIFEST-SIGNED** (HIGH, NEW): manifest envelope signed com HKDF info=`b"manifest-sig"` separado domain `b"ac-sig"`.
- **INV-MULTIPART-CONCURRENCY-BOUNDED** (HIGH, NEW): per-tenant semaphore caps SplitBlob concurrency (default 4); 429 if exhausted.
- **INV-AUTH-TENANTCTX-IMMUTABLE** (CRITICAL, registry §3.14): TenantCtx propagation; reuse S-03 invariant.

TLA+ alignment: `cas_integrity.tla` chunked-blob variant (forward S-09 TLA+ work).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Split/Splice handler | `crates/corelink-worker/src/reapi/cas/split_splice.rs` | Rust |
| Handler trait + Impl | `crates/corelink-worker/src/reapi/cas/handler.rs` | Rust |
| gRPC tonic wrapper | `crates/corelink-worker/src/reapi/cas/grpc.rs` | Rust |
| REST axum wrapper | `crates/corelink-worker/src/reapi/cas/rest.rs` | Rust |
| Property tests | `crates/corelink-worker/tests/prop_split_splice.rs` | Rust |
| Mann-Whitney timing tests | `crates/corelink-worker/tests/timing_split_splice.rs` | Rust |
| E2E integration | `tests/e2e_bazel_split_splice.rs` | Rust |
| Conformance harness | `tests/conformance/reapi_v2_split_splice.rs` | Rust |
| ADR-0038 | `specs/03_architecture/adrs/ADR-0038-split-splice-handler-invariants.md` | Markdown |
| Examples | `crates/corelink-worker/examples/multipart/` (4 examples) | Rust |
| OWASP API Top 10 checklist | `specs/_audits/2026-XX-XX-owasp-api-top10-multipart.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s05.001.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s05.001.2** rustdoc 100% public API + 4 examples + threat model README section.
- **14.s05.001.3** Test coverage ≥ 95% (security boundary).
- **14.s05.001.4** Latência: SplitBlob 10 MiB p99 ≤ 1s; Splice p99 (1 GiB) ≤ 10s; trace overhead ≤ 5%.
- **14.s05.001.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings`; cargo-fuzz target em SplitBlobRequest deserializer 1h CI nightly.
- **14.s05.001.6** Métricas: 7 listadas §6.1.7; alert if backend_unavailable > 5% sustained.
- **14.s05.001.7** Runbook: RB-FM-060 (multipart orphan) consumed by WI-S05-006 sweeper.
- **14.s05.001.8** REAPI v2.3+ spec compliance: vendored proto pinned commit; conformance suite green.
- **14.s05.001.9** Memory bounded: per-request stack ≤ 4 MiB (chunker buffer 2 MiB + headroom); zero-allocation iterator.
- **14.s05.001.10** Cost regression gate: Split ≤ $0.000020; Splice ≤ $0.000010; per-op CI bench.

## 15. Chaos Experiments

1. **R2 outage 1h during SplitBlob**: simulate R2 5xx mid-stream; verify graceful 503 + customer retry; no D1 inconsistency. Hypothesis: rollback via `is_chunked` flag NOT set; chunks may be partial in R2 (cleaned by sweeper).

2. **D1 timeout during chunks UPSERT batch**: simulate D1 503 mid-batch; verify atomic rollback via outbox pattern; retry idempotent.

3. **Cross-tenant attempt via crafted body**: red team sends SplitBlob with `request.metadata.tenant_id = "B"` while authenticated as A; handler ignores body field; D1 query uses TenantCtx.tenant_id (A). Property test asserts.

4. **Concurrency limit storm**: 1000 concurrent SplitBlob; verify semaphore caps at 4; queue rest with 429; metric alert.

5. **Manifest tampering** (post-persist): R2 manifest envelope flipped 1 byte; SpliceBlob verify_signature catches. Hypothesis: 100% detection.

6. **Chunk missing mid-stream** (S-06 GC race): chunk C tombstoned mid-Splice; handler fail-fast at chunk N; partial response with error trailer (gRPC).

7. **Streaming memory bound**: 5 GiB blob; assert per-request stack ≤ 4 MiB throughout; valgrind/MSAN no leak.

8. **REAPI conformance regression**: chaos PR introduces drift; CI nightly catches.

9. **Idempotent re-Split storm**: 1000 req/s same `(tenant, digest)`; verify is_chunked flag prevents re-chunking; no duplicate chunks in R2.

10. **gRPC + REST surface drift**: chaos PR fixes bug in REST handler but not gRPC; parity test catches.

11. **Bazel client v6.5 (pre-v2.3) attempts SplitBlob**: handler returns capabilities mismatch; client falls back to direct PUT.

12. **Audit emission gap mid-stream**: simulate handler crash mid-Splice; verify outbox already emitted `cas.splice.start`; reconcile catches missing `cas.splice.ok`.

## 16. PRR

PRR HIGH_RISK 11 sign-offs canonical gated em WI-S05-006 ship gate. Este WI mini-PRR Architect + AppSec + Security Lead.

- [ ] All Gherkin green.
- [ ] Property + Mann-Whitney + chaos green.
- [ ] REAPI conformance 100% green nightly.
- [ ] E2E Bazel v7+ staging green.
- [ ] Throughput ≥ 100 MB/s sustained.
- [ ] Cost regression gate green.
- [ ] ADR-0038 published.
- [ ] OWASP API Top 10 100%.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | REAPI v2.3+ proto vendoring (shared with WI-S04-001) | 1.5h |
| ST-002 | SplitSpliceHandler trait + Impl skeleton | 3h |
| ST-003 | gRPC tonic service wrapper | 2h |
| ST-004 | REST axum routes wrapper | 2h |
| ST-005 | SplitBlob flow steps [0]..[12] impl (streaming pipeline) | 6h |
| ST-006 | SpliceBlob flow streaming + per-chunk verify | 4h |
| ST-007 | Bounded concurrency semaphore per-tenant | 2h |
| ST-008 | chunks UPSERT refcount logic | 2h |
| ST-009 | manifest_chunks INSERT batch (250 cap) | 2h |
| ST-010 | Métricas emit (7 metrics) + trace spans | 2.5h |
| ST-011 | Property tests 10k iter (5 properties) | 4h |
| ST-012 | Mann-Whitney 3-prong test impl | 3h |
| ST-013 | gRPC + REST parity test | 2h |
| ST-014 | Conformance harness (bazelbuild/remote-apis SplitBlob subset) | 4h |
| ST-015 | Chaos suite (12 scenarios) | 4h |
| ST-016 | E2E integration test (real Bazel + staging + 1 GiB) | 4h |
| ST-017 | rustdoc + 4 examples + threat model README | 3h |
| ST-018 | ADR-0038 redação | 2h |
| ST-019 | Architect + AppSec review iteration | 3h |
| ST-020 | OWASP API Top 10 self-checklist + audit | 2h |
| ST-021 | Cost regression bench setup | 2h |

**Total Optimistic**: ~60h. **PERT** (O=52h, M=62h, P=92h): **~65h**.

## 18. Dependencies

### Hard blockers

- WI-S03-003 (Tower auth_stack + TenantCtx propagation) SEALED.
- WI-S03-002 (corelink-pat scopes cache_r/cache_w) SEALED.
- WI-S05-002 (corelink-chunker) SEALED.
- WI-S05-003 (R2 multipart adapter) SEALED.
- WI-S05-004 (D1 schema chunks + manifest_chunks + multipart_sessions) SEALED.
- WI-S05-005 (manifest builder/verifier) SEALED.
- WI-S01-001 (corelink-tenant-path) SEALED (tenant_prefix derivation).
- WI-S01-004 (audit_outbox table) SEALED (lesson Lote 10.4bis: WAS S01-005, é S01-004).

### Soft blockers

- WI-S05-006 (sweeper + RB-FM-060 + ship gate) — outbound; this WI provides handlers, WI-006 wraps em ship gate.

### Outbound

- WI-S05-006 consumes handler trait for conformance test invocations.
- S-06 GC reconcile consumes manifest_chunks reachability.
- S-07 INV-DEDUP-CONSISTENCY consumes refcount semantics.
- S-15 CLI/SDK consume gRPC client.

## 19. Effort PERT

O: 52h, M: 62h, P: 92h → PERT **65h**.

## 20. Time-boxing

**76h hard limit**. Se exceder → escalation: split em "core handler" + "conformance integration" sub-WIs.

## 21. Observability

7 métricas listadas §6.1.7. Trace spans:
- `multipart.split` com attributes: `tenant_id` (UUIDv7), `blob_digest_prefix` (16 hex), `result`, `chunks.count`, `total_size_bytes`, `manifest_digest_prefix`, `path` (stream|chunk|manifest_build|d1_batch).
- `multipart.splice` com attributes: `manifest_digest_prefix`, `chunks.count`, `bytes_streamed`, `verify_ok`.

Logs structured JSON; INFO em ok; WARN em concurrency_limited; ERROR em sig_invalid/backend_unavailable.

Dashboard DASH-MULTIPART (partial; full em WI-S05-006 + S-09):
- Split/Splice rate per result.
- Throughput rolling 1-min average.
- Dedup ratio per tenant_tier.
- Concurrency exhaustion rate.
- Sig invalid counter (alert if > 0 sustained — cripto incident).

## 22. Cost Analysis

**Per-request breakdown** (SplitBlob 10 MiB blob; 5 chunks):
- Worker invocation + streaming: ~1ms CPU per chunk × 5 = 5ms; ~$0.0000005.
- R2 GetObject (10 MiB stream): $0.36/M class A + $0/GB CF egress.
- R2 PUT × 5 chunks (2 MiB each): $4.5/M × 5 = $22.5/M = $0.0000225.
- R2 PUT × 1 manifest envelope: $4.5/M = $0.0000045.
- D1 SELECT + UPDATE + INSERT batch (chunks UPSERT × 5 + manifest_chunks INSERT × 5 + cas_blobs UPDATE): ~$0.000005.
- Audit outbox INSERT: ~$0.50/M.
- Per-Split (10 MiB): ~$0.000033.

**Per-request breakdown** (SpliceBlob 10 MiB blob; 5 chunks):
- Worker invocation: $0.50/M.
- D1 SELECT manifest_chunks: $1/M.
- R2 GET manifest envelope + sig verify: $0.36/M + ~1ms CPU.
- R2 GET × 5 chunks streaming: $0.36/M × 5 = $1.8/M = $0.0000018.
- Per-Splice (10 MiB): ~$0.000005.

**TCO 12m projection** (1M Split/dia + 10M Splice/dia at 10 MiB average; Bazel layered workload):
- Split: 1M × $0.000033 = $33/dia.
- Splice: 10M × $0.000005 = $50/dia.
- Total: ~$83/dia × 365 = **~$30k/yr**.
- R2 storage (chunks): ~10 GB × $0.015/GB/mo = $1.8/yr (small chunks dedup'd; mostly metadata cost).

**Cost regression gate**: Split ≤ $0.000040 (with 20% headroom over $0.000033); Splice ≤ $0.000010 (with 100% headroom over $0.000005).

**Comparison vs alternatives**:
- BuildBuddy multipart: similar per-op cost; less dedup (no intra-tenant chunk reuse) → 2× R2 storage.
- bazel-remote: no chunking; 100% R2 storage redundancy; 5-10× more.
- CoreLink S-05: **$30k/yr** at 1M Split + 10M Splice/dia + dedup ratio ≥ 1.5× = ~$20k effective.

## 23. API Contract

**gRPC** (REAPI v2.3+):
```proto
service ContentAddressableStorage {
  rpc SplitBlob(SplitBlobRequest) returns (SplitBlobResponse);
  rpc SpliceBlob(SpliceBlobRequest) returns (stream SpliceBlobResponse);
}
```

**REST**:
```
POST /v2/{instance}/blobs/{digest}/split → 200 SplitBlobResponse | 404 | 413 | 422 | 429 | 503
GET  /v2/{instance}/manifests/{digest}/splice → streaming response | 404 | 422 | 503
```

HTTP error mapping:
- `Blob not found` → 404 `COR_CAS_BLOB_NOT_FOUND`.
- `Already chunked` → 200 echo (idempotent).
- `Manifest invalid` → 422 `COR_MULTIPART_MERKLE_INVALID`.
- `Sig invalid` → 422 `COR_MULTIPART_SIG_INVALID` (CRITICAL audit).
- `Chunk missing` → 422 `COR_MULTIPART_CHUNK_MISSING`.
- `Blob too large (>160 GiB)` → 413 `COR_MULTIPART_BLOB_TOO_LARGE`.
- `Concurrency limited` → 429 `COR_MULTIPART_CONCURRENCY_LIMITED` + Retry-After.
- `Algorithm unsupported` → 422 `COR_MULTIPART_ALGO_UNSUPPORTED`.
- `Scope insufficient` → 403 `COR_AUTH_SCOPE_INSUFFICIENT`.
- `Backend unavailable` → 503 `COR_MULTIPART_BACKEND_UNAVAILABLE` + Retry-After.

## 24. Post-mortem Hooks

- Cross-tenant chunk leak detected (FM-303) → CRITICAL post-mortem + Privacy + breach notification.
- Manifest sig invalid sustained > 5/h → CRITICAL (suspected tampering campaign).
- Throughput < 50 MB/s sustained 7d → SEV-2 (workload tuning).
- INV-CAS-IDEMPOTENCY violation com FastCDC enabled → SEV-1 (determinism review).
- REAPI conformance regression → post-mortem + Bazel community engagement.
- Concurrency limit hit > 50% requests sustained → SEV-2 (workload outpaces; tier upgrade discussion).
- Dedup ratio < 1.2× sustained 7d → SEV-3 (workload analysis; chunker tuning).

## 25. Rollback / Recovery

Hot rollback via Wrangler `wrangler deploy --version-id <prev>`. Handler rollback:
- Chunked blobs created during bad version: idempotent re-Split returns existing manifest_digest; no duplicates.
- Manifest sig invalidations: customer must re-execute (re-upload original blob); audit emit captures.
- Audit emission outbox: drain worker continues; pre-rollback emissions delivered.
- RTO: ≤ 10 min.
- RPO: 0 (stateless handler; D1 + R2 source-of-truth survive).

Fallback: handler 503 if R2 OR D1 down; Bazel client falls back to direct PUT (graceful, expected REAPI behavior; v7+ supports both).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: TenantCtx-only tenant_id source (lesson WI-S04-001); manifest sig binds tenant_id em canonical_bytes.
- **Tampering**: Merkle dual-side verify (server pre-persist + client post-download via SDK); HKDF sig (CTRL-AC-002 pattern reused) detects envelope tampering; ON CONFLICT idempotent semantics preserve invariants.
- **Repudiation**: outbox audit emission (cas.split.ok / cas.splice.ok / sig_invalid); reuse WI-S03-007 chain integrity.
- **Information disclosure**: Mann-Whitney constant-time not_found vs manifest_invalid (404/422 paths timing-indistinguishable < 5ms |Δmedian|); chunks UNIQUE tenant-scoped; manifest envelope tenant-scoped path.
- **DoS**: bounded concurrency 4/tenant; max chunks/manifest 80k; max blob size 160 GiB single (stitched > 160 GiB); R2 rate limit absorbed via semaphore.
- **Elevation of privilege**: scope check Layer 3 (cache_r vs cache_w); 5-layer defense Layer 4 (HMAC path); no admin-style override em this WI.

**LINDDUN delta**:
- **Linkability**: tenant_id UUID v7 pseudonymous; chunk_digest content-hash (non-PII).
- **Identifiability**: blob content may contain PII (Docker layer with secrets); customer responsability primary; redact policy via S-09 macro.
- **Non-repudiation**: append-only audit chain.
- **Detectability**: timing constant via Mann-Whitney 3-prong; tampering detected via sig::verify; INV-DEDUP-CONSISTENCY reconcile S-07 forward.
- **Disclosure of information**: chunks bucket private (CORS empty; CI cron audit); manifest path tenant-scoped.
- **Unawareness**: SLA addendum + REAPI conformance customer-verifiable; ADR-0038 documents handler invariants.
- **Non-compliance**: LGPD Art. 38 + GDPR Art. 32 satisfied via tenant scoping + audit + DSR S-11 forward.

## 27. Knowledge Transfer

- **Tech talk** (1.5h): "REAPI SplitBlob/SpliceBlob + Streaming Pipeline + Manifest Tree Integration".
- **Doc** `docs/internal/multipart-handler.md` — sequence diagrams (Split happy, Split idempotent, Splice happy, Splice sig invalid, Splice chunk missing).
- **Doc** `docs/internal/multipart-tenant-isolation.md` — TenantCtx-only enforcement + chunks UNIQUE tenant-scoped.
- **ADR-0038** — handler invariants design rationale.
- **Workshop** (2h): com Architect + AppSec + Security Lead + downstream WI authors (WI-S05-002..006).
- **Onboarding test** (5 questions): TenantCtx propagation, streaming pipeline rationale, chunk size 2 MiB rationale, manifest sig domain separation, bounded concurrency rationale.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Cross-tenant chunk leak via manifest path bug | L | L | CRITICAL | L | LOW | TenantCtx-only enforcement; clippy lint; property test 100k; chaos test #3 |
| R-002 | Manifest sig forge via cripto break (BLAKE3 + HKDF) | L | L | CRITICAL | L | LOW | 256-bit MAC PRF-secure 2^128; ADR-0021/0038 documents |
| R-003 | Streaming memory exhaustion (160 GiB blob) | L | M | HIGH | L | LOW | Iterator<Chunk> zero-allocation; per-request stack ≤ 4 MiB; integration test 1 GiB |
| R-004 | Concurrency limit too tight (customer false-positive 429) | M | L | MEDIUM | L | LOW | Default 4 tunable per-tier S-13; metric alert; customer feedback iterate |
| R-005 | REAPI v2.3+ spec drift (Bazel 8 schema delta) | M | M | MEDIUM | M | LOW | Conformance suite nightly CI; vendored proto pinned; ADR-0038 documents bump policy |
| R-006 | gRPC + REST surface drift | M | L | LOW | L | LOW | Single trait impl; parity test 100 random ops; CI gate |
| R-007 | Mann-Whitney CI flake (1-em-20 false positive) | M | H | LOW | M | LOW | Šidák 3-trial gate; combined α ≈ 0.000125 |
| R-008 | R2 partial outage cascades para Split | M | H | HIGH | M | LOW | 503 graceful + Retry-After; sweeper aborts mid-flight; metric alert |
| R-009 | D1 batch atomicity violation (chunks UPSERT partial fail) | L | M | HIGH | L | LOW | D1 batch atomic; chaos test; reconcile via S-06 |
| R-010 | Idempotent re-Split storm (re-fetch full blob) | M | L | MEDIUM | L | LOW | is_chunked flag short-circuits at step [3.5]; no re-streaming |
| R-011 | Splice chunk missing race (S-06 GC mid-flight) | M | M | HIGH | M | LOW | Fail-fast at chunk N; 422 customer-actionable; refcount-aware GC S-06 forward |
| R-012 | Cost regression > 10% per-op | M | L | MEDIUM | L | LOW | §14.10 cost gate; weekly bench |
| R-013 | Audit emission gap mid-Splice (handler crash) | L | M | MEDIUM | L | LOW | outbox pre-stream + post-stream emits; correlation request_id; reconcile catches |
| R-014 | TenantId newtype enforcement bypassed via refactor | L | L | CRITICAL | L | LOW | Type-system enforcement via private constructor (lesson Lote 10.4bis WI-S04-001); CI gate clippy lint |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Security Lead review handler trait + 5-layer enforcement + TenantCtx-only pattern.
2. **AppSec (D+2)**: AppSec review tampering detection + sig integration boundary + audit emission ordering.
3. **Code (D+5)**: peer review (2 engineers).
4. **Crypto (D+6)**: Crypto SME review manifest sig domain separation (`b"manifest-sig"` info string).
5. **Adversarial (pre-merge D+8)**: red team — body tenant_id confusion attempts, manifest forge, concurrency storm, idempotency edge cases.
6. **Conformance (D+9)**: Bazel community engagement; REAPI v2.3+ conformance suite green em CI nightly.
7. **PRR (D+10)**: Architect mini sign-off (full ship gate em WI-S05-006).

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; **mandatory** — TenantCtx-only enforcement + 5-layer scoping_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD; PII redaction policy review (blob content metadata)_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; **mandatory** — REAPI conformance + handler trait composability_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — tampering detection + 5-layer enforcement + concurrency bounds_ | _pending_ | _pending_ |
| 13 | Crypto SME | _**mandatory** — manifest sig domain separation `b"manifest-sig"` review (pattern reuse from WI-S04-004 ADR-0021)_ | _pending_ | _pending_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S05-001 (Lote 10.5); SOTA pós-Lote 10.4bis (32 seções; 13-row sign-off; 14-row risk; Mann-Whitney 3-prong; cost TCO 12m; 12 chaos experiments; STRIDE+LINDDUN delta full; aplicada lições Lote 10.4bis: TenantCtx-only canonical, audit_outbox WI-S01-004, sig domain separation, no with_tenant_ctx claims). |

## 32. Anti-patterns evitados

- ❌ tenant_id from request body/query/header (TenantCtx-only).
- ❌ Skip manifest verify em SpliceBlob (fail-closed mandatory).
- ❌ Sync emit audit em hot path (outbox via WI-S01-004).
- ❌ Block Splice em chunk drift (warn-only S-06; fail-closed apenas if missing pre-Splice).
- ❌ Variable-time tenant compare (subtle).
- ❌ Multi-tenant manifest_digest namespace (UNIQUE per `(tenant_id, blob_digest)`).
- ❌ Custom proto schema (REAPI v2.3+ vendored).
- ❌ gRPC + REST duplicate impl (single trait).
- ❌ Buffered streaming (zero-allocation Iterator).
- ❌ Skip bounded concurrency per-tenant (DoS surface).
- ❌ Mann-Whitney p>0.05 sozinho (3-prong gate).
- ❌ Manifest sig info=`ac-sig` (cross-domain replay; use `b"manifest-sig"`).
- ❌ `with_tenant_ctx!` claim em D1 (Postgres-only; lesson Lote 10.4bis).

---

**Fim WI-S05-001.** Próximo: WI-S05-002 (corelink-chunker crate — fixed 2 MiB + FastCDC opt-in + ADR-0022).
