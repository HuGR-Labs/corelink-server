# corelink-client-verify

Stand-alone client-side BLAKE3 digest verify for CoreLink SDKs (Python pyO3,
Go cgo, JS/TS WASM). Default-on detection of bit rot + cache poisoning at
the SDK edge.

Implements layer of `INV-CAS-INTEGRITY` / CTRL-CAS-002
(`security_model.md §6.1`, `invariant_registry.md §3.2`). Sprint contract:
[`WI-S02-003`](../../specs/04_sprints/S02/work_items/WI-S02-003-corelink-client-verify-crate.md).

## Why this crate

Server-side write verify (CTRL-CAS-001 / `corelink-hash`) protects against
clients shipping tampered bodies into storage. CTRL-CAS-002 covers the
flip side: every body the SDK *receives* must match its claimed digest
before the consumer treats it as valid. Three cases motivate it:

1. **Bit rot in R2** (FM-051): storage media decay; a flipped bit in the
   stored object would be invisible to a client that didn't re-hash.
2. **Cache poisoning at transit** (THR-T-001): defense-in-depth on top of
   TLS 1.3 — verify catches a hypothetical MitM substitution.
3. **Server bug post-write** (FM-300): a future regression in the read
   path is caught by the SDK.

## API surface (deliberately small)

| Item | Use |
|---|---|
| `VerifyConfig::default()` / `::new()` | Default-on (`enabled=true`, `warn_on_optout=true`). |
| `VerifyConfig::disabled()` | Explicit opt-out (rare); emits `tracing::warn!` + counter increment when wrapped. |
| `ClientVerifier::new(VerifyConfig)` | Construct verifier. |
| `ClientVerifier::default_on()` | Sugar for `new(VerifyConfig::default())`. |
| `ClientVerifier::verify(&[u8], &Digest) -> Result<(), VerifyError>` | Sync constant-time verify. |
| `ClientVerifier::verify_stream(reader, expected)` (feature `stream`) | Incremental verify; returns `Stream<Item = Result<Bytes, StreamVerifyError>>`. |
| `opt_out_total() -> u64` | Process-wide opt-out counter (Grafana scrape target via S-15). |
| `Digest`, `DIGEST_LEN` | Re-exported from `corelink-hash`. |

C-ABI surface (feature `ffi`, consumed via cbindgen by Python pyO3 + Go
cgo wrappers in S-15) lives in module [`ffi`](src/ffi.rs):

- `corelink_verifier_new_default_on() -> *mut ClientVerifier` — canonical
  default-on construction. Counter does NOT tick.
- `corelink_verifier_new_disabled(uint8_t warn) -> *mut ClientVerifier` —
  **explicitly disabled** verifier (separate symbol so zero-init or
  argument-flag confusion cannot silently opt out). `warn=0` ⇒ silent
  (used by Python pyO3 `__del__`-driven teardown); `warn=1` ⇒
  `tracing::warn!`. The counter ticks either way.
- `corelink_verifier_verify(handle, body, body_len, hex, hex_len, *out_code) -> i32`
- `corelink_verifier_free(handle)`
- `corelink_verify_error_code_to_str(code) -> *const c_char`
- `corelink_verify_opt_out_total() -> u64` — canonical FFI accessor for the
  process-global `opt_out_total()` counter. Wrapper authors (Python pyO3, Go
  cgo) MUST surface this metric so observability dashboards can track silent
  default-on bypasses; do not duplicate the accounting per-language.

The constants `DIGEST_HEX_LEN`, `COR_VERIFY_OK`, `COR_VERIFY_ERR_*`
are also exported in the cbindgen-generated header at
[`include/corelink_client_verify.h`](include/corelink_client_verify.h).

### Building the FFI artifact

The FFI module is gated behind the `ffi` cargo feature so the default
workspace build does not pay the cdylib linker cost. SDK packagers
build with the feature enabled:

```bash
cargo build --release --features ffi
# Emits target/release/libcorelink_client_verify.{a,dylib,so}
```

When regenerating the C header via cbindgen, define the corresponding
preprocessor macros so the gated surface is emitted:

```bash
cbindgen --crate corelink-client-verify \
  --config crates/corelink-client-verify/cbindgen.toml \
  --output include/corelink_client_verify.h
# Downstream C consumers compile with:
#   -DCORELINK_CLIENT_VERIFY_FFI -DCORELINK_CLIENT_VERIFY_STREAM
```

JS/TS WASM consumers use a separate wasm-bindgen pipeline (lib.rs Rust-
native types crossing the wasm-bindgen boundary directly), NOT cbindgen.

## Default-on contract

Bypassing verify requires the explicit `VerifyConfig::disabled()` builder.
The underlying fields are `pub(crate)`, so a downstream caller cannot
silently flip `enabled = false` via a struct-update expression. Construction
of a disabled verifier:

- bumps the process-wide `opt_out_total` counter (always, even when warn
  is suppressed for tests / pyO3 teardown paths),
- emits `tracing::warn!(target = "corelink_client_verify::optout", code =
  "COR_CAS_VERIFY_DISABLED", ...)` when `warn_on_optout` is set,
- leaves `verify(...)` returning `VerifyError::VerifyDisabled` rather than
  silently `Ok(())` — this exists so a caller that thought they had verify
  on does not silently mask a configuration bug.

## Detection latency

`verify_stream` finalizes the BLAKE3 hasher at end-of-stream and surfaces
mismatch as the FINAL stream item (`Err(StreamVerifyError::Mismatch(...))`).
Per-chunk Merkle proofs that would catch corruption mid-download are
deferred to S-05 (WI-S05-005, multipart Merkle). The memory benefit
(incremental hash, no full-body buffering — peak < 2 MiB for a 5 MiB blob)
holds independently of the detection-latency tradeoff.

> **Streaming contract (consumer responsibility).** `verify_stream`
> yields chunks BEFORE the end-of-stream BLAKE3 finalization, so the
> SDK can pipeline I/O. **The consumer MUST NOT persist, hand off, or
> treat the body as integrity-checked until the stream terminates
> *cleanly* (no `Err(Mismatch)` final item).** A common pattern is to
> write the chunks to a *staging* path (tempfile, in-memory buffer)
> and atomically rename / commit only after the stream completes Ok.
> The `examples/verify_stream.rs` example demonstrates the staging
> pattern.
>
> The sync `verify` API does NOT have this footgun — it returns
> `Ok(())` only when the full body matches — so SDKs that can afford
> to buffer the whole body (≤ 4 MiB blobs by convention) should
> prefer it. Use `verify_stream` only when you actually need the
> memory bound (e.g., 5 MiB blobs on a constrained worker).

## Anti-patterns

- ❌ Don't construct `VerifyConfig` via struct-update (`VerifyConfig {
  enabled: false, ..VerifyConfig::default() }` — the field is private).
- ❌ Don't depend on Worker internals from this crate; it is the FFI
  distribution unit.
- ❌ Don't call `verify(...)` on a verifier constructed via
  `disabled()` and treat `Ok(())` as legal: `verify` returns
  `VerifyError::VerifyDisabled` and the caller MUST handle it.
- ❌ Don't use sync `verify(...)` for blobs > 4 MiB; use
  `verify_stream(...)` (feature `stream`) to keep memory bounded.

## Features

- `stream` — async `verify_stream` on top of Tokio `AsyncRead` and
  incremental BLAKE3.
- `ffi` — C-ABI surface for cbindgen (Python pyO3 + Go cgo).

Default features: none. Stand-alone consumers (the typical SDK) take only
the rlib and the sync `verify` API.
