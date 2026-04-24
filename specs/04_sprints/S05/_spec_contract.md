---
id: "SPEC-CONTRACT-S05"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s05", "multipart", "chunking", "merkle", "high-risk"]
---

# Spec Contract — S-05: Multipart Upload + Chunking + Merkle Trees

## 0. Metadata

| Sprint ID | S-05 |
|---|---|
| Nome | Multipart Upload + Chunking + Merkle (blobs > 5 MiB) |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-005 (CTRL-CAS-001 com chunking), FF-HR-002 (integrity propagation) |
| Duração estimada | 3 semanas |
| WIs antecipados | 6 |

## 1. Objetivo

Extender CAS para blobs > 5 MiB via R2 multipart upload + decomposição Merkle (Buildbarn-style: chunks 2 MiB + manifest tree). Cliente upload streaming; server valida cada chunk + manifest; dedup de chunks cross-blob dentro do tenant. Destrava cache de Docker layers + ML model files + binários grandes.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK
- **FF-HR-005**: integridade agora distribuída em tree — bug em 1 chunk compromete blob inteiro.
- **FF-HR-002**: chunks compartilhados intra-tenant — vazamento se chunk index não scopeado.

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "SECURITY-MODEL"
  - "DATA-MODEL"
  - "INVARIANT-REGISTRY"
  - "STORAGE-SEMANTICS-MATRIX"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
```

## 4. CAPs entregues

- **CAP-CAS-008**: Multipart upload (blobs 5 MiB – 5 TiB).
- **CAP-CAS-009**: Merkle chunking (2 MiB fixed-size + final partial).
- **CAP-CAS-010**: Intra-tenant chunk dedup (CAP-DEDUP-001 foundation).
- **CAP-CAS-011**: REAPI `SplitBlob` + `SpliceBlob` (REAPI v2 chunking).
- **CAP-CAS-012**: Orphan multipart cleanup (sweeper job).

## 5. Requirements específicos

- **R-S05-1**: REAPI `ContentAddressableStorage::SplitBlob` + `SpliceBlob` handlers.
- **R-S05-2**: Crate `corelink-chunker` com FastCDC optional + fixed-size 2 MiB default.
- **R-S05-3**: R2 multipart initialize/upload-parts/complete orchestration.
- **R-S05-4**: D1 schema: `chunks` table + `manifest_chunks` + `cas_blobs.is_chunked` flag.
- **R-S05-5**: Manifest tree builder + verifier (raw Merkle nodes).
- **R-S05-6**: Sweeper cron DO: abort multipart > 7d (PAT-SWEEPER-001).

## 6. Definition of Done

- [ ] 6 WIs SEALED.
- [ ] Upload 1 GiB single blob bem-sucedido; hash verify end-to-end.
- [ ] Dedup: mesmo chunk usado em 2 blobs distintos dentro do tenant.
- [ ] Orphan parts: abort testado chaos-style.
- [ ] Property test: manifest verify rejeita árvores inválidas.
- [ ] SLO-LAT-CAS-PUT-MULTIPART definido (novo SLO) + sustained.

## 7. Completeness Criteria (delta local)

- [ ] **10.s05.1** Throughput upload ≥ 100 MB/s steady em staging.
- [ ] **10.s05.2** Dedup ratio mensurável: `corelink_dedup_ratio{type=chunk}`.
- [ ] **10.s05.3** Zero orphan multipart > 7d em staging durante 72h.

## 8. Invariants

- INV-CAS-INTEGRITY (CRITICAL, TLA+): cada chunk hash verifies; manifest hash = Merkle root.
- INV-CAS-IDEMPOTENCY: same body → same chunking → same manifest digest.
- INV-TENANT-ISOLATION: chunks index é tenant-scoped (não cross-tenant dedup aqui — em S-07).

## 9. Quality Standards (delta local)

- **14.s05.1** Zero memory allocation em hot path chunker (streaming).
- **14.s05.2** Multipart parts ≤ 50 por blob (R2 limit = 10.000; margem).

## 10. Anti-scope

- ❌ Cross-tenant dedup (S-07 + ADR).
- ❌ Content-defined chunking (FastCDC) as default — optional only.
- ❌ Compression (fora de escopo S-05; Zstd considerado mas deferred).

## 11. Dependencies

- **Blocker:** S-01 SEALED (CAS single-blob foundation).
- **Blocker:** S-02 SEALED (read path para chunks).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S05-001 | REAPI SplitBlob + SpliceBlob handlers |
| WI-S05-002 | Crate corelink-chunker (fixed 2 MiB) |
| WI-S05-003 | R2 multipart adapter (init/parts/complete) |
| WI-S05-004 | D1 schema chunks + manifest_chunks |
| WI-S05-005 | Merkle manifest builder/verifier |
| WI-S05-006 | Sweeper cron DO (abort orphan parts) |

## 13. Duração

3 semanas; buffer 5 dias.

## 14. Critérios de promoção

- DoD + SLO-LAT-CAS-PUT-MULTIPART green.
- Dedup ratio > 1.5× em workload sintético (1.5× = cada chunk usado em ≥ 1.5 blobs).

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Multipart orphan accumulation (FM-060) | M | MEDIUM |
| Manifest verify lento pra trees grandes | M | MEDIUM |
| Chunk size escolha trade-off errada (dedup vs overhead) | M | MEDIUM |
| R2 multipart API quirk (ex: max parts, eventual consistency) | M | HIGH |

---

**Fim spec contract S-05.**
