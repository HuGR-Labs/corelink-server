//! Storage adapters for the CoreLink CAS hot path.
//!
//! Every backend (R2, KV, D1) lives behind a thin trait abstraction so the
//! crate's tests can run against in-memory fakes that exercise the same
//! tenant-path / error-mapping logic as production. Cloudflare-specific
//! bindings (`worker::R2Bucket`, …) are wired in at deploy time alongside the
//! REAPI handler (WI-S01-005); this module is intentionally CF-runtime-free
//! so it builds and tests on the developer host as well as on
//! `wasm32-unknown-unknown`.

pub mod blob_store;
pub mod error;
pub mod key;
pub mod metrics;
pub mod r2;

pub use blob_store::BlobStore;
