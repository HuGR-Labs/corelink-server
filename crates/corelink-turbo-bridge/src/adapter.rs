//! Adapter bridging `TurboArtifactHandler` onto opaque KV-style port traits.
//!
//! # Design rationale
//!
//! `corelink-handler-cas`'s `CasWriteHandler` verifies `claimed_hash`
//! against the bytes before storing (BLAKE3 or the test `fake_hash`).
//! Turbo's hash is opaque (xxhash, sha512-prefix, custom) so we **cannot**
//! pass it as `claimed_hash` through the standard CAS write path without
//! triggering a spurious `HashMismatch` error.
//!
//! Solution: expose two thin port traits (`CasReadStore` / `CasWriteStore`)
//! that accept `(tenant, key, bytes)` without hash verification.  The
//! `CasAdapterTurboHandler` wires them together with the Turbo audit/
//! validation surface.
//!
//! Production callers supply a `CasWriteStore` backed by R2 (via the
//! CF-Worker wasm32 adapter) where the object key is the raw Turbo hash
//! string.  Test callers supply `InMemoryKvStore` (shipped in this module).
//!
//! # Tenant isolation
//!
//! `team_id` is used as the storage partition key.  The adapter enforces
//! `team_id == caller_tenant` before any storage access (same invariant as
//! `InMemoryTurboHandler`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{TurboAuditEvent, TurboAuditEventKind, TurboAuditSink};
use crate::error::TurboBridgeError;
use crate::events::{TurboEventsRequest, TurboEventsResponse};
use crate::handler::{
    TurboArtifactHandler, TurboGetRequest, TurboGetResponse, TurboPutRequest, TurboPutResponse,
    TurboStatusResponse,
};
use crate::status::TurboStatusPayload;
use crate::MAX_HASH_LEN;

// ── Port traits ───────────────────────────────────────────────────────────────

/// Opaque KV read port.  Implementations do **not** verify hash integrity;
/// they simply return bytes for the given `(tenant, key)`.
pub trait CasReadStore: Send + Sync + core::fmt::Debug {
    /// Retrieve bytes for `(tenant, key)`.
    ///
    /// # Errors
    ///
    /// - [`TurboBridgeError::NotFound`] — no object at this key.
    /// - [`TurboBridgeError::Internal`] — backend error.
    fn read(
        &self,
        tenant: &str,
        key: &str,
    ) -> Result<Vec<u8>, TurboBridgeError>;
}

/// Opaque KV write port.  Implementations store bytes under `(tenant, key)`
/// without verifying hash integrity.
pub trait CasWriteStore: Send + Sync + core::fmt::Debug {
    /// Upsert `bytes` under `(tenant, key)`.
    ///
    /// # Errors
    ///
    /// Returns [`TurboBridgeError::Internal`] on backend error.
    fn write(
        &self,
        tenant: &str,
        key: &str,
        bytes: Vec<u8>,
    ) -> Result<(), TurboBridgeError>;
}

// ── InMemoryKvStore ───────────────────────────────────────────────────────────

/// Deterministic in-memory store implementing both [`CasReadStore`] and
/// [`CasWriteStore`].  Intended for adapter unit tests.
pub struct InMemoryKvStore {
    objects: Mutex<HashMap<(String, String), Vec<u8>>>,
}

impl core::fmt::Debug for InMemoryKvStore {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryKvStore").finish_non_exhaustive()
    }
}

impl InMemoryKvStore {
    /// Construct an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            objects: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryKvStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CasReadStore for InMemoryKvStore {
    fn read(&self, tenant: &str, key: &str) -> Result<Vec<u8>, TurboBridgeError> {
        self.objects
            .lock()
            .map_err(|_| TurboBridgeError::Internal("kv lock poisoned".into()))?
            .get(&(tenant.to_owned(), key.to_owned()))
            .cloned()
            .ok_or_else(|| TurboBridgeError::NotFound { hash: key.to_owned() })
    }
}

impl CasWriteStore for InMemoryKvStore {
    fn write(&self, tenant: &str, key: &str, bytes: Vec<u8>) -> Result<(), TurboBridgeError> {
        self.objects
            .lock()
            .map_err(|_| TurboBridgeError::Internal("kv lock poisoned".into()))?
            .insert((tenant.to_owned(), key.to_owned()), bytes);
        Ok(())
    }
}

// ── CasAdapterTurboHandler ────────────────────────────────────────────────────

/// [`TurboArtifactHandler`] backed by injected [`CasReadStore`] and
/// [`CasWriteStore`] port traits.
///
/// This adapter applies the same tenant-isolation, audit-fail-CLOSED, and
/// hash-length-guard invariants as `InMemoryTurboHandler` but delegates
/// storage to the provided ports.  This is the production wiring point:
/// swap in an R2-backed `CasWriteStore` without changing any audit/validation
/// logic.
pub struct CasAdapterTurboHandler {
    reader: Arc<dyn CasReadStore>,
    writer: Arc<dyn CasWriteStore>,
    audit: Arc<dyn TurboAuditSink>,
}

impl core::fmt::Debug for CasAdapterTurboHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CasAdapterTurboHandler").finish_non_exhaustive()
    }
}

impl CasAdapterTurboHandler {
    /// Construct a handler backed by `reader`, `writer`, and `audit`.
    #[must_use]
    pub fn new(
        reader: Arc<dyn CasReadStore>,
        writer: Arc<dyn CasWriteStore>,
        audit: Arc<dyn TurboAuditSink>,
    ) -> Self {
        Self {
            reader,
            writer,
            audit,
        }
    }

    fn check_tenant(team_id: &str, caller_tenant: &str) -> bool {
        team_id == caller_tenant
    }
}

impl TurboArtifactHandler for CasAdapterTurboHandler {
    fn put(&self, req: TurboPutRequest) -> Result<TurboPutResponse, TurboBridgeError> {
        if req.hash.len() > MAX_HASH_LEN {
            return Err(TurboBridgeError::HashTooLong {
                len: req.hash.len(),
                max: MAX_HASH_LEN,
            });
        }

        if !Self::check_tenant(&req.team_id, &req.caller_tenant) {
            self.audit
                .emit(TurboAuditEvent::new(
                    TurboAuditEventKind::PutDenied,
                    req.team_id.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                    req.slug.clone(),
                ))
                .map_err(|e| TurboBridgeError::AuditFailed(e.to_string()))?;
            return Err(TurboBridgeError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_team_id: req.team_id,
            });
        }

        // PutAttempted BEFORE mutation.
        self.audit
            .emit(TurboAuditEvent::new(
                TurboAuditEventKind::PutAttempted,
                req.team_id.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
                req.slug.clone(),
            ))
            .map_err(|e| TurboBridgeError::AuditFailed(e.to_string()))?;

        self.writer
            .write(&req.team_id, &req.hash, req.bytes)
            .map_err(|e| TurboBridgeError::Internal(e.to_string()))?;

        // PutCommitted AFTER durable store.
        self.audit
            .emit(TurboAuditEvent::new(
                TurboAuditEventKind::PutCommitted,
                req.team_id.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
                req.slug.clone(),
            ))
            .map_err(|e| TurboBridgeError::AuditFailed(e.to_string()))?;

        Ok(TurboPutResponse::for_artifact(&req.team_id, &req.hash))
    }

    fn get(&self, req: TurboGetRequest) -> Result<TurboGetResponse, TurboBridgeError> {
        if req.hash.len() > MAX_HASH_LEN {
            return Err(TurboBridgeError::HashTooLong {
                len: req.hash.len(),
                max: MAX_HASH_LEN,
            });
        }

        if !Self::check_tenant(&req.team_id, &req.caller_tenant) {
            self.audit
                .emit(TurboAuditEvent::new(
                    TurboAuditEventKind::GetDenied,
                    req.team_id.clone(),
                    req.hash.clone(),
                    req.principal.clone(),
                    req.at_unix_ms,
                    req.slug.clone(),
                ))
                .map_err(|e| TurboBridgeError::AuditFailed(e.to_string()))?;
            return Err(TurboBridgeError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_team_id: req.team_id,
            });
        }

        self.audit
            .emit(TurboAuditEvent::new(
                TurboAuditEventKind::GetAttempted,
                req.team_id.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
                req.slug.clone(),
            ))
            .map_err(|e| TurboBridgeError::AuditFailed(e.to_string()))?;

        let bytes = self.reader.read(&req.team_id, &req.hash)?;

        self.audit
            .emit(TurboAuditEvent::new(
                TurboAuditEventKind::GetServed,
                req.team_id.clone(),
                req.hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
                req.slug.clone(),
            ))
            .map_err(|e| TurboBridgeError::AuditFailed(e.to_string()))?;

        Ok(TurboGetResponse::new(bytes))
    }

    fn events(&self, _req: TurboEventsRequest) -> Result<TurboEventsResponse, TurboBridgeError> {
        Ok(TurboEventsResponse::new())
    }

    fn status(&self) -> Result<TurboStatusResponse, TurboBridgeError> {
        Ok(TurboStatusPayload::enabled())
    }
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
    use crate::audit::InMemoryTurboAuditSink;
    use std::sync::Arc;

    fn fixture() -> (
        Arc<InMemoryTurboAuditSink>,
        Arc<InMemoryKvStore>,
        CasAdapterTurboHandler,
    ) {
        let audit = Arc::new(InMemoryTurboAuditSink::new());
        let store = Arc::new(InMemoryKvStore::new());
        let h = CasAdapterTurboHandler::new(store.clone(), store.clone(), audit.clone());
        (audit, store, h)
    }

    #[test]
    fn adapter_put_get_round_trip() {
        let (_, _, h) = fixture();
        let data = b"webpack output chunk".to_vec();
        h.put(TurboPutRequest::new(
            "hash_abc",
            "team_x",
            "my-repo",
            data.clone(),
            None,
            "ci_runner",
            "team_x",
            1,
        ))
        .expect("put");
        let resp = h
            .get(TurboGetRequest::new(
                "hash_abc", "team_x", "my-repo", "ci_runner", "team_x", 2,
            ))
            .expect("get");
        assert_eq!(resp.bytes, data);
    }

    #[test]
    fn adapter_cross_tenant_put_denied() {
        let (audit, _, h) = fixture();
        let err = h
            .put(TurboPutRequest::new(
                "h1",
                "victim",
                "s",
                b"x".to_vec(),
                None,
                "evil",
                "attacker",
                1,
            ))
            .expect_err("denied");
        assert!(matches!(err, TurboBridgeError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("snap");
        assert_eq!(rows[0].kind, TurboAuditEventKind::PutDenied);
    }

    #[test]
    fn adapter_cross_tenant_get_denied() {
        let (audit, _, h) = fixture();
        let err = h
            .get(TurboGetRequest::new("h1", "victim", "s", "evil", "attacker", 1))
            .expect_err("denied");
        assert!(matches!(err, TurboBridgeError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("snap");
        assert_eq!(rows[0].kind, TurboAuditEventKind::GetDenied);
    }

    #[test]
    fn adapter_opaque_hash_accepted() {
        let (_, _, h) = fixture();
        // Simulate a Turbo xxhash (not hex, not sha256).
        let opaque = "turboxxhash-5e7c3b2a";
        h.put(TurboPutRequest::new(
            opaque,
            "t1",
            "s",
            b"data".to_vec(),
            None,
            "p",
            "t1",
            1,
        ))
        .expect("opaque hash stored");
        let resp = h
            .get(TurboGetRequest::new(opaque, "t1", "s", "p", "t1", 2))
            .expect("retrieved");
        assert_eq!(resp.bytes, b"data".to_vec());
    }
}
