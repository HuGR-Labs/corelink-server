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
//! The isolation/storage tenant is `caller_tenant` (the PAT-resolved
//! authenticated tenant, threaded from the route's `AuthTenant`). `team_id` is
//! a Turborepo team label demoted to a sub-namespace: the storage `key` is
//! `"<team_id>/<hash>"`, keeping teams partitioned WITHIN one tenant. There is
//! no `team_id == caller_tenant` check — `team_id` is not a tenant; isolation
//! is provided solely by `tenant = caller_tenant` reaching the store.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{TurboAuditEvent, TurboAuditEventKind, TurboAuditSink};
use crate::error::validate_team_id;
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
    fn read(&self, tenant: &str, key: &str) -> Result<Vec<u8>, TurboBridgeError>;
}

/// Opaque KV write port.  Implementations store bytes under `(tenant, key)`
/// without verifying hash integrity.
pub trait CasWriteStore: Send + Sync + core::fmt::Debug {
    /// Upsert `bytes` under `(tenant, key)`, returning whether the write stored a
    /// NEW key.
    ///
    /// `Ok(true)`  — the key did not previously exist (a fresh, durable insert).
    /// `Ok(false)` — the key already existed (an idempotent overwrite: Turbo
    ///               artifacts are content-keyed, so the same key holds the same
    ///               bytes). The route uses this to ROLL BACK the storage byte
    ///               charge on an overwrite (rt-nuclear #25 — every idempotent
    ///               re-write was double-charging storage bytes), mirroring the
    ///               CAS decorator's `durable == false` contract.
    ///
    /// # Errors
    ///
    /// Returns [`TurboBridgeError::Internal`] on backend error.
    fn write(&self, tenant: &str, key: &str, bytes: Vec<u8>)
        -> Result<bool, TurboBridgeError>;
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
            .ok_or_else(|| TurboBridgeError::NotFound {
                hash: key.to_owned(),
            })
    }
}

impl CasWriteStore for InMemoryKvStore {
    fn write(
        &self,
        tenant: &str,
        key: &str,
        bytes: Vec<u8>,
    ) -> Result<bool, TurboBridgeError> {
        // `HashMap::insert` returns the prior value: `Some` ⇒ the key already
        // existed (overwrite ⇒ `Ok(false)`); `None` ⇒ a fresh key (`Ok(true)`).
        let prior = self
            .objects
            .lock()
            .map_err(|_| TurboBridgeError::Internal("kv lock poisoned".into()))?
            .insert((tenant.to_owned(), key.to_owned()), bytes);
        Ok(prior.is_none())
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
        f.debug_struct("CasAdapterTurboHandler")
            .finish_non_exhaustive()
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
}

impl TurboArtifactHandler for CasAdapterTurboHandler {
    fn put(&self, req: TurboPutRequest) -> Result<TurboPutResponse, TurboBridgeError> {
        if req.hash.len() > MAX_HASH_LEN {
            return Err(TurboBridgeError::HashTooLong {
                len: req.hash.len(),
                max: MAX_HASH_LEN,
            });
        }

        // DoS / key-aliasing guard: `team_id` is interpolated into the storage
        // key (`"<team_id>/<hash>"`); reject empty / overlong / `/`-bearing
        // values BEFORE any audit emit or write (mirrors the hash guard above).
        validate_team_id(&req.team_id)?;

        // No `team_id == caller_tenant` check: `team_id` is a Turborepo team
        // label, NOT the tenant. Isolation is provided solely by the storage
        // `tenant = caller_tenant` (the authenticated tenant); `team_id` is a
        // sub-namespace inside it via the storage key.

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

        // Storage: tenant dimension is the authenticated `caller_tenant`; the
        // key carries `team_id` as a sub-namespace so two teams under one tenant
        // stay partitioned.
        let durable = self
            .writer
            .write(
                &req.caller_tenant,
                &format!("{}/{}", req.team_id, req.hash),
                req.bytes,
            )
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

        Ok(TurboPutResponse::for_artifact(
            &req.team_id,
            &req.hash,
            durable,
        ))
    }

    fn get(&self, req: TurboGetRequest) -> Result<TurboGetResponse, TurboBridgeError> {
        if req.hash.len() > MAX_HASH_LEN {
            return Err(TurboBridgeError::HashTooLong {
                len: req.hash.len(),
                max: MAX_HASH_LEN,
            });
        }

        // DoS / key-aliasing guard on `team_id` (key prefix) before any audit.
        validate_team_id(&req.team_id)?;

        // No `team_id == caller_tenant` check: isolation is the storage
        // `tenant = caller_tenant`; `team_id` is a sub-namespace key prefix.

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

        // Storage: tenant = authenticated `caller_tenant`; key carries the
        // `team_id` sub-namespace (mirrors the write path).
        let bytes = self
            .reader
            .read(&req.caller_tenant, &format!("{}/{}", req.team_id, req.hash))?;

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
                "hash_abc",
                "team_x",
                "my-repo",
                "ci_runner",
                "team_x",
                2,
            ))
            .expect("get");
        assert_eq!(resp.bytes, data);
    }

    /// rt-nuclear #25: the FIRST PUT of a key is durable (`durable == true`);
    /// an idempotent re-PUT of the SAME (tenant, team, hash) reports
    /// `durable == false`, so the route can roll back the byte charge and not
    /// double-charge storage on every re-write.
    #[test]
    fn adapter_put_reports_durable_then_overwrite() {
        let (_, _, h) = fixture();
        let mk = || {
            TurboPutRequest::new(
                "hash_dup", "team_x", "my-repo", b"same-bytes".to_vec(), None, "ci_runner",
                "team_x", 1,
            )
        };
        let first = h.put(mk()).expect("first put");
        assert!(first.durable, "first PUT of a fresh key must be durable=true");
        let second = h.put(mk()).expect("second put");
        assert!(
            !second.durable,
            "an idempotent re-PUT must report durable=false (no double-charge)"
        );
    }

    /// The opaque write port itself reports new-vs-overwrite (the bit the
    /// adapter threads up): `Ok(true)` on a fresh key, `Ok(false)` on overwrite.
    #[test]
    fn in_memory_kv_write_reports_new_then_overwrite() {
        let store = InMemoryKvStore::new();
        assert!(store.write("t", "k", b"v1".to_vec()).expect("w1"), "fresh key ⇒ true");
        assert!(
            !store.write("t", "k", b"v2".to_vec()).expect("w2"),
            "overwrite ⇒ false"
        );
    }

    #[test]
    fn adapter_cross_tenant_isolation_via_caller_tenant() {
        // NEW invariant: isolation comes from `caller_tenant`, not a
        // `team_id == caller_tenant` tautology. An artifact written under
        // caller_tenant="tenantA" is NOT visible to caller_tenant="tenantB"
        // even with the SAME team_id and hash (the storage tenant dimension
        // differs), so the cross-tenant GET returns NotFound — there is no
        // longer a denial path on the turbo adapter.
        let (_, _, h) = fixture();
        h.put(TurboPutRequest::new(
            "h1",
            "team_shared",
            "s",
            b"tenantA bytes".to_vec(),
            None,
            "userA",
            "tenantA",
            1,
        ))
        .expect("put under tenantA");
        let err = h
            .get(TurboGetRequest::new(
                "h1",
                "team_shared",
                "s",
                "userB",
                "tenantB",
                2,
            ))
            .expect_err("tenantB must not see tenantA's artifact");
        assert!(matches!(err, TurboBridgeError::NotFound { .. }));
    }

    #[test]
    fn adapter_same_tenant_different_team_partitioned() {
        // NEW invariant: under ONE caller_tenant, two different team_ids with
        // the same hash address different slots (key = "<team_id>/<hash>"), so
        // they do not collide.
        let (_, _, h) = fixture();
        let data_x = b"team_x bytes".to_vec();
        let data_y = b"team_y bytes".to_vec();
        h.put(TurboPutRequest::new(
            "shared_hash",
            "team_x",
            "s",
            data_x.clone(),
            None,
            "p",
            "tenant1",
            1,
        ))
        .expect("put team_x");
        h.put(TurboPutRequest::new(
            "shared_hash",
            "team_y",
            "s",
            data_y.clone(),
            None,
            "p",
            "tenant1",
            2,
        ))
        .expect("put team_y");
        let rx = h
            .get(TurboGetRequest::new(
                "shared_hash",
                "team_x",
                "s",
                "p",
                "tenant1",
                3,
            ))
            .expect("get team_x");
        let ry = h
            .get(TurboGetRequest::new(
                "shared_hash",
                "team_y",
                "s",
                "p",
                "tenant1",
                4,
            ))
            .expect("get team_y");
        assert_eq!(rx.bytes, data_x);
        assert_eq!(ry.bytes, data_y);
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

    #[test]
    fn adapter_put_rejects_invalid_team_id_without_audit() {
        let (audit, store, h) = fixture();
        for bad in [
            "",
            "a/b",
            "../other",
            &"a".repeat(crate::error::MAX_TEAM_ID_LEN + 1),
        ] {
            let err = h
                .put(TurboPutRequest::new(
                    "h1",
                    bad,
                    "s",
                    b"data".to_vec(),
                    None,
                    "p",
                    "t1",
                    1,
                ))
                .expect_err("invalid team_id rejected");
            assert!(matches!(err, TurboBridgeError::TeamIdInvalid { .. }));
        }
        // DoS guard path emits no audit and writes nothing.
        assert!(audit.snapshot().expect("snapshot").is_empty());
        assert!(store.read("t1", &format!("/{}", "h1")).is_err());
    }

    #[test]
    fn adapter_get_rejects_invalid_team_id() {
        let (_, _, h) = fixture();
        let err = h
            .get(TurboGetRequest::new("h1", "a/b", "s", "p", "t1", 1))
            .expect_err("invalid team_id rejected on get");
        assert!(matches!(err, TurboBridgeError::TeamIdInvalid { .. }));
    }

    #[test]
    fn adapter_accepts_valid_team_id() {
        let (_, _, h) = fixture();
        h.put(TurboPutRequest::new(
            "h1",
            "team_AbC-123",
            "s",
            b"data".to_vec(),
            None,
            "p",
            "t1",
            1,
        ))
        .expect("valid team_id accepted");
    }
}
