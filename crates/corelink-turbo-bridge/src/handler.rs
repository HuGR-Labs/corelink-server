//! `TurboArtifactHandler` trait + `InMemoryTurboHandler` deterministic fake.
//!
//! # Trait contract
//!
//! Every implementation **MUST**:
//!
//! 1. Validate `hash.len() <= MAX_HASH_LEN`; reject longer hashes with
//!    [`TurboBridgeError::HashTooLong`] without emitting an audit event.
//! 2. On a PUT/GET where `team_id != caller_tenant`: emit
//!    `PutDenied` / `GetDenied` audit event BEFORE returning
//!    [`TurboBridgeError::CrossTenantDenied`] (fail-CLOSED ordering).
//! 3. On a PUT: emit `PutAttempted` BEFORE storage mutation; emit
//!    `PutCommitted` AFTER durable store.  If audit emit fails, abort
//!    without mutation.
//! 4. On a GET: emit `GetAttempted` BEFORE storage lookup.  On success,
//!    emit `GetServed` AFTER bytes retrieved.
//! 5. No `unwrap()` / `expect()` / `panic!()` on any return path.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::audit::{TurboAuditEvent, TurboAuditEventKind, TurboAuditSink};
use crate::error::TurboBridgeError;
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
    /// `teamId` query parameter — must equal `caller_tenant`.
    pub team_id: String,
    /// `slug` query parameter — informational only (logged, not partitioned).
    pub slug: String,
    /// Artifact bytes.
    pub bytes: Vec<u8>,
    /// `x-artifact-duration` header in milliseconds (optional).
    pub duration_ms: Option<u64>,
    /// Authenticated principal identity.
    pub principal: String,
    /// Authenticated tenant from the auth layer (must equal `team_id`).
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
}

impl TurboPutResponse {
    /// Construct a [`TurboPutResponse`] with the given URLs.
    #[must_use]
    pub fn new(urls: Vec<String>) -> Self {
        Self { urls }
    }

    /// Construct the canonical single-URL response for a stored artifact.
    ///
    /// `tenant` and `hash` are used to form the canonical CoreLink URL.
    #[must_use]
    pub fn for_artifact(tenant: &str, hash: &str) -> Self {
        Self {
            urls: vec![format!(
                "/{version}/artifacts/{hash}?teamId={tenant}",
                version = VERCEL_API_VERSION,
                hash = hash,
                tenant = tenant,
            )],
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
    /// `teamId` query parameter — must equal `caller_tenant`.
    pub team_id: String,
    /// `slug` query parameter — informational only.
    pub slug: String,
    /// Authenticated principal identity.
    pub principal: String,
    /// Authenticated tenant from the auth layer (must equal `team_id`).
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
    /// - [`TurboBridgeError::CrossTenantDenied`] — `team_id != caller_tenant`.
    /// - [`TurboBridgeError::AuditFailed`] — audit sink unavailable.
    /// - [`TurboBridgeError::Internal`] — storage error.
    fn put(&self, req: TurboPutRequest) -> Result<TurboPutResponse, TurboBridgeError>;

    /// Handle `GET /v8/artifacts/<hash>` — retrieve artifact bytes.
    ///
    /// # Errors
    ///
    /// - [`TurboBridgeError::HashTooLong`] — hash exceeds [`MAX_HASH_LEN`].
    /// - [`TurboBridgeError::CrossTenantDenied`] — `team_id != caller_tenant`.
    /// - [`TurboBridgeError::NotFound`] — artifact not in store.
    /// - [`TurboBridgeError::AuditFailed`] — audit sink unavailable.
    /// - [`TurboBridgeError::Internal`] — storage error.
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
/// Storage key: `(tenant_id, turbo_hash_string)` → bytes.
/// The hash is stored as the opaque key Turbo supplies — no hash verification.
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

    fn check_tenant(team_id: &str, caller_tenant: &str) -> bool {
        team_id == caller_tenant
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

        // Cross-tenant check — audit BEFORE returning denial.
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

        // Durable store (opaque key — no hash verification).
        {
            let mut g = self
                .objects
                .lock()
                .map_err(|_| TurboBridgeError::Internal("storage lock poisoned".into()))?;
            g.insert((req.team_id.clone(), req.hash.clone()), req.bytes);
        }

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

        Ok(TurboPutResponse::for_artifact(&req.team_id, &req.hash))
    }

    fn get(&self, req: TurboGetRequest) -> Result<TurboGetResponse, TurboBridgeError> {
        // DoS guard.
        if req.hash.len() > MAX_HASH_LEN {
            return Err(TurboBridgeError::HashTooLong {
                len: req.hash.len(),
                max: MAX_HASH_LEN,
            });
        }

        // Cross-tenant check — audit BEFORE returning denial.
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
            g.get(&(req.team_id.clone(), req.hash.clone()))
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
        h.seed("team_c", "abc", b"x".to_vec()).expect("seed");
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
    fn put_cross_tenant_emits_put_denied_before_rejection() {
        let (audit, h) = fixture();
        let err = h
            .put(TurboPutRequest::new(
                "aaa",
                "victim_team",
                "s",
                b"bytes".to_vec(),
                None,
                "attacker",
                "attacker_team",
                1,
            ))
            .expect_err("denied");
        assert!(matches!(err, TurboBridgeError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("snapshot");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, TurboAuditEventKind::PutDenied);
    }

    #[test]
    fn get_cross_tenant_emits_get_denied_before_rejection() {
        let (audit, h) = fixture();
        let err = h
            .get(TurboGetRequest::new(
                "bbb",
                "victim_team",
                "s",
                "attacker",
                "attacker_team",
                1,
            ))
            .expect_err("denied");
        assert!(matches!(err, TurboBridgeError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("snapshot");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, TurboAuditEventKind::GetDenied);
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
