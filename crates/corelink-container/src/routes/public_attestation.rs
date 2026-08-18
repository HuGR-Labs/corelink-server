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
//!
//! # M22(a) — scoped per-IP rate limit
//!
//! This router is merged in `main.rs` OUTSIDE `build_with_factory`'s
//! `rate_limit_layer` (see the module docs above: no PAT, no residency guard,
//! no per-tenant token bucket), so before this fix it carried NO container-side
//! rate limiting at all — an internet-reachable, unauthenticated, D1-read-only
//! surface. [`rate_limit_public_verifier`] closes that gap with a SCOPED
//! per-IP token bucket local to this router only (never touches
//! [`crate::routes::ratelimit_layer`] or the data-plane gate). It reuses the
//! same `corelink-ratelimit` engine + bounded F-022 NoOp sinks
//! ([`NoOpRateLimitAuditSink`] / [`NoOpRateLimitMetrics`]) as
//! `ratelimit_layer.rs`, keyed by [`ip_key_uuid`] — a LOCAL FNV-1a-128
//! IP→UUID fold (mirrors `ratelimit_layer::tenant_key_uuid`'s shape, own
//! namespace, own function — the shared layer is not touched).
//!
//! Budget: [`PUBLIC_VERIFIER_REQ_PER_SEC`] / [`PUBLIC_VERIFIER_BURST`] —
//! deliberately generous (regulator/DPA/human verifier traffic, never CI/cache
//! volume) so shared-NAT/VPN/CI-egress callers are never falsely throttled.
//! FAIL-OPEN: the trusted `x-corelink-client-ip` header (set by the Worker
//! from `cf-connecting-ip`, mirrors [`crate::routes::signup`]'s
//! `extract_client_ip`) is REQUIRED to key a bucket; when absent (dev/CI/no
//! header) the request passes through untouched rather than sharing one
//! collapsed bucket — this surface has no pre-auth abuse budget to protect
//! (D1-read-only, no mutation), so unlike `signup.rs`'s `"_no_ip"` sentinel
//! the conservative choice here is availability, not a shared throttle.

use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    http::{header::RETRY_AFTER, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use uuid::Uuid;

use corelink_erasure_attestation::Region;
use corelink_ratelimit::{
    BucketKey, InMemoryTokenBucketRateLimiter, NoOpRateLimitAuditSink, NoOpRateLimitMetrics,
    RateLimitConfig, RateLimitDecision, RateLimiter,
};

use crate::storage::d1_http::{D1HttpClient, D1Row};
use crate::wall_clock::{self, WallClock};

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

/// Mount the two public verifier routes, wrapped in the M22(a) scoped per-IP
/// rate-limit layer ([`rate_limit_public_verifier`]).
pub fn router(state: PublicAttestationState, rl_state: PublicVerifierRateLimitState) -> Router {
    Router::new()
        .route(
            "/v1/public/attestation/{request_id}",
            get(handle_attestation),
        )
        .route(
            "/v1/public/keys/erasure/{region_pub}",
            get(handle_region_key),
        )
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            rl_state,
            rate_limit_public_verifier,
        ))
}

// --- M22(a): scoped per-IP rate limit ---------------------------------------

/// The trusted, Worker-injected client-IP header (see
/// [`crate::routes::signup::extract_client_ip`] for the sibling pre-auth
/// pattern; the Worker sets it from the unforgeable `cf-connecting-ip`).
const CLIENT_IP_HEADER: &str = "x-corelink-client-ip";

/// Generous sustained per-IP request rate (tokens/second) for the public
/// verifier surface. ~20 req/s is far above any single regulator/DPA/human
/// verifier's interactive rate while still bounding a single-IP flood; kept
/// intentionally loose so shared-NAT/VPN/CI-egress traffic is never falsely
/// throttled (this surface carries zero CI/cache traffic).
pub const PUBLIC_VERIFIER_REQ_PER_SEC: u32 = 20;

/// Per-IP burst capacity (tokens) for the public verifier surface. A 3×
/// burst-over-sustained window absorbs a verifier script fetching several
/// attestations + keys back-to-back before shedding with 429s.
pub const PUBLIC_VERIFIER_BURST: u32 = 60;

/// Per-request cost charged against the bucket (one token per HTTP request).
const COST_PER_REQUEST: u32 = 1;

/// Fixed namespace bytes for [`ip_key_uuid`] — LOCAL to this router (distinct
/// from [`crate::routes::ratelimit_layer`]'s `TENANT_NS`; the two limiters
/// never share a bucket map).
const PUBLIC_VERIFIER_IP_NS: [u8; 16] = *b"corelink-rl-pub!";

/// Derive a STABLE 128-bit bucket key from the raw client-IP string via
/// FNV-1a-128 (mirrors `ratelimit_layer::tenant_key_uuid`'s shape — a LOCAL
/// re-implementation, not a shared call, per the frozen M22(a) design: this
/// router's rate limiting must stay fully decoupled from the data-plane
/// `ratelimit_layer.rs`). A real IPv4/IPv6 literal folds deterministically so
/// the same client IP always lands in the same bucket and distinct IPs land
/// in distinct buckets with overwhelming probability.
#[must_use]
fn ip_key_uuid(raw_ip: &str) -> Uuid {
    const FNV_OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    let mut hash = FNV_OFFSET;
    for &b in PUBLIC_VERIFIER_IP_NS.iter().chain(raw_ip.as_bytes()) {
        hash ^= u128::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    Uuid::from_u128(hash)
}

/// Shared state for [`rate_limit_public_verifier`]: one per-IP token-bucket
/// limiter instance for the whole public-verifier router, plus the wall clock
/// anchoring bucket refill. Wired with the BOUNDED F-022 production sinks
/// ([`NoOpRateLimitAuditSink`] / [`NoOpRateLimitMetrics`]) — O(1) memory, no
/// unbounded capture buffers on ordinary in-budget traffic (the same posture
/// `ratelimit_layer.rs` documents; the TEST-capture `InMemory*` sinks are
/// never wired here).
#[derive(Clone)]
pub struct PublicVerifierRateLimitState {
    limiter: Arc<InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics>>,
    clock: Arc<dyn WallClock>,
}

impl std::fmt::Debug for PublicVerifierRateLimitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublicVerifierRateLimitState")
            .finish_non_exhaustive()
    }
}

impl PublicVerifierRateLimitState {
    /// Build the production state: the canonical generous per-IP cap
    /// ([`PUBLIC_VERIFIER_REQ_PER_SEC`] / [`PUBLIC_VERIFIER_BURST`]) and the
    /// system wall clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(wall_clock::default_wall_clock())
    }

    /// Build with an explicit wall clock (test wiring injects a deterministic
    /// fake; production uses [`wall_clock::default_wall_clock`]).
    #[must_use]
    pub fn with_clock(clock: Arc<dyn WallClock>) -> Self {
        // `with_overrides` only returns `None` on a self-inconsistent config
        // (zero burst, inverted Retry-After bounds); our constants are
        // statically valid, so fall back to the crate's canonical config if a
        // future edit ever breaks that invariant rather than panicking.
        let config = RateLimitConfig::with_overrides(
            PUBLIC_VERIFIER_REQ_PER_SEC,
            PUBLIC_VERIFIER_BURST,
            corelink_ratelimit::DEFAULT_RETRY_AFTER_FLOOR_SECS,
            corelink_ratelimit::RETRY_AFTER_HARD_CEILING_SECS,
            corelink_ratelimit::RETRY_AFTER_CANCELED_TENANT_SECS,
        )
        .unwrap_or_else(RateLimitConfig::canonical);
        let limiter = InMemoryTokenBucketRateLimiter::new(
            Arc::new(NoOpRateLimitAuditSink::new()),
            Arc::new(NoOpRateLimitMetrics::new()),
            config,
        );
        Self {
            limiter: Arc::new(limiter),
            clock,
        }
    }
}

impl Default for PublicVerifierRateLimitState {
    fn default() -> Self {
        Self::new()
    }
}

/// Axum middleware: scoped per-IP token-bucket gate for the public verifier
/// router ONLY (M22a). Reads [`CLIENT_IP_HEADER`]; FAIL-OPEN (pass through,
/// no bucket charged) when it is absent/empty — dev/CI/no-header traffic is
/// never throttled. On a present header, charges one token against the IP's
/// bucket and either forwards (`Allow`) or rejects with 429 + `Retry-After`
/// (RFC 6585 §4). On the limiter's OWN internal fault (mutex poison, sink
/// error) we fail-OPEN (allow) but log — availability of the public verifier
/// is prioritised over a perfectly-enforced cap, matching
/// `ratelimit_layer::rate_limit_layer`'s documented posture.
async fn rate_limit_public_verifier(
    State(state): State<PublicVerifierRateLimitState>,
    req: Request,
    next: Next,
) -> Response {
    let raw_ip = client_ip_from_headers(req.headers());
    let Some(raw_ip) = raw_ip else {
        // No trusted client-IP header ⇒ fail-OPEN (dev/CI/no-header traffic).
        return next.run(req).await;
    };

    let ip = ip_key_uuid(&raw_ip);
    let now_ms = state.clock.now_ms();
    let bucket_key = BucketKey::per_tenant(ip);

    match state
        .limiter
        .try_acquire(ip, bucket_key, COST_PER_REQUEST, now_ms)
    {
        Ok(outcome) => match outcome.decision {
            RateLimitDecision::Allow { .. } => next.run(req).await,
            RateLimitDecision::Deny429 {
                retry_after_secs, ..
            } => {
                tracing::warn!(
                    retry_after_secs,
                    "public_attestation: per-IP request rate exceeded (429)"
                );
                too_many_requests(retry_after_secs)
            }
            // `RateLimitDecision` is `#[non_exhaustive]`; any future non-Allow
            // arm fail-CLOSES to 429 (a new deny-shaped arm should not
            // silently become an allow).
            _ => too_many_requests(corelink_ratelimit::DEFAULT_RETRY_AFTER_FLOOR_SECS),
        },
        // Limiter's OWN internal fault ⇒ fail-OPEN for availability, logged.
        Err(err) => {
            tracing::warn!(
                error = %err,
                "public_attestation: rate limiter internal error; failing OPEN"
            );
            next.run(req).await
        }
    }
}

/// Extract the trusted client IP from [`CLIENT_IP_HEADER`]. Returns `None`
/// (caller fail-OPENs) when the header is absent, non-UTF8, or blank.
fn client_ip_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(CLIENT_IP_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Build the uniform 429 response with a clamped `Retry-After` header
/// (mirrors `ratelimit_layer::too_many_requests`, kept LOCAL/duplicated per
/// the frozen M22(a) design rather than importing from that module).
fn too_many_requests(retry_after_secs: u64) -> Response {
    let body = format!(
        "{{\"error\":\"rate_limited\",\"message\":\"public-verifier per-IP request rate \
         exceeded; retry after {retry_after_secs}s\"}}"
    );
    let mut resp = (
        StatusCode::TOO_MANY_REQUESTS,
        [("content-type", "application/json")],
        body,
    )
        .into_response();
    if let Ok(val) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        resp.headers_mut().insert(RETRY_AFTER, val);
    }
    resp
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
        assert_eq!(
            bundle["canonical_payload_jcs"],
            json!("{\"request_id\":\"req-1\"}")
        );
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
        // APAC is now a known attestation region → Some.
        assert_eq!(region_from_pub_param("apac.pub"), Some(Region::Apac));
        // Unknown region → None → 404.
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

    // ---- M22(a): scoped per-IP rate limit ---------------------------------

    mod rate_limit {
        use super::*;
        use crate::wall_clock::InMemoryFakeWallClock;
        use axum::{body::Body, http::Request as HttpRequest, routing::get as axum_get};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tower::ServiceExt; // for `.oneshot()`

        const IP_A: &str = "203.0.113.7";
        const IP_B: &str = "198.51.100.9";

        fn fixed_clock(unix_ms: u64) -> Arc<dyn WallClock> {
            Arc::new(InMemoryFakeWallClock::at_unix_ms(unix_ms))
        }

        /// A minimal stub app wrapped in ONLY the M22(a) middleware — mirrors
        /// `ratelimit_layer.rs`'s own test harness shape (a fake route +
        /// `.layer(from_fn_with_state(...))`), so the test exercises the exact
        /// middleware [`router`] wires, without needing a live D1 backend.
        fn app(state: PublicVerifierRateLimitState, hits: Arc<AtomicUsize>) -> Router {
            Router::new()
                .route(
                    "/v1/public/attestation/{request_id}",
                    axum_get(move || {
                        let h = hits.clone();
                        async move {
                            h.fetch_add(1, Ordering::SeqCst);
                            "ok"
                        }
                    }),
                )
                .layer(middleware::from_fn_with_state(
                    state,
                    rate_limit_public_verifier,
                ))
        }

        fn req(client_ip: Option<&str>) -> HttpRequest<Body> {
            let mut b = HttpRequest::builder().uri("/v1/public/attestation/req-1");
            if let Some(ip) = client_ip {
                b = b.header(CLIENT_IP_HEADER, ip);
            }
            b.body(Body::empty()).unwrap()
        }

        #[test]
        fn ip_key_uuid_is_stable_and_distinct() {
            assert_eq!(ip_key_uuid(IP_A), ip_key_uuid(IP_A));
            assert_ne!(ip_key_uuid(IP_A), ip_key_uuid(IP_B));
        }

        #[tokio::test]
        async fn within_budget_allows() {
            let hits = Arc::new(AtomicUsize::new(0));
            let app = app(PublicVerifierRateLimitState::new(), hits.clone());
            let resp = app.oneshot(req(Some(IP_A))).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            assert_eq!(hits.load(Ordering::SeqCst), 1);
        }

        /// Fail-OPEN: no `x-corelink-client-ip` header (dev/CI/no-header) →
        /// pass through untouched, never a 429, no bucket charged.
        #[tokio::test]
        async fn absent_client_ip_header_fails_open() {
            let hits = Arc::new(AtomicUsize::new(0));
            let app = app(PublicVerifierRateLimitState::new(), hits.clone());
            let resp = app.oneshot(req(None)).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
            assert_eq!(hits.load(Ordering::SeqCst), 1);
        }

        /// Burst+1 same client-IP, same fixed instant → the last request is
        /// 429 with a `Retry-After` header.
        #[tokio::test]
        async fn over_burst_is_429_with_retry_after() {
            let clock = fixed_clock(1_000_000);
            let state = PublicVerifierRateLimitState::with_clock(clock);
            let hits = Arc::new(AtomicUsize::new(0));

            let mut last = StatusCode::OK;
            for _ in 0..(PUBLIC_VERIFIER_BURST + 1) {
                let app = app(state.clone(), hits.clone());
                let resp = app.oneshot(req(Some(IP_A))).await.unwrap();
                last = resp.status();
                if last == StatusCode::TOO_MANY_REQUESTS {
                    assert!(
                        resp.headers().get(RETRY_AFTER).is_some(),
                        "429 must carry Retry-After"
                    );
                    break;
                }
            }
            assert_eq!(
                last,
                StatusCode::TOO_MANY_REQUESTS,
                "exceeding the per-IP burst must 429"
            );
        }

        /// A distinct client IP at the SAME instant is isolated from a drained
        /// bucket (INV-AVAIL-ISOLATION, per-IP scope).
        #[tokio::test]
        async fn distinct_ip_same_instant_is_isolated() {
            let clock = fixed_clock(2_000_000);
            let state = PublicVerifierRateLimitState::with_clock(clock);
            let hits = Arc::new(AtomicUsize::new(0));

            for _ in 0..(PUBLIC_VERIFIER_BURST + 1) {
                let app = app(state.clone(), hits.clone());
                let _ = app.oneshot(req(Some(IP_A))).await.unwrap();
            }
            // IP A is now drained to 429; IP B (different key) — fresh bucket.
            let app = app(state.clone(), hits.clone());
            let resp = app.oneshot(req(Some(IP_B))).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "distinct IP must not be throttled"
            );
        }
    }
}
