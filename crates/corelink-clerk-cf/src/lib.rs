//! Production Cloudflare Worker bindings for `corelink-clerk`.
//!
//! This crate provides concrete implementations of the two trait
//! abstractions defined in `corelink-clerk`:
//!
//! - [`CfKvJwksCache`]: implements [`corelink_clerk::KvJwksCache`] on top of a
//!   Cloudflare KV namespace (`worker::kv::KvStore`). Replaces the
//!   in-memory `fakes::InMemoryKvCache` used in unit tests.
//!
//! - [`CfJwksFetcher`]: implements [`corelink_clerk::JwksFetcher`] on top of
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

pub mod audit_sink;
pub mod cf_fetch;
pub mod cf_kv;
pub mod clerk_health_do;
pub mod dsr_statuspage_cron;
pub mod health;
pub mod prod_wiring;

// Wave-25: D1-backed `TenantConfigStore` wire for the Neon analytics
// shadow region pin. Gated by the `tenant-region-real` feature so the
// optional `corelink-audit-chain` + `uuid` deps stay out of the
// default wasm32 build surface. Closes the wave-21 audit-doc §7
// caveat "`corelink-clerk-cf::prod_wiring` not yet updated to
// construct `D1TenantRegionResolver`".
#[cfg(feature = "tenant-region-real")]
pub mod tenant_region_real;

pub use audit_sink::{AuditEvent, AuditSink};
pub use cf_fetch::CfJwksFetcher;
pub use cf_kv::CfKvJwksCache;
pub use clerk_health_do::{
    ClerkHealthLogic, ClerkHealthState, HealthDoError, HealthDoOp, HealthMethod,
    HealthRecord, ParsedRoute, RecordResponse, UpsertRequest, DEFAULT_TTL_MS,
};
// The `#[durable_object]` actor class is the wasm32 binding entry that
// the CF Workers runtime instantiates by name (matched to wrangler.toml
// `class_name = "ClerkHealthDo"`).
#[cfg(target_arch = "wasm32")]
pub use clerk_health_do::ClerkHealthDo;
pub use prod_wiring::{CfRealBindings, TenantContext, WiringError};
// Wave-26: CF Worker fetch-handler prefetch wire — orchestration glue
// between `D1TenantConfigStore::prefetch` (async) and
// `TenantRegionResolver::resolve_region` (sync). See
// `specs/_audits/2026-05-16-cf-worker-prefetch-wire.md`.
#[cfg(feature = "tenant-region-real")]
pub use prod_wiring::{
    prefetch_request_prelude, tenant_uuid_for_label, PrefetchWireError, RequestPrelude,
};
