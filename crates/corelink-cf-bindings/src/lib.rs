//! Production Cloudflare Worker binding adapters for CoreLink.
//!
//! This crate wires the trait-abstraction-defer pattern (used across
//! ~70 CoreLink crates) onto real `worker::*` types:
//!
//! | Trait surface (host)               | Adapter (CF wasm32)       | `worker::*` type                |
//! |------------------------------------|---------------------------|---------------------------------|
//! | `corelink_worker::storage::r2::R2Backend` | [`CfR2BucketAdapter`] | `worker::r2::Bucket`            |
//! | `corelink_worker::cache::kv::KvBackend`   | [`CfKvNamespaceAdapter`] | `worker::kv::KvStore`         |
//! | (D1 — no canonical trait yet; raw accessor) | [`CfD1DatabaseAdapter`]  | `worker::D1Database`           |
//! | (DO stub access)                   | [`CfDurableObjectAdapter`] | `worker::durable::ObjectNamespace` |
//!
//! ## Compile target
//!
//! `wasm32-unknown-unknown` ONLY. The crate root carries
//! `#![cfg(target_arch = "wasm32")]` so a native build (`cargo build
//! --workspace`) produces an empty rlib with no symbols — this allows
//! the workspace to compile on a developer host that does not have the
//! wasm32 target installed without per-crate target gates leaking into
//! every consumer.
//!
//! ```sh
//! cargo build --target wasm32-unknown-unknown -p corelink-cf-bindings
//! ```
//!
//! ## CTRL-PRIV-001 (logging)
//!
//! Adapters in this crate MUST NOT log R2 keys, D1 query results, or
//! KV values beyond a stable correlation_id / blob digest hash. The
//! caller is responsible for redaction; this crate emits no
//! `tracing::*` calls that carry binding payloads.
//!
//! ## Trait-fake coexistence
//!
//! `InMemoryR2 / InMemoryKv` fakes in `corelink-worker` are NOT removed
//! by this crate. They remain the canonical test surface for native
//! `cargo test --workspace` runs. The CF Worker boot path
//! (`apps/server/src/main.rs` + `crates/corelink-clerk-cf`) constructs
//! `Cf*Adapter` once per request and injects them into the same call
//! sites that accept any `R2Backend` / `KvBackend` impl in tests.

#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]

pub mod cf_d1;
pub mod cf_do;
pub mod cf_kv;
pub mod cf_r2;

pub use cf_d1::CfD1DatabaseAdapter;
pub use cf_do::CfDurableObjectAdapter;
pub use cf_kv::CfKvNamespaceAdapter;
pub use cf_r2::CfR2BucketAdapter;
