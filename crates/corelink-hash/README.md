# corelink-hash

CAS digest types for CoreLink — BLAKE3 + constant-time verify, type-driven
integrity at the write boundary.

Implements layer of `INV-CAS-INTEGRITY` / CTRL-CAS-001 (`security_model.md §6.1`,
`invariant_registry.md §3.2`). Sprint contract: [`WI-S01-002`](../../specs/04_sprints/S01/work_items/WI-S01-002-blake3-verify-at-write.md).

## Quickstart

```rust
use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};

let body    = Bytes::from_static(b"hello world");
let claimed = Digest::from_hex(
    "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24",
)?;

// Constructor runs the verify; mismatch -> Err(HashMismatch).
let vb = VerifiedBody::new(body, claimed)?;

// Storage adapters accept &VerifiedBody, never raw bytes.
// r2_writer.put(&ctx, &vb).await?;
```

## API surface (deliberately small)

| Item | Use |
|---|---|
| `Digest::compute(&[u8]) -> Digest` | Server-side BLAKE3 hash. |
| `Digest::from_hex(&str) -> Result<Digest, ParseError>` | Parse client-supplied hex (length + alphabet checked). |
| `Digest::to_hex(&self) -> String` | 64-char lowercase hex (`Display` matches). |
| `Digest::verify_constant_time(&Digest) -> bool` | **Use this** for adversarial timing-sensitive paths. |
| `VerifiedBody::new(Bytes, Digest) -> Result<Self, HashMismatch>` | Single constructor; runs constant-time verify. |
| `VerifiedBody::body() -> &Bytes` / `digest() -> &Digest` / `into_parts()` | Accessors. |
| `DIGEST_LEN: usize = 32` | Length constant. |

`PartialEq`/`Eq` on `Digest` are derived for non-adversarial use (set
membership, D1 lookups). Reach for `verify_constant_time` whenever the
caller is exposed to a timing oracle.

## Anti-patterns

- ❌ Don't construct `Digest` or `VerifiedBody` from raw bytes outside this
  crate (private fields).
- ❌ Don't use `==` / `assert_eq!` on `Digest` in adversarial paths — the
  derive short-circuits and leaks prefix-length through timing.
- ❌ Don't log `Bytes` body content or raw digest bytes; render via
  `Digest::to_hex()` and let the audit layer (`observability_model.md §S-09`)
  emit the canonical `corelink.cas.poisoning_attempt` event on mismatch.
- ❌ Don't add a `VerifiedBody::from_parts(body, digest)` constructor. The
  whole point is that there is no way to materialize a `VerifiedBody`
  without going through the verify step.

## Algorithm

```text
let digest = blake3::hash(body)         // 32 raw bytes (256-bit BLAKE3)
let ok     = digest.ct_eq(claimed)      // constant-time, subtle crate
if !ok { reject + audit-emit + 409 }
```

## Testing

```bash
cargo test  -p corelink-hash             # unit + property (10k iter)
cargo test  -p corelink-hash --release   # adds ct-variance + 5 MiB perf gate
cargo clippy -p corelink-hash --all-targets -- -D warnings
cargo bench -p corelink-hash             # criterion (release-only)
```

The test suite covers WI-S01-002 §8 acceptance criteria:

| AC | Test(s) |
|---|---|
| Successful write — digest matches body | `successful_write_hello_world` |
| Cache poisoning attempt — digest mismatch | `mismatch_rejected`, `prop_verified_body_only_on_match` (10k) |
| Constant-time verify (no timing oracle) | `constant_time_variance` (release-only, 2000 trials, < 5% delta) |
| BLAKE3 throughput SOTA | `perf_regression_5mib_under_50ms` (release-only) + `benches/blake3_bench.rs` |
| Property test — hash determinism | `prop_hash_determinism` (10k) |
| Property test — collision resistance heuristic | `prop_no_collision_10k`, `bulk_distinct_8192_no_collision` |
| `VerifiedBody` unconstructible without verify | enforced at compile time by private fields + single `pub fn new` |
| WASM compile target works | covered in CI workflow |

Plus `canonical_vectors` — four BLAKE3 reference values pinned in code:

| Body | Digest |
|---|---|
| `b""` | `af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262` |
| `b"a"` | `17762fddd969a453925d65717ac3eea21320b66b54342fde15128d6caf21215f` |
| `b"hello world"` | `d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24` |
| `b"The quick brown fox jumps over the lazy dog"` | `2f1514181aadccd913abd94cfa592701a5686ab23f8df1dff1b74710febc6d4a` |

These come from the official BLAKE3 reference test vectors and pin the
crate's algorithmic identity against accidental upstream drift.

## Lints / safety

- `#![forbid(unsafe_code)]` literal in `src/lib.rs` + `[lints.rust] unsafe_code = "forbid"` in Cargo.toml.
- Clippy: deny `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `todo`, `unimplemented`, `dbg_macro`, `print_stdout`, `print_stderr`.
- Zero strategic `#[allow]` in lib code (unlike `corelink-tenant-path`, here
  every fallible API surface is genuinely fallible — no provably-infallible
  Result branches needed).
- Test code carries the same allows as `corelink-tenant-path/tests`
  (assertion-driven; panic on failure is the contract).
