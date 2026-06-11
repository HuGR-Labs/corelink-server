//! Internal DSR erasure endpoint (WI-S11-008 Wave 0).
//!
//! `POST /_internal/dsr/erase` — the queue consumer (signup-worker
//! `dsr_consumer.ts`) forwards `dsr.queued.v1` messages here. The handler
//! verifies the internal-auth shared secret (constant-time, mirroring
//! [`crate::routes::internal_pat`]), deserializes the request, maps it to a
//! canonical [`ErasureRequest`], and drives the 12-backend erasure orchestrator.
//!
//! ## WAVE 0 PLACEHOLDER
//!
//! The 12 backend adapters are the in-memory no-op `InMemoryBackendErasureAdapter`s.
//! The full pipeline (Clerk `user.deleted` → queue → consumer → this endpoint →
//! orchestrator → audit + idempotency ledger) is wired and exercised end-to-end,
//! but **NO real data is deleted yet**. Wave 1 swaps each canonical adapter for
//! its real transport (D1 / R2 / Stripe / KV / Loki). Until then the endpoint
//! returns 200 (orchestrated, audited, ledger-recorded) without erasing.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use corelink_privacy_erasure_worker::audit_emit::{ErasureAuditSink, InMemoryErasureAuditSink};
use corelink_privacy_erasure_worker::backends::{
    BackendErasureAdapter, InMemoryBackendErasureAdapter,
};
use corelink_privacy_erasure_worker::event::{
    canonical_backend_kinds, BackendKind, ErasureDecision, ErasureRequest, ErasureSalt,
};
use corelink_privacy_erasure_worker::idempotency::{
    ErasureIdempotencyLedger, InMemoryErasureIdempotencyLedger,
};
use corelink_privacy_erasure_worker::orchestrator::{ErasureWorker, InMemoryErasureWorker};

// WI-S11-008 Wave 1 real D1 transports (ledger + audit sink + D1 erase
// adapter). The remaining 11 backends stay in-memory placeholders until
// Wave 1 increments 3-4 (R2 CAS/AC, Stripe, KV/Loki, pseudonymized).
mod adapter_d1;
mod adapter_r2_ac;
mod adapter_r2_cas;
mod adapter_stripe;
mod audit;
mod d1util;
mod ledger;

const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Wire shape of the `dsr.queued.v1` message produced by the Clerk
/// `user.deleted` webhook (`apps/signup-worker/src/webhooks/clerk.ts`). Unknown
/// fields (`schema`, `source`, `clerk_user_id`) are accepted and ignored.
#[derive(Debug, Deserialize)]
pub struct DsrQueuedV1 {
    /// Canonical UUID DSR id (idempotency key; deterministic per Clerk user).
    pub dsr_id: String,
    /// Tenant id whose data is erased.
    pub tenant_id: String,
    /// Data subject id (`== tenant_id` for a one-user-per-tenant account).
    pub subject_id: String,
    /// 64 hex chars (32-byte) per-DSR erasure salt.
    pub erasure_salt_hex: String,
    /// Enqueue instant (Unix epoch ms) — the SLA clock anchor.
    pub queued_at_ms: u64,
    /// Whether the upstream DSR ticket is under legal hold.
    pub legal_hold: bool,
}

/// Shared state for the DSR erasure route.
#[derive(Clone)]
pub struct DsrRouteState {
    internal_auth_key: String,
    worker: Arc<InMemoryErasureWorker>,
}

impl std::fmt::Debug for DsrRouteState {
    // Redact the internal-auth secret; never let it reach a log/Debug sink.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DsrRouteState")
            .field("internal_auth_key", &"<redacted>")
            .field("worker", &self.worker)
            .finish()
    }
}

/// Constant-time internal-auth check. Mirrors
/// `internal_pat::internal_auth_ok` byte-for-byte so the internal-auth gates
/// stay consistent (pad provided to the secret length, run `ct_eq`, fold in the
/// real length-equality so a longer/shorter value can never match).
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

/// Build the canonical 12-backend orchestrator. WAVE 0: every adapter is the
/// in-memory no-op placeholder; Wave 1 replaces each with its real transport.
///
/// # Errors
/// Returns an error string if the canonical 12-adapter invariant is violated
/// (should be impossible — `canonical_backend_kinds` is the source of truth).
pub fn build_placeholder_worker() -> Result<InMemoryErasureWorker, String> {
    let audit = Arc::new(InMemoryErasureAuditSink::new());
    let ledger = Arc::new(InMemoryErasureIdempotencyLedger::new());
    let adapters: Vec<Arc<dyn BackendErasureAdapter>> = canonical_backend_kinds()
        .iter()
        .map(|k| Arc::new(InMemoryBackendErasureAdapter::new(*k)) as Arc<dyn BackendErasureAdapter>)
        .collect();
    InMemoryErasureWorker::try_new(audit, ledger, adapters).map_err(|e| e.to_string())
}

/// Build the canonical orchestrator with the REAL D1-backed ledger + audit
/// sink + D1 erase adapter (WI-S11-008 Wave 1). The other 11 backends are
/// still in-memory placeholders (increments 3-4). `None` when `StorageEnv`
/// is not configured (so the route falls back to the all-placeholder
/// worker — e.g. in tests / unconfigured envs).
fn build_d1_worker() -> Option<InMemoryErasureWorker> {
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = Arc::new(crate::storage::d1_http::D1HttpClient::new(&storage_env).ok()?);

    let audit: Arc<dyn ErasureAuditSink> = Arc::new(audit::D1ErasureAuditSink::new(Arc::clone(&d1)));
    let ledger: Arc<dyn ErasureIdempotencyLedger> =
        Arc::new(ledger::D1ErasureIdempotencyLedger::new(Arc::clone(&d1)));
    let adapters: Vec<Arc<dyn BackendErasureAdapter>> = canonical_backend_kinds()
        .iter()
        .map(|k| -> Arc<dyn BackendErasureAdapter> {
            match *k {
                BackendKind::D1 => Arc::new(adapter_d1::D1EraseAdapter::new(Arc::clone(&d1))),
                BackendKind::R2Ac => {
                    Arc::new(adapter_r2_ac::R2AcEraseAdapter::new(Arc::clone(&d1)))
                }
                BackendKind::R2Cas => {
                    Arc::new(adapter_r2_cas::R2CasEraseAdapter::new(Arc::clone(&d1)))
                }
                BackendKind::Stripe => {
                    Arc::new(adapter_stripe::StripePseudonymizeAdapter::new(Arc::clone(&d1)))
                }
                other => Arc::new(InMemoryBackendErasureAdapter::new(other)),
            }
        })
        .collect();

    InMemoryErasureWorker::try_new(audit, ledger, adapters).ok()
}

/// Build the route state from env. `None` when `CORELINK_INTERNAL_AUTH_KEY` is
/// unset/empty (route not mounted) — mirrors `internal_pat::build_state_from_env`.
#[must_use]
pub fn build_state_from_env() -> Option<DsrRouteState> {
    let internal_auth_key = std::env::var("CORELINK_INTERNAL_AUTH_KEY").ok()?;
    if internal_auth_key.is_empty() {
        return None;
    }
    // Prefer the real D1-backed worker; fall back to the all-placeholder
    // worker when storage is unconfigured (keeps the route mountable in
    // tests / partially-configured envs).
    let worker = match build_d1_worker() {
        Some(w) => w,
        None => build_placeholder_worker().ok()?,
    };
    Some(DsrRouteState {
        internal_auth_key,
        worker: Arc::new(worker),
    })
}

/// Mount `POST /_internal/dsr/erase`.
pub fn router(state: DsrRouteState) -> Router {
    Router::new()
        .route("/_internal/dsr/erase", post(handle_erase))
        .with_state(state)
}

fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(0)
}

fn parse_request(msg: &DsrQueuedV1) -> Result<ErasureRequest, String> {
    let dsr_id = Uuid::parse_str(&msg.dsr_id).map_err(|e| format!("dsr_id: {e}"))?;
    let tenant_id = Uuid::parse_str(&msg.tenant_id).map_err(|e| format!("tenant_id: {e}"))?;
    let subject_id = Uuid::parse_str(&msg.subject_id).map_err(|e| format!("subject_id: {e}"))?;
    let salt_bytes = hex::decode(&msg.erasure_salt_hex).map_err(|e| format!("salt hex: {e}"))?;
    let salt_arr: [u8; 32] = salt_bytes
        .try_into()
        .map_err(|_| "erasure_salt_hex must decode to exactly 32 bytes".to_string())?;
    Ok(ErasureRequest {
        dsr_id,
        tenant_id,
        subject_id,
        erasure_salt: ErasureSalt::new(salt_arr),
        queued_at_ms: msg.queued_at_ms,
        legal_hold: msg.legal_hold,
    })
}

async fn handle_erase(
    State(state): State<DsrRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let msg: DsrQueuedV1 = match serde_json::from_slice(&body) {
        Ok(m) => m,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid body: {e}")).into_response(),
    };
    let request = match parse_request(&msg) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };
    match state.worker.process_erasure(&request, now_ms()) {
        // Cross-tenant / policy rejection (tenant pre-check). Should never
        // happen on the trusted internal path; fail closed if it does.
        Ok(ErasureDecision::Rejected { .. }) => {
            (StatusCode::UNPROCESSABLE_ENTITY, "erasure rejected").into_response()
        }
        Ok(_decision) => (
            StatusCode::OK,
            Json(serde_json::json!({ "ok": true, "dsr_id": msg.dsr_id })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("erasure failed: {e}"),
        )
            .into_response(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn placeholder_worker_builds_with_12_canonical_adapters() {
        // Construction enforces the canonical 12-backend order; an error here
        // means canonical_backend_kinds drifted from BACKEND_COUNT.
        assert!(build_placeholder_worker().is_ok());
    }

    #[test]
    fn parse_request_maps_wire_to_canonical() {
        let msg = DsrQueuedV1 {
            dsr_id: "00000000-0000-7000-8000-000000000001".to_string(),
            tenant_id: "00000000-0000-7000-8000-000000000002".to_string(),
            subject_id: "00000000-0000-7000-8000-000000000002".to_string(),
            erasure_salt_hex: "ab".repeat(32),
            queued_at_ms: 1_700_000_000_000,
            legal_hold: false,
        };
        let req = parse_request(&msg).unwrap();
        assert_eq!(req.queued_at_ms, 1_700_000_000_000);
        assert!(!req.legal_hold);
        assert_eq!(req.erasure_salt.as_bytes()[0], 0xab);
    }

    #[test]
    fn parse_request_rejects_bad_salt_length() {
        let msg = DsrQueuedV1 {
            dsr_id: "00000000-0000-7000-8000-000000000001".to_string(),
            tenant_id: "00000000-0000-7000-8000-000000000002".to_string(),
            subject_id: "00000000-0000-7000-8000-000000000002".to_string(),
            erasure_salt_hex: "abcd".to_string(), // 2 bytes, not 32
            queued_at_ms: 1,
            legal_hold: false,
        };
        assert!(parse_request(&msg).is_err());
    }
}
