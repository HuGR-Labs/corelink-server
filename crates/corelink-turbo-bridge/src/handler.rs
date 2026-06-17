//! `TurboArtifactHandler` trait + `InMemoryTurboHandler` deterministic fake.
//!
//! # Trait contract
//!
//! Every implementation **MUST**:
//!
//! 1. Validate `hash.len() <= MAX_HASH_LEN`; reject longer hashes with
//!    [`TurboBridgeError::HashTooLong`] without emitting an audit event.
//! 2. Key storage by `tenant = caller_tenant` (the authenticated tenant) with
//!    `team_id` demoted to a sub-namespace in the key (`"<team_id>/<hash>"`).
//!    `team_id` is NOT the tenant, so there is no `team_id == caller_tenant`
//!    check; cross-tenant isolation is provided solely by the `caller_tenant`
//!    storage dimension.
//! 3. On a PUT: emit `PutAttempted` BEFORE storage mutation; emit
//!    `PutCommitted` AFTER durable store.  If audit emit fails, abort
//!    without mutation.
//! 4. On a GET: emit `GetAttempted` BEFORE storage lookup.  On success,
//!    emit `GetServed` AFTER bytes retrieved.
//! 5. No `unwrap()` / `expect()` / `panic!()` on any return path.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{TurboAuditEvent, TurboAuditEventKind, TurboAuditSink};
use crate::error::{validate_team_id, TurboBridgeError};
use crate::events::{TurboEventsRequest, TurboEventsResponse};
use crate::status::TurboStatusPayload;
use crate::{MAX_HASH_LEN, VERCEL_API_VERSION};

// ── PUT ──────────────────────────────────────────────────────────────────────

/// Request envelope for `PUT /v8/artifacts/<hash>`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct TurboPutRequest {
    /// Turbo-native opaque artifact hash (up to [`MAX_HASH_LEN`] chars).
    pub hash: String,
    /// `teamId` query parameter — a Turborepo team label demoted to a storage
    /// sub-namespace (key prefix), NOT the tenant.
    pub team_id: String,
    /// `slug` query parameter — informational only (logged, not partitioned).
    pub slug: String,
    /// Artifact bytes.
    pub bytes: Vec<u8>,
    /// `x-artifact-duration` header in milliseconds (optional).
    pub duration_ms: Option<u64>,
    /// Authenticated principal identity.
    pub principal: String,
    /// Authenticated (PAT-resolved) tenant — the sole isolation/storage tenant.
    pub caller_tenant: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl TurboPutRequest {
    /// Construct a [`TurboPutRequest`].
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hash: impl Into<String>,
        team_id: impl Into<String>,
        slug: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
        duration_ms: Option<u64>,
        principal: impl Into<String>,
        caller_tenant: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            hash: hash.into(),
            team_id: team_id.into(),
            slug: slug.into(),
            bytes: bytes.into(),
            duration_ms,
            principal: principal.into(),
            caller_tenant: caller_tenant.into(),
            at_unix_ms,
        }
    }
}

/// Response for a successful `PUT /v8/artifacts/<hash>`.
///
/// Serialises to `{"urls": ["<store_url>"]}` per the Vercel spec.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TurboPutResponse {
    /// Store URL(s) returned to the Turbo client.  Turbo treats 200 + this
    /// body as "uploaded successfully"; the URL is informational.
    pub urls: Vec<String>,
    /// Whether the PUT stored a NEW key (`true`) or overwrote an existing one
    /// (`false`). The route rolls back the storage byte charge when `false`
    /// (an idempotent re-write stored nothing new), mirroring the CAS
    /// decorator's `durable` contract (rt-nuclear #25).
    pub durable: bool,
}

impl TurboPutResponse {
    /// Construct a [`TurboPutResponse`] with the given URLs (durable insert).
    #[must_use]
    pub fn new(urls: Vec<String>) -> Self {
        Self {
            urls,
            durable: true,
        }
    }

    /// Construct the canonical single-URL response for a stored artifact.
    ///
    /// `tenant` and `hash` are used to form the canonical CoreLink URL.
    /// `durable` is `true` for a fresh insert, `false` for an idempotent
    /// overwrite (the route uses it to avoid double-charging storage bytes).
    #[must_use]
    pub fn for_artifact(tenant: &str, hash: &str, durable: bool) -> Self {
        Self {
            urls: vec![format!(
                "/{version}/artifacts/{hash}?teamId={tenant}",
                version = VERCEL_API_VERSION,
                hash = hash,
                tenant = tenant,
            )],
            durable,
        }
    }
}

// ── GET ──────────────────────────────────────────────────────────────────────

/// Request envelope for `GET /v8/artifacts/<hash>`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct TurboGetRequest {
    /// Turbo-native opaque artifact hash (up to [`MAX_HASH_LEN`] chars).
    pub hash: String,
    /// `teamId` query parameter — a Turborepo team label demoted to a storage
    /// sub-namespace (key prefix), NOT the tenant.
    pub team_id: String,
    /// `slug` query parameter — informational only.
    pub slug: String,
    /// Authenticated principal identity.
    pub principal: String,
    /// Authenticated (PAT-resolved) tenant — the sole isolation/storage tenant.
    pub caller_tenant: String,
    /// Wall-clock timestamp in unix-millis.
    pub at_unix_ms: u64,
}

impl TurboGetRequest {
    /// Construct a [`TurboGetRequest`].
    #[must_use]
    pub fn new(
        hash: impl Into<String>,
        team_id: impl Into<String>,
        slug: impl Into<String>,
        principal: impl Into<String>,
        caller_tenant: impl Into<String>,
        at_unix_ms: u64,
    ) -> Self {
        Self {
            hash: hash.into(),
            team_id: team_id.into(),
            slug: slug.into(),
            principal: principal.into(),
            caller_tenant: caller_tenant.into(),
            at_unix_ms,
        }
    }
}

/// Response for a successful `GET /v8/artifacts/<hash>`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TurboGetResponse {
    /// Artifact bytes.
    pub bytes: Vec<u8>,
}

impl TurboGetResponse {
    /// Construct a [`TurboGetResponse`] with the given bytes.
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }
}

// ── STATUS ───────────────────────────────────────────────────────────────────

/// Response for `POST /v8/artifacts/status`.
///
/// Re-exports [`TurboStatusPayload`] as `TurboStatusResponse` so callers
/// import only from `handler`.
pub type TurboStatusResponse = TurboStatusPayload;

// ── TRAIT ────────────────────────────────────────────────────────────────────

/// Core trait every Turbo remote-cache handler implements.
///
/// The four methods map 1-to-1 onto the Vercel Remote Cache API endpoints.
pub trait TurboArtifactHandler: Send + Sync + core::fmt::Debug {
    /// Handle `PUT /v8/artifacts/<hash>` — store artifact bytes.
    ///
    /// # Errors
    ///
    /// - [`TurboBridgeError::HashTooLong`] — hash exceeds [`MAX_HASH_LEN`].
    /// - [`TurboBridgeError::AuditFailed`] — audit sink unavailable.
    /// - [`TurboBridgeError::Internal`] — storage error.
    ///
    /// Isolation is by `caller_tenant` (the storage tenant); `team_id` is a
    /// sub-namespace, so no cross-tenant denial arises on this path.
    fn put(&self, req: TurboPutRequest) -> Result<TurboPutResponse, TurboBridgeError>;

    /// Handle `GET /v8/artifacts/<hash>` — retrieve artifact bytes.
    ///
    /// # Errors
    ///
    /// - [`TurboBridgeError::HashTooLong`] — hash exceeds [`MAX_HASH_LEN`].
    /// - [`TurboBridgeError::NotFound`] — artifact not in store.
    /// - [`TurboBridgeError::AuditFailed`] — audit sink unavailable.
    /// - [`TurboBridgeError::Internal`] — storage error.
    ///
    /// Isolation is by `caller_tenant` (the storage tenant); `team_id` is a
    /// sub-namespace, so no cross-tenant denial arises on this path.
    fn get(&self, req: TurboGetRequest) -> Result<TurboGetResponse, TurboBridgeError>;

    /// Handle `POST /v8/artifacts/events` — accept telemetry and drop it.
    ///
    /// # Errors
    ///
    /// Returns [`TurboBridgeError::Internal`] only on lock poisoning; never
    /// on body parse failures (the body is not parsed).
    fn events(&self, req: TurboEventsRequest) -> Result<TurboEventsResponse, TurboBridgeError>;

    /// Handle `POST /v8/artifacts/status` — return static enabled response.
    ///
    /// # Errors
    ///
    /// Never errors in practice; signature uses `Result` for trait uniformity.
    fn status(&self) -> Result<TurboStatusResponse, TurboBridgeError>;
}

// ── IN-MEMORY FAKE ────────────────────────────────────────────────────────────

/// Deterministic in-memory Turbo handler.  Intended for unit tests and
/// property tests.
///
/// Storage key: `(caller_tenant, "<team_id>/<hash>")` → bytes. The tenant
/// dimension is the authenticated `caller_tenant`; `team_id` is a sub-namespace
/// prefix on the opaque Turbo hash — no hash verification.
pub struct InMemoryTurboHandler {
    objects: Mutex<HashMap<(String, String), Vec<u8>>>,
    audit: Arc<dyn TurboAuditSink>,
}

impl core::fmt::Debug for InMemoryTurboHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryTurboHandler")
            .finish_non_exhaustive()
    }
}

impl InMemoryTurboHandler {
    /// Construct an empty handler bound to `audit`.
    #[must_use]
    pub fn new(audit: Arc<dyn TurboAuditSink>) -> Self {
        Self {
            objects: Mutex::new(HashMap::new()),
            audit,
        }
    }

    /// Seed `bytes` under `(tenant, hash)` directly (bypasses audit/write
    /// path — for test fixtures only).
    ///
    /// # Errors
    ///
    /// Returns [`TurboBridgeError::Internal`] if the storage lock is poisoned.
    pub fn seed(
        &self,
        tenant: impl Into<String>,
        hash: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<(), TurboBridgeError> {
        let mut g = self
            .objects
            .lock()
            .map_err(|_| TurboBridgeError::Internal("storage lock poisoned".into()))?;
        g.insert((tenant.into(), hash.into()), bytes.into());
        Ok(())
    }
}

impl TurboArtifactHandler for InMemoryTurboHandler {
    fn put(&self, req: TurboPutRequest) -> Result<TurboPutResponse, TurboBridgeError> {
        // DoS guard: reject overlong hashes before any audit.
        if req.hash.len() > MAX_HASH_LEN {
            return Err(TurboBridgeError::HashTooLong {
                len: req.hash.len(),
                max: MAX_HASH_LEN,
            });
        }

        // DoS / key-aliasing guard: `team_id` is interpolated into the storage
        // key (`"<team_id>/<hash>"`), so an empty / overlong / `/`-bearing value
        // is rejected BEFORE any audit emit or mutation (mirrors the hash guard).
        validate_team_id(&req.team_id)?;

        // No `team_id == caller_tenant` check: `team_id` is a Turborepo team
        // label, NOT the tenant. Isolation is the storage `tenant =
        // caller_tenant`; `team_id` is a sub-namespace inside it (key prefix).

        // PutAttempted audit BEFORE mutation.
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

        // Durable store (opaque key — no hash verification). `insert` returns the
        // prior value: `Some` ⇒ idempotent overwrite (`durable = false`); `None`
        // ⇒ fresh key (`durable = true`) — see rt-nuclear #25.
        let durable = {
            let mut g = self
                .objects
                .lock()
                .map_err(|_| TurboBridgeError::Internal("storage lock poisoned".into()))?;
            // Storage: tenant = authenticated caller_tenant; key carries the
            // team_id sub-namespace so teams under one tenant stay partitioned.
            g.insert(
                (
                    req.caller_tenant.clone(),
                    format!("{}/{}", req.team_id, req.hash),
                ),
                req.bytes,
            )
            .is_none()
        };

        // PutCommitted audit AFTER durable store.
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
        // DoS guard.
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

        // GetAttempted audit BEFORE lookup.
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

        // Storage lookup.
        let bytes = {
            let g = self
                .objects
                .lock()
                .map_err(|_| TurboBridgeError::Internal("storage lock poisoned".into()))?;
            // Storage: tenant = authenticated caller_tenant; key carries the
            // team_id sub-namespace (mirrors the write path).
            g.get(&(
                req.caller_tenant.clone(),
                format!("{}/{}", req.team_id, req.hash),
            ))
            .cloned()
            .ok_or_else(|| TurboBridgeError::NotFound {
                hash: req.hash.clone(),
            })?
        };

        // GetServed audit AFTER successful lookup.
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
        // Accept-and-drop: no state mutation, no audit event.
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

    fn fixture() -> (Arc<InMemoryTurboAuditSink>, InMemoryTurboHandler) {
        let audit = Arc::new(InMemoryTurboAuditSink::new());
        let h = InMemoryTurboHandler::new(audit.clone());
        (audit, h)
    }

    #[test]
    fn put_happy_emits_attempted_then_committed() {
        let (audit, h) = fixture();
        let resp = h
            .put(TurboPutRequest::new(
                "abc123",
                "team_a",
                "my-app",
                b"artifact bytes".to_vec(),
                Some(100),
                "user_1",
                "team_a",
                1_000,
            ))
            .expect("put");
        assert!(!resp.urls.is_empty());
        let rows = audit.snapshot().expect("snapshot");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, TurboAuditEventKind::PutAttempted);
        assert_eq!(rows[1].kind, TurboAuditEventKind::PutCommitted);
    }

    #[test]
    fn get_after_put_returns_same_bytes() {
        let (_, h) = fixture();
        let data = b"turbo build output cache bytes".to_vec();
        h.put(TurboPutRequest::new(
            "deadbeef",
            "team_b",
            "slug",
            data.clone(),
            None,
            "p1",
            "team_b",
            1,
        ))
        .expect("put");
        let resp = h
            .get(TurboGetRequest::new(
                "deadbeef", "team_b", "slug", "p1", "team_b", 2,
            ))
            .expect("get");
        assert_eq!(resp.bytes, data);
    }

    #[test]
    fn get_emits_attempted_then_served() {
        let (audit, h) = fixture();
        // Storage key is now `(caller_tenant, "<team_id>/<hash>")` — seed under
        // the derived key so the GET (team_id=team_c, hash=abc) hits it.
        h.seed("team_c", "team_c/abc", b"x".to_vec()).expect("seed");
        h.get(TurboGetRequest::new("abc", "team_c", "s", "p", "team_c", 1))
            .expect("get");
        let rows = audit.snapshot().expect("snapshot");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, TurboAuditEventKind::GetAttempted);
        assert_eq!(rows[1].kind, TurboAuditEventKind::GetServed);
    }

    #[test]
    fn get_not_found_returns_not_found_error() {
        let (_, h) = fixture();
        let err = h
            .get(TurboGetRequest::new("missing", "t1", "s", "p", "t1", 1))
            .expect_err("not found");
        assert!(matches!(err, TurboBridgeError::NotFound { .. }));
    }

    #[test]
    fn put_cross_tenant_isolated_no_denial_path() {
        // Repurposed: the old "PutDenied before rejection" premise is impossible
        // now (no `team_id == caller_tenant` check). The audit ordering for a
        // PUT is still asserted by `put_happy_emits_attempted_then_committed`.
        // Here we assert the NEW isolation invariant: a PUT under
        // caller_tenant="tenantA" is unreachable from caller_tenant="tenantB"
        // (same team_id + hash) — cross-tenant reads MISS (NotFound), they are
        // not "denied". A normal PUT also no longer emits any *Denied row.
        let (audit, h) = fixture();
        h.put(TurboPutRequest::new(
            "aaa",
            "shared_team",
            "s",
            b"tenantA bytes".to_vec(),
            None,
            "userA",
            "tenantA",
            1,
        ))
        .expect("put succeeds — no denial path");
        // No *Denied audit row exists on the put path anymore.
        let rows = audit.snapshot().expect("snapshot");
        assert!(rows
            .iter()
            .all(|r| r.kind != TurboAuditEventKind::PutDenied));
        // tenantB cannot read tenantA's artifact despite identical team/hash.
        let err = h
            .get(TurboGetRequest::new(
                "aaa",
                "shared_team",
                "s",
                "userB",
                "tenantB",
                2,
            ))
            .expect_err("tenantB isolated from tenantA");
        assert!(matches!(err, TurboBridgeError::NotFound { .. }));
    }

    #[test]
    fn get_cross_tenant_isolated_returns_not_found() {
        // Repurposed: cross-tenant GET is now an isolation MISS, not a denial.
        // tenantB requesting tenantA's (team_id, hash) gets NotFound; no
        // GetDenied row is emitted (only GetAttempted, since the lookup misses
        // after the attempt audit).
        let (audit, h) = fixture();
        h.put(TurboPutRequest::new(
            "bbb",
            "shared_team",
            "s",
            b"tenantA bytes".to_vec(),
            None,
            "userA",
            "tenantA",
            1,
        ))
        .expect("seed tenantA");
        let err = h
            .get(TurboGetRequest::new(
                "bbb",
                "shared_team",
                "s",
                "userB",
                "tenantB",
                2,
            ))
            .expect_err("isolated miss");
        assert!(matches!(err, TurboBridgeError::NotFound { .. }));
        let rows = audit.snapshot().expect("snapshot");
        assert!(rows
            .iter()
            .all(|r| r.kind != TurboAuditEventKind::GetDenied));
    }

    #[test]
    fn hash_too_long_rejected_without_audit() {
        let (audit, h) = fixture();
        let long_hash = "a".repeat(MAX_HASH_LEN + 1);
        let err = h
            .put(TurboPutRequest::new(
                long_hash.clone(),
                "t1",
                "s",
                vec![],
                None,
                "p",
                "t1",
                1,
            ))
            .expect_err("too long");
        assert!(matches!(err, TurboBridgeError::HashTooLong { .. }));
        // No audit events emitted for DoS guard path.
        assert!(audit.snapshot().expect("snapshot").is_empty());
    }

    #[test]
    fn team_id_invalid_rejected_without_audit() {
        use crate::error::MAX_TEAM_ID_LEN;
        for bad in ["", "a/b", "../other", &"a".repeat(MAX_TEAM_ID_LEN + 1)] {
            let (audit, h) = fixture();
            let err = h
                .put(TurboPutRequest::new(
                    "h1",
                    bad,
                    "s",
                    vec![],
                    None,
                    "p",
                    "t1",
                    1,
                ))
                .expect_err("invalid team_id rejected");
            assert!(matches!(err, TurboBridgeError::TeamIdInvalid { .. }));
            // DoS guard path emits no audit events.
            assert!(audit.snapshot().expect("snapshot").is_empty());
        }
    }

    #[test]
    fn team_id_invalid_rejected_on_get() {
        let (_, h) = fixture();
        let err = h
            .get(TurboGetRequest::new("h1", "a/b", "s", "p", "t1", 1))
            .expect_err("invalid team_id rejected on get");
        assert!(matches!(err, TurboBridgeError::TeamIdInvalid { .. }));
    }

    #[test]
    fn team_id_valid_accepted() {
        let (_, h) = fixture();
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

    #[test]
    fn hash_exactly_max_len_accepted() {
        let (_, h) = fixture();
        let max_hash = "b".repeat(MAX_HASH_LEN);
        h.put(TurboPutRequest::new(
            max_hash,
            "t1",
            "s",
            b"data".to_vec(),
            None,
            "p",
            "t1",
            1,
        ))
        .expect("max len hash accepted");
    }

    #[test]
    fn events_always_succeeds_regardless_of_body() {
        let (_, h) = fixture();
        h.events(TurboEventsRequest::new(b"{}".to_vec(), "p", 1))
            .expect("empty body");
        h.events(TurboEventsRequest::new(vec![], "p", 1))
            .expect("empty body");
        h.events(TurboEventsRequest::new(b"garbage!!@#".to_vec(), "p", 1))
            .expect("garbage body");
    }

    #[test]
    fn status_returns_enabled() {
        let (_, h) = fixture();
        let s = h.status().expect("status");
        assert_eq!(s.status, "enabled");
    }

    #[test]
    fn put_audit_failure_aborts_before_storage() {
        let audit = Arc::new(InMemoryTurboAuditSink::new());
        audit.inject_failure("d1 down").expect("inject");
        let h = InMemoryTurboHandler::new(audit.clone());
        let err = h
            .put(TurboPutRequest::new(
                "abc",
                "t1",
                "s",
                b"data".to_vec(),
                None,
                "p",
                "t1",
                1,
            ))
            .expect_err("audit fail");
        assert!(matches!(err, TurboBridgeError::AuditFailed(_)));
        // Storage must be untouched.
        let g = h.objects.lock().expect("lock");
        assert!(g.is_empty());
    }

    #[test]
    fn opaque_hash_stored_verbatim_no_verification() {
        // Turbo may send any string as hash (xxhash, sha512-prefix, etc.).
        // CoreLink stores it verbatim without hash verification.
        let (_, h) = fixture();
        let opaque = "turbo-xxhash-5e7c3b2a1f"; // not a valid sha256 hex
        let data = b"arbitrary build output".to_vec();
        h.put(TurboPutRequest::new(
            opaque,
            "t1",
            "s",
            data.clone(),
            None,
            "p",
            "t1",
            1,
        ))
        .expect("opaque hash stored");
        let resp = h
            .get(TurboGetRequest::new(opaque, "t1", "s", "p", "t1", 2))
            .expect("retrieved");
        assert_eq!(resp.bytes, data, "bytes round-trip under opaque key");
    }

    #[test]
    fn two_tenants_isolated_by_team_id() {
        let (_, h) = fixture();
        let data_a = b"team-a artifact".to_vec();
        let data_b = b"team-b artifact".to_vec();
        // Same hash, different teams.
        h.put(TurboPutRequest::new(
            "shared_hash",
            "team_a",
            "s",
            data_a.clone(),
            None,
            "p",
            "team_a",
            1,
        ))
        .expect("put a");
        h.put(TurboPutRequest::new(
            "shared_hash",
            "team_b",
            "s",
            data_b.clone(),
            None,
            "p",
            "team_b",
            2,
        ))
        .expect("put b");
        let resp_a = h
            .get(TurboGetRequest::new(
                "shared_hash",
                "team_a",
                "s",
                "p",
                "team_a",
                3,
            ))
            .expect("get a");
        let resp_b = h
            .get(TurboGetRequest::new(
                "shared_hash",
                "team_b",
                "s",
                "p",
                "team_b",
                4,
            ))
            .expect("get b");
        assert_eq!(resp_a.bytes, data_a);
        assert_eq!(resp_b.bytes, data_b);
    }
}
