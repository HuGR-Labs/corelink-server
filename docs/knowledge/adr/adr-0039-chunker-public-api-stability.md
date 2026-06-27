---
type: "ADR"
title: "ADR-0039 — corelink-chunker public API stability + mask seed versioning"
description: "Freezes the chunker's algorithm constants and public API for the life of crate v1.x, because any drift remaps chunk boundaries and invalidates every customer's manifest cache."
source_files:
  - "specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md"
  - "crates/corelink-cas/src/chunker.rs"
  - "crates/corelink-cas/src/lib.rs"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s05", "chunker", "api-stability", "semver"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0039 — corelink-chunker public API stability + mask seed versioning

`corelink-chunker` is the crypto-load-bearing library that decides where blobs split, and its output
is a global cache key: if the same input ever maps to different boundaries, every existing customer's
manifest digest changes and the cache misses wholesale. This ADR is the stability contract that makes
the chunker safe to depend on — it pins the algorithm constants and the public API for the life of
v1.x so the [chunk/manifest buckets](/storage/chunk-manifest-buckets.md) and the
[CAS/AC core crate cluster](/crates/cas-ac-core.md) can assume determinism.

# Context

`corelink-chunker` (BLAKE3 inline + FastCDC mask seeds + bounded parser) is consumed by the worker,
the manifest builder, the client SDK, and external SLSA L3 reviewers; any drift in its public API or
canonical algorithm constants remaps chunk boundaries, so the same input yields a different
`manifest_digest` and a cache miss for every existing customer.

# Decision

For the life of crate v1.x the ADR freezes the algorithm constants (`MAX_BLOB_SIZE` 160 GiB,
`MAX_CHUNKS_PER_BLOB` 81 920, the FastCDC min/avg/max sizes, both FASTCDC mask seeds, and the
SplitMix64-derived Gear table), each pinned by a canonical-vector test. Drift in any of them is a
breaking change requiring a new ADR amending ADR-0022, a major version bump, and a coordinated
double-write migration window. The public API is frozen with one deliberate exception in evolution
policy: `ChunkerStep` is exhaustive (NOT `#[non_exhaustive]`) so a forgotten match arm cannot silently
stall the pipeline, while `ChunkerKind`/`ChunkerConfig`/`ChunkerAlgorithm`/`ChunkerError` stay
additively extensible via `#[non_exhaustive]`.

# Consequences

External reviewers can reproduce chunker output byte-for-byte, cross-crate constants stay in
lock-step, and additive variants don't break callers; the accepted cost is that an algorithm or
mask-seed bug requires a coordinated dual-write migration rather than a hotfix (mitigated by property
tests + a fuzz harness and the canonical-vector pins on the Gear table).

# Status vs shipped code

There is **no standalone `corelink-chunker` crate** in the workspace. The chunker ships as a **module of
`corelink-cas`** (`pub mod chunker;` at `crates/corelink-cas/src/lib.rs:86`, implemented in
`crates/corelink-cas/src/chunker.rs`), so the "public API stability for the life of crate v1.x" framing
is the stability contract for that module + its re-exports rather than a separately versioned crate.
The substance — frozen algorithm constants, exhaustive `ChunkerStep`, determinism — applies to the
module as shipped; only the "standalone crate" packaging differs.

# Citations

1. `specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md:28-40` — Context: chunker consumers + why any API/constant drift invalidates the global cache.
2. `specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md:45-60` — Decision: the frozen algorithm constants table + drift = breaking change requiring a coordinated migration.
3. `specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md:71-71` — `ChunkerStep` is frozen (NOT `#[non_exhaustive]`) so a forgotten arm cannot stall the pipeline.
4. `specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md:96-112` — Consequences: byte-for-byte reproducibility + lock-step constants vs the coordinated-migration cost.
5. `crates/corelink-cas/src/lib.rs:86` — `pub mod chunker;`: the chunker ships as a module of `corelink-cas`, not a standalone crate.
6. `crates/corelink-cas/src/chunker.rs:84-110` — the frozen public API surface (`ChunkerKind` dispatch + `#[non_exhaustive]` config) the stability contract governs.
