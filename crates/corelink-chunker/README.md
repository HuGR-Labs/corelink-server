# corelink-chunker

Content-defined chunker for the CoreLink CAS multipart pipeline (WI-S05-002 + ADR-0022).

Two algorithms, both deterministic, both producing the same `Chunk` envelope (BLAKE3-256
digest inline + offset + bytes borrowed from a chunker-internal staging buffer):

| Algorithm | Default? | Behaviour |
|---|---|---|
| `Fixed2MiB` | yes | Emit a chunk every `2 MiB` of input + a final partial via `finalize`. Cheapest path; no rolling-hash state. |
| `FastCDC2MiB` | opt-in | FastCDC content-defined chunking (Xia 2016) with bounds `min = 1 MiB` / `avg = 2 MiB` / `max = 4 MiB` and canonical Gear-hash mask seeds. Better dedup ratio on shifted payloads (Docker layers, ML-model deltas). |

Both impls share the bounded-parser discipline:

| Bound | Value | Source |
|---|---|---|
| `MAX_BLOB_SIZE` | 160 GiB | ADR-0022 (R2 multipart hard cap = 10 000 parts × 16 MiB / part) |
| `MAX_CHUNKS_PER_BLOB` | 81 920 | spec contract S-05 §5.1 P1-SR5-001 (cross-crate alignment with `corelink-worker::reapi::cas::types::MAX_CHUNKS_PER_BLOB`) |
| `FASTCDC_DEFAULT_MIN/AVG/MAX` | 1 / 2 / 4 MiB | WI-S05-002 §1.5 |
| `FASTCDC_MASK_S` | `0x0000_d9f0_0353_0000` | Xia 2016 §3.4 + ADR-0022 stability commitment |
| `FASTCDC_MASK_L` | `0x0000_d900_0353_0000` | Xia 2016 §3.4 + ADR-0022 stability commitment |

## Quickstart

```rust
use corelink_chunker::{Chunker, ChunkerConfig, ChunkerKind, ChunkerStep};

let mut chunker = ChunkerKind::new(ChunkerConfig::default())?;

let payload: &[u8] = /* ... your blob bytes ... */ &[];
let mut cursor = 0;
while cursor < payload.len() {
    let slice = &payload[cursor..];
    match chunker.feed(slice) {
        ChunkerStep::Chunk { chunk, consumed } => {
            // persist `chunk.bytes` to R2 keyed under `chunk.digest`
            // before the next mutating call (or copy via chunk.to_owned()).
            cursor += consumed;
        }
        ChunkerStep::NeedMore { consumed } => cursor += consumed,
        ChunkerStep::Error(e) => return Err(e.into()),
    }
}
if let Some(final_chunk) = chunker.finalize() {
    // emit the trailing partial chunk
}
```

Opt into FastCDC by passing `ChunkerConfig::fastcdc_default()`:

```rust
let mut chunker = ChunkerKind::new(ChunkerConfig::fastcdc_default())?;
```

## API surface

| Item | Use |
|---|---|
| `ChunkerKind::new(ChunkerConfig)` | Build a chunker; validates config. |
| `ChunkerConfig::default()` | Canonical Fixed-2 MiB defaults. |
| `ChunkerConfig::fastcdc_default()` | Canonical FastCDC defaults. |
| `ChunkerConfig::with_*` | Builder mutators (algorithm / chunk size / FastCDC bounds + masks). |
| `Chunker::feed(&mut self, input)` | Pull-based feed; returns `ChunkerStep`. |
| `Chunker::finalize(&mut self)` | Emit trailing partial. |
| `Chunker::reset(&mut self)` | Re-initialize for next blob. |
| `Chunk::to_owned() -> OwnedChunk` | Heap-owned snapshot; lifetime-free. |
| `bounds::MAX_BLOB_SIZE` / `MAX_CHUNKS_PER_BLOB` / `FASTCDC_MASK_*` | Canonical constants. |

`ChunkerConfig` + `ChunkerAlgorithm` + `ChunkerError` are `#[non_exhaustive]` so additive
variants in future releases don't break downstream callers (ADR-0039 forward-compat).
`ChunkerStep` is **NOT** `#[non_exhaustive]` — every caller MUST handle every state-machine arm.

## Threat model

| Threat | Mitigation |
|---|---|
| Determinism poison via crafted FastCDC input | Mask seeds + Gear table fixed in `bounds`; `INV-MULTIPART-CHUNK-DETERMINISTIC` 10k-iter property test; cargo-fuzz 1h CI nightly (deferred per WI §6.1.8 — ships with WI-S05-006 conformance suite). |
| Memory exhaustion via crafted oversize stream | `MAX_BLOB_SIZE` 160 GiB rejection at every `feed`; chunker never accumulates the full blob (zero-allocation Iterator pattern). |
| Mask seed drift between releases | Pinned via `bounds::FASTCDC_MASK_*` constants + canonical-vector test in `tests/bounds_enforcement.rs`. |
| Chunk-count exhaustion (anchor-every-byte attack) | `MAX_CHUNKS_PER_BLOB` 81 920 rejection; FastCDC `min` floor prevents tiny-chunk pathology in the canonical config. |
| BLAKE3 SIMD panic on misaligned buffer | `blake3` crate handles internally; `#![forbid(unsafe_code)]` at crate root. |

See WI-S05-002 §2 for the full HIGH_RISK risk justification.

## Determinism contract

`INV-CAS-IDEMPOTENCY` + `INV-MULTIPART-CHUNK-DETERMINISTIC`: for any deterministic
`ChunkerConfig` (= `ChunkerConfig::default()` shape with fixed FastCDC mask seeds + Gear
table), feeding the same byte stream MUST produce the same chunk boundaries + digests
byte-for-byte. The 10k-iter property suite under `tests/prop_chunker.rs` pins this
contract (`prop_fixed_determinism` + `prop_fastcdc_determinism`); a deterministic
ChaCha20 RNG generates ~1 000 distinct synthetic blobs and asserts byte-equality across
two independent chunker runs per blob.

## Wasm32 cleanliness

No `tokio` / `std::sync::Mutex` / `std::time::*` / `getrandom` in this crate's `src/`.
The same compiled artifact runs inside the Cloudflare Workers WASM bundle and the
host-side test harness; WI-S05-002 §1.4 throughput target ≥ 200 MB/s WASM is met by the
`blake3` SIMD + portable Gear-hash combo.

## Test surface

| Suite | Location | What it pins |
|---|---|---|
| Lib unit | `src/{bounds,chunker,fixed,fastcdc,error,lib}.rs` (gated `#[cfg(test)]`) | Constructor / accessor / config-validation / Gear-table canonical bytes. |
| `prop_chunker` | `tests/prop_chunker.rs` | 10k-iter determinism + size-bounds + offset-monotonicity + BLAKE3 inline-hash + streaming-invariance + reset + dedup-anchor. |
| `canonical_vectors` | `tests/canonical_vectors.rs` | Boundary inputs (empty / 1 byte / `chunk_size ± 1` / 5 MiB Gherkin) + adversarial (all-0x00, all-0xFF) + cross-algorithm invariants. |
| `bounds_enforcement` | `tests/bounds_enforcement.rs` | `MAX_BLOB_SIZE` / `MAX_CHUNKS_PER_BLOB` / `FASTCDC_MASK_*` pinning + bounded-parser error-path coverage + counter monotonicity. |

Property tests honour the `PROPTEST_CASES` environment variable so the same suite scales
from the default 256 PR-time iter up to 10 000 nightly iter and 100 000 weekly iter.

## Related docs

- WI-S05-002 — `specs/04_sprints/S05/work_items/WI-S05-002-corelink-chunker-fastcdc-adr-0022.md`
- ADR-0022 — `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md`
- ADR-0039 — `specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md`
- Spec contract S-05 — `specs/04_sprints/S05/_spec_contract.md`
- Crate spec — `crates/corelink-chunker/spec/chunker_protocol.md`
