//! PUBLIC, UNAUTHENTICATED erasure-attestation verifier endpoints
//! (Artifact 1, WP-C1 — closes brutal-review H1's "no verifier" gap).
//!
//! A signed GDPR erasure attestation is only meaningful if ANYONE can verify it
//! OFFLINE: fetch the signed bundle, fetch the region's public key, recompute the
//! Ed25519 signature over the canonical payload. These two GET handlers serve
//! exactly that — no PAT, no Clerk session, no internal-auth (an erasure proof is
//! public by design). They are mounted OUTSIDE the ratelimit/residency/auth layers
//! (see [`crate::main`]) and routed by the Worker's `/v1/public/*` arm with NO PAT
//! gate.
//!
//! 1. `GET /v1/public/attestation/{request_id}` — the signed attestation bundle
//!    for one DSR. Returns 200 with the bundle ONLY when the row exists AND both
//!    `signature_ed25519` and `canonical_payload_jcs` are non-NULL. A pre-0079
//!    unsigned row (either column NULL) is NOT a verifiable attestation → 404,
//!    never served as if signed. A missing row → 404.
//! 2. `GET /v1/public/keys/erasure/{region}.pub` — the active/overlap Ed25519
//!    public key(s) for a region, as a JSON list keyed by `key_id` so an offline
//!    verifier picks the key matching the attestation's `attestation_key_id`.
//!    Unknown region → 404; a known region with no live key → 404.
//!
//! Both are D1-reads only (no R2 dependency): the D1 index carries the signed
//! bytes; R2 is the authoritative archive but is not needed for verification.
//! All queries are parameterized (`request_id` / `region` are bound params) —
//! injection-safe.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};

use corelink_erasure_attestation::Region;

use crate::storage::d1_http::{D1HttpClient, D1Row};

/// Shared state for the public attestation routes (D1 index reader).
#[derive(Clone)]
pub struct PublicAttestationState {
    d1: Arc<D1HttpClient>,
}

impl std::fmt::Debug for PublicAttestationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicAttestationState")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

/// Build the route state from env. `None` (route not mounted) when the D1
/// `StorageEnv` is not configured — without the D1 index there is nothing to
/// serve. These are PUBLIC reads, so there is deliberately NO auth-key gate
/// (mirrors how [`crate::routes::audit_drain::build_state_from_env`] is D1-gated,
/// minus the internal-auth key).
#[must_use]
pub fn build_state_from_env() -> Option<PublicAttestationState> {
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = Arc::new(crate::storage::d1_http::D1HttpClient::new(&storage_env).ok()?);
    Some(PublicAttestationState { d1 })
}

/// Mount the two public verifier routes.
pub fn router(state: PublicAttestationState) -> Router {
    Router::new()
        .route("/v1/public/attestation/{request_id}", get(handle_attestation))
        .route("/v1/public/keys/erasure/{region_pub}", get(handle_region_key))
        .with_state(state)
}

/// Extract a non-NULL string column (`None` for SQL NULL / absent / non-string).
fn col_str(row: &D1Row, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Extract an `i64` column (`None` for SQL NULL / absent / non-integer).
fn col_i64(row: &D1Row, key: &str) -> Option<i64> {
    row.get(key).and_then(Value::as_i64)
}

/// Map a single `erasure_attestations` row to the served bundle JSON.
///
/// Returns `None` (caller → 404) unless the row is a genuine SIGNED attestation:
/// BOTH `signature_ed25519` AND `canonical_payload_jcs` must be non-NULL strings
/// (a pre-0079 unsigned row has them NULL and is never served as if signed), and
/// every NOT-NULL index column must be present/well-typed. On success returns the
/// self-contained bundle an offline verifier needs: the signature, the EXACT
/// canonical payload bytes that were signed, and the `attestation_key_id` that
/// selects the verifying public key.
fn attestation_row_to_response(request_id: &str, row: &D1Row) -> Option<Value> {
    // The two signed columns (added in migration 0079) — both REQUIRED non-NULL.
    let signature_ed25519 = col_str(row, "signature_ed25519")?;
    let canonical_payload_jcs = col_str(row, "canonical_payload_jcs")?;
    // The 0032 NOT-NULL index columns — defensive: a malformed index row is not a
    // verifiable attestation.
    let tenant_id = col_str(row, "tenant_id")?;
    let region = col_str(row, "region")?;
    let attestation_key_id = col_i64(row, "attestation_key_id")?;
    let r2_key = col_str(row, "r2_key")?;
    let signed_at_ms = col_i64(row, "signed_at_ms")?;
    let kms_provider = col_str(row, "kms_provider")?;
    let kms_key_id = col_str(row, "kms_key_id")?;
    let evidence_hash = col_str(row, "evidence_hash")?;
    Some(json!({
        "request_id": request_id,
        "tenant_id": tenant_id,
        "region": region,
        "attestation_key_id": attestation_key_id,
        "signature_ed25519": signature_ed25519,
        "canonical_payload_jcs": canonical_payload_jcs,
        "r2_key": r2_key,
        "signed_at_ms": signed_at_ms,
        "kms_provider": kms_provider,
        "kms_key_id": kms_key_id,
        "evidence_hash": evidence_hash,
    }))
}

/// Map the first row of an attestation result set to the served bundle (or `None`
/// → 404 for a missing row / an unsigned row).
fn first_attestation_response(request_id: &str, rows: Vec<D1Row>) -> Option<Value> {
    rows.into_iter()
        .next()
        .and_then(|r| attestation_row_to_response(request_id, &r))
}

/// Strip the trailing `.pub` extension from the `{region}.pub` path segment and
/// parse the canonical region. Returns `None` (caller → 404) for a missing `.pub`
/// suffix or an unknown region.
fn region_from_pub_param(param: &str) -> Option<Region> {
    let stem = param.strip_suffix(".pub")?;
    Region::parse(stem)
}

/// Map the `erasure_public_keys` rows for a region to the served JSON list. Each
/// entry carries `key_id`, `state`, and the PEM, so an offline verifier picks the
/// key whose `key_id == attestation.attestation_key_id`. Returns `None` (caller →
/// 404) when there is no live (active/overlap) key for the region.
fn public_keys_to_response(region: Region, rows: &[D1Row]) -> Option<Value> {
    let mut keys = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(public_key_pem) = col_str(row, "public_key_pem") else {
            continue;
        };
        let key_id = col_i64(row, "key_id");
        let state = col_str(row, "state");
        keys.push(json!({
            "key_id": key_id,
            "state": state,
            "public_key_pem": public_key_pem,
        }));
    }
    if keys.is_empty() {
        return None;
    }
    Some(json!({
        "region": region.as_str(),
        "keys": keys,
    }))
}

/// `GET /v1/public/attestation/{request_id}` — serve the signed attestation
/// bundle. 404 when the row is missing OR unsigned (sig/canonical NULL).
async fn handle_attestation(
    State(state): State<PublicAttestationState>,
    Path(request_id): Path<String>,
) -> Response {
    let rows = match state
        .d1
        .query(
            "SELECT signature_ed25519, canonical_payload_jcs, tenant_id, region, \
                    attestation_key_id, r2_key, signed_at_ms, kms_provider, kms_key_id, \
                    evidence_hash \
             FROM erasure_attestations WHERE request_id = ?1",
            &[json!(request_id)],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "public/attestation: D1 read failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    };
    match first_attestation_response(&request_id, rows) {
        Some(bundle) => Json(bundle).into_response(),
        // Missing row OR a pre-0079 unsigned row → 404 (never serve an unsigned
        // row as if it were a signed attestation).
        None => (StatusCode::NOT_FOUND, "attestation not found").into_response(),
    }
}

/// `GET /v1/public/keys/erasure/{region}.pub` — serve the active/overlap public
/// key(s) for a region. 404 for an unknown region or no live key.
async fn handle_region_key(
    State(state): State<PublicAttestationState>,
    Path(region_pub): Path<String>,
) -> Response {
    let Some(region) = region_from_pub_param(&region_pub) else {
        return (StatusCode::NOT_FOUND, "unknown region").into_response();
    };
    let rows = match state
        .d1
        .query(
            "SELECT key_id, state, public_key_pem FROM erasure_public_keys \
             WHERE region = ?1 AND state IN ('active', 'overlap')",
            &[json!(region.as_str())],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "public/keys: D1 read failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "lookup failed").into_response();
        }
    };
    match public_keys_to_response(region, &rows) {
        Some(body) => Json(body).into_response(),
        None => (StatusCode::NOT_FOUND, "no public key for region").into_response(),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    /// Build a `D1Row` (serde_json object map) from a JSON object literal.
    fn row(v: Value) -> D1Row {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn missing_row_is_404() {
        // Empty result set → None → 404.
        assert!(first_attestation_response("req-1", vec![]).is_none());
    }

    #[test]
    fn unsigned_row_null_signature_is_404() {
        // A pre-0079 row: signature_ed25519 NULL → not a verifiable attestation.
        let r = row(json!({
            "signature_ed25519": Value::Null,
            "canonical_payload_jcs": "{\"a\":1}",
            "tenant_id": "t",
            "region": "weur",
            "attestation_key_id": 1,
            "r2_key": "corelink-audit-weur/erasure_attestations/req-1.json",
            "signed_at_ms": 1_700_000_000_000_i64,
            "kms_provider": "corelink_d1r2_erase",
            "kms_key_id": "dsr:req-1",
            "evidence_hash": "ab".repeat(32),
        }));
        assert!(attestation_row_to_response("req-1", &r).is_none());
    }

    #[test]
    fn unsigned_row_null_canonical_is_404() {
        let r = row(json!({
            "signature_ed25519": "c2ln",
            "canonical_payload_jcs": Value::Null,
            "tenant_id": "t",
            "region": "weur",
            "attestation_key_id": 1,
            "r2_key": "k",
            "signed_at_ms": 1_700_000_000_000_i64,
            "kms_provider": "corelink_d1r2_erase",
            "kms_key_id": "dsr:req-1",
            "evidence_hash": "ab".repeat(32),
        }));
        assert!(attestation_row_to_response("req-1", &r).is_none());
    }

    #[test]
    fn signed_row_returns_full_bundle() {
        let r = row(json!({
            "signature_ed25519": "c2lnbmF0dXJl",
            "canonical_payload_jcs": "{\"request_id\":\"req-1\"}",
            "tenant_id": "00000000-0000-7000-8000-000000000def",
            "region": "weur",
            "attestation_key_id": 7,
            "r2_key": "corelink-audit-weur/erasure_attestations/req-1.json",
            "signed_at_ms": 1_700_000_000_000_i64,
            "kms_provider": "corelink_d1r2_erase",
            "kms_key_id": "dsr:req-1",
            "evidence_hash": "cd".repeat(32),
        }));
        let bundle = first_attestation_response("req-1", vec![r]).expect("signed row → bundle");
        assert_eq!(bundle["request_id"], json!("req-1"));
        assert_eq!(bundle["signature_ed25519"], json!("c2lnbmF0dXJl"));
        assert_eq!(bundle["canonical_payload_jcs"], json!("{\"request_id\":\"req-1\"}"));
        assert_eq!(bundle["attestation_key_id"], json!(7));
        assert_eq!(bundle["region"], json!("weur"));
        assert_eq!(bundle["evidence_hash"], json!("cd".repeat(32)));
    }

    #[test]
    fn region_pub_param_parses_known_strips_suffix() {
        assert_eq!(region_from_pub_param("weur.pub"), Some(Region::Weur));
        assert_eq!(region_from_pub_param("enam.pub"), Some(Region::Enam));
    }

    #[test]
    fn region_pub_param_rejects_unknown_and_missing_suffix() {
        // Unknown region → None → 404.
        assert_eq!(region_from_pub_param("apac.pub"), None);
        assert_eq!(region_from_pub_param("bogus.pub"), None);
        // Missing .pub suffix → None.
        assert_eq!(region_from_pub_param("weur"), None);
        // Case-sensitive (canonical lowercase only).
        assert_eq!(region_from_pub_param("WEUR.pub"), None);
    }

    #[test]
    fn public_keys_empty_is_404() {
        assert!(public_keys_to_response(Region::Weur, &[]).is_none());
    }

    #[test]
    fn public_keys_returns_pem_list_keyed_by_key_id() {
        let rows = vec![
            row(json!({
                "key_id": 1,
                "state": "overlap",
                "public_key_pem": "-----BEGIN PUBLIC KEY-----\nOLD\n-----END PUBLIC KEY-----",
            })),
            row(json!({
                "key_id": 2,
                "state": "active",
                "public_key_pem": "-----BEGIN PUBLIC KEY-----\nNEW\n-----END PUBLIC KEY-----",
            })),
        ];
        let body = public_keys_to_response(Region::Weur, &rows).expect("rows → key list");
        assert_eq!(body["region"], json!("weur"));
        let keys = body["keys"].as_array().expect("keys is a list");
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0]["key_id"], json!(1));
        assert_eq!(keys[1]["key_id"], json!(2));
        assert!(keys[1]["public_key_pem"].as_str().unwrap().contains("NEW"));
    }
}
