---
type: "ADR"
title: "ADR-0022 — Chunk size vs R2 multipart part size decoupling"
description: "Decouples the content-addressable chunk size (2 MiB) from the R2 multipart part size (16 MiB) so dedup granularity is independent of R2's API cost ceiling."
source_files:
  - "specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s05", "chunker", "multipart", "fastcdc", "r2"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0022 — Chunk size vs R2 multipart part size decoupling

CoreLink's chunker splits blobs into content-addressable units for dedup, while R2's multipart API has its own minimum part size — two distinct concepts that the S-05 corpus had silently conflated. This ADR keeps them decoupled: a 2 MiB chunk for good dedup granularity, batched 8-at-a-time into a 16 MiB R2 part to stay above R2's 5 MiB floor and cut R2 ops cost 8×. It matters because conflating them would force either terrible dedup (16 MiB chunks) or an impossible upload (2 MiB parts R2 rejects). It governs the [chunk + manifest bucket](/storage/chunk-manifest-buckets.md) storage layout.

# Context

A Codex audit surfaced a contradiction between WI-S05-002 (chunk size = 2 MiB) and WI-S05-003 (part size = 16 MiB): chunk size is the content-addressable dedup unit (driving `chunks` table cardinality + manifest fan-out), while R2 multipart part size is the API upload unit (min 5 MiB, max 5 GiB, ≤10 000 parts) — conflating them gives either 8× storage cost from poor dedup or an impossible sub-5 MiB part (`specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:31-37`).

# Decision

Decouple the two concepts with canonical S-05 defaults: chunk size 2 MiB (Fixed or FastCDC-average), R2 multipart part size 16 MiB (batching 8 chunks per `UploadPart` for 8× lower R2 ops cost), R2 max 10 000 parts (hard limit), and a 160 GiB max single multipart blob (10 000 × 16 MiB) beyond which manifests are stitched (`specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:41-48`). The decision matrix favored decoupling on dedup granularity, R2 ops cost, max blob size, FastCDC tunability, and Buildbarn/S3 industry alignment (`specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:52-61`).

# Consequences

- Dedup ratio is independent of R2's API cost ceiling, the chunker stays wasm32-clean with no R2 SDK dependency, R2 ops cost drops 8×, and per-tenant chunk tuning becomes feasible (`specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:66-70`).
- Two concepts must be kept aligned via a cross-crate alignment table + pinned constants, and stitching beyond 160 GiB needs manifest-layer support (`specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:72-75`).
- The canonical chunker constants (`MAX_BLOB_SIZE`, `MAX_CHUNKS_PER_BLOB`, FastCDC mask seeds, gear table, algorithm dispatch) are frozen for the v1.x life of the crate, with any change requiring a new ADR + major version bump + double-write migration (`specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:81-95`).

# Citations

1. `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:31-37` — the chunk-size vs part-size conflation and why it breaks.
2. `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:41-48` — the decision: decoupled canonical defaults.
3. `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:52-61` — the decision matrix favoring decoupling.
4. `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:66-70` — positive consequences (dedup, wasm-clean, ops cost, tunability).
5. `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:72-75` — the alignment + stitching costs.
6. `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md:81-95` — the frozen-constants stability commitment.
