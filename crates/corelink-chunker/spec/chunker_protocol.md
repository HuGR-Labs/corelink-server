# corelink-chunker — protocol spec

This document is the canonical protocol description for the FastCDC implementation in
`corelink-chunker`, mirroring the WI-S05-002 §1 source listing + ADR-0022 ratification
+ Xia 2016 §3.4 algorithm.

## 1. Algorithm summary

For every input byte `b`:

```text
h ← ((h << 1) + GEAR[b]) (mod 2^64)
```

After updating `h` we evaluate the boundary predicate:

| Window length `L` | Predicate fired? | Effect |
|---|---|---|
| `L < min` | No (predicate suppressed) | Continue absorbing bytes. |
| `min ≤ L < avg` | `(h & MASK_S) == 0` | Boundary fires (strict mask — harder; biases toward `avg`). |
| `avg ≤ L < max` | `(h & MASK_L) == 0` | Boundary fires (loose mask — easier; biases toward target). |
| `L == max` | Forced boundary | Boundary fires regardless of `h`. |

Each emitted chunk is hashed inline via BLAKE3-256 and surfaced as
`Chunk { bytes, digest, offset_in_blob, size_bytes }`.

## 2. Determinism + canonical constants

| Constant | Value | Pinned by |
|---|---|---|
| `min` (default) | 1 MiB | `bounds::FASTCDC_DEFAULT_MIN` |
| `avg` (default) | 2 MiB | `bounds::FASTCDC_DEFAULT_AVG` |
| `max` (default) | 4 MiB | `bounds::FASTCDC_DEFAULT_MAX` |
| `MASK_S` | `0x0000_d9f0_0353_0000` | `bounds::FASTCDC_MASK_S` (canonical Xia 2016 §3.4 + ADR-0022) |
| `MASK_L` | `0x0000_d900_0353_0000` | `bounds::FASTCDC_MASK_L` (canonical Xia 2016 §3.4 + ADR-0022) |
| `GEAR_TABLE` | 256-entry `[u64; 256]` | `fastcdc::gear::GEAR_TABLE` |

`GEAR_TABLE` is generated at compile time from a fixed SplitMix64 PRNG seed
(`0x6661_7374_6364_6331` = ASCII `"fastcdc1"`). The first / last / 42nd / 127th /
200th entries are pinned by canonical-vector tests so any drift in the table generator
or the seed trips a CI regression.

`INV-MULTIPART-CHUNK-DETERMINISTIC`: same input + same canonical config ⇒ same chunks
byte-for-byte across releases. ADR-0022 mandates that any change to `MASK_*` or
`GEAR_TABLE` is a breaking change requiring a major version bump + ADR amendment.

## 3. Bounded parser

| Bound | Value | Source |
|---|---|---|
| `MAX_BLOB_SIZE` | 160 GiB (`10_000 × 16 MiB`) | ADR-0022 (R2 multipart hard cap) |
| `MAX_CHUNKS_PER_BLOB` | 81 920 (`MAX_BLOB_SIZE / 2 MiB`) | spec contract S-05 §5.1 P1-SR5-001 |

Both bounds are enforced at every `Chunker::feed` call **before** absorbing the
offending bytes — the chunker never allocates more than `max + headroom` of staging
buffer regardless of input shape. Beyond `MAX_BLOB_SIZE` the caller MUST split into
multiple multipart sessions and stitch via the upstream manifest layer (out-of-scope
for WI-S05-002; ships in WI-S05-006).

## 4. State machine

```text
                ┌────────────────┐
                │  Initialised   │
                │  (post reset)  │
                └────┬───────────┘
                     │ feed(empty)
                     ▼
                ┌────────────────┐
   ┌────────────│  Buffering     │◄──────────┐
   │            └────┬───────────┘           │
   │ boundary-fired  │ feed(more)            │
   │  OR finalize()  │                       │
   ▼                 ▼                       │
┌────────────┐  ┌────────────────┐           │
│  Emitted   │──│  Pending-Drain │  next     │
│  Chunk     │  │  (Chunk<'a>    │  feed()/  │
│            │  │  alive)        │  finalize()
└────┬───────┘  └────┬───────────┘           │
     │               │                       │
     │ borrow drops  │                       │
     ▼               ▼                       │
┌────────────────┐                           │
│  Drained       │ ──────────────────────────┘
│  (ready for    │
│  next chunk)   │
└────────────────┘
```

Once `finalize` returns, the chunker is in the *finalized* state: subsequent `feed` /
`finalize` calls return `ChunkerStep::Error(BlobTooLarge)` until `reset` is invoked.

## 5. Memory contract

`INV-MULTIPART-STREAMING-MEMORY`: per-request stack budget ≤ `4 MiB` (FastCDC `max`
+ rolling-hash + BLAKE3 state). The staging buffer is sized to `max` exactly; the
emitted `Chunk<'a>` borrows from this buffer (no copy). `OwnedChunk` is available for
callers that want a heap-owned snapshot — single allocation per chunk, exactly
`size_bytes` bytes.

## 6. Cross-crate alignment

| Crate | Constant | Value | Source |
|---|---|---|---|
| `corelink-chunker` | `bounds::MAX_CHUNKS_PER_BLOB` | 81 920 | this WI |
| `corelink-worker` | `reapi::cas::types::MAX_CHUNKS_PER_BLOB` | 81 920 | WI-S05-001 |
| `corelink-manifest` (forward) | `bounds::MAX_CHUNK_COUNT` | 81 920 | WI-S05-005 (forward) |

CI must include a `const_assert_eq!` cross-crate constant comparison when the manifest
crate lands so any drift trips at compile time.

## 7. Cripto rationale

- BLAKE3-256 hash inline per chunk: matches the rest of the CoreLink CAS stack
  (`corelink-hash` + `corelink-ac` + S-01 single-blob path). Same family across the
  whole stack means SIMD path is exercised end-to-end and there's no interop cost for
  multi-hash REAPI clients.
- Gear-hash for FastCDC rolling fingerprint: O(1) per byte; not security-critical (the
  rolling hash is for boundary detection only, not content addressing). The
  content-address authority is BLAKE3.
- Mask seeds + Gear table fixed: any change to the boundary-detection algorithm
  invalidates the global cache (chunks for the same input map to different boundaries
  → different `chunk_digest` set → different `manifest_digest`). Treated as a
  breaking change.

## 8. References

- Xia et al., *FastCDC: a Fast and Efficient Content-Defined Chunking Approach for Data
  Deduplication*, USENIX ATC 2016.
- BLAKE3 Specification, O'Connor et al. 2020.
- Steele et al., *Fast Splittable Pseudorandom Number Generators* (SplitMix64), OOPSLA 2014.
- Cloudflare R2 Multipart API: <https://developers.cloudflare.com/r2/api/s3/multipart-uploads/>
- Buildbarn manifest format: <https://github.com/buildbarn/bb-storage>
- ADR-0022 — `../../specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md`
- ADR-0039 — `../../specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md`
- WI-S05-002 — `../../specs/04_sprints/S05/work_items/WI-S05-002-corelink-chunker-fastcdc-adr-0022.md`
