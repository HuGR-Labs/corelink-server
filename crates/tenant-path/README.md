# corelink-tenant-path

Single canonical entry point for deriving the per-tenant path prefix used in
CoreLink R2 / KV / D1 keys.

Implements layer 5 of `INV-TENANT-ISOLATION` (see `specs/03_architecture/auth_model.md §8.1`)
via HMAC-SHA256 over the tenant UUID, base64-URL-no-pad encoded and truncated
to 16 ASCII characters per `specs/03_architecture/remote_cache_product_profile.md §7.1`.

Authoritative ADR: [`ADR-0043`](../../specs/03_architecture/adrs/ADR-0043-hmac-tenant-prefix-algorithm.md).
Sprint contract: [`WI-S01-001`](../../specs/04_sprints/_sealed/S01/work_items/WI-S01-001-tenant-path-hmac.md).

## Quickstart

```rust
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use uuid::Uuid;
use zeroize::Zeroizing;

let tdk        = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
let tenant_id  = Uuid::parse_str("01938af0-abcd-7123-8456-000000000001")?;
let prefix     = derive_prefix(&tdk, tenant_id);

assert_eq!(prefix.as_str().len(), 16);
let r2_key = format!("cas-wnam/{prefix}/blake3/ab/cd/abcd...");
```

`from_bytes` takes [`zeroize::Zeroizing<[u8; 32]>`] so the caller's source
buffer is securely scrubbed when the wrapper goes out of scope; the internal
copy in `TenantDerivationKey` is also zeroed on drop. There is a brief window
inside `from_bytes` itself where both copies are live (the `[u8; 32]` is
`Copy`, so moving the `Zeroizing` wrapper into the constructor materializes
a stack copy), but both copies are owned by zeroize-aware types — nothing
leaks past the function boundary. See `from_bytes` rustdoc for the precise
lifetime walkthrough.

## API surface (deliberately small)

| Item | Kind | Use |
|---|---|---|
| `TenantDerivationKey` | newtype `[u8; 32]` | Per-region HMAC secret loaded from a Cloudflare Secrets binding. Constructed via `from_bytes(Zeroizing<[u8; 32]>)`; source buffer is scrubbed on drop. `Debug` redacts. No `PartialEq`/`Eq`. `Zeroize`-on-drop. |
| `TenantPrefix` | newtype `[u8; 16]` (private field) | 16 ASCII chars from the URL-safe base64 alphabet. Constructible only via `derive_prefix`. Serialize via `as_str()` or `Display`; for D1 `BLOB(16)` binding, use `prefix.as_str().as_bytes()`. |
| `derive_prefix(&TDK, Uuid) -> TenantPrefix` | infallible function | Single canonical derivation. |
| `TENANT_PREFIX_LEN` | `usize = 16` | Length constant for FFI / migration writers. |
| `DeriveError` | enum | Defense-in-depth; not currently constructible by `derive_prefix`. |

## Anti-patterns

- ❌ **Do not** add `derive_prefix_for_testing`, `derive_prefix_raw`, or any
  alternative derivation. The whole point is one trust boundary, one
  function.
- ❌ **Do not** construct `TenantPrefix` from arbitrary bytes outside this
  crate. The field is private precisely so `derive_prefix` is the unique
  producer.
- ❌ **Do not** `Debug`-log or `Display`-print `TenantDerivationKey`. The
  `Debug` impl returns `"TenantDerivationKey(REDACTED)"` and there is no
  `Display` impl.
- ❌ **Do not** rely on the prefix for authorization. It is a *namespacing*
  primitive; layers 1-4 of `auth_model.md §8.1` (PAT validation, tenant
  resolution, scope enforcement, request-time HMAC binding) carry the
  authorization invariant.

## Algorithm

```text
mac    = HMAC-SHA256(key = tdk, msg = tenant_id.as_bytes())   // 32 bytes
b64    = base64::URL_SAFE_NO_PAD.encode(mac)                  // 43 chars
prefix = b64[..16]                                            // 16 ASCII chars
```

Implementation: [`src/prefix.rs`](src/prefix.rs).

`tenant_id.as_bytes()` returns the canonical 16-byte big-endian UUID
representation, stable across `uuid` crate versions.

## Testing

```bash
cargo test  -p corelink-tenant-path
cargo clippy -p corelink-tenant-path --all-targets -- -D warnings
cargo bench -p corelink-tenant-path
```

The test suite covers WI-S01-001 §8 acceptance criteria AC-1..7:

| AC | Test(s) |
|---|---|
| AC-1 (determinism) | `determinism_1000_iter`, `determinism_with_cloned_tdk` |
| AC-2 (10k injectivity) | `prop_injectivity_distinct_tenant_ids` (proptest 10k cases), `bulk_injectivity_1024` |
| AC-3 (cross-TDK separation) | `cross_tdk_unit`, `prop_cross_tdk_separation` |
| AC-4 (16 ASCII URL-safe) | `encoding_format_unit`, `encoding_no_padding_or_url_reserved`, `prop_encoding_alphabet` |
| AC-5 (no-panic) | `prop_no_panic`, `boundary_uuids_accepted`, `zero_tdk_accepted`, `all_ones_tdk_accepted`, `fuzz/fuzz_targets/derive_prefix.rs` |
| AC-6 (perf p99 < 100µs) | `benches/derive.rs` |
| AC-7 (Debug REDACTED) | `tdk_debug_redacted` |

Plus `canonical_vectors` — five cross-language reference values computed
independently with the Python reference snippet below.

## Cross-language reference

Any other implementation (Python harness, Go SDK, browser SDK) must produce
the same prefix for the same `(tdk, tenant_id)` pair. Reference Python:

```python
import hmac, hashlib, base64, uuid

def derive_prefix(tdk: bytes, tenant_id: str) -> str:
    tid_bytes = uuid.UUID(tenant_id).bytes
    mac       = hmac.new(tdk, tid_bytes, hashlib.sha256).digest()
    b64       = base64.urlsafe_b64encode(mac).decode().rstrip("=")
    return b64[:16]
```

The five canonical vectors hardcoded in `tests/prop_tenant_path.rs::canonical_vectors`
were generated with this snippet.

## Lints / safety

- `#![forbid(unsafe_code)]` declared as a literal attribute in `src/lib.rs`,
  in addition to the `[lints.rust] unsafe_code = "forbid"` package directive
  in `Cargo.toml` — belt-and-suspenders.
- `clippy::unwrap_used = deny`, `expect_used = deny`, `panic = deny`,
  `indexing_slicing = deny`.
- Strategic `#[allow(clippy::expect_used, reason = "...")]` is used in
  exactly **three** places where the `Result::Err` arm is provably
  unreachable: (a) `TenantPrefix::as_str` — `from_utf8` over an ASCII-only
  buffer; (b) `derive_prefix` — `Hmac::<Sha256>::new_from_slice` with a
  fixed 32-byte key; (c) `derive_prefix` — `URL_SAFE_NO_PAD::encode_slice`
  into a 43-byte buffer for a 32-byte input. See `src/prefix.rs` for the
  per-call-site rationales.

## Performance

The criterion baseline at the time of this README is **~3.2 μs/call** on
optimized x86-64 (`cargo bench` release profile). The AC-6 budget per
WI-S01-001 §8 is **p99 < 100 μs**, leaving roughly 31× of headroom.

A regression gate test, `perf_regression_10k_under_1s`, runs **only in
release builds** (`#[cfg_attr(debug_assertions, ignore)]`) and asserts
that 10 000 iterations complete in under 1 s — i.e. mean < 100 μs / call.
CI invokes `cargo test --release perf_regression` to exercise it.

## Entropy

`HMAC16` displays 16 ASCII chars from the URL-safe base64 alphabet, which
is `16 × 6 = 96` bits of output entropy.

- **Per-pair collision probability**: `≈ 2^-96` (vanishingly small).
- **Birthday bound**: `≈ 2^48` independent tenants would be required for
  the probability of any collision in the system to reach 50%. At
  realistic scale (e.g. `10^9 ≈ 2^30` tenants) the expected number of
  pairwise collisions is `≈ 2^{2·30 - 96 - 1} = 2^{-37}`.
- Collisions never grant authorization (which is enforced by layers 1-4
  of `auth_model.md §8.1`); they would only mean two tenants share a
  storage namespace prefix, which is harmless given those upstream
  layers.
