//! `axum` router that dispatches the OCI `/v2/...` endpoint surface.
//!
//! Route table (per oci.md §2):
//!
//! | Method | Path | Handler |
//! |---|---|---|
//! | `GET` | `/v2/` | [`handlers::api_version`] |
//! | `GET` | `/v2/_catalog` | [`handlers::catalog`] (always 401 — disabled) |
//! | `GET` | `/token` | [`handlers::token`] (PAT → bearer exchange) |
//! | `POST` | `/token` | [`handlers::token_post`] (OAuth2 `grant_type=password`, real `docker push`) |
//! | `GET` | `/v2/:name/blobs/:digest` | [`crate::oci::pull::blob::get`] |
//! | `HEAD` | `/v2/:name/blobs/:digest` | [`crate::oci::pull::blob::head`] |
//! | `POST` | `/v2/:name/blobs/uploads/` | [`crate::oci::push::upload::open`] |
//! | `PATCH` | `/v2/:name/blobs/uploads/:uuid` | [`crate::oci::push::upload::patch`] |
//! | `PUT` | `/v2/:name/blobs/uploads/:uuid` | [`crate::oci::push::upload::put`] |
//! | `GET` | `/v2/:name/manifests/:reference` | [`crate::oci::pull::manifest::get`] |
//! | `HEAD` | `/v2/:name/manifests/:reference` | [`crate::oci::pull::manifest::head`] |
//! | `PUT` | `/v2/:name/manifests/:reference` | [`crate::oci::push::manifest::put`] |
//! | `DELETE` | `/v2/:name/manifests/:reference` | always `404` (we map 405 → 404) |
//! | `GET` | `/v2/:name/tags/list` | [`crate::oci::tags::list`] |
//!
//! `<name>` may be multi-segment (`namespace/name/sub/…`), so we
//! register `/v2/*rest` and split manually in
//! [`dispatch::parse_v2_tail`].

pub mod core;
pub mod dispatch;
pub mod handlers;

pub use self::core::{err_response, status_for, wallclock_unix_ms, AppState};
pub use self::dispatch::{parse_v2_tail, urldecode, validate_repo_name, V2Path};

use axum::middleware;
use axum::routing::{any, get};
use axum::Router;

/// Build the `axum` router from a pre-constructed [`AppState`].
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v2/", get(handlers::api_version))
        .route("/v2", get(handlers::api_version))
        .route("/v2/_catalog", get(handlers::catalog))
        .route("/token", get(handlers::token).post(handlers::token_post))
        .route("/v2/*rest", any(handlers::dispatch_v2))
        .with_state(state)
        .layer(middleware::from_fn(log_request))
}

async fn log_request(
    req: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let m = req.method().clone();
    let p = req.uri().path().to_string();
    let resp = next.run(req).await;
    tracing::debug!(method = %m, path = %p, status = %resp.status(), "oci request");
    resp
}
