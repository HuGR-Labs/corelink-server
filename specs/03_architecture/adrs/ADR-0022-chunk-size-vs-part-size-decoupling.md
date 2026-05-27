---
id: "ADR-0022"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-05-01"
title: "Chunk size vs R2 multipart part size decoupling"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner)", "Architect with Crypto SME specialization (mandatory emphatic per WI-S05-002 §30)"]
status_history:
  - {date: "2026-04-25", status: "DRAFT", by: "Gustavo Schneiter (forward-looking ADR cited from WI-S05-002 + spec contract S-05)"}
  - {date: "2026-05-01", status: "FROZEN", by: "Gustavo Schneiter (via Claude Opus 4.7) — ratified at WI-S05-002 SEAL; corelink-chunker bounds + mask seeds pinned in code"}
context_links:
  - "specs/04_sprints/_sealed/S05/work_items/WI-S05-002-corelink-chunker-fastcdc-adr-0022.md"
  - "specs/04_sprints/_sealed/S05/work_items/WI-S05-001-reapi-splitblob-spliceblob-handlers.md"
  - "specs/04_sprints/_sealed/S05/work_items/WI-S05-005-merkle-manifest-builder-verifier.md"
  - "specs/04_sprints/_sealed/S05/_spec_contract.md"
  - "crates/corelink-chunker/spec/chunker_protocol.md"
tags: ["adr", "s05", "chunker", "multipart", "fastcdc", "r2", "cripto-load-bearing"]
---

# ADR-0022 — Chunk Size vs R2 Multipart Part Size Decoupling

## Context

Codex Lote 9.4 surfaced a contradiction between WI-S05-002 §1 (chunk size = 2 MiB) and WI-S05-003 §5 (part size = 16 MiB). The two concepts had been silently conflated across the S-05 corpus. Resolving the ambiguity is a prerequisite for the multipart pipeline ratification — chunk size and R2 multipart part size are **distinct** concepts with different load-bearing semantics:

- **Chunk size**: content-addressable unit; granularity at which dedup happens; determines `chunks` table cardinality + manifest fan-out.
- **R2 multipart part size**: API unit at which R2 accepts upload-part operations; `min = 5 MiB`, `max = 5 GiB`, hard cap of `10 000` parts per session.

If the two are conflated, picking 16 MiB chunks (R2 minimum) gives bad dedup and 8× the R2-bound storage cost (large chunks dedupe poorly because a single byte change shifts 16 MiB of content). Picking 2 MiB parts is impossible (R2 rejects parts < 5 MiB).

## Decision

**Chunk size and R2 multipart part size are decoupled.** Canonical defaults for S-05:

| Concept | Default | Tunable | Notes |
|---|---|---|---|
| **Chunk size** | **2 MiB** (`Fixed`) OR avg 2 MiB (`FastCDC`) | Per-tenant config, S-13 forward | Content-addressable unit. Determines dedup granularity + `chunks` D1 cardinality. |
| **R2 multipart part size** | **16 MiB** (8 chunks per part) | Operationally fixed at S-05 GA | R2 hard min 5 MiB; 16 MiB batches 8 × 2 MiB chunks per `UploadPart` call → 8× lower R2 ops cost. |
| **R2 max parts** | **10 000** | R2 hard limit (cannot tune) | Beyond → stitched flow (WI-S05-006). |
| **Max single multipart blob** | **160 GiB** | `10 000 × 16 MiB` | Larger blobs require multiple multipart sessions stitched at the manifest layer. |

## Decision matrix

| Criterion | Coupled (chunk = part) | Decoupled (chosen) |
|---|---|---|
| Dedup granularity | bound to R2 min (5 MiB) → poor ratio | `2 MiB` (target tradeoff per Buildbarn precedent) |
| R2 ops cost per blob | 1 op per chunk → 80 000 ops on 160 GiB blob | 1 op per part → 10 000 ops on 160 GiB blob (8× cheaper) |
| Max single multipart | bound by R2 part count × min part size → 50 GiB | 160 GiB (10 000 × 16 MiB) |
| FastCDC opt-in | constrained by R2 part bounds | independent (FastCDC mask seeds + bounds tuned per workload) |
| Buildbarn / S3 industry alignment | ✗ (Buildbarn decouples) | ✓ |
| Customer-tunable per-tenant chunk | impossible (R2 cap forces) | feasible via S-13 admin plane |

**Winner: Decoupled.**

## Consequences

### Positive

- Dedup ratio independent of R2 API cost ceiling.
- `corelink-chunker` is wasm32-clean (no R2 SDK dependency).
- 8× lower R2 ops cost on multipart upload (`UploadPart` per 16 MiB vs per 2 MiB).
- Customer-tunable chunk size per-tenant is feasible (S-13 forward).

### Negative

- **Two concepts to keep aligned**: documentation discipline + `crates/corelink-chunker/spec/chunker_protocol.md` §6 cross-crate alignment table. Mitigation: `MAX_BLOB_SIZE` / `MAX_CHUNKS_PER_BLOB` constants pinned in `corelink-chunker::bounds` + matching constants in `corelink-worker::reapi::cas::types` + canonical-vector test asserting parity.
- **Stitching > 160 GiB requires manifest-layer support**: handled in WI-S05-006 (sweeper + stitched flow); out of scope for WI-S05-002.

### Neutral

- Customer can use chunker without ever calling R2 directly (e.g. for client-side dedup hints in S-15 SDK).

## Stability commitment

The canonical chunker constants — `MAX_BLOB_SIZE`, `MAX_CHUNKS_PER_BLOB`,
`FASTCDC_MASK_S`, `FASTCDC_MASK_L`, the `GEAR_TABLE`, and the `Fixed2MiB` /
`FastCDC2MiB` algorithm dispatch — are **frozen** for the life of crate v1.x. Any
change is a breaking change requiring:

1. A new ADR amending this one.
2. Major version bump of `corelink-chunker`.
3. Cross-crate version bump of `corelink-worker::reapi::cas::types::MAX_CHUNKS_PER_BLOB`.
4. Deployment plan that double-writes both old + new chunker outputs during a
   migration window (otherwise existing manifests become unverifiable).

ADR-0039 enumerates the specific stability commitments + the semver discipline
that enforces them.

## Alternatives considered

1. **Coupled (chunk = part)**: rejected per matrix above (poor dedup, R2 ops cost,
   no customer tunability).
2. **Variable chunk size > 16 MiB**: rejected (anti-scope per spec contract S-05 §10
   — over-engineering at GA).
3. **R2 part size = 5 MiB minimum**: rejected (8× higher R2 ops cost vs 16 MiB; no
   dedup ratio improvement).

## Test vectors

- `crates/corelink-chunker/tests/canonical_vectors.rs::fixed_canonical_5mib_three_chunks` —
  canonical Gherkin (5 MiB blob → `[2 MiB, 2 MiB, 1 MiB]`).
- `crates/corelink-chunker/tests/bounds_enforcement.rs::max_blob_size_is_canonical_160_gib` —
  pin the canonical 160 GiB cap.
- `crates/corelink-chunker/tests/bounds_enforcement.rs::fastcdc_canonical_masks_pinned` —
  pin `MASK_S` + `MASK_L` byte-for-byte.

## Status history

- 2026-04-25: DRAFT — referenced forward from WI-S05-002 + spec contract S-05.
- 2026-05-01: **ACCEPTED + FROZEN** — ratified at WI-S05-002 SEAL; `corelink-chunker`
  ships with the canonical bounds + mask seeds + Gear table pinned in code.
