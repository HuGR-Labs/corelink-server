// R2 storage adapter using the AWS S3-compatible API.
//
// This module provides:
//
// - [`R2S3Client`] — low-level async `put_object` / `get_object`
//   wrapper over `aws-sdk-s3` pointed at the R2 S3 endpoint.
// - [`R2CasHandler`] — a sync `CasReadHandler` + `CasWriteHandler`
//   implementation that uses [`R2S3Client`] for durable storage and
//   `derive_prefix` for tenant-scoped R2 keys.
//
// # Key scheme
//
// ```text
// <region>/<tenant_prefix_16>/<digest>
// ```
//
// The `tenant_prefix_16` is derived via
// `corelink_tenant_path::derive_prefix` so cross-tenant key
// co-residence is impossible (layer 5 of `INV-TENANT-ISOLATION`).
//
// # Sync wrapper
//
// The `CasReadHandler` / `CasWriteHandler` traits are synchronous
// (they exist in the pre-async R-prep layer). `R2CasHandler` bridges
// the async S3 SDK into the sync trait surface by using
// `tokio::runtime::Handle::current().block_on(...)`. The server runs
// inside a tokio runtime, so a handle is always available.
//
// # Security charter compliance
//
// - No credentials in code; constructed from [`StorageEnv`].
// - No secrets logged; tracing events contain bucket + key only.
// - No `unwrap()` / `expect()` / `panic!()` outside `#[cfg(test)]`.

use std::sync::Arc;
use std::time::Instant;

use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;
use corelink_byok::{CryptoContext, CryptoMode, Tcs};
use corelink_handler_cas::{
    AuditEvent, AuditEventKind, AuditSink, CasDeleteHandler, CasHandlerError, CasListHandler,
    CasReadHandler, CasReadRequest, CasReadResponse, CasWriteHandler, CasWriteRequest,
    CasWriteResponse, DigestAlgo, SliObservation, SliObserver,
};
// `InMemoryAuditSink` is now used only by tests (the deployed builder wires the
// durable D1 sink); gate the import so the non-test build stays warning-clean.
#[cfg(test)]
use corelink_handler_cas::InMemoryAuditSink;
// Same reason: the capture-everything SLI observer is a TEST fixture now that
// the deployed builders wire the constant-memory `CountingSliObserver` (B-057).
#[cfg(test)]
use corelink_handler_cas::InMemorySliObserver;
use corelink_hash::Digest;
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};

use crate::storage::d1_audit_sink::{
    ac_audit_sink_from_d1_concrete, cas_audit_sink_from_d1_concrete,
};
use crate::storage::d1_http::D1HttpClient;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq;
use tracing::{debug, warn};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::byok_cas::{
    ac_crypto_context, ac_crypto_context_for, cas_crypto_context, cas_crypto_context_for,
    decrypt_cas_blob, encrypt_cas_blob, engagement_for, harden_digest, ByokConfigCache,
    ByokEngagement, ModeBEncryptor, TcsResolver,
};
use super::StorageEnv;
use crate::customer_d1::ByokCryptoMode;

/// Low-level async R2/S3 client.
///
/// Wraps `aws-sdk-s3` with a custom endpoint set to the R2
/// S3-compatible URL. Credentials are taken from [`StorageEnv`] and
/// never logged.
#[derive(Debug)]
#[non_exhaustive]
pub struct R2S3Client {
    inner: Client,
    /// Default bucket for CAS blobs (e.g. `corelink-cas-prod`).
    bucket: String,
    /// Per-key serialization locks for [`Self::delete_if_present`] (rt-nuclear
    /// #6/#10/#14 — concurrent double-DELETE over-release). HEAD-then-DELETE is
    /// non-atomic and S3 `DeleteObject` neither reports prior size nor supports a
    /// "delete-and-return-size" op, so two racing deletes of the same key both
    /// HEAD the size and both report it reclaimed → the byte accountant releases
    /// it twice → free headroom. We serialize the measure-and-delete per key in
    /// this process so AT MOST ONE racer observes the object present (HEAD ⇒
    /// `Some(size)`) and removes it; every other racer HEADs absent AFTER the
    /// delete and returns `None` (releases 0). The map is pruned on release so it
    /// does not grow without bound.
    delete_locks:
        std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
}

/// Outcome of [`R2S3Client::get_capped`].
///
/// Three states, not two: "absent" and "present but refused" are different
/// answers and the caller must not be able to conflate them — a 404 for an
/// object that exists would tell the client to re-upload bytes we already hold.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CappedGet {
    /// The object exists and is within the ceiling.
    Found(Vec<u8>),
    /// No such key.
    Missing,
    /// The object exists but is over the ceiling and was NOT read.
    /// `actual_bytes` is `None` when the storage layer reported no content
    /// length at all — refused for the same reason, since the size that would
    /// have been buffered is unknown.
    TooLarge {
        /// Size the storage layer reported, when it reported one.
        actual_bytes: Option<u64>,
    },
}
