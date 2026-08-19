//! `POST /_internal/public/revoke` — the `_public` shared-dedup blob
//! revocation endpoint (F3.2 BLOCKER-1, increment B1b).
//!
//! The write-side, incident-response counterpart to the per-tenant DSR erase
//! ([`crate::routes::cas_erase`]). A poisoned, malicious, or misclassified blob
//! that reached the cross-tenant [`PUBLIC_NAMESPACE`](crate::adapter_cache::PUBLIC_NAMESPACE)
//! (`_public`) cache serves to EVERY tenant, so it needs an operator kill-switch
//! that (a) durably and atomically blocks it from ever being served or
//! re-mirrored again, and (b) hard-deletes its bytes from R2.
//!
//! # The `_public` storage model (why this is NOT the DSR erase path)
//!
//! `_public` MoatCache blobs ([`crate::adapter_cache::MoatCache`]) are stored
//! content-addressed under a SINGLE reserved sentinel prefix
//! (`r2_s3::public_namespace_prefix`, derived from the TDK + the `_public`
//! sentinel UUID) — NOT under any tenant's HMAC prefix, and NOT under the
//! surface's service principal (`npm-adapter-host`, …): the principal is only
//! an audit actor on the CAS write, while the CAS-layer *tenant* is the literal
//! namespace `_public` (see `MoatCache::put`). So the same public bytes mirrored
//! by the brew, pip, and npm surfaces all dedup to the ONE `_public` R2 key.
//! Revoking a digest therefore erases exactly one key per region — NOT one per
//! surface principal. (An earlier design assumed per-principal copies; the code
//! path proves there is a single shared prefix.)
//!
//! # Ordering (linearize on the blocklist insert)
//!
//! 1. **INSERT `public_blocklist`** (migration 0097) — the durable, atomic,
//!    cross-region linearization point. Once this row exists, the `_public`
//!    read-guard (`adapter_cache.rs` `NOT EXISTS (public_blocklist)` join) and
//!    the BEFORE INSERT/UPDATE triggers refuse the digest even if a few R2 bytes
//!    briefly survive the (best-effort, multi-region) delete below. NEVER delete
//!    R2 bytes before this row is durable.
//! 2. **DELETE `adapter_cache_map`** rows for `(namespace='_public',
//!    content_hash)` — drops the url→hash mappings (all `url_hash`es for the
//!    same bytes) so nothing points at the revoked digest.
//! 3. **Emit a CloudEvents audit row** into `audit_outbox` (UNCHAINED, drained
//!    later by the S-09 worker — mirrors [`crate::routes::dsr`] `D1ErasureAuditSink`).
//! 4. **Hard-delete the R2 CAS bytes** under the `_public` prefix across all five
//!    CAS regions (best-effort; NoSuchKey is an idempotent success). A failure
//!    here is REPORTED, not fatal — the blocklist row (step 1) already guarantees
//!    the digest is never served, and a retry re-runs the idempotent delete.
//!
//! # Auth
//!
//! Gated by the DEDICATED `CORELINK_ERASE_AUTH_KEY` (constant-time compared),
//! the SAME irreversible-delete authority as [`crate::routes::cas_erase`] — NOT
//! the admin key (finding H4: an admin-key leak must not be able to drive
//! irreversible deletes). The route path is `/_internal/public/revoke` — kept
//! OUT of `/_internal/admin/*` ON PURPOSE: the worker edge front-gate
//! (`worker/src/index.ts` `internalConsumerForPath`) maps `/_internal/admin/*`
//! to the ADMIN consumer key but any other `/_internal/*` path to the ERASE
//! consumer key, so an `/_internal/admin/…` path would gate on the admin key at
//! the edge and 401 against this erase-keyed handler (the live blocker the B1b
//! prod proof caught, 2026-08-15). Edge consumer == handler gate == ERASE key. Unlike the DSR erase this path has NO per-subject
//! `dsr_requested` legitimacy anchor: revoking a public blob is content
//! moderation / incident response, not a data-subject request, so there is no
//! per-blob ticket to bind to. The controls are the dedicated erase key + the
//! durable audit row. Residual risk: a leaked erase key could revoke arbitrary
//! public digests (a denial-of-cache), which the blocklist makes permanent
//! (re-mirror is refused). Dual-control is a deliberate follow-up (B-later); for
//! launch, fast single-operator revoke is the right trade during a live-malware
//! incident, and the action is fully audited.
//!
//! # Composition
//!
//! Reuses the [`CasBlobEraser`](crate::routes::cas_erase::CasBlobEraser) seam
//! (its production [`R2CasBlobEraser`](crate::routes::cas_erase::R2CasBlobEraser)
//! is `_public`-aware — it derives the sentinel prefix via the same
//! `r2_s3::public_namespace_prefix` the writer used, so the erase key matches the
//! stored object by construction). [`build_state_from_env`] fail-CLOSES (route
//! unmounted) unless the erase key + R2 TDK + D1 are all present.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use subtle::ConstantTimeEq;

use corelink_adapter_host::oci::digest::OciDigest;

use crate::adapter_cache::PUBLIC_NAMESPACE;
use crate::routes::cas_erase::{CasBlobEraser, R2CasBlobEraser};
use crate::storage::d1_http::D1HttpClient;

/// HTTP header carrying the shared internal/erase secret (mirrors
/// [`crate::routes::cas_erase`] / [`crate::routes::internal_pat`] byte-for-byte).
const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Canonical revoke route path (matchit `{name}` captures — none needed; the
/// digest travels in the JSON body so it is never logged in a URL).
pub const PUBLIC_REVOKE_ROUTE: &str = "/_internal/public/revoke";

/// Max accepted `reason` / `approver` length (bounds the D1 rows; never PII).
const MAX_FIELD_LEN: usize = 256;

/// Default audit actor when the caller does not name an approver. The auth is a
/// shared key (no caller identity), so this is a role label, not a person.
const DEFAULT_APPROVER: &str = "erase-key-operator";

// ──────────────────────────────────────────────────────────────────────────────
// Collaborator traits (so the handler is hermetically testable without D1/R2)
// ──────────────────────────────────────────────────────────────────────────────

/// Outcome of the `public_blocklist` INSERT — distinguishes a first revocation
/// from an idempotent re-revoke (so the audit reuses the ORIGINAL event id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlocklistInsertOutcome {
    /// A fresh blocklist row was inserted.
    Inserted,
    /// The digest was already revoked; carries the existing row's audit id so
    /// the audit emission stays idempotent on a retry.
    AlreadyRevoked {
        /// The audit event id from the pre-existing blocklist row.
        existing_audit_event_id: String,
    },
}

/// Durable store for the `public_blocklist` + `adapter_cache_map` mutations.
#[async_trait]
pub trait PublicRevocationStore: Send + Sync + std::fmt::Debug {
    /// INSERT the blocklist row (the linearization point). Idempotent: a PK
    /// conflict returns [`BlocklistInsertOutcome::AlreadyRevoked`] with the
    /// existing `audit_event_id`, never an error.
    async fn insert_blocklist(
        &self,
        content_hash: &str,
        revoked_at_ms: i64,
        reason: &str,
        approver: &str,
        audit_event_id: &str,
    ) -> Result<BlocklistInsertOutcome, String>;

    /// DELETE every `adapter_cache_map` row for `(namespace='_public',
    /// content_hash)`. Idempotent (deleting zero rows is success).
    async fn delete_cache_map(&self, content_hash: &str) -> Result<(), String>;

    /// Resolve an UPSTREAM `_public` OCI digest wire string (`sha256:<hex>`) to
    /// its BLAKE3 `content_hash` via the SAME `adapter_cache_map` the OCI
    /// read/mirror path keys by — the `_public` moat stores the OCI digest
    /// verbatim as the `url_hash` (`crate::routes::oci`), so this is exactly the
    /// `OCI-digest → blake3-content-hash` indirection, reused (NOT a new table).
    ///
    /// Returns `None` when NO `_public` row maps that digest — the operator's
    /// incident action would silently do nothing, which [`handle_revoke`]
    /// surfaces as a LOUD error rather than a false 200.
    ///
    /// Deliberately NOT blocklist-filtered (unlike the read-side `_public`
    /// lookup in `adapter_cache.rs`): an already-revoked digest must still
    /// resolve so a re-revoke by upstream digest stays idempotent.
    async fn resolve_public_digest(&self, oci_digest_wire: &str) -> Result<Option<String>, String>;
}

/// The revocation audit record (a CloudEvents envelope is built from it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicRevocationAuditEvent {
    /// Stable event id (ties the audit to the blocklist row).
    pub audit_event_id: String,
    /// The revoked 64-hex content digest.
    pub content_hash: String,
    /// Actor label recorded for the audit (role, not a person — auth is a key).
    pub approver: String,
    /// Free-form bounded audit reason.
    pub reason: String,
    /// Revocation wall-clock time (Unix ms).
    pub revoked_at_ms: i64,
}

/// Durable audit sink — appends an UNCHAINED CloudEvents row to `audit_outbox`.
#[async_trait]
pub trait PublicRevocationAuditSink: Send + Sync + std::fmt::Debug {
    /// Append the revocation as a CloudEvents envelope (idempotent by event id).
    async fn emit_revocation(&self, event: &PublicRevocationAuditEvent) -> Result<(), String>;
}

// ──────────────────────────────────────────────────────────────────────────────
// Route state
// ──────────────────────────────────────────────────────────────────────────────

/// Shared state for the revoke route.
#[derive(Clone)]
pub struct PublicRevokeRouteState {
    /// `public_blocklist` + `adapter_cache_map` persistence (D1 in prod).
    pub store: Arc<dyn PublicRevocationStore>,
    /// R2 blob byte-deletion (the `_public`-aware CAS eraser seam).
    pub eraser: Arc<dyn CasBlobEraser>,
    /// Durable CloudEvents audit sink (`audit_outbox`).
    pub audit: Arc<dyn PublicRevocationAuditSink>,
    /// Dedicated erase authority (`CORELINK_ERASE_AUTH_KEY`), constant-time compared.
    pub erase_auth_key: Arc<str>,
}

impl std::fmt::Debug for PublicRevokeRouteState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicRevokeRouteState")
            .field("erase_auth_key", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

/// Revoke request body. The operator names the offending blob in EXACTLY ONE of
/// two mutually-exclusive spaces (see [`handle_revoke`] for why a bare 64-hex is
/// ambiguous between them and is refused):
///
/// - [`content_hash`](Self::content_hash) — the raw 64-hex BLAKE3 CoreLink
///   content digest, revoked directly (a pre-emptive block of a not-yet-active
///   digest is legitimately idempotent).
/// - [`upstream_digest`](Self::upstream_digest) — an UPSTREAM OCI digest in
///   canonical wire form (`sha256:<hex>`), resolved to its BLAKE3 content_hash
///   via the `_public` `adapter_cache_map` before revocation. This is the space
///   an operator holds during an OCI base-layer poisoning incident.
///
/// `reason` is a bounded audit string; `approver` is an optional actor label
/// (the auth carries no identity).
#[derive(Debug, Clone, Deserialize)]
pub struct PublicRevokeRequest {
    /// The raw 64-hex BLAKE3 content digest of the public blob to revoke.
    /// Mutually exclusive with [`upstream_digest`](Self::upstream_digest).
    #[serde(default)]
    pub content_hash: Option<String>,
    /// An UPSTREAM OCI digest to revoke by (`sha256:<hex>`), resolved to its
    /// BLAKE3 content_hash via the `_public` `adapter_cache_map`. MUST carry the
    /// explicit `sha256:`/`sha512:` algorithm prefix — a bare 64-hex here is
    /// refused as ambiguous. Mutually exclusive with
    /// [`content_hash`](Self::content_hash).
    #[serde(default)]
    pub upstream_digest: Option<String>,
    /// Bounded audit reason (why the blob is being revoked).
    #[serde(default)]
    pub reason: String,
    /// Optional actor label; defaults to a role when absent (auth is a key).
    #[serde(default)]
    pub approver: Option<String>,
}

/// Revoke response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicRevokeResponse {
    /// Always `true` once the blocklist row is durable (the digest is now
    /// un-servable regardless of R2 delete outcome).
    pub revoked: bool,
    /// `true` if the digest was already blocklisted (idempotent re-revoke).
    pub already_revoked: bool,
    /// Per-region R2 delete failures (reported, non-fatal — the blocklist row
    /// already guarantees the bytes are never served; a retry re-deletes).
    pub r2_delete_failures: Vec<String>,
    /// The RESOLVED 64-hex BLAKE3 content_hash that was revoked (for BOTH revoke
    /// spaces: the raw `content_hash` space returns it verbatim, the
    /// `upstream_digest` space returns the hash it resolved to). The Worker
    /// revoke seam marks the brew/pip edge `pubblock:<hash>` KV from THIS
    /// authoritative field rather than parsing the request body, so a
    /// revoke-by-upstream_digest collapses the edge-serve window too (finding
    /// F-1). Always present on a 200.
    pub content_hash: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

/// Build the revoke router (mounted top-level like `cas_erase`).
pub fn router(state: PublicRevokeRouteState) -> Router {
    Router::new()
        .route(PUBLIC_REVOKE_ROUTE, post(handle_revoke))
        .with_state(state)
}

/// `POST /_internal/public/revoke`.
///
/// Order (fail-CLOSED): erase-auth gate (constant-time, BEFORE any work) →
/// resolve the target BLAKE3 content_hash from the request (either the raw
/// `content_hash`, or an upstream `sha256:` OCI digest resolved via the
/// `_public` `adapter_cache_map`) → INSERT blocklist (linearize) → DELETE map →
/// emit audit → best-effort R2 delete under the `_public` prefix. The blocklist
/// insert is the durability barrier; everything after it is safe to retry.
///
/// # Two revoke spaces, and why a bare 64-hex is refused
///
/// A CoreLink BLAKE3 content digest and an upstream OCI `sha256` digest are
/// BOTH 64 hex characters, so a bare 64-hex is ambiguous between them. During an
/// OCI base-layer poisoning incident the operator holds the UPSTREAM `sha256:`
/// digest, NOT the BLAKE3 content_hash the blocklist keys by — so a bare 64-hex
/// interpreted as a content_hash would INSERT a value that matches no row,
/// return success, and audit a silent no-op while the poisoned blob keeps
/// serving. The endpoint therefore demands the caller name the space:
///
/// - `content_hash` (raw 64-hex BLAKE3) → revoked directly. A pre-emptive block
///   of a digest with no currently-active row is legitimately idempotent (200).
/// - `upstream_digest` (`sha256:<hex>`, algo prefix REQUIRED) → resolved to its
///   BLAKE3 content_hash via [`PublicRevocationStore::resolve_public_digest`]. A
///   digest that resolves to NO `_public` blob is a LOUD error (NOT 200): the
///   operator's incident action did nothing and that MUST surface. A bare 64-hex
///   supplied here (no `sha256:` prefix) is refused as ambiguous.
///
/// Exactly one of the two fields must be present.
async fn handle_revoke(
    State(state): State<PublicRevokeRouteState>,
    headers: HeaderMap,
    Json(req): Json<PublicRevokeRequest>,
) -> Response {
    // 1. Erase-auth gate — constant-time, before any work.
    if !internal_auth_ok(state.erase_auth_key.as_bytes(), &headers) {
        return error_response(StatusCode::UNAUTHORIZED, "unauthorized");
    }

    // 2. Resolve the target BLAKE3 content_hash from whichever space the caller
    //    named (fail-LOUD on an unresolvable / ambiguous / absent request).
    let content_hash = match resolve_target_content_hash(&state, &req).await {
        Ok(h) => h,
        Err(resp) => return resp,
    };

    if req.reason.len() > MAX_FIELD_LEN {
        return error_response(StatusCode::BAD_REQUEST, "reason too long");
    }
    let approver = req
        .approver
        .clone()
        .unwrap_or_else(|| DEFAULT_APPROVER.to_owned());
    if approver.len() > MAX_FIELD_LEN {
        return error_response(StatusCode::BAD_REQUEST, "approver too long");
    }

    let revoked_at_ms = now_ms();
    let fresh_audit_event_id = uuid_v4_string();

    // 3. INSERT public_blocklist — the linearization point.
    let outcome = match state
        .store
        .insert_blocklist(
            &content_hash,
            revoked_at_ms,
            &req.reason,
            &approver,
            &fresh_audit_event_id,
        )
        .await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::error!(error = %e, "public_revoke: blocklist insert failed");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal");
        }
    };
    let (already_revoked, audit_event_id) = match outcome {
        BlocklistInsertOutcome::Inserted => (false, fresh_audit_event_id),
        BlocklistInsertOutcome::AlreadyRevoked {
            existing_audit_event_id,
        } => (true, existing_audit_event_id),
    };

    // 4. DELETE the `_public` map rows for this content_hash (idempotent).
    if let Err(e) = state.store.delete_cache_map(&content_hash).await {
        tracing::error!(error = %e, "public_revoke: cache_map delete failed");
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    }

    // 5. Emit the CloudEvents audit row (idempotent on the deterministic id).
    let audit = PublicRevocationAuditEvent {
        audit_event_id,
        content_hash: content_hash.clone(),
        approver,
        reason: req.reason.clone(),
        revoked_at_ms,
    };
    if let Err(e) = state.audit.emit_revocation(&audit).await {
        tracing::error!(error = %e, "public_revoke: audit emit failed");
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal");
    }

    // 6. Best-effort R2 hard-delete under the SINGLE `_public` sentinel prefix
    //    (the eraser fans across all CAS regions internally). Non-fatal.
    let mut r2_delete_failures = Vec::new();
    if let Err(e) = state
        .eraser
        .erase_blob(PUBLIC_NAMESPACE, &content_hash)
        .await
    {
        tracing::error!(error = %e, "public_revoke: R2 erase failed (non-fatal; blocklist holds)");
        r2_delete_failures.push(e);
    }

    Json(PublicRevokeResponse {
        revoked: true,
        already_revoked,
        r2_delete_failures,
        content_hash,
    })
    .into_response()
}

/// Resolve the request to the single BLAKE3 `content_hash` to revoke, or an
/// early error [`Response`] (fail-LOUD — never a false 200). Enforces the
/// two-space contract: exactly one of `content_hash` / `upstream_digest`, an
/// explicit `sha256:` prefix on the upstream space (a bare 64-hex is ambiguous),
/// and a LOUD `404` when an upstream digest maps to no `_public` blob.
async fn resolve_target_content_hash(
    state: &PublicRevokeRouteState,
    req: &PublicRevokeRequest,
) -> Result<String, Response> {
    match (req.content_hash.as_deref(), req.upstream_digest.as_deref()) {
        (Some(_), Some(_)) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "provide exactly one of content_hash or upstream_digest, not both",
        )),
        (None, None) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "provide content_hash (64-hex blake3) or upstream_digest (sha256:<hex>)",
        )),
        // Raw BLAKE3 content_hash space — direct, idempotent (pre-emptive block OK).
        (Some(content_hash), None) => {
            if !is_64_hex(content_hash) {
                return Err(error_response(
                    StatusCode::BAD_REQUEST,
                    "content_hash must be 64 hex characters",
                ));
            }
            Ok(content_hash.to_ascii_lowercase())
        }
        // Upstream OCI digest space — REQUIRE the algo prefix, then resolve.
        (None, Some(upstream_digest)) => {
            // A bare 64-hex could be either space; refuse it explicitly so the
            // operator cannot revoke the wrong thing during an incident.
            if !upstream_digest.contains(':') {
                return Err(error_response(
                    StatusCode::BAD_REQUEST,
                    "upstream_digest is ambiguous: pass an explicit OCI digest \
                     (e.g. sha256:<hex>), not a bare 64-hex",
                ));
            }
            let digest = OciDigest::parse(upstream_digest).map_err(|_| {
                error_response(
                    StatusCode::BAD_REQUEST,
                    "upstream_digest must be a valid OCI digest (sha256:<hex>)",
                )
            })?;
            // Resolve OCI-digest → blake3 via the `_public` adapter_cache_map
            // (the same map the OCI read/mirror path keys by; the digest wire
            // string IS the moat url_hash).
            match state.store.resolve_public_digest(&digest.to_wire()).await {
                Ok(Some(content_hash)) => Ok(content_hash.to_ascii_lowercase()),
                // LOUD: nothing maps this upstream digest ⇒ revoking by it would
                // have silently done nothing. Surface it (NOT a 200).
                Ok(None) => Err(error_response(
                    StatusCode::NOT_FOUND,
                    "upstream_digest maps to no cached _public blob; nothing was revoked",
                )),
                Err(e) => {
                    tracing::error!(error = %e, "public_revoke: digest resolve failed");
                    Err(error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "internal",
                    ))
                }
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// D1-backed implementations
// ──────────────────────────────────────────────────────────────────────────────

/// `public_blocklist` + `adapter_cache_map` D1-over-HTTP store (migration 0097).
pub struct D1PublicRevocationStore {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for D1PublicRevocationStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1PublicRevocationStore")
            .finish_non_exhaustive()
    }
}

impl D1PublicRevocationStore {
    /// Construct over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}

#[async_trait]
impl PublicRevocationStore for D1PublicRevocationStore {
    async fn insert_blocklist(
        &self,
        content_hash: &str,
        revoked_at_ms: i64,
        reason: &str,
        approver: &str,
        audit_event_id: &str,
    ) -> Result<BlocklistInsertOutcome, String> {
        let sql = "INSERT INTO public_blocklist \
                   (content_hash, revoked_at_ms, reason, approver, audit_event_id) \
                   VALUES (?1, ?2, ?3, ?4, ?5)";
        let params = vec![
            json!(content_hash),
            json!(revoked_at_ms),
            json!(reason),
            json!(approver),
            json!(audit_event_id),
        ];
        match self.d1.query(sql, &params).await {
            Ok(_) => Ok(BlocklistInsertOutcome::Inserted),
            Err(e) if is_unique_conflict(&e) => {
                // Already revoked — read the existing audit id so the audit
                // emission reuses the ORIGINAL event id (idempotent retry).
                let rows = self
                    .d1
                    .query(
                        "SELECT audit_event_id FROM public_blocklist WHERE content_hash = ?1",
                        &[json!(content_hash)],
                    )
                    .await?;
                let existing = rows
                    .first()
                    .and_then(|r| r.get("audit_event_id"))
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| "blocklisted row missing audit_event_id".to_owned())?;
                Ok(BlocklistInsertOutcome::AlreadyRevoked {
                    existing_audit_event_id: existing.to_owned(),
                })
            }
            Err(e) => Err(e),
        }
    }

    async fn delete_cache_map(&self, content_hash: &str) -> Result<(), String> {
        self.d1
            .query(
                "DELETE FROM adapter_cache_map WHERE namespace = ?1 AND content_hash = ?2",
                &[json!(PUBLIC_NAMESPACE), json!(content_hash)],
            )
            .await
            .map(|_| ())
    }

    async fn resolve_public_digest(&self, oci_digest_wire: &str) -> Result<Option<String>, String> {
        // The `_public` moat stores the OCI digest wire string verbatim as the
        // `url_hash` (see `crate::routes::oci`), so the OCI-digest → blake3
        // indirection is a plain `(namespace, url_hash)` lookup. NOT
        // blocklist-filtered on purpose (see the trait doc): a re-revoke of an
        // already-blocklisted digest must still resolve.
        let rows = self
            .d1
            .query(
                "SELECT content_hash FROM adapter_cache_map \
                 WHERE namespace = ?1 AND url_hash = ?2 LIMIT 1",
                &[json!(PUBLIC_NAMESPACE), json!(oci_digest_wire)],
            )
            .await?;
        Ok(rows.into_iter().next().and_then(|r| {
            r.get("content_hash")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        }))
    }
}

/// D1-backed CloudEvents audit sink over the canonical `audit_outbox` table
/// (migration 0001). Writes an UNCHAINED row (`emitted_at` NULL) — the BLAKE3
/// hash-chain seal is the not-yet-live S-09 drain's job (mirrors the DSR sink).
pub struct D1PublicRevocationAuditSink {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for D1PublicRevocationAuditSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("D1PublicRevocationAuditSink")
            .finish_non_exhaustive()
    }
}

impl D1PublicRevocationAuditSink {
    /// Construct over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}

#[async_trait]
impl PublicRevocationAuditSink for D1PublicRevocationAuditSink {
    async fn emit_revocation(&self, event: &PublicRevocationAuditEvent) -> Result<(), String> {
        let event_type = "public.revoke";
        // Deterministic keys → an INSERT OR IGNORE re-emit of the same logical
        // revocation is a no-op (idempotent on a pipeline/retry).
        let id = format!("public.revoke:{}", event.content_hash);
        let request_id = event.audit_event_id.clone();
        let time_iso = crate::customer_d1::ms_to_iso8601(clamp_ms(event.revoked_at_ms));
        let payload = json!({
            "specversion": "1.0",
            "type": event_type,
            "source": "corelink/f3.2/public-revoke",
            "id": id,
            "subject": event.content_hash,
            "time": time_iso,
            "data": {
                "content_hash": event.content_hash,
                "approver": event.approver,
                "reason": event.reason,
            }
        });
        let payload_json =
            serde_json::to_string(&payload).map_err(|e| format!("audit payload serialize: {e}"))?;

        // `_public` is not a real tenant, so the tenant subquery yields NULL and
        // the residency `region` falls to 'wnam' (COALESCE) — the same
        // correlated-subquery shape the DSR sink uses so the residency trigger
        // (migration 0023) never aborts the audit INSERT.
        let sql = "INSERT OR IGNORE INTO audit_outbox \
             (id, tenant_id, digest, request_id, event_type, payload_json, enqueued_at, emitted_at, region) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, \
                     COALESCE((SELECT primary_region FROM tenant WHERE tenant_id = ?2), 'wnam'))";
        let params = vec![
            json!(id),
            json!(PUBLIC_NAMESPACE),
            json!(event.content_hash),
            json!(request_id),
            json!(event_type),
            json!(payload_json),
            json!(clamp_ms(event.revoked_at_ms)),
        ];
        self.d1.query(sql, &params).await.map(|_| ())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Composition root
// ──────────────────────────────────────────────────────────────────────────────

/// Build the route state from env. Returns `Some` only when the erase key + R2
/// TDK + D1 are ALL present; any missing piece ⇒ `None` and the route is NOT
/// mounted (fail-CLOSED) — the container never runs a half-built revoke that
/// could block-and-fail-to-erase (or fail to build the eraser and silently no-op
/// the R2 delete). Mirrors [`crate::routes::cas_erase::build_state_from_env`].
#[must_use]
pub fn build_state_from_env(erase_auth_key: Option<Arc<str>>) -> Option<PublicRevokeRouteState> {
    let erase_auth_key = erase_auth_key?;

    // Reuse the `_public`-aware production eraser (fail-CLOSED without TDK/D1).
    let eraser = R2CasBlobEraser::from_env()?;

    // Shared D1 client for the blocklist/map mutations + the audit sink.
    let env = crate::storage::StorageEnv::from_env()?;
    let d1 = Arc::new(crate::storage::d1_http::D1HttpClient::new(&env).ok()?);

    let store: Arc<dyn PublicRevocationStore> =
        Arc::new(D1PublicRevocationStore::new(Arc::clone(&d1)));
    let audit: Arc<dyn PublicRevocationAuditSink> =
        Arc::new(D1PublicRevocationAuditSink::new(Arc::clone(&d1)));

    Some(PublicRevokeRouteState {
        store,
        eraser: Arc::new(eraser),
        audit,
        erase_auth_key,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Constant-time erase-auth check (mirrors [`crate::routes::cas_erase`]
/// `internal_auth_ok` byte-for-byte: length-aware, no early return).
#[must_use]
fn internal_auth_ok(expected: &[u8], headers: &HeaderMap) -> bool {
    let provided = headers
        .get(INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let provided_bytes = provided.as_bytes();
    let provided_padded: Vec<u8> = if provided_bytes.len() >= expected.len() {
        provided_bytes.get(..expected.len()).unwrap_or(&[]).to_vec()
    } else {
        let mut v = provided_bytes.to_vec();
        v.resize(expected.len(), 0);
        v
    };
    let content_ok = expected.ct_eq(&provided_padded).unwrap_u8();
    let len_ok = u8::from(expected.len() == provided_bytes.len());
    (content_ok & len_ok) == 1
}

/// `true` iff `s` is exactly 64 ASCII hex digits (canonical BLAKE3 digest).
#[must_use]
fn is_64_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Detect a D1 UNIQUE / PRIMARY KEY conflict from the error text.
#[must_use]
fn is_unique_conflict(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("unique constraint failed") || lower.contains("primary key")
}

/// Current Unix time in milliseconds (saturating; 0 on a pre-epoch clock).
#[must_use]
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Clamp a millisecond timestamp into the non-negative `u64` domain
/// `ms_to_iso8601` expects (inlined; the DSR `clamp_ms` is `pub(super)`).
#[must_use]
fn clamp_ms(ms: i64) -> i64 {
    ms.max(0)
}

/// A random UUIDv4 string (audit event id). Uses `uuid::Uuid::new_v4`.
#[must_use]
fn uuid_v4_string() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn error_response(status: StatusCode, msg: &str) -> Response {
    (
        status,
        Json(ErrorResponse {
            error: msg.to_owned(),
        }),
    )
        .into_response()
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
    use axum::body::to_bytes;
    use axum::http::HeaderValue;
    use std::collections::HashMap;
    use tokio::sync::Mutex;

    const KEY: &str = "test-erase-key-0000000000000000000000";
    const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    /// An UPSTREAM OCI digest an operator holds during an incident (64-hex, but a
    /// DIFFERENT value from any CoreLink blake3 content_hash — this is the bug).
    const UPSTREAM_DIGEST: &str =
        "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    #[derive(Debug, Default)]
    struct FakeStoreInner {
        blocklist: HashMap<String, String>, // content_hash -> audit_event_id
        map_deletes: Vec<String>,
        // `_public` adapter_cache_map: OCI digest wire string -> blake3 content_hash.
        resolve: HashMap<String, String>,
    }

    #[derive(Debug, Default)]
    struct FakeStore {
        inner: Mutex<FakeStoreInner>,
    }

    #[async_trait]
    impl PublicRevocationStore for FakeStore {
        async fn insert_blocklist(
            &self,
            content_hash: &str,
            _revoked_at_ms: i64,
            _reason: &str,
            _approver: &str,
            audit_event_id: &str,
        ) -> Result<BlocklistInsertOutcome, String> {
            let mut g = self.inner.lock().await;
            if let Some(existing) = g.blocklist.get(content_hash) {
                return Ok(BlocklistInsertOutcome::AlreadyRevoked {
                    existing_audit_event_id: existing.clone(),
                });
            }
            g.blocklist
                .insert(content_hash.to_owned(), audit_event_id.to_owned());
            Ok(BlocklistInsertOutcome::Inserted)
        }

        async fn delete_cache_map(&self, content_hash: &str) -> Result<(), String> {
            self.inner
                .lock()
                .await
                .map_deletes
                .push(content_hash.to_owned());
            Ok(())
        }

        async fn resolve_public_digest(
            &self,
            oci_digest_wire: &str,
        ) -> Result<Option<String>, String> {
            Ok(self
                .inner
                .lock()
                .await
                .resolve
                .get(oci_digest_wire)
                .cloned())
        }
    }

    #[derive(Debug, Default)]
    struct FakeAudit {
        events: Mutex<Vec<PublicRevocationAuditEvent>>,
    }

    #[async_trait]
    impl PublicRevocationAuditSink for FakeAudit {
        async fn emit_revocation(&self, event: &PublicRevocationAuditEvent) -> Result<(), String> {
            self.events.lock().await.push(event.clone());
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct FakeEraser {
        calls: Mutex<Vec<(String, String)>>,
        fail: bool,
    }

    #[async_trait]
    impl CasBlobEraser for FakeEraser {
        async fn erase_blob(&self, tenant: &str, digest: &str) -> Result<(), String> {
            if self.fail {
                return Err(format!("r2 down for {tenant}"));
            }
            self.calls
                .lock()
                .await
                .push((tenant.to_owned(), digest.to_owned()));
            Ok(())
        }
    }

    fn state_with(
        store: Arc<FakeStore>,
        audit: Arc<FakeAudit>,
        eraser: Arc<FakeEraser>,
    ) -> PublicRevokeRouteState {
        PublicRevokeRouteState {
            store,
            eraser,
            audit,
            erase_auth_key: Arc::from(KEY),
        }
    }

    fn auth_headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(INTERNAL_AUTH_HEADER, HeaderValue::from_static(KEY));
        h
    }

    /// A revoke request in the raw-BLAKE3 `content_hash` space.
    fn body(hash: &str) -> PublicRevokeRequest {
        PublicRevokeRequest {
            content_hash: Some(hash.to_owned()),
            upstream_digest: None,
            reason: "malware".to_owned(),
            approver: Some("sec-oncall".to_owned()),
        }
    }

    /// A revoke request in the upstream OCI `sha256:` digest space.
    fn upstream_body(digest: &str) -> PublicRevokeRequest {
        PublicRevokeRequest {
            content_hash: None,
            upstream_digest: Some(digest.to_owned()),
            reason: "poisoned base layer".to_owned(),
            approver: Some("sec-oncall".to_owned()),
        }
    }

    async fn parse(resp: Response) -> PublicRevokeResponse {
        let b = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&b).unwrap()
    }

    #[tokio::test]
    async fn happy_path_revokes_deletes_map_audits_and_erases_once() {
        let store = Arc::new(FakeStore::default());
        let audit = Arc::new(FakeAudit::default());
        let eraser = Arc::new(FakeEraser::default());
        let resp = handle_revoke(
            State(state_with(store.clone(), audit.clone(), eraser.clone())),
            auth_headers(),
            Json(body(HASH)),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let out = parse(resp).await;
        assert!(out.revoked);
        assert!(!out.already_revoked);
        assert!(out.r2_delete_failures.is_empty());
        // F-1: the response carries the resolved content_hash so the Worker seam
        // can mark the brew/pip edge from the RESPONSE (raw space: verbatim).
        assert_eq!(out.content_hash, HASH);

        assert_eq!(store.inner.lock().await.blocklist.len(), 1);
        assert_eq!(store.inner.lock().await.map_deletes, vec![HASH.to_owned()]);
        assert_eq!(audit.events.lock().await.len(), 1);
        // Erase hits the `_public` prefix ONCE (not per-principal).
        let calls = eraser.calls.lock().await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (PUBLIC_NAMESPACE.to_owned(), HASH.to_owned()));
    }

    #[tokio::test]
    async fn re_revoke_is_idempotent_reuses_audit_id() {
        let store = Arc::new(FakeStore::default());
        let audit = Arc::new(FakeAudit::default());
        let eraser = Arc::new(FakeEraser::default());
        let st = state_with(store.clone(), audit.clone(), eraser.clone());

        let _ = handle_revoke(State(st.clone()), auth_headers(), Json(body(HASH))).await;
        let resp = handle_revoke(State(st), auth_headers(), Json(body(HASH))).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let out = parse(resp).await;
        assert!(out.revoked);
        assert!(out.already_revoked);

        assert_eq!(store.inner.lock().await.blocklist.len(), 1);
        let events = audit.events.lock().await;
        assert_eq!(
            events.len(),
            2,
            "both re-revokes emit (idempotent by id at D1)"
        );
        assert_eq!(
            events[0].audit_event_id, events[1].audit_event_id,
            "the re-revoke reuses the ORIGINAL audit id"
        );
    }

    #[tokio::test]
    async fn bad_hash_is_400_and_touches_nothing() {
        let store = Arc::new(FakeStore::default());
        let audit = Arc::new(FakeAudit::default());
        let eraser = Arc::new(FakeEraser::default());
        let resp = handle_revoke(
            State(state_with(store.clone(), audit.clone(), eraser.clone())),
            auth_headers(),
            Json(body("not-hex")),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(store.inner.lock().await.blocklist.is_empty());
        assert!(audit.events.lock().await.is_empty());
        assert!(eraser.calls.lock().await.is_empty());
    }

    #[tokio::test]
    async fn wrong_auth_is_401_and_touches_nothing() {
        let store = Arc::new(FakeStore::default());
        let audit = Arc::new(FakeAudit::default());
        let eraser = Arc::new(FakeEraser::default());
        let mut h = HeaderMap::new();
        h.insert(INTERNAL_AUTH_HEADER, HeaderValue::from_static("wrong"));
        let resp = handle_revoke(
            State(state_with(store.clone(), audit.clone(), eraser.clone())),
            h,
            Json(body(HASH)),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(store.inner.lock().await.blocklist.is_empty());
        assert!(audit.events.lock().await.is_empty());
        assert!(eraser.calls.lock().await.is_empty());
    }

    #[tokio::test]
    async fn r2_erase_failure_is_reported_not_fatal() {
        let store = Arc::new(FakeStore::default());
        let audit = Arc::new(FakeAudit::default());
        let eraser = Arc::new(FakeEraser {
            calls: Mutex::new(Vec::new()),
            fail: true,
        });
        let resp = handle_revoke(
            State(state_with(store.clone(), audit.clone(), eraser)),
            auth_headers(),
            Json(body(HASH)),
        )
        .await;
        // Blocklist row is durable → revoked=true even though R2 failed.
        assert_eq!(resp.status(), StatusCode::OK);
        let out = parse(resp).await;
        assert!(out.revoked);
        assert_eq!(out.r2_delete_failures.len(), 1);
        assert_eq!(store.inner.lock().await.blocklist.len(), 1);
        assert_eq!(audit.events.lock().await.len(), 1);
    }

    #[test]
    fn is_64_hex_guards() {
        assert!(is_64_hex(HASH));
        assert!(!is_64_hex("abc"));
        assert!(!is_64_hex(&"g".repeat(64)));
        assert!(!is_64_hex(&"a".repeat(63)));
    }

    // ── WP-F: revoke-by-sha256 upstream digest ──────────────────────────────

    /// DoD: revoke by an operator-held `sha256:` digest that maps to a LIVE
    /// `_public` blob → the blob's BLAKE3 content_hash is blocklisted + erased
    /// (serving is killed). The upstream digest and the blake3 differ; the map
    /// resolves the indirection.
    #[tokio::test]
    async fn revoke_by_upstream_sha256_that_maps_kills_serving() {
        let store = Arc::new(FakeStore::default());
        // Seed the `_public` adapter_cache_map: upstream OCI digest → blake3.
        store
            .inner
            .lock()
            .await
            .resolve
            .insert(UPSTREAM_DIGEST.to_owned(), HASH.to_owned());
        let audit = Arc::new(FakeAudit::default());
        let eraser = Arc::new(FakeEraser::default());

        let resp = handle_revoke(
            State(state_with(store.clone(), audit.clone(), eraser.clone())),
            auth_headers(),
            Json(upstream_body(UPSTREAM_DIGEST)),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let out = parse(resp).await;
        assert!(out.revoked);
        assert!(!out.already_revoked);
        // F-1: the response returns the RESOLVED blake3 content_hash (NOT the
        // sha256 upstream digest) — this is what lets the Worker seam mark the
        // brew/pip edge `pubblock:<hash>` for the upstream_digest incident path.
        assert_eq!(out.content_hash, HASH);
        assert_ne!(out.content_hash, UPSTREAM_DIGEST);
        // The BLAKE3 content_hash (NOT the sha256 digest) is what got blocklisted.
        let g = store.inner.lock().await;
        assert!(g.blocklist.contains_key(HASH));
        assert!(!g.blocklist.contains_key(UPSTREAM_DIGEST));
        assert_eq!(g.map_deletes, vec![HASH.to_owned()]);
        drop(g);
        let calls = eraser.calls.lock().await;
        assert_eq!(calls[0], (PUBLIC_NAMESPACE.to_owned(), HASH.to_owned()));
    }

    /// DoD: a `sha256:` digest that resolves to NOTHING → EXPLICIT error (assert
    /// NOT 200) and touches nothing. This is the silent-audited-no-op the WP
    /// exists to kill: the operator's incident action MUST surface, not succeed.
    #[tokio::test]
    async fn revoke_by_upstream_sha256_resolving_to_nothing_is_loud_error() {
        let store = Arc::new(FakeStore::default()); // empty resolve map
        let audit = Arc::new(FakeAudit::default());
        let eraser = Arc::new(FakeEraser::default());

        let resp = handle_revoke(
            State(state_with(store.clone(), audit.clone(), eraser.clone())),
            auth_headers(),
            Json(upstream_body(UPSTREAM_DIGEST)),
        )
        .await;

        assert_ne!(resp.status(), StatusCode::OK, "must NOT be a false success");
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert!(store.inner.lock().await.blocklist.is_empty());
        assert!(store.inner.lock().await.map_deletes.is_empty());
        assert!(audit.events.lock().await.is_empty());
        assert!(eraser.calls.lock().await.is_empty());
    }

    /// DoD: a pre-emptive block by a raw BLAKE3 content_hash with no currently
    /// active row → still 200 idempotent (a legitimate pre-emptive block).
    #[tokio::test]
    async fn preemptive_raw_blake3_block_with_no_active_row_is_200() {
        let store = Arc::new(FakeStore::default());
        let audit = Arc::new(FakeAudit::default());
        let eraser = Arc::new(FakeEraser::default());

        let resp = handle_revoke(
            State(state_with(store.clone(), audit.clone(), eraser.clone())),
            auth_headers(),
            Json(body(HASH)),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let out = parse(resp).await;
        assert!(out.revoked);
        assert!(!out.already_revoked);
        assert!(store.inner.lock().await.blocklist.contains_key(HASH));
    }

    /// DoD: a bare 64-hex supplied in the upstream space (no `sha256:` prefix,
    /// but INTENDED as an upstream digest) → refused as ambiguous, touches
    /// nothing. It could be a blake3 or a stripped sha256 — the operator must
    /// name the space.
    #[tokio::test]
    async fn bare_64hex_as_upstream_digest_is_rejected_ambiguous() {
        let store = Arc::new(FakeStore::default());
        let audit = Arc::new(FakeAudit::default());
        let eraser = Arc::new(FakeEraser::default());

        let resp = handle_revoke(
            State(state_with(store.clone(), audit.clone(), eraser.clone())),
            auth_headers(),
            Json(upstream_body(HASH)), // HASH is a bare 64-hex, no `sha256:`
        )
        .await;

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        assert!(store.inner.lock().await.blocklist.is_empty());
        assert!(audit.events.lock().await.is_empty());
        assert!(eraser.calls.lock().await.is_empty());
    }

    /// Guard: neither field, or both fields, is a 400 (exactly-one contract).
    #[tokio::test]
    async fn neither_or_both_fields_is_400() {
        let mk = || {
            (
                Arc::new(FakeStore::default()),
                Arc::new(FakeAudit::default()),
                Arc::new(FakeEraser::default()),
            )
        };

        let (s, a, e) = mk();
        let neither = PublicRevokeRequest {
            content_hash: None,
            upstream_digest: None,
            reason: String::new(),
            approver: None,
        };
        let resp = handle_revoke(State(state_with(s, a, e)), auth_headers(), Json(neither)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let (s, a, e) = mk();
        let both = PublicRevokeRequest {
            content_hash: Some(HASH.to_owned()),
            upstream_digest: Some(UPSTREAM_DIGEST.to_owned()),
            reason: String::new(),
            approver: None,
        };
        let resp = handle_revoke(State(state_with(s, a, e)), auth_headers(), Json(both)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
