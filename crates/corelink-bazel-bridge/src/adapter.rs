//! REAPI v2 → CoreLink handler adapter.
//!
//! [`BazelAdapter`] is the central integration point: it holds
//! `Arc<dyn …>` references to the four CoreLink handler traits and
//! exposes REAPI-shaped methods that:
//!
//! 1. Validate the REAPI [`Digest`] (hash + size_bytes).
//! 2. Check tenant isolation (caller_tenant == instance).
//! 3. On CAS PUT, validate that the blob byte length matches
//!    `digest.size_bytes` (INV-BAZEL-DIGEST-VALIDATE).
//! 4. Delegate to the appropriate CoreLink handler.
//! 5. Map [`CasHandlerError`] / [`AcHandlerError`] → [`BazelBridgeError`].
//!
//! # Audit correctness
//!
//! The CoreLink handlers already carry the audit-emit-BEFORE-mutation
//! invariant. The adapter does NOT re-emit audit rows; it simply
//! translates error envelopes. The audit trail is owned by the handlers.
//!
//! # Thread safety
//!
//! `BazelAdapter` is `Send + Sync` because all four handler trait objects
//! require `Send + Sync` and `Arc` is `Send + Sync` when its contents are.

use std::sync::Arc;

use corelink_handler_ac::{
    AcHandlerError, AcLookupHandler, AcLookupRequest, AcUpdateHandler, AcUpdateRequest,
};
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasWriteHandler, CasWriteRequest, DigestAlgo,
};

use crate::digest::Digest;
use crate::error::BazelBridgeError;

/// Caller context for a REAPI v2 write (`cas_put` / `ac_put`).
///
/// Groups the per-call metadata that travels alongside the payload so the write
/// methods stay within the argument-count budget (one cohesive context value
/// instead of four loose params).
#[derive(Clone, Copy, Debug)]
pub struct WriteCtx<'a> {
    /// Already-authenticated caller principal.
    pub principal: &'a str,
    /// Caller's authenticated tenant (must equal the `instance`).
    pub caller_tenant: &'a str,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
    /// The tenant's Worker-resolved per-tier storage cap in bytes, threaded to
    /// the byte-accounting reservation to seed a FRESH `tenant_storage_state`
    /// row. `None` ⇒ indeterminate ⇒ fail-CLOSED on an unseeded tenant; `Some(0)`
    /// ⇒ genuine-unlimited tier. See `CasWriteRequest::with_storage_quota_bytes`.
    pub storage_quota_bytes: Option<i64>,
}

/// REAPI v2 adapter wrapping the four CoreLink handler traits.
///
/// Callers obtain one by calling [`BazelAdapter::new`]; the adapter is
/// `Clone` (cloning increments four Arc ref-counts).
#[derive(Clone)]
pub struct BazelAdapter {
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    ac_lookup: Arc<dyn AcLookupHandler>,
    ac_update: Arc<dyn AcUpdateHandler>,
}

impl core::fmt::Debug for BazelAdapter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BazelAdapter").finish_non_exhaustive()
    }
}

impl BazelAdapter {
    /// Construct a new adapter from the four CoreLink handler trait objects.
    #[must_use]
    pub fn new(
        cas_read: Arc<dyn CasReadHandler>,
        cas_write: Arc<dyn CasWriteHandler>,
        ac_lookup: Arc<dyn AcLookupHandler>,
        ac_update: Arc<dyn AcUpdateHandler>,
    ) -> Self {
        Self {
            cas_read,
            cas_write,
            ac_lookup,
            ac_update,
        }
    }

    /// Serve a REAPI v2 CAS read: `GET /{instance}/blobs/{hash}/{size}`.
    ///
    /// Returns the raw bytes on success.
    ///
    /// # Errors
    ///
    /// - [`BazelBridgeError::CrossTenantDenied`] if `caller_tenant !=
    ///   instance`.
    /// - [`BazelBridgeError::NotFound`] if the blob is absent.
    /// - [`BazelBridgeError::AuditFailed`] if the audit sink is down.
    /// - [`BazelBridgeError::Internal`] on unexpected handler errors.
    pub fn cas_get(
        &self,
        instance: &str,
        digest: &Digest,
        principal: &str,
        caller_tenant: &str,
        at_unix_ms: u64,
    ) -> Result<Vec<u8>, BazelBridgeError> {
        check_tenant(instance, caller_tenant)?;
        // REAPI v2 content-addresses with SHA-256: tag the read into the
        // surface-partitioned `bazel/sha256/` keyspace so the durable gate
        // re-verifies it as SHA-256 (never BLAKE3) on the bitrot read path.
        let req = CasReadRequest::new(instance, &digest.hash, principal, caller_tenant, at_unix_ms)
            .with_algo(DigestAlgo::Sha256);
        self.cas_read
            .read(req)
            .map(|resp| resp.bytes)
            .map_err(|e| map_cas_error(e, instance, &digest.hash))
    }

    /// Serve a REAPI v2 CAS write:
    /// `POST /{instance}/uploads/{uuid}/blobs/{hash}/{size}`.
    ///
    /// `bytes` MUST have length equal to `digest.size_bytes`.
    ///
    /// # Errors
    ///
    /// - [`BazelBridgeError::SizeMismatch`] if `bytes.len() !=
    ///   digest.size_bytes`.
    /// - [`BazelBridgeError::CrossTenantDenied`] if tenant isolation fails.
    /// - [`BazelBridgeError::AuditFailed`] if the audit sink is down.
    /// - [`BazelBridgeError::Internal`] on unexpected handler errors.
    pub fn cas_put(
        &self,
        instance: &str,
        digest: &Digest,
        bytes: Vec<u8>,
        ctx: WriteCtx<'_>,
    ) -> Result<(), BazelBridgeError> {
        check_tenant(instance, ctx.caller_tenant)?;
        // INV-BAZEL-DIGEST-VALIDATE: size_bytes MUST match actual bytes len.
        let actual_len = bytes.len() as u64;
        if actual_len != digest.size_bytes {
            return Err(BazelBridgeError::SizeMismatch {
                digest_size: digest.size_bytes,
                actual: actual_len,
            });
        }
        // Thread the Worker-resolved per-tier storage cap into the reservation so
        // the byte-accounting decorator seeds a fresh row with the real cap;
        // `None` ⇒ fail-CLOSED on an unseeded tenant (never uncapped).
        // REAPI v2 content-addresses with SHA-256: tag the write into the
        // surface-partitioned `bazel/sha256/` keyspace; the durable gate
        // verifies `claimed_hash == SHA-256(bytes)` (never BLAKE3) and stores
        // under that sub-prefix so read-back is single-function.
        let req = CasWriteRequest::new(
            instance,
            &digest.hash,
            bytes,
            ctx.principal,
            ctx.caller_tenant,
            ctx.at_unix_ms,
        )
        .with_storage_quota_bytes(ctx.storage_quota_bytes)
        .with_algo(DigestAlgo::Sha256);
        self.cas_write
            .write(req)
            .map(|_| ())
            .map_err(|e| map_cas_error(e, instance, &digest.hash))
    }

    /// Serve a REAPI v2 AC read: `GET /{instance}/blobs/ac/{hash}/{size}`.
    ///
    /// Returns the raw action result payload on success.
    ///
    /// # Errors
    ///
    /// - [`BazelBridgeError::NotFound`] if there is no AC entry for the
    ///   given action digest.
    /// - [`BazelBridgeError::CrossTenantDenied`] on tenant isolation
    ///   failure.
    /// - [`BazelBridgeError::AuditFailed`] if the audit sink is down.
    pub fn ac_get(
        &self,
        instance: &str,
        digest: &Digest,
        principal: &str,
        caller_tenant: &str,
        at_unix_ms: u64,
    ) -> Result<Vec<u8>, BazelBridgeError> {
        check_tenant(instance, caller_tenant)?;
        let req =
            AcLookupRequest::new(instance, &digest.hash, principal, caller_tenant, at_unix_ms);
        self.ac_lookup
            .lookup(req)
            .map(|resp| resp.result_payload)
            .map_err(|e| map_ac_error(e, instance, &digest.hash))
    }

    /// Serve a REAPI v2 AC write: `PUT /{instance}/blobs/ac/{hash}/{size}`.
    ///
    /// The `payload` bytes are stored as the action result under the given
    /// action digest.
    ///
    /// # Errors
    ///
    /// - [`BazelBridgeError::CrossTenantDenied`] on tenant isolation
    ///   failure.
    /// - [`BazelBridgeError::AuditFailed`] if the audit sink is down.
    pub fn ac_put(
        &self,
        instance: &str,
        digest: &Digest,
        payload: Vec<u8>,
        ctx: WriteCtx<'_>,
    ) -> Result<(), BazelBridgeError> {
        check_tenant(instance, ctx.caller_tenant)?;
        // Thread the Worker-resolved per-tier cap (see `cas_put`); `None` ⇒
        // fail-CLOSED on an unseeded tenant.
        let req = AcUpdateRequest::new(
            instance,
            &digest.hash,
            payload,
            ctx.principal,
            ctx.caller_tenant,
            ctx.at_unix_ms,
        )
        .with_storage_quota_bytes(ctx.storage_quota_bytes);
        self.ac_update
            .update(req)
            .map(|_| ())
            .map_err(|e| map_ac_error(e, instance, &digest.hash))
    }
}

/// Tenant isolation guard. Returns `CrossTenantDenied` when the caller
/// tenant does not match the requested instance. The handler layer also
/// checks this, but an early check here avoids a round-trip.
fn check_tenant(instance: &str, caller_tenant: &str) -> Result<(), BazelBridgeError> {
    if instance != caller_tenant {
        return Err(BazelBridgeError::CrossTenantDenied {
            caller: caller_tenant.to_owned(),
            requested: instance.to_owned(),
        });
    }
    Ok(())
}

/// Map a [`CasHandlerError`] to the appropriate [`BazelBridgeError`].
fn map_cas_error(e: CasHandlerError, tenant: &str, hash: &str) -> BazelBridgeError {
    match e {
        CasHandlerError::NotFound { .. } => BazelBridgeError::NotFound {
            tenant: tenant.to_owned(),
            hash: hash.to_owned(),
        },
        CasHandlerError::CrossTenantDenied {
            caller,
            requested_tenant,
        } => BazelBridgeError::CrossTenantDenied {
            caller,
            requested: requested_tenant,
        },
        CasHandlerError::AuditFailed(msg) => BazelBridgeError::AuditFailed(msg),
        CasHandlerError::HashMismatch { claimed, actual } => BazelBridgeError::Internal(format!(
            "CAS hash mismatch on claimed={claimed} actual={actual}"
        )),
        CasHandlerError::Internal(msg) => BazelBridgeError::Internal(msg),
        _ => BazelBridgeError::Internal("unexpected CAS error".into()),
    }
}

/// Map an [`AcHandlerError`] to the appropriate [`BazelBridgeError`].
fn map_ac_error(e: AcHandlerError, tenant: &str, hash: &str) -> BazelBridgeError {
    match e {
        AcHandlerError::Miss { .. } => BazelBridgeError::NotFound {
            tenant: tenant.to_owned(),
            hash: hash.to_owned(),
        },
        AcHandlerError::CrossTenantDenied {
            caller,
            requested_tenant,
        } => BazelBridgeError::CrossTenantDenied {
            caller,
            requested: requested_tenant,
        },
        AcHandlerError::AuditFailed(msg) => BazelBridgeError::AuditFailed(msg),
        AcHandlerError::Internal(msg) => BazelBridgeError::Internal(msg),
        _ => BazelBridgeError::Internal("unexpected AC error".into()),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::digest::sha256_hex;
    use corelink_handler_ac::{
        InMemoryAcHandler, InMemoryAuditSink as AcAuditSink, InMemorySliObserver as AcSliObserver,
    };
    use corelink_handler_cas::handler::fake_hash;
    use corelink_handler_cas::{InMemoryAuditSink, InMemoryCasHandler, InMemorySliObserver};

    fn make_adapter() -> BazelAdapter {
        // CAS and AC have separate trait definitions for AuditSink / SliObserver;
        // each handler gets its own pair of sinks.
        let cas_audit = Arc::new(InMemoryAuditSink::new());
        let cas_sli = Arc::new(InMemorySliObserver::new());
        let cas = Arc::new(InMemoryCasHandler::new(cas_audit, cas_sli));
        let ac_audit = Arc::new(AcAuditSink::new());
        let ac_sli = Arc::new(AcSliObserver::new());
        let ac = Arc::new(InMemoryAcHandler::new(ac_audit, ac_sli));
        BazelAdapter::new(
            cas.clone() as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            ac.clone() as Arc<dyn AcLookupHandler>,
            ac as Arc<dyn AcUpdateHandler>,
        )
    }

    const TENANT: &str = "acme";

    fn seed_cas_blob(adapter: &BazelAdapter, tenant: &str, bytes: &[u8]) -> Digest {
        // The Bazel CAS keyspace content-addresses with SHA-256 (the adapter
        // tags writes `DigestAlgo::Sha256`), so the in-memory fake now verifies
        // a real SHA-256 digest.
        let hash = sha256_hex(bytes);
        let digest = Digest::new(&hash, bytes.len() as u64).expect("digest");
        adapter
            .cas_put(
                tenant,
                &digest,
                bytes.to_vec(),
                WriteCtx {
                    principal: "p1",
                    caller_tenant: tenant,
                    at_unix_ms: 0,
                    storage_quota_bytes: Some(0),
                },
            )
            .expect("seed cas");
        digest
    }

    #[test]
    fn cas_put_and_get_round_trip() {
        let adapter = make_adapter();
        let bytes = b"bazel build target".to_vec();
        let digest = seed_cas_blob(&adapter, TENANT, &bytes);
        let got = adapter
            .cas_get(TENANT, &digest, "p1", TENANT, 0)
            .expect("get");
        assert_eq!(got, bytes);
    }

    #[test]
    fn cas_get_not_found_returns_not_found() {
        let adapter = make_adapter();
        let hash = "a".repeat(64);
        let digest = Digest::new(&hash, 0).expect("digest");
        let err = adapter
            .cas_get(TENANT, &digest, "p1", TENANT, 0)
            .expect_err("not found");
        assert!(matches!(err, BazelBridgeError::NotFound { .. }));
    }

    #[test]
    fn cas_put_size_mismatch_is_rejected() {
        let adapter = make_adapter();
        let bytes = b"hello".to_vec();
        let hash = fake_hash(&bytes);
        // Claim size_bytes = 10 but actual len = 5.
        let digest = Digest::new(&hash, 10).expect("digest");
        let err = adapter
            .cas_put(
                TENANT,
                &digest,
                bytes,
                WriteCtx {
                    principal: "p1",
                    caller_tenant: TENANT,
                    at_unix_ms: 0,
                    storage_quota_bytes: Some(0),
                },
            )
            .expect_err("size mismatch");
        assert!(matches!(err, BazelBridgeError::SizeMismatch { .. }));
    }

    #[test]
    fn cas_get_cross_tenant_denied() {
        let adapter = make_adapter();
        let hash = "b".repeat(64);
        let digest = Digest::new(&hash, 0).expect("digest");
        let err = adapter
            .cas_get("victim", &digest, "p1", "attacker", 0)
            .expect_err("cross-tenant");
        assert!(matches!(err, BazelBridgeError::CrossTenantDenied { .. }));
    }

    #[test]
    fn ac_put_and_get_round_trip() {
        let adapter = make_adapter();
        let hash = "c".repeat(64);
        let digest = Digest::new(&hash, 8).expect("digest");
        let payload = b"action_result_bytes".to_vec();
        adapter
            .ac_put(
                TENANT,
                &digest,
                payload.clone(),
                WriteCtx {
                    principal: "p1",
                    caller_tenant: TENANT,
                    at_unix_ms: 0,
                    storage_quota_bytes: Some(0),
                },
            )
            .expect("ac put");
        let got = adapter
            .ac_get(TENANT, &digest, "p1", TENANT, 0)
            .expect("ac get");
        assert_eq!(got, payload);
    }

    #[test]
    fn ac_get_miss_returns_not_found() {
        let adapter = make_adapter();
        let hash = "d".repeat(64);
        let digest = Digest::new(&hash, 0).expect("digest");
        let err = adapter
            .ac_get(TENANT, &digest, "p1", TENANT, 0)
            .expect_err("miss");
        assert!(matches!(err, BazelBridgeError::NotFound { .. }));
    }

    #[test]
    fn ac_put_cross_tenant_denied() {
        let adapter = make_adapter();
        let hash = "e".repeat(64);
        let digest = Digest::new(&hash, 0).expect("digest");
        let err = adapter
            .ac_put(
                "victim",
                &digest,
                vec![],
                WriteCtx {
                    principal: "p1",
                    caller_tenant: "attacker",
                    at_unix_ms: 0,
                    storage_quota_bytes: Some(0),
                },
            )
            .expect_err("denied");
        assert!(matches!(err, BazelBridgeError::CrossTenantDenied { .. }));
    }

    #[test]
    fn cas_put_audit_failure_propagates() {
        let cas_audit = Arc::new(InMemoryAuditSink::new());
        let cas_sli = Arc::new(InMemorySliObserver::new());
        let cas = Arc::new(InMemoryCasHandler::new(cas_audit.clone(), cas_sli));
        let ac_audit = Arc::new(AcAuditSink::new());
        let ac_sli = Arc::new(AcSliObserver::new());
        let ac = Arc::new(InMemoryAcHandler::new(ac_audit, ac_sli));
        cas_audit.inject_failure("sink down").expect("inject");
        let adapter = BazelAdapter::new(
            cas.clone() as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            ac.clone() as Arc<dyn AcLookupHandler>,
            ac as Arc<dyn AcUpdateHandler>,
        );
        let bytes = b"x".to_vec();
        let hash = fake_hash(&bytes);
        let digest = Digest::new(&hash, 1).expect("digest");
        let err = adapter
            .cas_put(
                TENANT,
                &digest,
                bytes,
                WriteCtx {
                    principal: "p1",
                    caller_tenant: TENANT,
                    at_unix_ms: 0,
                    storage_quota_bytes: Some(0),
                },
            )
            .expect_err("audit fail");
        assert!(matches!(err, BazelBridgeError::AuditFailed(_)));
    }
}
