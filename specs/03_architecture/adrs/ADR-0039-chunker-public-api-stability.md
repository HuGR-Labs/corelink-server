---
id: "ADR-0039"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-01"
updated: "2026-05-01"
title: "corelink-chunker public API stability + mask seed versioning policy"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner)", "Architect with Crypto SME specialization (mandatory emphatic per WI-S05-002 §30)"]
status_history:
  - {date: "2026-05-01", status: "FROZEN", by: "Gustavo Schneiter (via Claude Opus 4.7) — published at WI-S05-002 SEAL alongside ADR-0022 ratification"}
context_links:
  - "specs/04_sprints/S05/work_items/WI-S05-002-corelink-chunker-fastcdc-adr-0022.md"
  - "specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md"
  - "crates/corelink-chunker/spec/chunker_protocol.md"
tags: ["adr", "s05", "chunker", "api-stability", "semver"]
---

# ADR-0039 — `corelink-chunker` public API stability + mask seed versioning policy

## Context

`corelink-chunker` is a cripto-load-bearing library (BLAKE3 inline + FastCDC mask
seeds + canonical bounded parser) consumed by:

- `corelink-worker` (Cloudflare Worker bundle; multipart SplitBlob/SpliceBlob handler).
- `corelink-manifest` (forward, WI-S05-005; manifest builder + verifier).
- `corelink-cli` / `corelink-sdk` (forward, S-15; client-side dedup hints).
- External SLSA L3 reviewers (test vectors annex must reproduce byte-for-byte).

Any drift in the public API or the canonical algorithm constants invalidates the
global cache (chunks for the same input map to different boundaries → different
`manifest_digest` → cache misses for every existing customer).

## Decision

The following items are **frozen** for the life of crate v1.x:

### Algorithm constants (frozen)

| Constant | Value | Pinned in |
|---|---|---|
| `bounds::MAX_BLOB_SIZE` | `160 GiB` | `corelink-chunker::bounds` + canonical-vector test |
| `bounds::MAX_CHUNKS_PER_BLOB` | `81 920` | `corelink-chunker::bounds` + cross-crate alignment to `corelink-worker::reapi::cas::types::MAX_CHUNKS_PER_BLOB` |
| `bounds::FIXED_DEFAULT_CHUNK_SIZE` | `2 MiB` | `corelink-chunker::bounds` |
| `bounds::FASTCDC_DEFAULT_MIN/AVG/MAX` | `1 / 2 / 4 MiB` | `corelink-chunker::bounds` |
| `bounds::FASTCDC_MASK_S` | `0x0000_d9f0_0353_0000` | `corelink-chunker::bounds` + canonical-vector pin |
| `bounds::FASTCDC_MASK_L` | `0x0000_d900_0353_0000` | `corelink-chunker::bounds` + canonical-vector pin |
| `fastcdc::gear::GEAR_TABLE[0..256]` | derived from SplitMix64 seed `"fastcdc1"` | `corelink-chunker::fastcdc::gear` + canonical-vector pin (entries 0, 42, 127, 200, 255) |

Drift in any of these constants is a **breaking change** requiring a new ADR
amending ADR-0022, a major version bump of `corelink-chunker`, and a coordinated
deployment plan that double-writes both old + new chunker outputs during the
migration window (otherwise existing customer manifests become unverifiable).

### Public API (frozen)

| Item | Stability |
|---|---|
| `Chunker` trait | Stable. New methods require major bump. |
| `ChunkerKind` enum | Stable. Variant addition allowed via `#[non_exhaustive]` (callers must use a wildcard arm). |
| `ChunkerConfig` struct | Stable. Field addition allowed via `#[non_exhaustive]` + `with_*` builder mutators. |
| `ChunkerAlgorithm` enum | Stable. Variant addition allowed via `#[non_exhaustive]`. |
| `ChunkerError` enum | Stable. Variant addition allowed via `#[non_exhaustive]`. |
| `ChunkerStep` enum | **Frozen** (NOT `#[non_exhaustive]`). New variants require major bump. Rationale: every caller MUST handle every state-machine arm — a forgotten arm is a stuck pipeline. |
| `Chunk<'a>` struct | Stable. Field addition is a major bump (struct fields visible to callers). |
| `OwnedChunk` struct | Stable; same rules as `Chunk<'a>`. |
| `bounds` constants | Frozen per "Algorithm constants" above. |

### Forward-compat surface (additive without breaking)

- New `ChunkerAlgorithm` variants (e.g. `FastCDC4MiB`, `BuildbarnCompat`).
- New `ChunkerConfig` fields (e.g. compression hints, encryption hooks).
- New `ChunkerError` variants (e.g. `EncryptionFailed` once BYOK lands).
- New `Chunker` impls (e.g. `BuildbarnCompatChunker`).

### Migration discipline if a frozen item must change

1. New ADR amending ADR-0022 / ADR-0039.
2. Major version bump of `corelink-chunker` (`0.x → 1.0` at S-05 SEAL; `1.x → 2.0`
   thereafter).
3. Deployment plan with dual-write window:
   - During window, both algorithms produce manifests; reads accept either.
   - Cutover only when 100% of stored manifests are migrated.
4. Cross-crate version bump of `corelink-worker::reapi::cas::types::MAX_CHUNKS_PER_BLOB`
   if the bound changes.

## Consequences

### Positive

- External reviewers (SLSA L3, customer engineering teams) can reproduce chunker
  output byte-for-byte against the canonical-vector test vectors.
- Cross-crate constants stay in lock-step automatically (CI will assert parity once
  `corelink-manifest` lands in WI-S05-005).
- Forward-compat additive variants don't break callers (every `ChunkerConfig` /
  `ChunkerAlgorithm` / `ChunkerError` consumer must handle the wildcard arm).

### Negative

- A bug in the FastCDC algorithm or mask seeds requires a coordinated migration —
  not a hotfix. Mitigation: 10k-iter property test + cargo-fuzz harness 1h CI nightly
  catches algorithmic regressions BEFORE release.
- The Gear table is large and derived (256 × 8 bytes); drift in the SplitMix64
  generator or seed silently breaks determinism. Mitigation: canonical-vector tests
  pin entries 0 / 42 / 127 / 200 / 255 byte-for-byte.

### Neutral

- `ChunkerStep` is exhaustive (deliberate); future variants require major bump.

## Test vectors

- `crates/corelink-chunker/tests/bounds_enforcement.rs::fastcdc_canonical_masks_pinned` —
  pin `MASK_S` + `MASK_L`.
- `crates/corelink-chunker/src/fastcdc.rs::gear::tests::table_canonical_*` — pin
  Gear table entries 0 / 42 / 127 / 200 / 255 byte-for-byte.
- `crates/corelink-chunker/tests/prop_chunker.rs::max_chunks_per_blob_is_canonical` —
  pin cross-crate `MAX_CHUNKS_PER_BLOB` value (= 81 920) + `MAX_BLOB_SIZE`
  invariant (`FIXED_DEFAULT_CHUNK_SIZE × MAX_CHUNKS_PER_BLOB == MAX_BLOB_SIZE`).

## Alternatives considered

1. **All public API frozen including `ChunkerStep`**: chosen for `ChunkerStep`
   (every caller MUST handle every variant). Rejected for `ChunkerError` /
   `ChunkerConfig` / `ChunkerAlgorithm` because additive evolution is normal there.
2. **All public API `#[non_exhaustive]`**: rejected for `ChunkerStep` (forgotten
   arms = pipeline stalls). Accepted for the rest.
3. **Mask seeds tunable per tenant via S-13 admin plane**: rejected pre-GA; chunker
   determinism is a global invariant. Future ADR may amend.

## Status

ACCEPTED + FROZEN — published at WI-S05-002 SEAL.
