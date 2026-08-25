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
//! [`InMemoryFindMissing`] delegates to any [`corelink_handler_cas::CasReadHandler`],
//! using the CHEAP [`corelink_handler_cas::CasReadHandler::exists`]
//! HEAD-semantics probe — never a full `read()` (which would download +
//! rehash every present blob; r34 #10). `exists() == Ok(false)` means the
//! blob is missing; `Ok(true)` means present; every other error is
//! propagated (audit failures, cross-tenant denials, etc.) so fail-CLOSED
//! semantics are preserved.
//!
//! When the concrete handler offers the OPTIONAL
//! [`corelink_handler_cas::CasReadHandler::exists_batch`] capability (the R2
//! handler does, in production), the whole digest list is handed over at once
//! so the handler can collapse the per-digest network round trips; every
//! handler that does not (all the in-process/test handlers) keeps the
//! unchanged per-digest `exists()` loop. Both paths classify outcomes through
//! the same [`classify_exists`], so "missing" means the same thing and errors
//! map the same way either way.

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

        // findMissingBlobs is a REAPI-only (Bazel) surface; existence MUST
        // be probed in the SHA-256 `bazel/sha256/` keyspace the writes land
        // in, else every Bazel blob reports missing (cache never hits).
        let reqs: Vec<CasReadRequest> = digests
            .iter()
            .map(|digest| {
                CasReadRequest::new(
                    tenant,
                    digest.hash.as_str(),
                    principal,
                    caller_tenant,
                    at_unix_ms,
                )
                .with_algo(DigestAlgo::Sha256)
            })
            .collect();

        // BATCH PATH — taken only when the concrete handler provides one
        // (`Some(..)`); today that is the R2 handler with the durable D1
        // audit seam wired, i.e. production. It answers the identical
        // question with the identical audit rows, but in a bounded number of
        // round trips instead of one per digest. Outcomes come back in
        // REQUEST ORDER and are classified by the SAME `classify_exists` the
        // serial loop uses, so the two paths cannot diverge on what "missing"
        // means or on how an error is mapped.
        if let Some(batch) = self.cas.exists_batch(&reqs) {
            let flags = batch.map_err(map_exists_error)?;
            if flags.len() != reqs.len() {
                // A batch handler that answers a different number of digests
                // than were asked would silently mis-report presence. Refuse.
                return Err(BazelBridgeError::Internal(
                    "cas exists_batch returned a mismatched result count during find_missing"
                        .into(),
                ));
            }
            let mut missing = Vec::new();
            for (digest, present) in digests.iter().zip(flags) {
                if classify_exists(Ok(present))? {
                    missing.push(digest.clone());
                }
            }
            return Ok(missing);
        }

        // SERIAL FALLBACK — byte-identical to the behaviour every handler
        // had before the batch seam existed (all the test handlers, and any
        // `CasReadHandler` that does not override `exists_batch`).
        let mut missing = Vec::new();
        for (digest, req) in digests.iter().zip(reqs) {
            // CHEAP existence probe — `exists()` is HEAD semantics (no
            // body download, no rehash). The R2 handler overrides it with
            // an `HeadObject`; this is what removes the per-digest
            // full-GET + rehash amplification (r34 #10).
            if classify_exists(self.cas.exists(req))? {
                missing.push(digest.clone());
            }
        }

        Ok(missing)
    }
}

/// Classify ONE existence outcome: `Ok(true)` ⇒ the digest is MISSING and
/// belongs in the response, `Ok(false)` ⇒ present.
///
/// The single place that decides what an `exists` outcome means, shared by the
/// serial loop and the batch path so the two can never drift. Present ⇒ NOT
/// missing; absent (`Ok(false)`) ⇒ missing; the storage-layer `NotFound` is
/// normally absorbed into `Ok(false)` by `exists` and never reaches here, but
/// is mapped to "missing" defensively for handlers that surface it. Every
/// other error propagates (fail-CLOSED — audit failures and cross-tenant
/// denials become errors, never an empty "nothing is missing" answer).
fn classify_exists(result: Result<bool, CasHandlerError>) -> Result<bool, BazelBridgeError> {
    match result {
        Ok(true) => Ok(false),
        Ok(false) | Err(CasHandlerError::NotFound { .. }) => Ok(true),
        Err(other) => Err(map_exists_error(other)),
    }
}

/// Map a fatal [`CasHandlerError`] from an existence probe onto the bridge's
/// error type — the exact mapping the per-digest loop has always used.
fn map_exists_error(err: CasHandlerError) -> BazelBridgeError {
    match err {
        CasHandlerError::CrossTenantDenied {
            caller,
            requested_tenant,
        } => BazelBridgeError::CrossTenantDenied {
            caller,
            requested: requested_tenant,
        },
        CasHandlerError::AuditFailed(msg) => BazelBridgeError::AuditFailed(msg),
        CasHandlerError::HashMismatch { .. } | CasHandlerError::Internal(_) => {
            BazelBridgeError::Internal(
                "cas exists returned unexpected error during find_missing".into(),
            )
        }
        _ => BazelBridgeError::Internal(
            "cas exists returned unrecognised error during find_missing".into(),
        ),
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
/// Returns [`BazelBridgeError::InvalidRequest`] if the JSON body cannot be
/// deserialised (a client error → 400, not a server 500).
pub fn parse_find_missing_request(body: &str) -> Result<Vec<Digest>, BazelBridgeError> {
    let req: FindMissingRequest =
        serde_json::from_str(body).map_err(|e| BazelBridgeError::InvalidRequest {
            reason: e.to_string(),
        })?;

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
    use std::sync::Mutex;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

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

    // ------------------------------------------------------------------
    // The OPTIONAL `exists_batch` seam.
    //
    // Every fake above answers `None` from the default `exists_batch`, so
    // all of them exercise the SERIAL FALLBACK — that is the regression
    // guard proving the fallback stayed byte-identical. The fakes below add
    // the batch capability so the batch path itself is covered.
    // ------------------------------------------------------------------

    /// A `CasReadHandler` that PROVIDES `exists_batch` and FORBIDS the
    /// per-digest `exists()` (and `read()`): once a handler advertises the
    /// batch capability, `find_missing` must hand it the whole list in one
    /// call, never fall back to the loop.
    #[derive(Debug)]
    struct BatchOnlyCas {
        present: std::collections::HashSet<String>,
        /// Every `reqs` slice this handler was handed, as hashes — proves
        /// batching happened AND preserves the order it was asked in.
        batches: Mutex<Vec<Vec<String>>>,
        /// Forced batch-level failure, returned instead of the flags.
        /// `Mutex` because `CasHandlerError` is not `Clone`.
        fail_with: Mutex<Option<CasHandlerError>>,
        /// When set, return this many flags regardless of how many digests
        /// were asked about (a misbehaving handler).
        force_len: Option<usize>,
    }

    impl BatchOnlyCas {
        fn new(present: &[&str]) -> Self {
            Self {
                present: present.iter().map(|s| (*s).to_owned()).collect(),
                batches: Mutex::new(Vec::new()),
                fail_with: Mutex::new(None),
                force_len: None,
            }
        }

        fn failing(err: CasHandlerError) -> Self {
            let mut me = Self::new(&[]);
            me.fail_with = Mutex::new(Some(err));
            me
        }

        fn with_forced_len(len: usize) -> Self {
            let mut me = Self::new(&[]);
            me.force_len = Some(len);
            me
        }

        fn recorded(&self) -> Vec<Vec<String>> {
            self.batches.lock().expect("not poisoned").clone()
        }
    }

    impl CasReadHandler for BatchOnlyCas {
        fn read(&self, _req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            panic!("batch-capable handler must never be driven through read()");
        }

        fn exists(&self, _req: CasReadRequest) -> Result<bool, CasHandlerError> {
            panic!("batch-capable handler must never be driven through the per-digest loop");
        }

        fn exists_batch(
            &self,
            reqs: &[CasReadRequest],
        ) -> Option<Result<Vec<bool>, CasHandlerError>> {
            self.batches
                .lock()
                .expect("not poisoned")
                .push(reqs.iter().map(|r| r.hash.clone()).collect());
            if let Some(err) = self.fail_with.lock().expect("not poisoned").take() {
                return Some(Err(err));
            }
            let flags: Vec<bool> = match self.force_len {
                Some(n) => vec![false; n],
                None => reqs
                    .iter()
                    .map(|r| self.present.contains(&r.hash))
                    .collect(),
            };
            Some(Ok(flags))
        }
    }

    #[test]
    fn batch_capable_handler_is_asked_once_in_request_order() {
        let cas = Arc::new(BatchOnlyCas::new(&[HASH_B]));
        let fm = InMemoryFindMissing::new(Arc::clone(&cas) as Arc<dyn CasReadHandler>);
        let digests = vec![
            valid_digest(HASH_A),
            valid_digest(HASH_B),
            valid_digest(HASH_C),
        ];
        let missing = fm
            .find_missing("t1", "p1", "t1", 7, &digests)
            .expect("find_missing");

        let recorded = cas.recorded();
        assert_eq!(
            recorded.len(),
            1,
            "exactly ONE batch call, not one per digest"
        );
        assert_eq!(
            recorded[0],
            vec![HASH_A.to_owned(), HASH_B.to_owned(), HASH_C.to_owned()],
            "the handler is handed the digests in REQUEST order"
        );
        // B is present; A and C are missing, in request order.
        assert_eq!(
            missing,
            vec![valid_digest(HASH_A), valid_digest(HASH_C)],
            "response order follows request order, not completion order"
        );
    }

    #[test]
    fn batch_path_uses_the_bazel_sha256_keyspace_and_request_fields() {
        // The batch path must build the SAME `CasReadRequest` the serial
        // loop built — SHA-256 keyspace included, else every Bazel blob
        // reports missing and the cache never hits.
        #[derive(Debug)]
        struct CapturingCas(Mutex<Vec<CasReadRequest>>);
        impl CasReadHandler for CapturingCas {
            fn read(&self, _r: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
                unreachable!()
            }
            fn exists(&self, _r: CasReadRequest) -> Result<bool, CasHandlerError> {
                unreachable!()
            }
            fn exists_batch(
                &self,
                reqs: &[CasReadRequest],
            ) -> Option<Result<Vec<bool>, CasHandlerError>> {
                self.0
                    .lock()
                    .expect("not poisoned")
                    .extend(reqs.iter().cloned());
                Some(Ok(vec![true; reqs.len()]))
            }
        }
        let cas = Arc::new(CapturingCas(Mutex::new(Vec::new())));
        let fm = InMemoryFindMissing::new(Arc::clone(&cas) as Arc<dyn CasReadHandler>);
        fm.find_missing("t1", "p1", "t1", 99, &[valid_digest(HASH_A)])
            .expect("find_missing");
        let seen = cas.0.lock().expect("not poisoned").clone();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].algo, DigestAlgo::Sha256, "Bazel keyspace");
        assert_eq!(seen[0].tenant, "t1");
        assert_eq!(seen[0].principal, "p1");
        assert_eq!(seen[0].caller_tenant, "t1");
        assert_eq!(seen[0].at_unix_ms, 99);
        assert_eq!(seen[0].hash, HASH_A);
    }

    #[test]
    fn batch_audit_failure_fails_closed() {
        // A failed audit must surface as AuditFailed (→ 503), NEVER as an
        // empty "nothing is missing" answer.
        let cas = Arc::new(BatchOnlyCas::failing(CasHandlerError::AuditFailed(
            "d1 down".to_owned(),
        )));
        let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);
        let err = fm
            .find_missing("t1", "p1", "t1", 0, &[valid_digest(HASH_A)])
            .expect_err("must fail closed");
        assert!(matches!(err, BazelBridgeError::AuditFailed(ref m) if m == "d1 down"));
    }

    #[test]
    fn batch_cross_tenant_denial_propagates() {
        let cas = Arc::new(BatchOnlyCas::failing(CasHandlerError::CrossTenantDenied {
            caller: "attacker".to_owned(),
            requested_tenant: "victim".to_owned(),
        }));
        let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);
        let err = fm
            .find_missing("victim", "p", "victim", 0, &[valid_digest(HASH_A)])
            .expect_err("denied");
        match err {
            BazelBridgeError::CrossTenantDenied { caller, requested } => {
                assert_eq!(caller, "attacker");
                assert_eq!(requested, "victim");
            }
            other => panic!("expected CrossTenantDenied, got {other:?}"),
        }
    }

    #[test]
    fn batch_storage_error_maps_to_internal_exactly_as_the_serial_loop_does() {
        let cas = Arc::new(BatchOnlyCas::failing(CasHandlerError::Internal(
            "r2 unreachable".to_owned(),
        )));
        let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);
        let err = fm
            .find_missing("t1", "p1", "t1", 0, &[valid_digest(HASH_A)])
            .expect_err("internal");
        assert!(
            matches!(err, BazelBridgeError::Internal(ref m)
                if m == "cas exists returned unexpected error during find_missing"),
            "got {err:?}"
        );
    }

    #[test]
    fn batch_result_count_mismatch_is_refused_not_guessed() {
        // A handler answering a different number of digests than asked
        // would silently mis-report presence. Refuse loudly instead.
        let cas = Arc::new(BatchOnlyCas::with_forced_len(1));
        let fm = InMemoryFindMissing::new(cas as Arc<dyn CasReadHandler>);
        let err = fm
            .find_missing(
                "t1",
                "p1",
                "t1",
                0,
                &[valid_digest(HASH_A), valid_digest(HASH_B)],
            )
            .expect_err("mismatched count");
        assert!(matches!(err, BazelBridgeError::Internal(_)), "got {err:?}");
    }

    #[test]
    fn duplicate_digests_behave_identically_on_both_paths() {
        // A REAPI client may repeat a digest inside one request. Both the
        // batch path and the serial fallback must answer per POSITION — the
        // response carries one entry per requested digest, duplicates
        // included — so the two paths agree exactly.
        let digests = vec![
            valid_digest(HASH_A),
            valid_digest(HASH_B),
            valid_digest(HASH_A),
            valid_digest(HASH_B),
        ];

        let batch_cas = Arc::new(BatchOnlyCas::new(&[HASH_B]));
        let batched = InMemoryFindMissing::new(Arc::clone(&batch_cas) as Arc<dyn CasReadHandler>)
            .find_missing("t1", "p1", "t1", 0, &digests)
            .expect("batch path");

        let serial_cas = Arc::new(ProbeOnlyCas::new(&[HASH_B]));
        let serial = InMemoryFindMissing::new(Arc::clone(&serial_cas) as Arc<dyn CasReadHandler>)
            .find_missing("t1", "p1", "t1", 0, &digests)
            .expect("serial path");

        assert_eq!(batched, serial, "the two paths must not diverge");
        assert_eq!(
            batched,
            vec![valid_digest(HASH_A), valid_digest(HASH_A)],
            "both occurrences of the absent digest are reported"
        );
        // The duplicate is passed through to the handler as-is — the bridge
        // does NOT dedupe, so N digests still mean N `ReadAttempted` events
        // (the durable sink's INSERT OR IGNORE is what collapses the two
        // identical rows, exactly as it does for a serial re-emit).
        assert_eq!(batch_cas.recorded()[0].len(), 4);
        assert_eq!(serial_cas.exists_calls.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn handler_without_batch_capability_still_uses_the_serial_loop() {
        // The default `exists_batch` returns None, so nothing changes for
        // any handler that did not opt in.
        let cas = Arc::new(ProbeOnlyCas::new(&[HASH_A]));
        let fm = InMemoryFindMissing::new(Arc::clone(&cas) as Arc<dyn CasReadHandler>);
        let missing = fm
            .find_missing(
                "t1",
                "p1",
                "t1",
                0,
                &[valid_digest(HASH_A), valid_digest(HASH_B)],
            )
            .expect("find_missing");
        assert_eq!(missing, vec![valid_digest(HASH_B)]);
        assert_eq!(cas.exists_calls.load(Ordering::SeqCst), 2);
        assert_eq!(cas.read_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn empty_digest_list_asks_nothing_of_a_batch_handler() {
        let cas = Arc::new(BatchOnlyCas::new(&[]));
        let fm = InMemoryFindMissing::new(Arc::clone(&cas) as Arc<dyn CasReadHandler>);
        let missing = fm.find_missing("t1", "p1", "t1", 0, &[]).expect("empty");
        assert!(missing.is_empty());
        assert_eq!(
            cas.recorded(),
            vec![Vec::<String>::new()],
            "an empty batch is still one call, and it carries no digests"
        );
    }
}
