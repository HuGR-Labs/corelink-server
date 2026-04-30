//! Canonical `Capabilities.GetCapabilities` response (WI-S01-005 §6.1.3).
//!
//! Per WI v1.1.0 + spec contract S-01 v1.4.0:
//!
//! - `max_batch_total_size_bytes = 4 * 1024 * 1024` (4 MiB) — the
//!   `BatchUpdateBlobsRequest` aggregate cap. Clients exceeding it should
//!   either fragment their batch or use `ByteStream::Write` for the
//!   oversized blob.
//! - `max_cas_blob_size_bytes = 5 * 1024 * 1024` (5 MiB) — single-blob upper
//!   bound (matches `corelink_worker::storage::r2::SINGLE_BLOB_LIMIT_BYTES`).
//! - `digest_functions = [BLAKE3]` — BLAKE3 is the canonical S-01 hash and
//!   the only function the write path accepts. We deliberately do **not**
//!   advertise SHA256 (or any other) here: REAPI clients negotiate the
//!   digest function from this list, so advertising a function the server
//!   does not honor would yield silently-rejected uploads on the wire.
//!   Multi-algo support lands in S-12 (hash agility); the WI-S01-005 v1.2.0
//!   contract pins BLAKE3-only for S-01.
//! - All other Cache fields default-zeroed (no compression in S-01, no AC
//!   update in S-01, no symlink/CDC in S-01).
//! - `execution_capabilities` UNSET — CoreLink S-01 is cache-only.
//! - SemVer fields populated with REAPI v2.12.0 (the vendored upstream).
//!
//! Tested via property test (`tests/capabilities.rs`) that the canonical
//! values never drift without a deliberate edit; the WI changelog must
//! capture any change.

/// Canonical `MaxBatchTotalSizeBytes` value advertised in
/// `CacheCapabilities.max_batch_total_size_bytes`. 4 MiB.
pub const MAX_BATCH_TOTAL_SIZE_BYTES: i64 = 4 * 1024 * 1024;

/// Canonical `max_cas_blob_size_bytes` value advertised in
/// `CacheCapabilities.max_cas_blob_size_bytes`. 5 MiB. Matches
/// `corelink_worker::storage::r2::SINGLE_BLOB_LIMIT_BYTES`.
pub const MAX_CAS_BLOB_SIZE_BYTES: i64 = 5 * 1024 * 1024;

/// REAPI version advertised in [`server_capabilities`]. Bumped in lock-step
/// with the vendored proto under `proto/build/bazel/remote/execution/v2/`.
pub const REAPI_VERSION: SemVer = SemVer {
    major: 2,
    minor: 12,
    patch: 0,
};

/// Pure-data SemVer struct, matching the vendored
/// `build.bazel.semver.SemVer` proto field tags. The `host-server` feature
/// converts this to the proto type at the gRPC seam in `handler.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemVer {
    /// Major version.
    pub major: i32,
    /// Minor version.
    pub minor: i32,
    /// Patch version.
    pub patch: i32,
}

/// Pure-data digest function enum, matching the vendored
/// `build.bazel.remote.execution.v2.DigestFunction.Value` field tags.
///
/// Only the values CoreLink S-01 advertises are exposed; `Unknown` is
/// included so callers reading a wire-decoded message can match exhaustively.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum DigestFunction {
    /// Wire tag `0`. MUST NOT be returned by the server in CoreLink.
    Unknown = 0,
    /// Wire tag `1`. SHA-256 digest function. Advertised for legacy clients.
    Sha256 = 1,
    /// Wire tag `9`. BLAKE3 digest function. Canonical S-01 hash.
    Blake3 = 9,
}

/// Canonical `CacheCapabilities` payload. Pure-data; the `host-server`
/// feature converts this to the proto type at the gRPC seam.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheCapabilities {
    /// Digest functions advertised. `[Blake3, Sha256]` for S-01.
    pub digest_functions: Vec<DigestFunction>,
    /// Aggregate batch cap. 4 MiB.
    pub max_batch_total_size_bytes: i64,
    /// Single-blob cap. 5 MiB.
    pub max_cas_blob_size_bytes: i64,
}

/// Canonical `ServerCapabilities` payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerCapabilities {
    /// Cache half (always populated; CoreLink is cache-first).
    pub cache_capabilities: CacheCapabilities,
    /// SemVer advertised as `low_api_version` AND `high_api_version`.
    pub api_version: SemVer,
}

/// Build the canonical [`CacheCapabilities`] for S-01 deploys.
#[must_use]
pub fn cache_capabilities() -> CacheCapabilities {
    CacheCapabilities {
        // BLAKE3 only — see module rustdoc for the canonical contract.
        digest_functions: vec![DigestFunction::Blake3],
        max_batch_total_size_bytes: MAX_BATCH_TOTAL_SIZE_BYTES,
        max_cas_blob_size_bytes: MAX_CAS_BLOB_SIZE_BYTES,
    }
}

/// Build the canonical [`ServerCapabilities`] for S-01 deploys.
#[must_use]
pub fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        cache_capabilities: cache_capabilities(),
        api_version: REAPI_VERSION,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;

    #[test]
    fn batch_cap_is_4_mib_canonical() {
        assert_eq!(MAX_BATCH_TOTAL_SIZE_BYTES, 4 * 1024 * 1024);
        assert_eq!(MAX_BATCH_TOTAL_SIZE_BYTES, 4_194_304);
    }

    #[test]
    fn single_blob_cap_is_5_mib_canonical() {
        assert_eq!(MAX_CAS_BLOB_SIZE_BYTES, 5 * 1024 * 1024);
        assert_eq!(MAX_CAS_BLOB_SIZE_BYTES, 5_242_880);
    }

    #[test]
    fn reapi_version_is_2_12_0() {
        assert_eq!(REAPI_VERSION.major, 2);
        assert_eq!(REAPI_VERSION.minor, 12);
        assert_eq!(REAPI_VERSION.patch, 0);
    }

    #[test]
    fn digest_function_wire_tags_match_proto() {
        assert_eq!(DigestFunction::Unknown as i32, 0);
        assert_eq!(DigestFunction::Sha256 as i32, 1);
        assert_eq!(DigestFunction::Blake3 as i32, 9);
    }

    #[test]
    fn cache_capabilities_advertises_blake3_only() {
        let c = cache_capabilities();
        assert_eq!(c.digest_functions, vec![DigestFunction::Blake3]);
    }

    #[test]
    fn server_capabilities_pin_to_cache_payload() {
        let s = server_capabilities();
        assert_eq!(s.cache_capabilities, cache_capabilities());
        assert_eq!(s.api_version, REAPI_VERSION);
    }

    #[test]
    fn single_blob_matches_corelink_worker_constant() {
        // `corelink_worker::storage::r2::SINGLE_BLOB_LIMIT_BYTES` is `usize`
        // there; cross-cast both sides via i64 to keep the assertion precise.
        let from_worker: i64 = corelink_worker::storage::r2::SINGLE_BLOB_LIMIT_BYTES
            .try_into()
            .unwrap_or(i64::MAX);
        assert_eq!(from_worker, MAX_CAS_BLOB_SIZE_BYTES);
    }
}
