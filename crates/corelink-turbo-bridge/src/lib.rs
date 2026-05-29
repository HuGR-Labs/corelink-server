//! `corelink-turbo-bridge` — Turborepo remote-cache bridge for CoreLink CAS.
//!
//! # What this crate ships
//!
//! Turborepo (by Vercel) supports any HTTP server that speaks the Vercel
//! `/v8/artifacts/:hash` remote-cache protocol. Users set:
//!
//! ```text
//! TURBO_API=https://corelink-api.humangr.com
//! TURBO_TOKEN=<their CoreLink PAT>
//! ```
//!
//! and Turbo treats CoreLink CAS as its build-output cache, backed by R2,
//! instead of paying Vercel.
//!
//! # API surface
//!
//! The minimum subset Turbo requires:
//!
//! - `PUT  /v8/artifacts/<hash>?teamId=<team_id>&slug=<slug>` — store artifact.
//! - `GET  /v8/artifacts/<hash>?teamId=<team_id>&slug=<slug>` — retrieve artifact.
//! - `POST /v8/artifacts/events` — telemetry (accept-and-drop).
//! - `POST /v8/artifacts/status` — returns `{"status":"enabled"}`.
//!
//! # Hash semantics
//!
//! Turbo's `<hash>` is opaque to CoreLink. It may be xxhash, a SHA-512 prefix,
//! or any other algorithm Turbo chooses. CoreLink stores bytes under the raw
//! hash string as a key without verifying hash⟶bytes correspondence — Turbo
//! owns the hash algorithm; CoreLink owns the durability.
//!
//! Accepted hash strings: any hex string up to 128 characters (DoS guard).
//! Longer strings are rejected with [`TurboBridgeError::HashTooLong`].
//!
//! # Crate modules
//!
//! - [`handler`] — [`TurboArtifactHandler`] trait + [`InMemoryTurboHandler`]
//!   deterministic fake for tests.
//! - [`adapter`] — [`CasAdapterTurboHandler`] that bridges to a pair of
//!   [`CasReadStore`] / [`CasWriteStore`] port traits (opaque-key semantics).
//! - [`events`] — telemetry accept-and-drop types.
//! - [`status`] — static `/v8/artifacts/status` response types.
//! - [`audit`] — [`TurboAuditEvent`] + [`TurboAuditEventKind`] (5 variants).
//! - [`error`] — [`TurboBridgeError`] taxonomy.
//!
//! # Invariants
//!
//! - `team_id` from the request MUST equal the authenticated `caller_tenant`;
//!   violations return [`TurboBridgeError::CrossTenantDenied`] + emit a
//!   [`TurboAuditEventKind::PutDenied`] / [`TurboAuditEventKind::GetDenied`]
//!   audit event BEFORE returning.
//! - All audit emits happen BEFORE any state mutation (fail-CLOSED ordering).
//! - No `unwrap()` / `expect()` / `panic!()` outside `#[cfg(test)]`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod adapter;
pub mod audit;
pub mod error;
pub mod events;
pub mod handler;
pub mod status;

pub use audit::{TurboAuditEvent, TurboAuditEventKind, TurboAuditSink};
pub use error::TurboBridgeError;
pub use events::{TurboEventsRequest, TurboEventsResponse};
pub use handler::{
    InMemoryTurboHandler, TurboArtifactHandler, TurboGetRequest, TurboGetResponse,
    TurboPutRequest, TurboPutResponse, TurboStatusResponse,
};
pub use status::TurboStatusRequest;

/// Vercel Remote Cache API version this bridge implements.
pub const VERCEL_API_VERSION: &str = "v8";

/// Maximum byte length of a Turbo artifact hash string accepted by the bridge.
/// Hashes longer than this are rejected with [`TurboBridgeError::HashTooLong`]
/// as a denial-of-service guard (key-space amplification).
pub const MAX_HASH_LEN: usize = 128;
