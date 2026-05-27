# corelink-chunker test vectors annex (WI-S05-002 §6.1.9)

This annex enumerates the canonical input → expected output mappings asserted by
`tests/canonical_vectors.rs` + `tests/bounds_enforcement.rs`. External SLSA L3
reviewers can reproduce each vector independently using any FastCDC implementation
that honours the canonical mask seeds + Gear table from `chunker_protocol.md`.

## Fixed2MiB vectors

All vectors below use `ChunkerAlgorithm::Fixed2MiB` with the indicated `chunk_size`.

| ID | Input | Expected chunks |
|---|---|---|
| `FX-001` | empty (0 bytes) | none |
| `FX-002` | `[0x42]` (1 byte), `chunk_size = 256` | 1 chunk: offset=0, size=1, digest=BLAKE3(`[0x42]`) |
| `FX-003` | `[0x77; 255]`, `chunk_size = 256` | 1 chunk: offset=0, size=255, digest=BLAKE3(`[0x77; 255]`) |
| `FX-004` | `[0x77; 256]`, `chunk_size = 256` | 1 chunk: offset=0, size=256, digest=BLAKE3(`[0x77; 256]`) |
| `FX-005` | `[0x42; 256] ++ [0x99]`, `chunk_size = 256` | 2 chunks: (0, 256, BLAKE3(`[0x42; 256]`)), (256, 1, BLAKE3(`[0x99]`)) |
| `FX-006` | `[0x00; 2048]`, `chunk_size = 512` | 4 chunks of size 512 each, all digest=BLAKE3(`[0x00; 512]`) |
| `FX-007` | `[0xFF; 2048]`, `chunk_size = 512` | 4 chunks of size 512 each, all digest=BLAKE3(`[0xFF; 512]`) |
| `FX-008` | `[0x00; 5 MiB]`, `chunk_size = 2 MiB` (canonical) | 3 chunks: 2 MiB, 2 MiB, 1 MiB |

## FastCDC2MiB vectors

All vectors below use `ChunkerAlgorithm::FastCDC2MiB` with the indicated bounds and
the canonical `(MASK_S, MASK_L) = (0x0000_d9f0_0353_0000, 0x0000_d900_0353_0000)`.

| ID | Input | Bounds (min/avg/max) | Expected behaviour |
|---|---|---|---|
| `FC-001` | empty | 64 / 256 / 1024 | none |
| `FC-002` | `[0x42]` | 64 / 256 / 1024 | 1 chunk: size=1 (sub-min, finalised) |
| `FC-003` | `[0x00; 32]` | 64 / 256 / 1024 | 1 chunk: size=32 (sub-min, finalised) |
| `FC-004` | `[0x00; 1024]` | 64 / 256 / 1024 | 1 chunk: size=1024 (forced boundary at max) |
| `FC-005` | `[0x00; 1500]` | 64 / 256 / 1024 | ≥ 2 chunks; total size = 1500; on all-zeros every byte past `min` triggers boundary because `(0 & MASK) == 0` for any mask |
| `FC-006` | periodic `[i % 256; 1024]` | 64 / 256 / 1024 | non-empty chunk list; deterministic across runs (asserted in `fastcdc_canonical_vector_periodic_input`) |
| `FC-007` | periodic `[(31i + 7) % 256; 4 KiB]` | 1 MiB / 2 MiB / 4 MiB (canonical defaults) | 1 chunk: size=4 KiB (sub-min, finalised); digest=BLAKE3 of payload |

## Bounded-parser vectors

| ID | Scenario | Expected outcome |
|---|---|---|
| `BP-001` | `bounds::MAX_BLOB_SIZE` | `160 GiB` (`160 × 1024 × 1024 × 1024`) |
| `BP-002` | `bounds::MAX_CHUNKS_PER_BLOB` | `81 920` |
| `BP-003` | `bounds::FIXED_DEFAULT_CHUNK_SIZE` | `2 MiB` (`2 × 1024 × 1024`) |
| `BP-004` | `bounds::FASTCDC_MASK_S` | `0x0000_d9f0_0353_0000` |
| `BP-005` | `bounds::FASTCDC_MASK_L` | `0x0000_d900_0353_0000` |
| `BP-006` | `bounds::FIXED_DEFAULT_CHUNK_SIZE × MAX_CHUNKS_PER_BLOB` | `MAX_BLOB_SIZE` (cross-crate alignment invariant) |
| `BP-007` | `Fixed{size=0}` | `ChunkerError::FixedChunkSizeInvalid` |
| `BP-008` | `FastCDC{min=1024, avg=512, max=2048}` | `ChunkerError::FastCdcConfigInvalid` |
| `BP-009` | feed after `finalize()` | `ChunkerError::BlobTooLarge` |
| `BP-010` | `MAX_CHUNKS_PER_BLOB + 1` chunks attempted | `ChunkerError::TooManyChunks` (gated `#[ignore]`; release-mode `--ignored`) |

## Gear table vectors

The `GEAR_TABLE` is a `[u64; 256]` derived at compile time from the SplitMix64 PRNG
seeded with `0x6661_7374_6364_6331` (= ASCII `"fastcdc1"`). The following entries
are pinned byte-for-byte by the lib unit tests (`fastcdc::gear::tests`):

| Index | Expected `u64` value |
|---|---|
| `0` | `0xDC55_CAD3_41EA_78AE` |
| `42` | `0xC298_3FC3_5D2F_C77C` |
| `127` | `0x52B9_3749_5DDA_4116` |
| `200` | `0xD6A5_1366_74BB_7D09` |
| `255` | `0xE140_6804_3C76_4128` |

Reproducible reference (Rust):

```rust
fn splitmix64(state: &mut u64) -> u64 {
    let mut z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    *state = z;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn build_table() -> [u64; 256] {
    let mut table = [0u64; 256];
    let mut state: u64 = 0x6661_7374_6364_6331; // "fastcdc1"
    for i in 0..256 {
        table[i] = splitmix64(&mut state);
    }
    table
}
```

## Determinism property tests (10k iter)

| Test | Iterations | What it pins |
|---|---|---|
| `prop_fixed_determinism` | 256 PR; 10 000 nightly | `Fixed2MiB` chunks byte-for-byte across two runs of the same payload. |
| `prop_fastcdc_determinism` | 256 PR; 10 000 nightly | `FastCDC2MiB` chunks byte-for-byte across two runs of the same payload (mask seeds + Gear table fixed). |
| `prop_total_size_invariant_*` | 256 PR; 10 000 nightly | Sum of chunk sizes equals input length. |
| `prop_chunk_size_bounds_*` | 256 PR; 10 000 nightly | Fixed: every chunk except final = `chunk_size`. FastCDC: every non-final ∈ `[min, max]`. |
| `prop_offset_monotonic_*` | 256 PR; 10 000 nightly | Strictly monotonic offsets; `offset[k+1] = offset[k] + size[k]`. |
| `prop_blake3_digest_matches_*` | 256 PR; 10 000 nightly | `chunk.digest == BLAKE3(chunk.bytes)` for every chunk. |
| `prop_split_invariance_*` | 256 PR; 10 000 nightly | Streaming determinism: same chunks regardless of feed-batch size. |
| `determinism_10k_iter_*` | 1 000 random blobs (deterministic ChaCha20 RNG) | Cross-blob coverage of the determinism property. |

## CI gate

All vectors above are exercised by `cargo test -p corelink-chunker --all-targets`.
The 10k nightly variant is enabled via `PROPTEST_CASES=10000 cargo test -p
corelink-chunker --release`.

External SLSA L3 reviewers SHOULD also run the `cargo-fuzz` harness at 1h budget
(deferred per WI-S05-002 §6.1.8 — ships with WI-S05-006 conformance suite alongside
the criterion benchmark harness).
