//! `POST /_internal/audit/cas-attempted` — emit the `ReadAttempted` audit rows
//! for an existence probe the **edge** performed, so the edge can serve
//! `findMissingBlobs` without becoming a second author of the audit row.
//!
//! # Why this route exists
//!
//! The container answers `findMissingBlobs` at ~13 digests/s: the ceiling is a
//! 0.25-vCPU instance reaching R2 over the public S3 endpoint, not our fan-out
//! (`docs/design/2026-08-25-adr-edge-native-find-missing.md`). Probing R2 from
//! the Worker's in-colo binding removes that constant — measured 8.65 s → 2.10 s
//! at n=100, with 43/43 answer parity.
//!
//! What the edge CANNOT do is the other half of
//! [`crate::storage::r2_s3::R2CasHandler::exists_batch`]: write one
//! `ReadAttempted` row per digest into `audit_outbox` and refuse to answer if
//! that write fails. Those rows are the evidence the S-09 chain drains, and
//! `findMissingBlobs` is a REAPI **read** surface.
//!
//! Writing them from the Worker over the D1 binding was considered and
//! rejected. The row is a contract — UUIDv7 `id`, a CloudEvents 1.0
//! `payload_json`, `UNIQUE (request_id, event_type)` dedup, and a `region`
//! column a table trigger `RAISE(ABORT)`s on if it disagrees with the tenant.
//! A TypeScript re-implementation would drift from
//! [`crate::storage::d1_audit_sink::D1AuditOutboxSink::build_row`] while still
//! passing its own tests, and the drift would surface as rows the drain
//! rejects. So the edge probes, and calls **here** to have the SAME sink emit
//! the SAME rows. One author of the row shape, forever.
//!
//! See `docs/design/2026-08-26-adr-edge-find-missing-audit-seam.md`.
//!
//! # Contract
//!
//! The caller MUST await this and MUST NOT serve its probe result unless this
//! returned `204`. Every other outcome — `4xx`, `503`, timeout, transport
//! error — means the caller falls through to the container, which will attempt
//! the audit itself and fail closed in its own taxonomy. "We could not audit"
//! must never become "we answered".
//!
//! # Auth
//!
//! Gated by the **dedicated** `CORELINK_AUDIT_ATTEMPTED_AUTH_KEY`, constant-time,
//! with NO fallback to the shared `CORELINK_INTERNAL_AUTH_KEY` — the same
//! posture as `/_internal/pat/mint`. This route writes tenant-attributed audit
//! rows on behalf of an arbitrary tenant; a shared-secret holder must not be
//! able to forge them. Unset or under 32 chars ⇒ the route is NOT mounted.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use subtle::ConstantTimeEq;

use corelink_handler_cas::{AuditEvent, AuditEventKind};

use crate::storage::d1_audit_sink::D1AuditOutboxSink;

const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Maximum digests one call may audit.
///
/// Mirrors `EDGE_FIND_MISSING_MAX_DIGESTS` in
/// `worker/src/lib/edge_find_missing.ts`, which is the edge's own subrequest-
/// budget cap — above it the edge does not probe at all, so above it there is
/// nothing for this route to audit. Deliberately far below the container's
/// `FIND_MISSING_BLOB_CAP` (4096): this route is not an alternative intake for
/// a full-size batch, and a caller that sends one is malfunctioning.
pub const AUDIT_CAS_ATTEMPTED_MAX_DIGESTS: usize = 256;

/// Minimum length of the dedicated auth key. Mirrors the fleet-wide floor
/// (`openssl rand -hex 32` → 64 chars; the floor is 32).
const MIN_AUTH_KEY_LEN: usize = 32;

/// Shared state for the route.
#[derive(Clone)]
pub struct AuditCasAttemptedState {
    internal_auth_key: String,
    sink: Arc<D1AuditOutboxSink>,
}

impl core::fmt::Debug for AuditCasAttemptedState {
    // Never surface the auth key.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AuditCasAttemptedState")
            .finish_non_exhaustive()
    }
}

/// Request body. Field names mirror `CasReadRequest`'s so the two cannot drift
/// apart in meaning: `tenant` is whose blobs were probed, `caller_tenant` is
/// whose credential asked, and they must be equal.
#[derive(Debug, Deserialize)]
pub struct AuditCasAttemptedRequest {
    /// The tenant whose blobs were probed — whose `audit_outbox` rows these are.
    pub tenant: String,
    /// The authenticated principal that asked, copied onto every row.
    pub principal: String,
    /// The tenant the CREDENTIAL belongs to. Must equal `tenant`; a mismatch is
    /// audited as `ReadDenied` and refused, exactly as `exists_batch` does.
    pub caller_tenant: String,
    /// Probe time in unix ms, stamped by the caller so all rows in one batch
    /// share it (the container's `at_unix_ms` per `CasReadRequest`).
    pub at_unix_ms: u64,
    /// The probed digests, in request order. Lowercase hex sha256 — the REAPI
    /// keyspace `findMissingBlobs` probes.
    pub digests: Vec<String>,
}

/// Constant-time internal-auth check. Mirrors
/// [`crate::routes::audit_drain`]'s byte-for-byte (pad to the secret length,
/// `ct_eq`, then fold in real length equality so a longer or shorter value can
/// never match).
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

/// True for a canonical lowercase hex sha256.
#[must_use]
fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn err(status: StatusCode, reason: &str) -> Response {
    (status, Json(json!({ "error": reason }))).into_response()
}

/// Emit the rows, or refuse. `204` is the ONLY outcome that authorizes the
/// caller to serve its probe result.
async fn handle_cas_attempted(
    State(state): State<AuditCasAttemptedState>,
    headers: HeaderMap,
    Json(req): Json<AuditCasAttemptedRequest>,
) -> Response {
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return err(StatusCode::UNAUTHORIZED, "unauthorized");
    }

    // CROSS-TENANT, FIRST — before any row is built, mirroring `exists_batch`,
    // which scans the whole slice before dispatching so one poisoned digest
    // cannot let the others through. The denial is AUDITED (a `ReadDenied` row,
    // the same kind the container writes) and only then refused: a denial that
    // leaves no trace is the one an attacker wants.
    if req.tenant != req.caller_tenant {
        let denied = AuditEvent::new(
            AuditEventKind::ReadDenied,
            req.tenant.clone(),
            // `ReadDenied` in `exists_batch` carries the offending digest; here
            // the whole request is cross-tenant, so attribute the first digest
            // if there is one and an empty hash otherwise, rather than invent.
            req.digests.first().cloned().unwrap_or_default(),
            req.principal.clone(),
            req.at_unix_ms,
        );
        if let Err(e) = state.sink.emit_cas_batch_async(&[denied]).await {
            tracing::error!("audit/cas-attempted: cross-tenant denial audit failed: {e}");
            return err(StatusCode::SERVICE_UNAVAILABLE, "audit_unavailable");
        }
        return err(StatusCode::FORBIDDEN, "cross_tenant_denied");
    }

    if req.digests.len() > AUDIT_CAS_ATTEMPTED_MAX_DIGESTS {
        return err(StatusCode::PAYLOAD_TOO_LARGE, "batch_too_large");
    }
    if req.digests.iter().any(|d| !is_sha256_hex(d)) {
        return err(StatusCode::BAD_REQUEST, "malformed_digest");
    }
    if req.digests.is_empty() {
        // The serial path writes no row for an empty probe either. Nothing to
        // audit is not an audit failure, and the caller served nothing.
        return StatusCode::NO_CONTENT.into_response();
    }

    let events: Vec<AuditEvent> = req
        .digests
        .iter()
        .map(|hash| {
            AuditEvent::new(
                AuditEventKind::ReadAttempted,
                req.tenant.clone(),
                hash.clone(),
                req.principal.clone(),
                req.at_unix_ms,
            )
        })
        .collect();

    match state.sink.emit_cas_batch_async(&events).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            // 503, not 500: this is "try the container", not "your request was
            // bad". The caller's contract is to fall through on anything but
            // 204, but the status still has to tell the truth.
            tracing::error!("audit/cas-attempted: outbox write failed: {e}");
            err(StatusCode::SERVICE_UNAVAILABLE, "audit_unavailable")
        }
    }
}

/// Build the route state from env, or `None` — the route is then NOT mounted
/// (fail-CLOSED), and the edge's awaited call gets a 404 it must treat like any
/// other non-204: fall through to the container.
///
/// Requires the DEDICATED `CORELINK_AUDIT_ATTEMPTED_AUTH_KEY` (≥ 32 chars, no
/// shared-key fallback) and a configured D1 `StorageEnv`.
#[must_use]
pub fn build_state_from_env() -> Option<AuditCasAttemptedState> {
    let internal_auth_key = match std::env::var("CORELINK_AUDIT_ATTEMPTED_AUTH_KEY") {
        Ok(k) if k.len() >= MIN_AUTH_KEY_LEN => k,
        _ => {
            tracing::warn!(
                "no usable CORELINK_AUDIT_ATTEMPTED_AUTH_KEY (dedicated; NO shared fallback; \
                 < 32 chars); /_internal/audit/cas-attempted NOT mounted (fail-CLOSED) — the \
                 edge findMissingBlobs path will fall through to the container"
            );
            return None;
        }
    };
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = crate::storage::d1_http::D1HttpClient::new(&storage_env);
    let sink = crate::storage::d1_audit_sink::cas_audit_sink_from_d1_concrete(d1).ok()?;
    Some(AuditCasAttemptedState {
        internal_auth_key,
        sink,
    })
}

/// Mount `POST /_internal/audit/cas-attempted`.
#[must_use]
pub fn router(state: AuditCasAttemptedState) -> Router {
    Router::new()
        .route("/_internal/audit/cas-attempted", post(handle_cas_attempted))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(INTERNAL_AUTH_HEADER, value.parse().expect("header value"));
        h
    }

    #[test]
    fn auth_rejects_missing_wrong_prefix_and_extension() {
        let expected = "k".repeat(64);
        assert!(!internal_auth_ok(expected.as_bytes(), &HeaderMap::new()));
        assert!(!internal_auth_ok(
            expected.as_bytes(),
            &headers_with(&"k".repeat(63))
        ));
        assert!(!internal_auth_ok(
            expected.as_bytes(),
            &headers_with(&format!("{expected}x"))
        ));
        assert!(internal_auth_ok(
            expected.as_bytes(),
            &headers_with(&expected)
        ));
    }

    #[test]
    fn digest_validation_matches_the_reapi_keyspace() {
        assert!(is_sha256_hex(&"a".repeat(64)));
        assert!(is_sha256_hex(&"0".repeat(64)));
        // Uppercase is NOT canonical: the container writes lowercase keys, and
        // accepting both would audit one digest under two spellings.
        assert!(!is_sha256_hex(&"A".repeat(64)));
        assert!(!is_sha256_hex(&"a".repeat(63)));
        assert!(!is_sha256_hex(&"a".repeat(65)));
        assert!(!is_sha256_hex(&"g".repeat(64)));
        assert!(!is_sha256_hex(""));
    }

    #[test]
    fn the_cap_mirrors_the_edge_and_stays_under_the_container_cap() {
        assert_eq!(AUDIT_CAS_ATTEMPTED_MAX_DIGESTS, 256);
        assert!(AUDIT_CAS_ATTEMPTED_MAX_DIGESTS < corelink_bazel_bridge::FIND_MISSING_BLOB_CAP);
    }
}
