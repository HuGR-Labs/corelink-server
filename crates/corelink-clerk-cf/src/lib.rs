//! Production Cloudflare Worker bindings for `corelink-clerk`.
//!
//! This crate provides concrete implementations of the two trait
//! abstractions defined in `corelink-clerk`:
//!
//! - [`CfKvJwksCache`]: implements [`KvJwksCache`] on top of a
//!   Cloudflare KV namespace (`worker::kv::KvStore`). Replaces the
//!   in-memory `fakes::InMemoryKvCache` used in unit tests.
//!
//! - [`CfJwksFetcher`]: implements [`JwksFetcher`] on top of
//!   `worker::Fetch::Url` (CF Workers Fetch API). Replaces the
//!   in-memory `fakes::StaticJwksFetcher` used in unit tests.
//!
//! Additionally this crate wires a `GET /health` end-to-end HTTP
//! handler that reads from KV (last-seen timestamp) and writes to D1
//! (an audit row) to prove the full binding chain compiles and can be
//! exercised via `wrangler dev`.
//!
//! # Compile target
//!
//! This crate MUST only be compiled to `wasm32-unknown-unknown`. There
//! is no native-host shim — the `worker::kv::KvStore` and
//! `worker::d1::D1Database` types depend on `js-sys` / `wasm-bindgen`
//! JsValue types.
//!
//! ```sh
//! cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf
//! ```
//!
//! # Trait surface bugs found
//!
//! See `docs/dev/cf-worker-deployment-poc.md` for the full analysis.

#![forbid(unsafe_code)]

pub mod cf_fetch;
pub mod cf_kv;
pub mod health;

pub use cf_fetch::CfJwksFetcher;
pub use cf_kv::CfKvJwksCache;
