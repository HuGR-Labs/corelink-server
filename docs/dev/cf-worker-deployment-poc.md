# CF Worker Deployment POC — `corelink-clerk-cf`

**Date:** 2026-05-07
**Author:** Autonomous hardening sprint (agent 3-of-3)
**Status:** Build verified; smoke test ready for `wrangler dev`

## Summary

This document records what was wired up, what trait surface bugs were found,
and what next steps look like for deploying `corelink-clerk-cf` to a real
Cloudflare Worker.

## What was wired up

New crate `crates/corelink-clerk-cf` provides production implementations of the
two trait abstractions defined in `corelink-clerk`:

### `CfKvJwksCache` (`src/cf_kv.rs`)

Implements `KvJwksCache` on top of `worker::kv::KvStore`. The JWKS document is
serialised as a JSON envelope `{"stored_at_unix_secs": <u64>, "jwks_json":
"<raw jwks string>"}` so the adapter's belt-and-suspenders freshness check
(comparing `stored_at + ttl > now`) works independently of CF KV's own TTL
expiry.

### `CfJwksFetcher` (`src/cf_fetch.rs`)

Implements `JwksFetcher` using `worker::Fetch::Request`. This is a zero-size
struct; the CF Fetch API is a runtime global, not a binding. Correct for the
production case where the JWKS endpoint is the Clerk SSO HTTPS URL.

### `GET /health` handler (`src/health.rs`)

End-to-end HTTP handler that:
1. Reads `clerk:health:last_seen` from the `CLERK_JWKS_KV` namespace.
2. Writes the current ISO timestamp back to that key.
3. Creates (if absent) and inserts into `clerk_audit_health` D1 table.
4. Returns `{"status":"ok","kv_value":"<old value or none>","d1_rowid":<n>}`.

The handler is wired via `#[worker::event(fetch)]` in `health.rs`.

### `wrangler.toml` (`crates/corelink-clerk-cf/wrangler.toml`)

Declares the required bindings with PLACEHOLDER IDs:
- `CLERK_JWKS_KV` — KV namespace
- `CLERK_DB` — D1 database

## Trait surface bugs found

### Bug 1 (CRITICAL): `corelink-clerk` pulls `ring` unconditionally — wasm32 incompatible

**Symptom:** `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`
failed with `ring: unable to create target: 'No available targets are
compatible with triple "wasm32-unknown-unknown"'`.

**Root cause:** `corelink-clerk` depends on `jsonwebtoken` unconditionally.
`jsonwebtoken` pulls in `ring 0.17` which compiles C code via `cc-rs`. Apple
clang (and most standard Rust toolchains on macOS/Linux) do NOT include a
wasm32 C frontend. Ring's `wasm32_unknown_unknown_js` feature exists but only
affects random-number generation — it does NOT skip the C compilation of
RSA/EC primitives.

**Fix applied:** Added a `jwt-adapter` feature flag to `corelink-clerk` that
gates `jsonwebtoken` and the `adapter` module. The `default` feature set
includes `jwt-adapter` (backward compatible for all native crates). The
wasm32 CF binding crate uses `default-features = false` to exclude ring.

Also required: updating `[workspace.dependencies]` to set
`default-features = false` for `corelink-clerk`, then explicitly adding
`features = ["jwt-adapter"]` to all native consumers
(`corelink-worker` in both `[dependencies]` and `[dev-dependencies]`,
`corelink-clerk` own dev-dependencies).

**Impact for other 40 crates:** ANY crate that needs to compile to
wasm32-unknown-unknown AND depends on `corelink-clerk` must use
`default-features = false`. The trait surfaces themselves (JwksFetcher,
KvJwksCache, Jwks, ClerkPrincipal, etc.) are wasm32-clean.

**Verdict:** The design decision to put `ClerkAdapter` and the JWT validation
in the same crate as the trait definitions forces this feature-gate split.
For long-term hygiene, consider extracting traits into a separate
`corelink-clerk-traits` crate (no `ring` dep).

### Bug 2 (IMPORTANT): `KvJwksCacheFuture` requires `Send`; CF KV futures are `!Send`

**Symptom:** `Box::pin(async move { self.store.get(...).text().await ... })`
failed with `Rc<RefCell<futures::Inner>> cannot be sent between threads safely`.

**Root cause:** The `KvJwksCacheFuture<'a, T>` type alias is defined as:

```rust
Pin<Box<dyn Future<Output = Result<T, KvJwksCacheError>> + Send + 'a>>
```

The `+ Send` bound was chosen for the in-memory fake (which IS Send, since it
uses `std::sync::Mutex`). However, `worker::kv::KvStore`'s async methods
return futures containing `js_sys::JsFuture`, which wraps
`Rc<RefCell<...>>` and is therefore `!Send` by Rust's type system.

**Fix applied:** Used `worker::send::SendFuture::new(async { ... })` as the
canonical workers-rs pattern to wrap the non-Send KV futures. This is safe
because CF Workers are single-threaded and never actually move futures across
threads. The `unsafe impl Send for KvStore` in workers-rs confirms this is
intentional.

**Impact assessment:** The `JwksFetchFuture<'a>` type alias in `jwks.rs` has
the same `+ Send` bound. This means `CfJwksFetcher::fetch` also needed
`worker::send::SendFuture`. Both are fixed.

**Long-term recommendation:** Document on the trait definitions that
`Send` is a polite fiction for CF Worker impls; all CF impls MUST wrap
with `worker::send::SendFuture`. Alternatively, consider a `?Send` version
of the future type alias for the wasm32 profile.

### Non-bug confirmed: `&self` on trait methods is correct for CF

The `KvJwksCache` trait uses `&self` (not `&mut self`) on `get`, `set`, and
`delete`. This was validated as CORRECT for CF Workers: the KV operations are
async calls to the JS runtime, not in-process state mutations. No interior
mutability is needed on the Rust side.

### Non-bug confirmed: `D1ResultMeta.last_row_id` is `Option<i64>`

After an INSERT the field is `Some(row_id)`. After a DDL statement (CREATE
TABLE) it is `None`. The handler handles both via `unwrap_or(0)` — reviewed
and confirmed correct.

## Build verification

```
cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf
# => Finished dev profile (zero errors, zero warnings in corelink-clerk-cf)

RUSTFLAGS="-D warnings" cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf
# => Finished dev profile (zero errors, zero warnings in corelink-clerk-cf)

cargo build --target wasm32-unknown-unknown --release -p corelink-clerk-cf
# => Finished release profile (zero errors, zero warnings in corelink-clerk-cf)

cargo check --workspace
# => Finished (no errors)

cargo test --workspace --no-run
# => Finished (no errors)
```

## Next steps to actually deploy

1. Create real CF resources:
   ```sh
   wrangler kv namespace create CLERK_JWKS_KV
   wrangler d1 create clerk-audit-db
   ```
2. Replace PLACEHOLDER IDs in `crates/corelink-clerk-cf/wrangler.toml`.
3. Run smoke test locally:
   ```sh
   wrangler dev --config crates/corelink-clerk-cf/wrangler.toml
   curl http://localhost:8787/health
   ```
4. To deploy:
   ```sh
   wrangler deploy --config crates/corelink-clerk-cf/wrangler.toml
   ```

## Files changed

| File | Change |
|------|--------|
| `crates/corelink-clerk-cf/` | NEW crate (4 source files + Cargo.toml + wrangler.toml) |
| `crates/corelink-clerk/Cargo.toml` | Added `jwt-adapter` feature; made `jsonwebtoken` optional |
| `crates/corelink-clerk/src/lib.rs` | Gated `adapter` module on `jwt-adapter` |
| `crates/corelink-clerk/src/principal.rs` | Gated `pub(crate) fn new` constructors on `jwt-adapter` |
| `crates/corelink-worker/Cargo.toml` | Explicit `features = ["jwt-adapter"]` on `corelink-clerk` deps |
| `Cargo.toml` | Added `corelink-clerk-cf` workspace member; `corelink-clerk` workspace dep gets `default-features = false` |
