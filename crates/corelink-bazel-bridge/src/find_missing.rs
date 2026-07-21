//! `findMissingBlobs` handler — REAPI v2 batch blob-existence check.
//!
//! # Protocol
//!
//! REAPI v2 `findMissingBlobs` accepts a JSON body:
//!
//! ```json
//! { "blobDigests": [{"hash":"…","sizeBytes":N}, …] }
//! ```
//!
//! And returns:
//!
//! ```json
//! { "missingBlobDigests": [{"hash":"…","sizeBytes":N}, …] }
//! ```
//!
//! Only the digests that are absent from the CAS appear in the response.
//! This lets Bazel upload only the blobs it does not yet know are cached,
//! dramatically reducing upload traffic on warm builds.
//!
//! # Cap
//!
//! Per [`crate::FIND_MISSING_BLOB_CAP`] (4096), requests with more digests
//! than the cap are rejected with [`crate::error::BazelBridgeError::BatchTooLarge`].
//!
//! # Implementation
//!
//! [`InMemoryFindMissing`] delegates to any [`corelink_handler_cas::CasReadHandler`]
//! via a sequential loop, using the CHEAP [`corelink_handler_cas::CasReadHandler::exists`]
//! HEAD-semantics probe — never a full `read()` (which would download +
//! rehash every present blob; r34 #10). `exists() == Ok(false)` means the
//! blob is missing; `Ok(true)` means present; every other error is
//! propagated (audit failures, cross-tenant denials, etc.) so fail-CLOSED
//! semantics are preserved.

use std::sync::Arc;

use corelink_handler_cas::{CasHandlerError, CasReadHandler, CasReadRequest, DigestAlgo};
use serde::{Deserialize, Serialize};

use crate::digest::{Digest, DigestJson};
use crate::error::BazelBridgeError;

/// JSON request body for `findMissingBlobs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindMissingRequest {
    /// The digests to check.
    #[serde(rename = "blobDigests")]
    pub blob_digests: Vec<DigestJson>,
}

/// JSON response body for `findMissingBlobs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindMissingResponse {
    /// The subset of requested digests that are absent from the CAS.
    #[serde(rename = "missingBlobDigests")]
    pub missing_blob_digests: Vec<DigestJson>,
}

/// Trait for a `findMissingBlobs` handler.
///
/// Implementors receive a `tenant`, a `principal`, a `caller_tenant`, a
/// timestamp, and the validated [`Digest`] slice. They return the subset
/// that is missing from the CAS.
///
/// The trait is synchronous (matching the rest of the CoreLink handler
/// surface). Async wrappers can call this from a `spawn_blocking` task.
pub trait FindMissingHandler: Send + Sync + core::fmt::Debug {
    /// Check which of the requested blobs are absent from the CAS.
    ///
    /// # Errors
    ///
    /// Returns [`BazelBridgeError`] on audit failure, cross-tenant denial,
    /// or internal error. `NotFound` for individual blobs is NOT an error
    /// at the handler level — missing blobs are returned in the response
    /// body.
    fn find_missing(
        &self,
        tenant: &str,
        principal: &str,
        caller_tenant: &str,
        at_unix_ms: u64,
        digests: &[Digest],
    ) -> Result<Vec<Digest>, BazelBridgeError>;
}

/// In-memory `findMissingBlobs` implementation backed by any
/// [`CasReadHandler`]. Delegates existence checks via the cheap
/// `handler.exists()` HEAD probe — NOT `read()`.
///
/// # Why `exists()` and not `read()` (r34 #10)
///
/// `findMissingBlobs` is supposed to be a cheap HEAD/exists probe.
/// Probing existence via `read()` forced a FULL blob download + content
/// rehash per digest; looped up to the batch cap (4096) that became tens
/// of GiB of R2 GET egress + rehash CPU per request, repeatable by any
/// free tenant. [`CasReadHandler::exists`] is a HEAD-semantics probe (the
/// R2 handler overrides it with an `HeadObject`), so the probe transfers
/// no body and performs no rehash.
///
/// `exists()` returns `Ok(false)` for an absent blob (the digest is added
/// to the missing list) and `Ok(true)` for a present one (not missing).
/// Every other [`CasHandlerError`] variant is converted to the
/// appropriate [`BazelBridgeError`] and propagated so audit/cross-tenant
/// invariants are preserved (the storage-layer `NotFound` is absorbed by
/// `exists` into `Ok(false)` and never surfaces here).
pub struct InMemoryFindMissing {
    cas: Arc<dyn CasReadHandler>,
}

impl core::fmt::Debug for InMemoryFindMissing {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryFindMissing")
            .finish_non_exhaustive()
    }
}

impl InMemoryFindMissing {
    /// Construct a new `InMemoryFindMissing` backed by the given CAS read
    /// handler.
    #[must_use]
    pub fn new(cas: Arc<dyn CasReadHandler>) -> Self {
        Self { cas }
    }
}

impl FindMissingHandler for InMemoryFindMissing {
    fn find_missing(
        &self,
        tenant: &str,
        principal: &str,
        caller_tenant: &str,
        at_unix_ms: u64,
        digests: &[Digest],
    ) -> Result<Vec<Digest>, BazelBridgeError> {
        // Cap check is the caller's responsibility (adapter layer), but we
        // double-check here for defence in depth.
        if digests.len() > crate::FIND_MISSING_BLOB_CAP {
            return Err(BazelBridgeError::BatchTooLarge {
                requested: digests.len(),
                cap: crate::FIND_MISSING_BLOB_CAP,
            });
        }

        let mut missing = Vec::new();

        for digest in digests {
            // findMissingBlobs is a REAPI-only (Bazel) surface; existence MUST
            // be probed in the SHA-256 `bazel/sha256/` keyspace the writes land
            // in, else every Bazel blob reports missing (cache never hits).
            let req = CasReadRequest::new(
                tenant,
                digest.hash.as_str(),
                principal,
                caller_tenant,
                at_unix_ms,
            )
            .with_algo(DigestAlgo::Sha256);
            // CHEAP existence probe — `exists()` is HEAD semantics (no
            // body download, no rehash). The R2 handler overrides it with
            // an `HeadObject`; this is what removes the per-digest
            // full-GET + rehash amplification (r34 #10). Present ⇒ NOT
            // missing; absent (`Ok(false)`) ⇒ missing; the storage-layer
            // `NotFound` is absorbed into `Ok(false)` by `exists` so it
            // never reaches this match. Every other error propagates
            // exactly as before (fail-CLOSED, audit/cross-tenant intact).
            match self.cas.exists(req) {
                Ok(true) => {
                    // Blob present — do not add to missing list.
                }
                Ok(false) => {
                    missing.push(digest.clone());
                }
                Err(CasHandlerError::NotFound { .. }) => {
                    missing.push(digest.clone());
                }
                Err(CasHandlerError::CrossTenantDenied {
                    caller,
                    requested_tenant,
                }) => {
                    return Err(BazelBridgeError::CrossTenantDenied {
                        caller,
                        requested: requested_tenant,
                    });
                }
                Err(CasHandlerError::AuditFailed(msg)) => {
                    return Err(BazelBridgeError::AuditFailed(msg));
                }
                Err(CasHandlerError::HashMismatch { .. }) | Err(CasHandlerError::Internal(_)) => {
                    return Err(BazelBridgeError::Internal(
                        "cas exists returned unexpected error during find_missing".into(),
                    ));
                }
                Err(_) => {
                    return Err(BazelBridgeError::Internal(
                        "cas exists returned unrecognised error during find_missing".into(),
                    ));
                }
            }
        }

        Ok(missing)
    }
}

/// Parse a `findMissingBlobs` JSON request body and validate all digests.
///
/// Returns the validated list of [`Digest`]s or the first validation error
/// encountered.
///
/// # Errors
///
/// Returns [`BazelBridgeError::BatchTooLarge`] if the list exceeds
/// [`crate::FIND_MISSING_BLOB_CAP`].
/// Returns [`BazelBridgeError::InvalidDigest`] if any digest fails
/// validation.
/// Returns [`BazelBridgeError::Internal`] if the JSON cannot be
/// deserialised.
pub fn parse_find_missing_request(body: &str) -> Result<Vec<Digest>, BazelBridgeError> {
    let req: FindMissingRequest = serde_json::from_str(body)
        .map_err(|e| BazelBridgeError::InvalidRequest { reason: e.to_string() })?;

    if req.blob_digests.len() > crate::FIND_MISSING_BLOB_CAP {
        return Err(BazelBridgeError::BatchTooLarge {
            requested: req.blob_digests.len(),
            cap: crate::FIND_MISSING_BLOB_CAP,
        });
    }

    req.blob_digests
        .into_iter()
        .map(Digest::try_from)
        .collect::<Result<Vec<_>, _>>()
}

/// Serialise a list of missing [`Digest`]s into a `findMissingBlobs`
/// JSON response body.
///
/// # Errors
///
/// Returns [`BazelBridgeError::Internal`] if serialisation fails (should
/// never happen with well-formed `Digest` values).
pub fn build_find_missing_response(missing: Vec<Digest>) -> Result<String, BazelBridgeError> {
    let resp = FindMissingResponse {
        missing_blob_digests: missing.into_iter().map(DigestJson::from).collect(),
    };
    serde_json::to_string(&resp).map_err(|e| BazelBridgeError::Internal(e.to_string()))
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use corelink_handler_cas::handler::fake_hash;
    use corelink_handler_cas::{
        CasReadResponse, InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver,
    };

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    /// A `CasReadHandler` whose `read()` is FORBIDDEN on the
    /// `findMissingBlobs` probe path: it increments a counter that MUST
    /// remain 0. The cheap `exists()` HEAD probe is the only method
    /// `find_missing` may invoke (r34 #10). `exists()` answers from an
    /// in-memory presence set without ever touching `read()`.
    #[derive(Debug)]
    struct ProbeOnlyCas {
        /// Hashes that are "present" in this fake store.
        present: std::collections::HashSet<String>,
        /// Incremented on EVERY `read()` call — must stay 0 on the probe
        /// path (a full GET would be an egress+rehash amplification bug).
        read_calls: AtomicUsize,
        /// Incremented on every `exists()` call — proves the probe path
        /// actually drove the cheap method.
        exists_calls: AtomicUsize,
    }

    impl ProbeOnlyCas {
        fn new(present: &[&str]) -> Self {
            Self {
                present: present.iter().map(|s| (*s).to_owned()).collect(),
                read_calls: AtomicUsize::new(0),
                exists_calls: AtomicUsize::new(0),
            }
        }
    }

    impl CasReadHandler for ProbeOnlyCas {
        fn read(&self, _req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            // A full-GET on the probe path is the bug WP-F removes. Record
            // it so the assertion can fail loudly rather than silently
            // re-introducing the egress+rehash amplification.
            self.read_calls.fetch_add(1, Ordering::SeqCst);
            panic!("find_missing MUST NOT call read() on the existence-probe path (r34 #10)");
        }

        fn exists(&self, req: CasReadRequest) -> Result<bool, CasHandlerError> {
            self.exists_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.present.contains(&req.hash))
        }
    }

    fn make_handler() -> (Arc<InMemoryAuditSink>, InMemoryCasHandler) {
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        let h = InMemoryCasHandler::new(audit.clone(), sli);
        (audit, h)
    }

    fn valid_digest(hash: &str) -> Digest {
        Digest::new(hash, 5).expect("valid digest")
    }

    #[test]
    fn find_missing_all_absent() {
        let (_, handler) = make_handler();
        let fm = InMemoryFindMissing::new(Arc::new(handler));
        let digests = vec![valid_digest(HASH_A), valid_digest(HASH_B)];
        let missing = fm
            .find_missing("t1", "p1", "t1", 0, &digests)
            .expect("find_missing");
        assert_eq!(missing.len(), 2);
    }

    #[test]
    fn find_missing_all_present() {
        let (_, handler) = make_handler();
        let bytes = b"hello".to_vec();
        let hash = fake_hash(&bytes);
        handler.seed("t1", &hash, bytes.clone()).expect("seed");
        let digest = Digest::new(&hash, bytes.len() as u64).expect("digest");
        let fm = InMemoryFindMissing::new(Arc::new(handler));
        let missing = fm
            .find_missing("t1", "p1", "t1", 0, &[digest])
            .expect("find_missing");
        assert!(missing.is_empty(), "all blobs present → empty missing list");
    }

    #[test]
    fn find_missing_partial() {
        let (_, handler) = make_handler();
        let bytes = b"world".to_vec();
        let hash = fake_hash(&bytes);
        handler.seed("t1", &hash, bytes.clone()).expect("seed");

        let present = Digest::new(&hash, bytes.len() as u64).expect("present digest");
        let absent = valid_digest(HASH_A);

        let fm = InMemoryFindMissing::new(Arc::new(handler));
        let missing = fm
            .find_missing("t1", "p1", "t1", 0, &[present, absent.clone()])
            .expect("find_missing");
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], absent);
    }

    #[test]
    fn find_missing_cross_tenant_propagates_denied() {
        let (_, handler) = make_handler();
        let fm = InMemoryFindMissing::new(Arc::new(handler));
        let digests = vec![valid_digest(HASH_A)];
        // caller_tenant != tenant → cross-tenant denial
        let err = fm
            .find_missing("victim", "attacker", "attacker_t", 0, &digests)
            .expect_err("denied");
        assert!(matches!(err, BazelBridgeError::CrossTenantDenied { .. }));
    }

    #[test]
    fn find_missing_batch_too_large_via_handler() {
        let (_, handler) = make_handler();
        let fm = InMemoryFindMissing::new(Arc::new(handler));
        // Build a slice of 4097 identical digests.
        let d = valid_digest(HASH_A);
        let digests: Vec<Digest> = (0..=crate::FIND_MISSING_BLOB_CAP)
            .map(|_| d.clone())
            .collect();
        let err = fm
            .find_missing("t1", "p1", "t1", 0, &digests)
            .expect_err("too large");
        assert!(matches!(err, BazelBridgeError::BatchTooLarge { .. }));
    }

    /// r34 #10 regression: the `findMissingBlobs` probe path MUST use the
    /// cheap `exists()` HEAD probe and MUST NOT call `read()` (a full GET +
    /// rehash). `ProbeOnlyCas::read()` panics, so any full-GET on the probe
    /// path would crash the test; the assertions also pin the explicit
    /// call counters (read==0, exists==N) and the present/absent split.
    #[test]
    fn find_missing_probe_uses_exists_never_read() {
        let cas = Arc::new(ProbeOnlyCas::new(&[HASH_A]));
        let fm = InMemoryFindMissing::new(cas.clone());

        let present = valid_digest(HASH_A);
        let absent = valid_digest(HASH_B);
        let missing = fm
            .find_missing("t1", "p1", "t1", 0, &[present, absent.clone()])
            .expect("find_missing");

        // Only the absent digest is reported missing — present semantics
        // preserved under the exists() path.
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0], absent);

        // The load-bearing assertion: NOT ONE read() (full GET) happened on
        // the probe path — only the cheap exists() HEAD probe ran, once per
        // digest.
        assert_eq!(
            cas.read_calls.load(Ordering::SeqCst),
            0,
            "find_missing must never call read() on the existence-probe path (r34 #10)"
        );
        assert_eq!(
            cas.exists_calls.load(Ordering::SeqCst),
            2,
            "find_missing must probe each digest via exists() exactly once"
        );
    }

    #[test]
    fn parse_find_missing_request_valid() {
        let body = format!(r#"{{"blobDigests":[{{"hash":"{HASH_A}","sizeBytes":10}}]}}"#);
        let digests = parse_find_missing_request(&body).expect("parse");
        assert_eq!(digests.len(), 1);
        assert_eq!(digests[0].hash, HASH_A);
        assert_eq!(digests[0].size_bytes, 10);
    }

    #[test]
    fn parse_find_missing_request_empty_list() {
        let body = r#"{"blobDigests":[]}"#;
        let digests = parse_find_missing_request(body).expect("parse empty");
        assert!(digests.is_empty());
    }

    #[test]
    fn parse_find_missing_request_invalid_digest_propagates_error() {
        let body = r#"{"blobDigests":[{"hash":"badhash","sizeBytes":10}]}"#;
        let err = parse_find_missing_request(body).expect_err("bad digest");
        assert!(matches!(err, BazelBridgeError::InvalidDigest { .. }));
    }

    #[test]
    fn parse_find_missing_request_bad_json_is_client_bad_request() {
        // A syntactically-invalid client body is a 4xx, NOT a 5xx: it must not
        // pollute the server error-rate/SLO signal (real-client hygiene finding).
        let err = parse_find_missing_request("not json").expect_err("bad json");
        assert!(matches!(err, BazelBridgeError::InvalidRequest { .. }));
        assert_eq!(err.http_status(), 400);
    }

    #[test]
    fn build_find_missing_response_serialises_correctly() {
        let d = Digest::new(HASH_A, 42).expect("digest");
        let json = build_find_missing_response(vec![d]).expect("build response");
        assert!(json.contains("missingBlobDigests"));
        assert!(json.contains(HASH_A));
        assert!(json.contains("42"));
    }

    #[test]
    fn build_find_missing_response_empty() {
        let json = build_find_missing_response(vec![]).expect("empty response");
        let resp: FindMissingResponse = serde_json::from_str(&json).expect("parse");
        assert!(resp.missing_blob_digests.is_empty());
    }
}
