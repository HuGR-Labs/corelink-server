//! Production Cloudflare Worker binding adapters for CoreLink.
//!
//! This crate wires the trait-abstraction-defer pattern (used across
//! ~70 CoreLink crates) onto real `worker::*` types:
//!
//! | Trait surface (host)                          | Adapter (CF wasm32)       | `worker::*` type                   |
//! |-----------------------------------------------|---------------------------|------------------------------------|
//! | `corelink_worker::storage::r2::R2Backend`     | [`CfR2BucketAdapter`]     | `worker::r2::Bucket`               |
//! | `corelink_worker::storage::r2::R2Backend` (+ extended ops) | [`r2_real::CfR2BucketReal`] | `worker::r2::Bucket`     |
//! | `corelink_worker::cache::kv::KvBackend`       | [`CfKvNamespaceAdapter`]  | `worker::kv::KvStore`              |
//! | (D1 — no canonical trait yet; raw accessor)   | [`CfD1DatabaseAdapter`]   | `worker::D1Database`               |
//! | (DO stub access)                              | [`CfDurableObjectAdapter`] | `worker::durable::ObjectNamespace` |
//!
//! ## Compile target
//!
//! Most adapters in this crate are `wasm32-unknown-unknown` only — they
//! depend on `worker::*` types that wrap `js-sys` / `wasm-bindgen`
//! `JsValue` and have no native shim. Per-module `#[cfg(target_arch =
//! "wasm32")]` gating keeps the crate buildable on native targets
//! (developer host, CI workspace build, native unit tests) while
//! preserving the wasm32 production code path.
//!
//! [`r2_real::CfR2BucketReal`] is the canonical exception: it provides a
//! **native stub** (returning `R2Error::Backend("WasmOnly: ...")`) so
//! callers can construct the type on the host for trait-bound testing
//! without conditional compilation in every consumer crate. See the
//! crate-level `cf-binding-real-pattern` doc for the replication recipe
//! used by D1/KV/DO follow-ups.
//!
//! ```sh
//! cargo build --target wasm32-unknown-unknown -p corelink-cf-bindings
//! cargo build -p corelink-cf-bindings   # native stub build
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

#![forbid(unsafe_code)]

#[cfg(target_arch = "wasm32")]
pub mod cf_d1;
#[cfg(target_arch = "wasm32")]
pub mod cf_do;
#[cfg(target_arch = "wasm32")]
pub mod cf_kv;
#[cfg(target_arch = "wasm32")]
pub mod cf_r2;

/// Real CF R2 binding with extended operations (head/get/put/delete/list +
/// multipart) and tenant-prefix enforcement. Dual-target: wasm32 wires
/// `worker::r2::Bucket`; native build provides a stub that returns
/// `R2Error::Backend("WasmOnly: …")` so callers can construct the type on the
/// host without conditional compilation. See module docs for the replication
/// pattern (D1/KV/DO follow-ups).
pub mod r2_real;

/// Real CF Durable Object binding with tenant-scoped naming
/// (`tenant:<id>:<purpose>`) + audit fence (fail-CLOSED) on every stub
/// fetch. Dual-target: wasm32 wires `worker::ObjectNamespace` /
/// `worker::Stub`; native build provides a stub that returns
/// `DoError::Backend("WasmOnly: …")` and a `FakeDoRouter` injection
/// point so tests can exercise the full round-trip without the wasm32
/// toolchain. See module docs and the pattern doc
/// `specs/_audits/2026-05-15-cf-binding-real-pattern.md`.
pub mod do_real;

#[cfg(target_arch = "wasm32")]
pub use cf_d1::CfD1DatabaseAdapter;
#[cfg(target_arch = "wasm32")]
pub use cf_do::CfDurableObjectAdapter;
#[cfg(target_arch = "wasm32")]
pub use cf_kv::CfKvNamespaceAdapter;
#[cfg(target_arch = "wasm32")]
pub use cf_r2::CfR2BucketAdapter;

pub use r2_real::{CfR2BucketReal, R2Op, TenantPrefix, TenantScopedKey};

pub use do_real::{CfDurableObjectReal, DoError, DoOp, DoTenantPrefix, TenantScopedName};
