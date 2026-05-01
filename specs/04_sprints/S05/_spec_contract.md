---
id: "SPEC-CONTRACT-S05"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.4.0"
created: "2026-04-24"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s05", "multipart", "chunking", "merkle", "fastcdc", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-05: Multipart Upload + Chunking + Merkle Trees (Buildbarn-style)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-05 |
| Nome | Multipart Upload + Chunking + Merkle (blobs > 5 MiB) |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-005 (CTRL-CAS-001 distribuído em tree), FF-HR-002 (chunks compartilhados intra-tenant; vazamento se index não scopeado) |
| Duração estimada | 3 semanas + buffer 5 dias |
| WIs antecipados | 6 |
| SOTA target | Multipart blobs até 5 TiB + Merkle dual-side + dedup intra-tenant ≥ 1.5× workload sintético + throughput ≥ 100 MB/s steady |

## 1. Objetivo

Estender CAS para **blobs > 5 MiB até 5 TiB** via R2 multipart upload + decomposição Merkle (Buildbarn-style: chunks 2 MiB + manifest tree). Cliente upload streaming com hash inline; server valida cada chunk + manifest tree completo; dedup de chunks cross-blob dentro do tenant (CAP-DEDUP-001 foundation para S-07). Destrava cache de **Docker layers, ML model files, binários grandes** (qualquer artefato > 5 MiB que é cap mínimo R2 multipart).

**Por que SOTA:** competitors (BuildBuddy, NativeLink) implementam multipart com fixed-size 4 MiB chunks sem dedup intra-tenant; trees Merkle não verified end-to-end; orphan multipart parts accumulam (R2 cost overhead). CoreLink S-05 entrega: (a) FastCDC content-defined chunking opt-in (Xia 2016) — better dedup ratio em payloads similar; (b) **Merkle dual-side verify**; (c) **orphan multipart sweeper** PAT-SWEEPER-001; (d) **chunks tenant-scoped** UNIQUE index (foundation para S-07 INV-DEDUP-CONSISTENCY); (e) clarification **chunk size vs multipart part size** (codex finding S-05:55 vs S-05:94 contradiction). Reference: **FastCDC paper (Xia et al., USENIX 2016)**, **Buildbarn manifest format**, **R2 multipart API limits**, **Merkle 1979** original paper.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (11 sign-offs canonical per framework §33.5.4.3 + ADR-0034; Crypto SME folds into Architect; DBA folds into Architect; AppSec é o 11º slot).
- **FF-HR-005**: integridade agora distribuída em tree — bug em 1 chunk compromete blob inteiro.
- **FF-HR-002**: chunks compartilhados intra-tenant — vazamento se chunk index não scopeado.

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"  # multipart semantics
  - "SECURITY-MODEL"                # CTRL-CAS-001
  - "DATA-MODEL"                    # chunks, manifest_chunks schemas
  - "INVARIANT-REGISTRY"            # INV-CAS-INTEGRITY, INV-CAS-IDEMPOTENCY, INV-TENANT-ISOLATION
  - "STORAGE-SEMANTICS-MATRIX"      # multipart life cycle
  - "RESILIENCE-PATTERNS"           # PAT-SWEEPER-001
  - "OBSERVABILITY-MODEL"           # SLO-LAT-CAS-PUT-MULTIPART
  - "FAILURE-MODES"                 # FM-060 (multipart orphan)
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-CAS-008** | Multipart upload (blobs 5 MiB – 5 TiB) | R2 multipart orchestration; streaming. |
| **CAP-CAS-009** | Merkle chunking | Default 2 MiB fixed-size + final partial; FastCDC opt-in (ADR-0022). |
| **CAP-CAS-010** | Intra-tenant chunk dedup | UNIQUE `(tenant_id, chunk_digest)`; foundation para S-07 INV-DEDUP-CONSISTENCY. |
| **CAP-CAS-011** | REAPI SplitBlob + SpliceBlob | REAPI v2 chunking semantics. |
| **CAP-CAS-012** | Orphan multipart cleanup | Sweeper cron DO (PAT-SWEEPER-001); abort > 7d. |
| **CAP-CAS-013** | Chunk size vs part size separation | **Lote 9.4 codex S-05:55/94 finding fix**: chunk size = 2 MiB content; multipart part size = 16 MiB R2 (8 chunks/part); decoupled per ADR-0022. |

## 5. Requirements específicos

### 5.1 Chunking + Merkle (CAP-CAS-009 + CAP-CAS-013)

- **R-S05-1**: Crate `corelink-chunker` com:
  - **Default**: fixed-size 2 MiB chunks (content) + final partial.
  - **Opt-in**: FastCDC content-defined chunking (ADR-0022) — better dedup ratio em payloads similar (e.g., Docker layers).
  - Streaming API: `chunker.feed(bytes) -> impl Iterator<Chunk>`; zero allocation hot path.
- **R-S05-2**: Manifest tree builder/verifier:
  - Manifest = Merkle tree of chunk digests (BLAKE3 root).
  - Builder em-streaming (chunks chegam, tree builds incremental).
  - Verifier rejects invalid trees pre-persist.
- **R-S05-3**: **Chunk size vs multipart part size separation** (Lote 9.4 codex fix):
  - **Chunk size**: 2 MiB (content addressing unit; dedup granularity).
  - **R2 multipart part size**: 16 MiB (R2 minimum 5 MiB; we batch 8 chunks per part; reduces multipart API overhead).
  - **R2 max parts**: 10,000 (R2 hard limit) → max blob = 10k × 16 MiB = **160 GiB max single multipart upload**.
  - **Beyond 160 GiB**: chunked upload via multiple multipart sessions stitched via manifest (rare; >160 GiB Docker layers ou ML models gigantes).
  - Documented em `data_model.md §4.3` (multipart semantics).

### 5.2 R2 Orchestration (CAP-CAS-008 + CAP-CAS-012)

- **R-S05-4**: R2 multipart initialize/upload-parts/complete orchestration:
  - `InitiateMultipartUpload` returns `upload_id`.
  - `UploadPart` per 16 MiB part; ETag tracking.
  - `CompleteMultipartUpload` com part list ordenada.
  - `AbortMultipartUpload` em failure path.
- **R-S05-5**: Sweeper cron DO (PAT-SWEEPER-001): abort multipart sessions > 7d:
  - List ongoing via `ListMultipartUploads`.
  - Abort + audit emit `corelink.multipart.orphan_aborted`.

### 5.3 Schema (CAP-CAS-010)

- **R-S05-6**: D1 schema:
  - `chunks` table: `(tenant_id, chunk_digest, r2_object_key, size_bytes, refcount)` — UNIQUE `(tenant_id, chunk_digest)`.
  - `manifest_chunks` table: `(blob_digest, chunk_index, chunk_digest)` ordered.
  - `cas_blobs.is_chunked` bool flag.
  - `multipart_sessions` table: `(upload_id, tenant_id, blob_digest_expected, started_at, last_activity_at)`.

### 5.4 REAPI Surface (CAP-CAS-011)

- **R-S05-7**: REAPI `ContentAddressableStorage::SplitBlob` + `SpliceBlob` handlers conformance.

## 6. Definition of Done

- [ ] **WIs SEALED**: 6/6 (EVT-031).
- [ ] **Upload 1 GiB single blob** bem-sucedido; hash verify end-to-end (EVT-018).
- [ ] **Dedup**: mesmo chunk usado em 2 blobs distintos dentro do tenant; refcount += 1 (EVT-002).
- [ ] **Orphan parts**: abort testado chaos-style — induce client disconnect mid-upload → sweeper aborts em 7d (EVT-023).
- [ ] **Property test 100k**: manifest verify rejeita árvores inválidas (EVT-002).
- [ ] **SLO-LAT-CAS-PUT-MULTIPART** definido (novo SLO em SLO-CATALOG) + sustained (EVT-021).
- [ ] **PRR HIGH_RISK 11 sign-offs canonical** (per framework §33.5.4.3 + ADR-0034): Owner + Final Approver + Architect (Crypto SME specialization for Merkle BLAKE3 + FastCDC determinism + DBA specialization for D1 schema sizing) + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance + Privacy + AppSec advisor (EVT-031).
- [ ] **Throughput** ≥ 100 MB/s steady em staging (EVT-021).
- [ ] **Dedup ratio** measurable em workload sintético; baseline ≥ 1.5× (EVT-021).
- [ ] **Chunk vs part size** documentação clarification em `data_model.md §4.3` + ADR-0022 (EVT-046 if Legal needed; EVT-027).
- [ ] **160 GiB max single multipart** documentação + > 160 GiB stitched flow E2E test (EVT-018).
- [ ] **RB-FM-060** (multipart orphan) dry-run executado (criar runbook stub se ausente) (EVT-017).
- [ ] **Cost regression gate** (§14.10): multipart hot path benchmark; PR > 10% regression bloqueia.

## 7. Completeness Criteria (delta local)

- [ ] **10.s05.1** Throughput upload ≥ 100 MB/s steady em staging (EVT-021).
- [ ] **10.s05.2** Dedup ratio mensurável: `corelink_dedup_ratio{type=chunk}` ≥ 1.5× sintético (EVT-021).
- [ ] **10.s05.3** Zero orphan multipart > 7d em staging durante 72h (EVT-023).
- [ ] **10.s05.4** **160 GiB single multipart** test pass + > 160 GiB stitched flow.
- [ ] **10.s05.5** **Chunk-vs-part decoupling** documented em ADR-0022.
- [ ] **10.s05.6** **FastCDC opt-in** disponível via flag; default fixed 2 MiB.

## 8. Invariants

### Mantidas

- **INV-CAS-INTEGRITY** (CRITICAL, TLA+): cada chunk hash verifies; manifest hash = Merkle root; tree invalid rejected.
- **INV-CAS-IDEMPOTENCY** (CRITICAL): same body → same chunking → same manifest digest (FastCDC adds rolling-hash anchor determinism).
- **INV-TENANT-ISOLATION** (CRITICAL): chunks index é tenant-scoped UNIQUE `(tenant_id, chunk_digest)`; cross-tenant impossible.

## 9. Quality Standards (delta local)

- **14.s05.1 Zero memory allocation** em hot path chunker (streaming via `Iterator`).
- **14.s05.2 Multipart parts ≤ 10,000** por blob (R2 hard limit); 16 MiB part size = 160 GiB blob max single session.
- **14.s05.3 Cost regression gate** (§14.10): multipart hot path benchmark; PR > 10% regression bloqueia.
- **14.s05.4 Streaming verify**: manifest tree verify pode ser progressive (não esperar todo blob); fail-fast em invalid chunk.
- **14.s05.5 BLAKE3 throughput**: chunker hot path SIMD-optimized; benchmark ≥ 2 GB/s single core.

## 10. Anti-scope

- ❌ Cross-tenant dedup (S-07 + ADR).
- ❌ Compression (fora de escopo S-05; Zstd considerado mas deferred — pós-GA decision).
- ❌ Encryption-at-write per chunk (envelope encryption é BYOK S-14; S-05 trabalha com plaintext em transit + R2 SSE).
- ❌ Variable chunk size > 16 MiB (excede R2 part minimum benefit; complexity sem ganho).
- ❌ Erasure coding chunks (Reed-Solomon; over-engineering at GA).
- ❌ Resumable upload (mid-upload restart) — anti-scope para GA; restart full multipart.
- ❌ Chunk size customer-configurable per blob — single 2 MiB default at GA.

## 11. Dependencies

### Hard blockers

- **S-01 SEALED** (CAS single-blob foundation).
- **S-02 SEALED** (read path para chunks).

### Soft blockers

- **S-09** (DASH-CAS extension; ok defer).

### Outbound

- **S-06** (GC tem que entender manifest_chunks reachability).
- **S-07** (INV-DEDUP-CONSISTENCY foundation; uses chunks UNIQUE constraint).
- **S-15** (CLI `corelink put` for large blobs).
- **S-20** (GA exige throughput sustained + 0 orphan).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S05-001** | REAPI SplitBlob + SpliceBlob handlers | gRPC + REST handlers; auth middleware; tenant context | 10h | 14h | 22h | **14.7h** |
| **WI-S05-002** | Crate corelink-chunker (fixed 2 MiB + FastCDC opt-in) + ADR-0022 | chunker scaffold; fixed-size impl; FastCDC opt-in; ADR-0022 chunk vs part decoupling | 14h | 22h | 36h | **23.0h** |
| **WI-S05-003** | R2 multipart adapter (init/parts/complete/abort) | R2 SDK integration; ETag tracking; abort path; chaos test client disconnect | 12h | 18h | 30h | **19.0h** |
| **WI-S05-004** | D1 schema chunks + manifest_chunks + multipart_sessions + UNIQUE constraint | schema migration; UNIQUE `(tenant_id, chunk_digest)`; manifest_chunks; multipart_sessions; integration test | 10h | 14h | 22h | **14.7h** |
| **WI-S05-005** | Merkle manifest builder/verifier dual-side + property test 100k invalid trees | builder streaming; verifier; pre-persist reject; client-side verify; property test | 14h | 22h | 36h | **23.0h** |
| **WI-S05-006** | Sweeper cron DO + RB-FM-060 dry-run + PRR + 160 GiB stitched flow | sweeper logic; abort > 7d; RB stub; PRR; > 160 GiB stitched test | 12h | 18h | 30h | **19.0h** |

**Total PERT:** ~113h ≈ 14 dias work × 1 eng. Buffer 5 dias confere com 3 semanas.

## 13. Duração + Timeline

- **Duração:** 3 semanas (15 dias úteis) + buffer 5 dias.
- **Marcos:**
  - **D+3:** WI-001 + WI-004 SEALED (handlers + schema).
  - **D+8:** WI-002 SEALED (chunker + ADR-0022).
  - **D+11:** WI-003 + WI-005 SEALED (R2 adapter + Merkle).
  - **D+15:** WI-006 SEALED (sweeper + RB + PRR).
  - **D+17:** Sprint review.

## 14. Critérios de promoção

- DoD complete.
- SLO-LAT-CAS-PUT-MULTIPART green sustained.
- Dedup ratio ≥ 1.5× workload sintético.
- 160 GiB single + > 160 GiB stitched flows tested.
- ADR-0022 ratificado.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido — 6 colunas)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Multipart orphan accumulation** (FM-060) | M | M | MEDIUM | M | LOW | Sweeper cron DO 7d + RB-FM-060 + cost monitoring R2 ongoing parts. |
| **Manifest verify lento** pra trees grandes | M | L | MEDIUM | L | LOW | Streaming verify (fail-fast); BLAKE3 SIMD throughput ≥ 2 GB/s; benchmark CI. |
| **Chunk size escolha trade-off errada** (dedup vs overhead) | M | L | MEDIUM | L | LOW | 2 MiB default justified ADR-0022; FastCDC opt-in para Docker layers; iterate post-launch. |
| **R2 multipart API quirk** (max parts, eventual consistency) | M | M | HIGH | M | LOW | R2 hard limit 10k parts respected; documented; CompleteMultipart consistency check. |
| **Chunk vs part size confusion** (codex S-05:55/94 contradiction) | L | L | MEDIUM (DX) | L | LOW (Lote 9.4 fix) | ADR-0022 explicit decoupling; data_model.md §4.3 docs; clarified em §4 CAP-CAS-013. |
| **FastCDC determinism breaks INV-CAS-IDEMPOTENCY** | L | M | HIGH | L | LOW | FastCDC rolling-hash deterministic per spec; property test 10k same input → same chunks. |
| **Streaming chunker memory leak** | L | L | MEDIUM (FM-403) | L | LOW | Zero-allocation Iterator pattern; valgrind/MSAN CI; trimestral container restart. |
| **Cross-tenant chunk leak** via UNIQUE bug | L | M | CRITICAL (FM-303) | L | LOW | INV-TENANT-ISOLATION + UNIQUE `(tenant_id, chunk_digest)` + property test 100k. |

## 16. Benchmarks SOTA externos

| Critério | BuildBuddy | NativeLink | bazel-remote | Buildbarn | **CoreLink target S-05** |
|---|---|---|---|---|---|
| Multipart blobs até 5 TiB | Yes | Yes | Yes | Yes | **Yes — 160 GiB single + stitched** |
| FastCDC opt-in | No | No | No | Yes | **Yes — opt-in via flag (ADR-0022)** |
| Merkle dual-side verify | Server only | Server only | Server only | Server only | **Yes — server + client** |
| Intra-tenant dedup | Limited | No | No | Yes | **Yes — UNIQUE (tenant_id, chunk_digest)** |
| Orphan multipart sweeper | Manual | Manual | Manual | Yes | **Yes — PAT-SWEEPER-001 cron DO** |
| Chunk vs part size separation | Conflated | Conflated | Conflated | Decoupled | **Decoupled (ADR-0022)** |
| Throughput target | 50-100 MB/s | 100-200 MB/s | 50-100 MB/s | 200+ MB/s | **≥ 100 MB/s steady** |
| BLAKE3 SIMD optimized | Yes | Yes | No | Yes | **Yes — ≥ 2 GB/s single core** |

## 17. References (RFCs, papers, standards)

- **Merkle 1979** — A digital signature based on a conventional encryption function.
- **FastCDC paper** — Xia et al., USENIX ATC 2016.
- **BLAKE3 Specification** + SIMD optimizations.
- **R2 Multipart API** documentation <https://developers.cloudflare.com/r2/api/s3/multipart-uploads/>.
- **Buildbarn manifest format** <https://github.com/buildbarn/bb-storage>.
- **REAPI v2 SplitBlob/SpliceBlob** <https://github.com/bazelbuild/remote-apis>.
- ADR-0022 (chunk vs part size — criar durante S-05).
- `specs/03_architecture/error_taxonomy.md` — `COR_MULTIPART_*` error codes.

## 18. Post-mortem hooks

- Cross-tenant chunk leak detected → CRITICAL post-mortem + Privacy + breach notification consideration.
- Multipart orphan > 30 days sem auto-abort → 5-Why + sweeper review.
- Manifest invalid persisted (escape from dual-side verify) → CRITICAL post-mortem.
- INV-CAS-IDEMPOTENCY violation com FastCDC → post-mortem + determinism review.
- R2 multipart API quirks novos → post-mortem + Cloudflare engagement.

## 19. Waiver policy

S-05 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ INV-CAS-INTEGRITY Merkle dual-side verify — security baseline.
- ❌ INV-TENANT-ISOLATION chunks UNIQUE constraint — security baseline.
- ❌ ADR-0022 chunk vs part decoupling — DX clarity baseline.
- ❌ Sweeper PAT-SWEEPER-001 7d abort — cost/operational baseline.

Itens waivable com Architect + ADR:

- ⚠️ Throughput 100 MB/s → 80 MB/s com plan to optimize (BLAKE3 SIMD review).
- ⚠️ FastCDC opt-in → defer pós-GA Q1 (default fixed 2 MiB sufficient).
- ⚠️ Dedup ratio 1.5× sintético → 1.2× com customer workload review.

---

## 20. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo | Initial sprint contract S-05 (Multipart + Chunking + Merkle dual-side; HIGH_RISK 11 sign-offs canonical). |
| 1.1.0 | 2026-04-25 | Gustavo | Lote 10.5bis P0 fixes (Agent R4 review remediation): pull-based chunk API; BLAKE3 throughput recalibration ≥500 MB/s native + ≥200 MB/s WASM; partial UNIQUE WHERE state='in_progress'; canonical_bytes 102 bytes; ADR canonical path; INV §3.16 promotion; ADR-0040 substantive content (per-region 5 shards). |
| 1.2.0 | 2026-04-25 | Gustavo | **Lote 10.5-tris fixes** (Sonnet R5 independent review; 4 NEW P0s + 4 P1s): (a) **P0-SR5-001** WI-001 SplitBlob `ChunkPutReceipt` type + try_join_all all R2 PUTs awaited before manifest::build; manifest sign happens AFTER all chunk persistence verified; (b) **P0-SR5-002** WI-001 SpliceBlob explicit per-chunk pipeline (sequential verify-then-write within chunk; pipeline at chunk-level; NO unverified bytes ever reach client; 1-chunk lookahead 2 MiB buffer); (c) **P0-SR5-003** WI-005 manifest memory budget corrected (Vec<ChunkRef> 81920 × 40 bytes = 3.28 MB heap; verify_streaming redesigned O(1) memory via D1 manifest_chunks per-chunk read; INV-MULTIPART-STREAMING-MEMORY updated); (d) **P0-SR5-004** ADR-0040 §A1 `multipart_sessions` cross-shard migration discipline (reconcile job scope explicit; sweeper multi-shard aware during dual-write window); INV-MULTIPART-ORPHAN-DETECTABLE updated registry §3.16; (e) **P1-SR5-001** cross-crate alignment MAX_CHUNKS_PER_BLOB 80000→81920 (corelink-chunker matches corelink-manifest); registry INV-MULTIPART-BOUNDED-PARSER updated; (f) **P1-SR5-002** SplitBlob created_at_ms captured ONCE before signing; reused on retry. |
| 1.4.0 | 2026-05-01 | Gustavo (via Claude Opus 4.7) | **WI-S05-001 SEALED — implementation phase, pure-logic SplitBlob/SpliceBlob handler shipped.** Module `crates/corelink-worker/src/reapi/cas/` ships the canonical `SplitSpliceHandler` trait + `SplitSpliceHandlerImpl` plus 4 trait abstractions (`SessionStore`, `ChunkStore`, `BlobAssembler`, `AuditSink`) each with InMemory fakes preserving every documented semantic — same trait-abstraction-defer pattern that landed `reapi::ac` in S-04. Quality gates: 18 lib unit + 4 property tests at ~22k iter total (`prop_split_tenant_isolation` + `prop_split_idempotent_finalize` 10k each, `prop_abort_safety` + `prop_chunk_ordering_canonical` 1k each — nightly opts into 100k via `PROPTEST_CASES`); `cargo test --workspace --all-targets --features corelink-worker/tower-middleware` 0 failures; `cargo clippy ... -D warnings` clean; `validate_specs.py` + `validate_references.py` clean. F-001 closure replicated (no global mutable state — every shared collection on `Arc<Mutex<…>>` field). MAX_CHUNKS_PER_BLOB = 81920 cross-crate alignment per §5.1 P1-SR5-001 honored in the worker's `corelink-worker::reapi::cas::types::MAX_CHUNKS_PER_BLOB` constant. Tonic gRPC + axum REST surfaces deferred to WI-S05-006 alongside the conformance suite (handler trait is the integration seam). Real chunker / R2 multipart adapter / D1 schema / Merkle codec / sweeper still upstream (WI-S05-002..006). |

---

**Fim spec contract S-05 v1.2.0 SOTA.**
