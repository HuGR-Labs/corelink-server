---
id: "S-05"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-002"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "SECURITY-MODEL"
  - "DATA-MODEL"
  - "INVARIANT-REGISTRY"
  - "STORAGE-SEMANTICS-MATRIX"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["sprint", "s05", "multipart", "chunking", "merkle", "fastcdc", "high-risk"]
---

# Sprint S-05 — Multipart Upload + Chunking + Merkle Trees (Buildbarn-style; até 5 TiB)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-25**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (Lote 9.4 SOTA)

> **🚦 Phase boundary:** Fase 1 — Remote Cache.

---

## 1. Objetivo

Estender CAS para **blobs > 5 MiB até 5 TiB** via R2 multipart + decomposição Merkle (Buildbarn-style: chunks 2 MiB content + manifest tree). Cliente streaming hash inline; server valida cada chunk + manifest tree completo; dedup intra-tenant via UNIQUE `(tenant_id, chunk_digest)` (foundation para S-07 INV-DEDUP-CONSISTENCY). Destrava cache de Docker layers, ML model files, binários grandes.

## 2. Escopo

### 2.1 In-scope

- **WI-S05-001**: REAPI SplitBlob + SpliceBlob handlers.
- **WI-S05-002**: Crate `corelink-chunker` (fixed 2 MiB default + FastCDC opt-in) + ADR-0022 chunk-vs-part decoupling.
- **WI-S05-003**: R2 multipart adapter (init/parts/complete/abort).
- **WI-S05-004**: D1 schema `chunks` + `manifest_chunks` + `multipart_sessions` + UNIQUE constraint.
- **WI-S05-005**: Merkle manifest builder/verifier dual-side + property test 100k invalid trees.
- **WI-S05-006**: Sweeper cron DO + RB-FM-060 dry-run + 160 GiB stitched flow + PRR.

### 2.2 Anti-scope

- ❌ Cross-tenant dedup (S-07 + ADR).
- ❌ Compression (Zstd deferred pós-GA).
- ❌ Encryption-at-write per chunk (S-14 envelope BYOK).
- ❌ Erasure coding (Reed-Solomon over-engineering).
- ❌ Resumable upload mid-restart (anti-scope GA).
- ❌ Variable chunk size > 16 MiB.

## 3. Customer Impact & Journey

**JTBD:** "Como dev de ML/Docker, preciso fazer upload de blobs grandes (> 5 MiB até GiB-scale) com integridade Merkle dual-side e dedup automático intra-tenant para reduzir storage cost ≥ 1.5×."

**CAPs entregues:** CAP-CAS-008..013 (multipart até 5 TiB + Merkle chunking + intra-tenant dedup + REAPI SplitBlob/SpliceBlob + sweeper orphan + chunk vs part separation).

## 4. Capability Mapping

Ver `_spec_contract.md §4`. Foundation para INV-DEDUP-CONSISTENCY (S-07).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S05-D1 | REAPI SplitBlob + SpliceBlob handlers | `crates/corelink-worker/src/reapi/multipart.rs` | gRPC + REST handlers; auth middleware; tenant context |
| S05-D2 | Crate corelink-chunker + ADR-0022 | `crates/corelink-chunker/` + ADR | Fixed 2 MiB default; FastCDC opt-in via flag; ADR documents chunk-vs-part decoupling |
| S05-D3 | R2 multipart adapter | `crates/corelink-worker/src/storage/multipart.rs` | Init/parts/complete/abort; ETag tracking; chaos test client disconnect |
| S05-D4 | D1 schema chunks + manifest_chunks + multipart_sessions | `migrations/004_multipart.sql` | UNIQUE `(tenant_id, chunk_digest)`; integration test |
| S05-D5 | Merkle manifest builder/verifier dual-side | `crates/corelink-chunker/src/merkle.rs` | Streaming builder; verifier rejects invalid pre-persist + client-side; property test 100k |
| S05-D6 | Sweeper cron DO + RB + PRR + 160 GiB stitched | `crates/corelink-worker/src/multipart/sweeper.rs` | Abort > 7d; RB-FM-060 dry-run; > 160 GiB stitched flow E2E test |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Chunking

- **Default**: fixed-size 2 MiB chunks.
- **Opt-in**: FastCDC content-defined (Xia 2016).
- **Chunk size vs R2 part size separation**: chunk = 2 MiB content unit; R2 part = 16 MiB (8 chunks/part); R2 max parts 10k → 160 GiB max single multipart upload.
- **> 160 GiB**: stitched via multiple multipart sessions + manifest tree.

### 6.2 Storage

- R2 multipart API: InitiateMultipartUpload + UploadPart + CompleteMultipartUpload + AbortMultipartUpload.
- D1 chunks table com UNIQUE `(tenant_id, chunk_digest)`.

### 6.3 Invariants

- INV-CAS-INTEGRITY (CRITICAL, TLA+): chunk hash verifies; manifest hash = Merkle root.
- INV-CAS-IDEMPOTENCY (CRITICAL): same body → same chunking → same manifest digest.
- INV-TENANT-ISOLATION (CRITICAL): chunks index é tenant-scoped UNIQUE.

### 6.4 SLOs

- SLO-LAT-CAS-PUT-MULTIPART (novo SLO em slo_catalog).
- Throughput ≥ 100 MB/s steady em staging.

## 7. Definition of Done (HIGH_RISK)

- [ ] 6 WIs SEALED (EVT-031).
- [ ] Upload 1 GiB single blob hash verify E2E (EVT-018).
- [ ] Dedup: mesmo chunk em 2 blobs distintos refcount += 1 (EVT-002).
- [ ] Orphan parts: client disconnect chaos → sweeper aborts em 7d (EVT-023).
- [ ] Property test 100k manifest verify rejeita árvores inválidas (EVT-002).
- [ ] SLO-LAT-CAS-PUT-MULTIPART definido + sustained (EVT-021).
- [ ] PRR HIGH_RISK 10–12 sign-offs (EVT-031).
- [ ] Throughput ≥ 100 MB/s steady (EVT-021).
- [ ] Dedup ratio ≥ 1.5× sintético (EVT-021).
- [ ] ADR-0022 ratificado (EVT-027).
- [ ] 160 GiB single + > 160 GiB stitched flow E2E test (EVT-018).
- [ ] RB-FM-060 dry-run (EVT-017).
- [ ] Cost regression gate § 14.10 (EVT-002).

## 8. Dependencies

### Hard blockers

- **S-01 SEALED** (CAS single-blob foundation).
- **S-02 SEALED** (read path para chunks).

### Outbound

- S-06 (GC entende manifest_chunks reachability).
- S-07 (INV-DEDUP-CONSISTENCY foundation).
- S-15 (CLI `corelink put` large blobs).
- S-20 (GA exige throughput sustained).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-02 SEALED).
- **Mid-check**: D+8.
- **Sprint close**: D+17 (3 semanas + 5 dias buffer).

## 10. Risk Register

Ver `_spec_contract.md §15` (8 risks 6-col).

## 11. Observability Plan

DASH-CAS extension (multipart timeline + dedup ratio).

## 12. Security & Privacy

STRIDE: chunks UNIQUE constraint impede cross-tenant; FastCDC determinism mantém INV-CAS-IDEMPOTENCY.

## 13. Post-mortem hooks

Cross-tenant chunk leak / multipart orphan > 30d / manifest invalid persisted / INV-CAS-IDEMPOTENCY violation com FastCDC / R2 API quirks.

## 14. Sign-off (HIGH_RISK 10–12)

11 roles incl. Crypto SME (Merkle BLAKE3 review) + AppSec.

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-05 (Lote 9.5b). |

---

**Fim de S-05 sprint contract.**
