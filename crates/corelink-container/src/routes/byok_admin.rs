//! `POST /v1/admin/byok/activate` + `POST /v1/admin/byok/deactivate` —
//! the operator-gated BYOK **activation** control plane.
//!
//! These two routes are the WRITE authority that closes the H5 gap: migration
//! `0081` created `tenant_byok_config` + `tenant_byok_secret`, and the r2_s3 CAS
//! store already encrypts when `tenant_byok_config.state == 'active'` — but
//! until this endpoint flips a tenant to `active` (with its CMK identity +
//! CMK-wrapped Tcs) that gate can never engage. `activate` performs the flip;
//! `deactivate` is the crypto-shred kill switch.
//!
//! # Auth boundary (operator-only, fail-CLOSED)
//!
//! BYOK custody is control-plane state, so both routes are gated EXACTLY like
//! `/v1/admin/*` and `/_internal/pat/mint`: the caller MUST present the
//! operator shared secret in `x-corelink-internal-auth`
//! ([`crate::routes::admin::internal_auth_ok`]). No tenant PAT can reach them.
//! The gate is evaluated BEFORE the body is parsed (M3 pattern): an
//! unauthenticated caller is 403'd without the JSON ever being deserialised.
//! When the key is unset at boot (dev/CI) the routes fail CLOSED (403). When
//! the D1 writer is absent (no storage creds) the routes fail CLOSED (503) —
//! never a silent no-op.
//!
//! # Tenant scope
//!
//! The target tenant is an explicit body field; the writer scopes every SQL
//! statement to it by primary key (INV-TENANT-ISOLATION). The operator asserts
//! the tenant — this is a privileged control-plane surface, not a self-serve
//! tenant path.
//!
//! # SECURITY — key material
//!
//! The wrapped Tcs travels as a base64 `tcs_wrapped_b64` field and is the
//! CMK-WRAPPED ciphertext, never the plaintext Tcs. It is NEVER logged; the
//! audit trail records tenant + provider + CMK identity + state only.
//!
//! `deactivate` requires an explicit non-destructive `cancel` or destructive
//! `shred` action so retrying one can never be mistaken for the other.

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use corelink_byok::KmsProvider;
use serde::Deserialize;

use crate::byok_control_transition::D1ByokControl;
use crate::customer_d1::{ByokActivation, ByokCryptoMode, ByokMode, ByokWriteError};
use crate::routes::admin::internal_auth_ok;
use crate::wall_clock::{SystemWallClock, WallClock};

/// Canonical activation route path (matchit `:name` grammar — braces are the
/// matchit-0.8 form and would panic under the workspace-pinned axum/matchit).
pub const BYOK_ACTIVATE_ROUTE: &str = "/v1/admin/byok/activate";

/// Canonical activation-cancel / crypto-shred route path.
pub const BYOK_DEACTIVATE_ROUTE: &str = "/v1/admin/byok/deactivate";

/// Shared route state for the BYOK activation control plane.
#[derive(Clone)]
pub struct ByokAdminRouteState {
    /// D1-backed writer; `None` in dev/CI (no storage creds) → both routes
    /// fail CLOSED (503) rather than silently no-op.
    pub writer: Option<Arc<D1ByokControl>>,
    /// Operator-only shared secret for the `x-corelink-internal-auth` gate.
    /// `None` when unset at boot → every route fails CLOSED (403).
    pub internal_auth_key: Option<Arc<str>>,
}

impl core::fmt::Debug for ByokAdminRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ByokAdminRouteState")
            .finish_non_exhaustive()
    }
}

impl ByokAdminRouteState {
    /// Build the production state from process env.
    ///
    /// The D1 writer is wired when `StorageEnv::from_env()` yields real CF
    /// creds (production); otherwise `None` (dev/CI) and the routes fail
    /// CLOSED (503). The operator gate reuses the shared admin key resolver
    /// so BYOK activation rotates with the rest of the admin control plane.
    #[must_use]
    pub fn from_env() -> Self {
        let writer = build_writer_from_env();
        Self {
            writer,
            internal_auth_key: crate::routes::admin::internal_auth_key_from_env(),
        }
    }
}

/// Construct the D1-backed [`D1ByokControl`] when storage creds are
/// present, else `None` (dev/CI). Mirrors `routes::admin::build_handlers`.
#[must_use]
fn build_writer_from_env() -> Option<Arc<D1ByokControl>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use crate::storage::{d1_http::D1HttpClient, StorageEnv};

        let env = StorageEnv::from_env()?;
        // D1HttpClient::new is sync; keep the block_in_place bridge in place to
        // match admin.rs / cas.rs (routes are built inside #[tokio::main]).
        let built: Result<D1HttpClient, String> = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async { D1HttpClient::new(&env) })
        });
        match built {
            Ok(client) => {
                tracing::info!("BYOK activation writer: D1 (real storage)");
                Some(Arc::new(D1ByokControl::new(Arc::new(client))))
            }
            Err(e) => {
                tracing::error!(error = %e, "BYOK activation writer: D1 build failed → fail CLOSED (503)");
                None
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// Build the axum `Router` exposing the two BYOK activation routes.
pub fn router(state: ByokAdminRouteState) -> Router {
    Router::new()
        .route(BYOK_ACTIVATE_ROUTE, post(handle_activate))
        .route(BYOK_DEACTIVATE_ROUTE, post(handle_deactivate))
        .with_state(state)
}

/// JSON body for `POST /v1/admin/byok/activate`.
#[derive(Clone, Debug, Deserialize)]
pub struct ByokActivateBody {
    /// Target tenant id (operator-asserted; scopes the D1 write).
    pub tenant: String,
    /// Key-custody rung — `byok` or `hyok`.
    pub mode: String,
    /// Crypto mode — `convergent` (default) or `random`.
    #[serde(default)]
    pub crypto_mode: Option<String>,
    /// CMK provider — `aws` / `gcp` / `azure` / `vault`.
    pub cmk_provider: String,
    /// CMK identity (ARN / resource name / URI / path).
    pub cmk_key_id: String,
    /// CMK region (optional).
    #[serde(default)]
    pub cmk_region: Option<String>,
    /// Base64 of the CMK-WRAPPED Tenant Convergence Secret ciphertext. NEVER
    /// the plaintext Tcs; never logged.
    pub tcs_wrapped_b64: String,
}

/// Explicit action for `POST /v1/admin/byok/deactivate`.
///
/// This discriminator is intentionally required. A tenant-only request is
/// ambiguous after a rotation cancellation restores the preceding active
/// generation: interpreting its retry as a shred would destroy live data.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ByokDeactivateAction {
    /// Cancel an unpublished (`pending` / copy-phase) activation.
    Cancel,
    /// Irreversibly shred a published/active BYOK generation.
    Shred,
}

/// JSON body for `POST /v1/admin/byok/deactivate`.
#[derive(Clone, Debug, Deserialize)]
pub struct ByokDeactivateBody {
    /// Target tenant id.
    pub tenant: String,
    /// Required operation intent; there is no destructive legacy default.
    pub action: ByokDeactivateAction,
}

impl ByokActivateBody {
    /// Parse the wire body into a [`ByokActivation`], decoding the base64 Tcs
    /// and validating the enum fields. Returns a static 400 marker on any
    /// malformed field.
    fn into_activation(self) -> Result<ByokActivation, &'static str> {
        let mode = self.mode.parse::<ByokMode>().map_err(|_| "invalid_mode")?;
        let crypto_mode = match self.crypto_mode.as_deref() {
            None | Some("convergent") => ByokCryptoMode::Convergent,
            Some("random") => ByokCryptoMode::Random,
            Some(_) => return Err("invalid_crypto_mode"),
        };
        let tcs_wrapped = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(self.tcs_wrapped_b64.as_bytes())
                .map_err(|_| "invalid_tcs_wrapped_b64")?
        };
        Ok(ByokActivation {
            tenant_id: self.tenant,
            mode,
            crypto_mode,
            cmk_provider: self.cmk_provider,
            cmk_key_id: self.cmk_key_id,
            cmk_region: self.cmk_region,
            tcs_wrapped,
        })
    }
}

/// Map a [`ByokWriteError`] to the canonical HTTP response.
fn map_write_err(e: &ByokWriteError) -> axum::response::Response {
    match e {
        ByokWriteError::Invalid(_) => {
            (StatusCode::BAD_REQUEST, "invalid_activation").into_response()
        }
        ByokWriteError::IllegalTransition { .. } => {
            (StatusCode::CONFLICT, "illegal_transition").into_response()
        }
        // Fail-CLOSED: a durable-write failure is a 503, never a false 200.
        ByokWriteError::Transport(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, "byok_write_unavailable").into_response()
        }
    }
}

/// `POST /v1/admin/byok/activate` — flip a tenant to BYOK `active`.
async fn handle_activate(
    axum::extract::State(state): axum::extract::State<ByokAdminRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Operator gate FIRST — before the body is parsed (M3).
    if !internal_auth_ok(state.internal_auth_key.as_ref(), &headers) {
        tracing::warn!(
            event = "ByokActivateUnauthorized",
            "byok activate rejected: missing/invalid x-corelink-internal-auth (fail-CLOSED)"
        );
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    // Parse and validate the wire shape before touching KMS or D1. This keeps
    // malformed/unknown provider input a deterministic 400 with no audit or
    // mutation side effect.
    let parsed: ByokActivateBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "byok activate: invalid request body");
            return (StatusCode::BAD_REQUEST, "invalid_body").into_response();
        }
    };
    let activation = match parsed.into_activation() {
        Ok(a) => a,
        Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
    };
    if !matches!(
        activation.cmk_provider.as_str(),
        "aws" | "gcp" | "azure" | "vault"
    ) {
        return (StatusCode::BAD_REQUEST, "invalid_cmk_provider").into_response();
    }
    // Storage authority is required for both the idempotency preflight and
    // the eventual preparation batch. Stop before constructing or contacting
    // KMS when it is absent; there is no durable operation to authorize.
    let Some(writer) = state.writer.as_ref() else {
        tracing::error!("byok activate: D1 writer unwired → 503 (fail-CLOSED)");
        return (StatusCode::SERVICE_UNAVAILABLE, "byok_write_unavailable").into_response();
    };

    // A byte-for-byte retry of durable live work (or of an activation that is
    // still actively committed) does not need fresh KMS evidence: no custody
    // material or state will be changed. Resolve that idempotency authority
    // before constructing/contacting KMS. `prepare_activation` repeats this
    // check later to close the race between this public-safe read and write.
    match writer.activation_retry_preflight(&activation).await {
        Ok(true) => {
            return (StatusCode::ACCEPTED, "activation_pending_backfill").into_response();
        }
        Ok(false) => {}
        Err(error) => return map_write_err(&error),
    }

    // Provider construction is deliberately runtime-gated as well as
    // compile-time-selected. Missing credentials, an unavailable endpoint, or
    // a binary built without a real provider must all stop before the D1
    // writer. A feature flag alone is not evidence that KMS is usable.
    let provider = match crate::byok_orchestrator::make_provider().await {
        Ok(provider) => provider,
        Err(error) => {
            tracing::error!(
                event = "ByokActivateNotAvailable",
                provider = crate::byok_orchestrator::active_provider().as_str(),
                reason = "kms_provider_unavailable",
                error = %error,
                "byok activate rejected: real KMS provider unavailable; state NOT mutated"
            );
            return (
                StatusCode::NOT_IMPLEMENTED,
                "{\"error\":\"byok_not_available\"}",
            )
                .into_response();
        }
    };
    if activation.cmk_provider != provider.provider_kind().as_str() {
        tracing::warn!(
            requested = %activation.cmk_provider,
            configured = provider.provider_kind().as_str(),
            "byok activate rejected: CMK provider does not match binary KMS provider"
        );
        return (StatusCode::BAD_REQUEST, "cmk_provider_mismatch").into_response();
    }
    let key_id = corelink_byok::KmsKeyId {
        provider: provider.provider_kind(),
        key_arn_or_id: activation.cmk_key_id.clone(),
        region: activation.cmk_region.clone().unwrap_or_default(),
    };
    if !matches!(
        provider.check_access(&key_id).await,
        Ok(corelink_byok::KmsAccessStatus::Ok)
    ) {
        tracing::error!(
            event = "ByokActivateNotAvailable",
            provider = crate::byok_orchestrator::active_provider().as_str(),
            reason = "cmk_access_unavailable",
            "byok activate rejected: CMK access could not be confirmed; state NOT mutated"
        );
        return (
            StatusCode::NOT_IMPLEMENTED,
            "{\"error\":\"byok_not_available\"}",
        )
            .into_response();
    }
    let now_ms = i64::try_from(SystemWallClock.now_ms()).unwrap_or(i64::MAX);
    match writer.prepare_activation(&activation, now_ms).await {
        Ok(()) => (StatusCode::ACCEPTED, "activation_pending_backfill").into_response(),
        Err(e) => map_write_err(&e),
    }
}

/// `POST /v1/admin/byok/deactivate` — explicit activation cancel or shred.
async fn handle_deactivate(
    axum::extract::State(state): axum::extract::State<ByokAdminRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if !internal_auth_ok(state.internal_auth_key.as_ref(), &headers) {
        tracing::warn!(
            event = "ByokDeactivateUnauthorized",
            "byok deactivate rejected: missing/invalid x-corelink-internal-auth (fail-CLOSED)"
        );
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    let parsed: ByokDeactivateBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "byok deactivate: invalid request body");
            return (StatusCode::BAD_REQUEST, "invalid_body").into_response();
        }
    };
    let Some(writer) = state.writer.as_ref() else {
        tracing::error!("byok deactivate: D1 writer unwired → 503 (fail-CLOSED)");
        return (StatusCode::SERVICE_UNAVAILABLE, "byok_write_unavailable").into_response();
    };
    let now_ms = i64::try_from(SystemWallClock.now_ms()).unwrap_or(i64::MAX);
    let result = match parsed.action {
        ByokDeactivateAction::Cancel => writer.cancel_activation(&parsed.tenant, now_ms).await,
        ByokDeactivateAction::Shred => writer.shred(&parsed.tenant, now_ms).await,
    };
    match result {
        Ok(()) => match parsed.action {
            ByokDeactivateAction::Cancel => (StatusCode::OK, "cancelled").into_response(),
            ByokDeactivateAction::Shred => (StatusCode::OK, "shredded").into_response(),
        },
        Err(e) => map_write_err(&e),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use axum::body::Body;
    use axum::http::{self, Request};
    use tower::ServiceExt;

    use super::*;

    const TENANT: &str = "0192f0c1-2345-7890-abcd-ef0123456789";
    const KEY: &str = "test-internal-auth-key-32-bytes-x";

    fn state_no_writer(key: Option<&str>) -> ByokAdminRouteState {
        ByokAdminRouteState {
            writer: None,
            internal_auth_key: key.map(Arc::from),
        }
    }

    #[test]
    fn route_constants_use_matchit_grammar() {
        assert_eq!(BYOK_ACTIVATE_ROUTE, "/v1/admin/byok/activate");
        assert_eq!(BYOK_DEACTIVATE_ROUTE, "/v1/admin/byok/deactivate");
        // Router must construct against the workspace-pinned axum/matchit.
        let _r = router(state_no_writer(Some(KEY)));
    }

    /// Unauthenticated caller sending a large non-JSON body → 403 BEFORE parse
    /// (M3): the auth gate fires first, and the writer-absent 503 never masks a
    /// missing gate.
    #[tokio::test]
    async fn activate_unauthenticated_is_403_before_parse() {
        let app = router(state_no_writer(Some(KEY)));
        let big = "Z".repeat(64 * 1024);
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(BYOK_ACTIVATE_ROUTE)
            .body(Body::from(big))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// Gate unconfigured (key None) → 403 regardless of any header.
    #[tokio::test]
    async fn activate_fails_closed_when_key_unset() {
        let app = router(state_no_writer(None));
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(BYOK_ACTIVATE_ROUTE)
            .header("x-corelink-internal-auth", "anything")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    /// A valid authenticated request with no D1 authority must stop at 503.
    /// This assertion is provider-feature-independent: reaching provider
    /// construction in the default build would instead produce the old 501.
    #[tokio::test]
    async fn activate_without_writer_is_503_before_kms() {
        let app = router(state_no_writer(Some(KEY)));
        let body = format!(
            "{{\"tenant\":\"{TENANT}\",\"mode\":\"byok\",\"cmk_provider\":\"aws\",\
             \"cmk_key_id\":\"arn:aws:kms:us-east-1:1:key/abc\",\"tcs_wrapped_b64\":\"AQID\"}}"
        );
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(BYOK_ACTIVATE_ROUTE)
            .header("x-corelink-internal-auth", KEY)
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "missing D1 authority must stop before KMS construction"
        );
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(text, "byok_write_unavailable");
    }

    /// Adversarial regression: an unknown provider is rejected before provider
    /// construction and before the D1 writer can be reached. In particular,
    /// `developer` must never be treated as an alias for a KMS backend.
    #[cfg(not(any(
        feature = "byok-aws-real",
        feature = "byok-gcp-real",
        feature = "byok-azure-real",
        feature = "byok-vault-real",
    )))]
    #[tokio::test]
    async fn activate_unknown_provider_is_400_without_mutation() {
        let app = router(state_no_writer(Some(KEY)));
        let body = format!(
            "{{\"tenant\":\"{TENANT}\",\"mode\":\"byok\",\"cmk_provider\":\"developer\",\
             \"cmk_key_id\":\"not-a-kms-key\",\"tcs_wrapped_b64\":\"AQID\"}}"
        );
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(BYOK_ACTIVATE_ROUTE)
            .header("x-corelink-internal-auth", KEY)
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn deactivate_unauthenticated_is_403() {
        let app = router(state_no_writer(Some(KEY)));
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(BYOK_DEACTIVATE_ROUTE)
            .body(Body::from(format!("{{\"tenant\":\"{TENANT}\"}}")))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn deactivate_rejects_ambiguous_tenant_only_body() {
        let app = router(state_no_writer(Some(KEY)));
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(BYOK_DEACTIVATE_ROUTE)
            .header("x-corelink-internal-auth", KEY)
            .body(Body::from(format!("{{\"tenant\":\"{TENANT}\"}}")))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn deactivate_rejects_unknown_action() {
        let app = router(state_no_writer(Some(KEY)));
        let req = Request::builder()
            .method(http::Method::POST)
            .uri(BYOK_DEACTIVATE_ROUTE)
            .header("x-corelink-internal-auth", KEY)
            .body(Body::from(format!(
                "{{\"tenant\":\"{TENANT}\",\"action\":\"deactivate\"}}"
            )))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn deactivate_accepts_only_explicit_cancel_or_shred() {
        for action in ["cancel", "shred"] {
            let app = router(state_no_writer(Some(KEY)));
            let req = Request::builder()
                .method(http::Method::POST)
                .uri(BYOK_DEACTIVATE_ROUTE)
                .header("x-corelink-internal-auth", KEY)
                .body(Body::from(format!(
                    "{{\"tenant\":\"{TENANT}\",\"action\":\"{action}\"}}"
                )))
                .unwrap();
            let resp = app.oneshot(req).await.unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::SERVICE_UNAVAILABLE,
                "valid explicit action must pass parsing before fail-closed writer wiring"
            );
        }
    }

    /// Body parsing: `into_activation` decodes base64 + validates enums.
    #[test]
    fn into_activation_decodes_and_validates() {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3, 4]);
        let body = ByokActivateBody {
            tenant: TENANT.to_owned(),
            mode: "byok".to_owned(),
            crypto_mode: Some("random".to_owned()),
            cmk_provider: "aws".to_owned(),
            cmk_key_id: "arn:aws:kms:us-east-1:1:key/abc".to_owned(),
            cmk_region: Some("us-east-1".to_owned()),
            tcs_wrapped_b64: b64,
        };
        let act = body.into_activation().expect("valid body");
        assert_eq!(act.tenant_id, TENANT);
        assert_eq!(act.mode, ByokMode::Byok);
        assert_eq!(act.crypto_mode, ByokCryptoMode::Random);
        assert_eq!(act.tcs_wrapped, vec![1, 2, 3, 4]);
    }

    #[test]
    fn into_activation_rejects_bad_base64_and_enums() {
        let base = |mode: &str, cm: Option<&str>, b64: &str| ByokActivateBody {
            tenant: TENANT.to_owned(),
            mode: mode.to_owned(),
            crypto_mode: cm.map(str::to_owned),
            cmk_provider: "aws".to_owned(),
            cmk_key_id: "arn".to_owned(),
            cmk_region: None,
            tcs_wrapped_b64: b64.to_owned(),
        };
        // Good base64 for the enum-error cases so the enum is what trips.
        assert_eq!(
            base("nonsense", None, "AQID")
                .into_activation()
                .unwrap_err(),
            "invalid_mode"
        );
        assert_eq!(
            base("byok", Some("homomorphic"), "AQID")
                .into_activation()
                .unwrap_err(),
            "invalid_crypto_mode"
        );
        assert_eq!(
            base("byok", None, "!!not-base64!!")
                .into_activation()
                .unwrap_err(),
            "invalid_tcs_wrapped_b64"
        );
    }
}
