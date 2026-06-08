//! Top-level axum endpoint handlers (the `/v2/`, `/v2/_catalog`,
//! `/token`, and `/v2/*rest` dispatchers).
//!
//! Extracted from `server.rs` to keep the route-registration file
//! under L2.10's 500-LOC HARD CAP. The dispatch logic itself —
//! including the path-tail parser + URL decoder + repo-name validator
//! — lives in [`super::dispatch`].

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use axum::response::IntoResponse;

use super::dispatch::{parse_v2_tail, urldecode, V2Path};
use crate::oci::audit::OciAuditEvent;
use crate::oci::error::OciAdapterError;
use crate::oci::server::core::{err_response, status_for, AppState};

/// `GET /v2/` — liveness + auth probe per OCI Distribution Spec v1.1
/// §2.1. Returns 200 if a valid bearer token is supplied, 401 +
/// `Www-Authenticate: Bearer …` otherwise.
pub async fn api_version(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> axum::response::Response {
    let _ = status_for;
    let auth = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if let Some(token_header) = auth.and_then(|h| h.strip_prefix("Bearer ")) {
        let now = (state.clock_unix_ms)() / 1000;
        match crate::oci::auth::verify(&state.config.token_signing_key, token_header, now) {
            Ok(_) => {
                let mut h = HeaderMap::new();
                if let Ok(v) = "registry/2.0".parse() {
                    h.insert("Docker-Distribution-API-Version", v);
                }
                return (StatusCode::OK, h, Body::empty()).into_response();
            }
            Err(_) => {
                let now_ms = (state.clock_unix_ms)();
                let _ = crate::oci::audit::emit(
                    state.config.auditor.as_ref(),
                    &corelink_core::TenantId::from_uuid(uuid::Uuid::nil()),
                    &OciAuditEvent::AuthDenied {
                        path: "/v2/",
                        reason: "bad-bearer",
                    },
                    now_ms,
                );
            }
        }
    }
    err_response(
        &OciAdapterError::Auth(String::from("missing or invalid bearer token")),
        Some(&state.config.bearer_realm),
        Some("repository:*:pull"),
    )
}

/// `GET /v2/_catalog` — disabled-by-default. Returns 401 + audit.
pub async fn catalog(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> axum::response::Response {
    let anonymous = !headers.contains_key(axum::http::header::AUTHORIZATION);
    let now_ms = (state.clock_unix_ms)();
    let _ = crate::oci::audit::emit(
        state.config.auditor.as_ref(),
        &corelink_core::TenantId::from_uuid(uuid::Uuid::nil()),
        &OciAuditEvent::CatalogDenied { anonymous },
        now_ms,
    );
    err_response(
        &OciAdapterError::CatalogDisabled,
        Some(&state.config.bearer_realm),
        Some("registry:catalog:*"),
    )
}

/// `GET /token?service=...&scope=...` — exchange `Basic` for `Bearer`.
pub async fn token(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> axum::response::Response {
    let Some(basic) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
        return err_response(
            &OciAdapterError::Auth(String::from("missing basic auth on /token")),
            Some(&state.config.bearer_realm),
            Some("repository:*:pull"),
        );
    };
    let pat = match crate::oci::auth::parse_basic_authorization(basic) {
        Ok(p) => p,
        Err(e) => return err_response(&e, Some(&state.config.bearer_realm), None),
    };
    let scope_str = uri
        .query()
        .and_then(|q| {
            q.split('&')
                .find_map(|p| p.strip_prefix("scope=").map(urldecode))
        })
        .unwrap_or_default();
    let scope = match crate::oci::auth::OciScope::parse(&scope_str) {
        Ok(s) => s,
        Err(e) => return err_response(&e, None, None),
    };
    let resolved = match state
        .config
        .tenant_resolver
        .resolve_pat_capability(&pat)
        .await
    {
        Ok(r) => r,
        Err(e) => return err_response(&OciAdapterError::Auth(e), None, None),
    };
    // SECURITY: downscope the grant to the PAT's real capability. A
    // read-only (`cas:r`) PAT requesting `push` is granted pull-only, so
    // the data-plane `scope.allows(repo, "push")` check denies the write
    // (no scope escalation via the token exchange).
    let granted = if resolved.can_write {
        scope
    } else {
        scope.restricted_to_read()
    };
    let now_secs = (state.clock_unix_ms)() / 1000;
    let token = match crate::oci::auth::mint(
        &state.config.token_signing_key,
        &resolved.tenant,
        &granted,
        now_secs,
        state.config.token_ttl_secs,
    ) {
        Ok(t) => t,
        Err(e) => return err_response(&e, None, None),
    };
    let body = serde_json::json!({
        "token": token,
        "access_token": token,
        "expires_in": state.config.token_ttl_secs,
        "issued_at": now_secs,
    });
    let mut h = HeaderMap::new();
    if let Ok(v) = "application/json".parse() {
        h.insert(axum::http::header::CONTENT_TYPE, v);
    }
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    (StatusCode::OK, h, Body::from(bytes)).into_response()
}

/// Wildcard `/v2/*rest` dispatch. Parses the tail and routes to the
/// per-op handler.
pub async fn dispatch_v2(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> axum::response::Response {
    let path = uri.path();
    let Some(tail) = path.strip_prefix("/v2/") else {
        return err_response(
            &OciAdapterError::NotFound,
            Some(&state.config.bearer_realm),
            None,
        );
    };
    let Some(bearer) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    else {
        return err_response(
            &OciAdapterError::Auth(String::from("missing bearer")),
            Some(&state.config.bearer_realm),
            Some("repository:*:pull"),
        );
    };
    let now_secs = (state.clock_unix_ms)() / 1000;
    let verified = match crate::oci::auth::verify(&state.config.token_signing_key, bearer, now_secs)
    {
        Ok(v) => v,
        Err(e) => {
            let now_ms = (state.clock_unix_ms)();
            let _ = crate::oci::audit::emit(
                state.config.auditor.as_ref(),
                &corelink_core::TenantId::from_uuid(uuid::Uuid::nil()),
                &OciAuditEvent::AuthDenied {
                    path,
                    reason: "bad-bearer",
                },
                now_ms,
            );
            return err_response(&e, Some(&state.config.bearer_realm), None);
        }
    };
    let tenant = verified.tenant;
    let scope = &verified.scope;
    let Some(parsed) = parse_v2_tail(tail) else {
        return err_response(
            &OciAdapterError::NotFound,
            Some(&state.config.bearer_realm),
            None,
        );
    };
    let now_ms = (state.clock_unix_ms)();
    let result = route_dispatch(
        parsed, method, uri, headers, body, &state, &tenant, scope, now_ms,
    )
    .await;
    match result {
        Ok(resp) => resp,
        Err(e) => err_response(&e, Some(&state.config.bearer_realm), None),
    }
}

/// Per-V2Path dispatch table.
#[allow(
    clippy::too_many_arguments,
    reason = "router-shape dispatch, intentionally flat for readability"
)]
async fn route_dispatch(
    parsed: V2Path,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
    state: &AppState,
    tenant: &corelink_core::TenantId,
    scope: &crate::oci::auth::OciScope,
    now_ms: u64,
) -> Result<axum::response::Response, OciAdapterError> {
    match parsed {
        V2Path::Blob { repo, digest } => match method {
            Method::GET => {
                crate::oci::pull::blob::get(
                    state.config.cas.as_ref(),
                    tenant,
                    scope,
                    &repo,
                    &digest,
                )
                .await
            }
            Method::HEAD => {
                crate::oci::pull::blob::head(
                    state.config.cas.as_ref(),
                    tenant,
                    scope,
                    &repo,
                    &digest,
                )
                .await
            }
            _ => Err(OciAdapterError::NotFound),
        },
        V2Path::BlobUploadsOpen { repo } => match method {
            Method::POST => {
                crate::oci::push::upload::open(state.config.cas.as_ref(), tenant, scope, &repo)
                    .await
            }
            _ => Err(OciAdapterError::NotFound),
        },
        V2Path::BlobUploadsSession { repo, uuid } => {
            dispatch_blob_upload_session(
                state, tenant, scope, &repo, &uuid, method, uri, body, now_ms,
            )
            .await
        }
        V2Path::Manifest { repo, reference } => {
            dispatch_manifest(
                state, tenant, scope, &repo, &reference, method, &headers, body, now_ms,
            )
            .await
        }
        V2Path::TagsList { repo } => match method {
            Method::GET => {
                crate::oci::tags::list(state.config.metadata_kv.as_ref(), tenant, scope, &repo)
                    .await
            }
            _ => Err(OciAdapterError::NotFound),
        },
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "router-shape dispatch, intentionally flat for readability"
)]
async fn dispatch_blob_upload_session(
    state: &AppState,
    tenant: &corelink_core::TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    uuid: &str,
    method: Method,
    uri: Uri,
    body: Body,
    now_ms: u64,
) -> Result<axum::response::Response, OciAdapterError> {
    let chunk = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|e| OciAdapterError::Cas(format!("body drain: {e}")))?;
    match method {
        Method::PATCH => {
            crate::oci::push::upload::patch(
                state.config.cas.as_ref(),
                state.config.auditor.as_ref(),
                tenant,
                scope,
                repo,
                uuid,
                chunk,
                state.config.blob_size_limit_bytes,
                now_ms,
            )
            .await
        }
        Method::PUT => {
            let declared = uri
                .query()
                .and_then(|q| {
                    q.split('&')
                        .find_map(|p| p.strip_prefix("digest=").map(urldecode))
                })
                .unwrap_or_default();
            let trailing = if chunk.is_empty() { None } else { Some(chunk) };
            crate::oci::push::upload::put(
                state.config.cas.as_ref(),
                state.config.auditor.as_ref(),
                tenant,
                scope,
                repo,
                uuid,
                &declared,
                trailing,
                state.config.blob_size_limit_bytes,
                now_ms,
            )
            .await
        }
        _ => Err(OciAdapterError::NotFound),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "router-shape dispatch, intentionally flat for readability"
)]
async fn dispatch_manifest(
    state: &AppState,
    tenant: &corelink_core::TenantId,
    scope: &crate::oci::auth::OciScope,
    repo: &str,
    reference: &str,
    method: Method,
    headers: &HeaderMap,
    body: Body,
    now_ms: u64,
) -> Result<axum::response::Response, OciAdapterError> {
    match method {
        Method::GET => {
            crate::oci::pull::manifest::get(
                state.config.metadata_kv.as_ref(),
                tenant,
                scope,
                repo,
                reference,
            )
            .await
        }
        Method::HEAD => {
            crate::oci::pull::manifest::head(
                state.config.metadata_kv.as_ref(),
                tenant,
                scope,
                repo,
                reference,
            )
            .await
        }
        Method::PUT => {
            let ct = headers
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            let bytes = axum::body::to_bytes(body, usize::MAX)
                .await
                .map_err(|e| OciAdapterError::Kv(format!("body drain: {e}")))?;
            crate::oci::push::manifest::put(
                state.config.metadata_kv.as_ref(),
                state.config.auditor.as_ref(),
                tenant,
                scope,
                repo,
                reference,
                ct,
                bytes,
                now_ms,
            )
            .await
        }
        // OCI Distribution Spec v1.1 §delete-manifest says servers MAY
        // return 405. We map to 404 NAME_UNKNOWN to keep the wire
        // shape uniform with non-existent references.
        Method::DELETE => Err(OciAdapterError::NotFound),
        _ => Err(OciAdapterError::NotFound),
    }
}
