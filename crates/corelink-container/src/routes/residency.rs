//! Data-residency guard middleware (backlog #29 — a live Schrems II leak).
//!
//! # What this closes
//!
//! The edge Worker now fans EU (`weur`) tenants out to the London (`lhr`)
//! regional Worker and stamps the trusted `x-corelink-primary-region:<macro>`
//! header (see `worker/src/index.ts`). This middleware is the container-side
//! BACKSTOP: it independently verifies that the macro the Worker claims actually
//! matches the region THIS container serves (`R2_CAS_REGION`), and rejects the
//! request with HTTP 409 `residency_violation` BEFORE any handler runs — so a
//! mis-bound regional Worker (or a future routing bug) can never land an EU
//! tenant's bytes in a US container. Defence-in-depth: the worker is fail-closed
//! AND the container refuses cross-region traffic.
//!
//! # Disjointness
//!
//! This is a router `layer`, wired with ONE line in `routes::build_with_factory`.
//! It does NOT touch the cas.rs / ac.rs handler bodies — the residency decision
//! is made entirely here, before the request reaches any handler, with ZERO
//! storage I/O on the reject path.
//!
//! # Behaviour
//!
//! - **Header absent** → pass through. The Worker only sets the header on the
//!   non-IAD fan-out path; local/IAD-resident traffic (wnam/enam) never carries
//!   it and is served locally (the IAD container's `R2_CAS_REGION` is `iad`).
//! - **Header present, maps to this container's colo** → pass through.
//! - **Header present, maps to a DIFFERENT colo** → 409 `residency_violation`.
//! - **Header present but unknown/unprovisioned macro (e.g. `afr`)** → 409
//!   (fail-closed; never serve an unmappable region).

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::storage::region_map::colo_for_macro;

/// The trusted, edge-set header carrying the tenant's data-residency MACRO
/// region. The edge Worker strips any client-supplied value and sets this from
/// the D1 `tenant.primary_region` on the regional fan-out path.
pub const PRIMARY_REGION_HEADER: &str = "x-corelink-primary-region";

/// Read this container's serving colo from `R2_CAS_REGION` (FROZEN per-env
/// contract; default `iad`). This is the SAME env read the CAS handler keys its
/// bucket from (`cas.rs` → `env_or("R2_CAS_REGION", "iad")`), so the guard and
/// the storage layer agree by construction.
#[must_use]
pub fn container_colo() -> String {
    crate::storage::env_or("R2_CAS_REGION", "iad")
}

/// Tower/axum middleware enforcing data residency before any handler runs.
///
/// Wired as `.layer(axum::middleware::from_fn(residency_guard))` on the composed
/// router. Returns 409 `residency_violation` on a cross-region request; otherwise
/// forwards to the next layer untouched. The container's own colo is read once
/// here from [`container_colo`]; the decision itself is the pure
/// [`residency_decision`] (so tests need no process-env mutation).
pub async fn residency_guard(req: Request, next: Next) -> Response {
    let claimed_macro = req
        .headers()
        .get(PRIMARY_REGION_HEADER)
        .and_then(|v| v.to_str().ok());
    let own_colo = container_colo();

    match residency_decision(claimed_macro, &own_colo) {
        ResidencyDecision::Allow => next.run(req).await,
        ResidencyDecision::Reject { claimed_macro } => {
            tracing::warn!(
                claimed_macro = %claimed_macro,
                container_colo = %own_colo,
                "residency_violation: cross-region/unmappable request rejected (409)"
            );
            residency_violation(&claimed_macro, &own_colo)
        }
    }
}

/// The pure residency decision: does a request claiming `claimed_macro` belong
/// on a container serving `container_colo`? Extracted so it is unit-testable
/// with ZERO process-env mutation (avoids the parallel-test `set_var` race).
///
/// - `None` claimed macro (header absent) → Allow (local/IAD path; the Worker
///   only stamps the header on the non-IAD fan-out).
/// - claimed macro maps to `container_colo` → Allow.
/// - claimed macro maps to a DIFFERENT colo, or is unknown/unprovisioned (afr) →
///   Reject (fail-closed).
#[must_use]
pub fn residency_decision(claimed_macro: Option<&str>, container_colo: &str) -> ResidencyDecision {
    let Some(macro_region) = claimed_macro else {
        return ResidencyDecision::Allow;
    };
    match colo_for_macro(macro_region) {
        Some(expected) if expected == container_colo => ResidencyDecision::Allow,
        _ => ResidencyDecision::Reject {
            claimed_macro: macro_region.to_owned(),
        },
    }
}

/// Outcome of [`residency_decision`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResidencyDecision {
    /// Forward to the next layer.
    Allow,
    /// Reject with 409 `residency_violation`.
    Reject {
        /// The macro the (trusted) header claimed — for the 409 body + log.
        claimed_macro: String,
    },
}

/// Build the uniform 409 `residency_violation` response. No body inspection, no
/// storage I/O — purely the rejection envelope.
fn residency_violation(claimed_macro: &str, container_colo: &str) -> Response {
    let body = format!(
        "{{\"error\":\"residency_violation\",\"message\":\"tenant region '{claimed_macro}' \
         is not served by this container (colo '{container_colo}')\"}}"
    );
    (
        StatusCode::CONFLICT,
        [("content-type", "application/json")],
        body,
    )
        .into_response()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request as HttpRequest, StatusCode},
        routing::get,
        Router,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tower::ServiceExt; // for `.oneshot()`

    // ── Pure-decision tests (no process-env mutation → parallel-safe) ─────────

    #[test]
    fn decision_matching_region_allows() {
        // weur → lhr; on an lhr container → allow.
        assert_eq!(
            residency_decision(Some("weur"), "lhr"),
            ResidencyDecision::Allow
        );
        // enam → iad; on an iad container → allow.
        assert_eq!(
            residency_decision(Some("enam"), "iad"),
            ResidencyDecision::Allow
        );
    }

    #[test]
    fn decision_absent_header_allows() {
        // No header (local/IAD path) → allow.
        assert_eq!(residency_decision(None, "iad"), ResidencyDecision::Allow);
        assert_eq!(residency_decision(None, "lhr"), ResidencyDecision::Allow);
    }

    #[test]
    fn decision_cross_region_rejects() {
        // weur (→lhr) to an iad container → reject (the leak this fix closes).
        assert_eq!(
            residency_decision(Some("weur"), "iad"),
            ResidencyDecision::Reject {
                claimed_macro: "weur".to_owned()
            }
        );
        // sam (→sam) to an iad container → reject.
        assert_eq!(
            residency_decision(Some("sam"), "iad"),
            ResidencyDecision::Reject {
                claimed_macro: "sam".to_owned()
            }
        );
    }

    #[test]
    fn decision_unprovisioned_or_unknown_rejects() {
        // afr → no colo → reject (fail-closed).
        assert!(matches!(
            residency_decision(Some("afr"), "iad"),
            ResidencyDecision::Reject { .. }
        ));
        // Unknown macro → reject (never serve an unmappable region).
        assert!(matches!(
            residency_decision(Some("zzz"), "iad"),
            ResidencyDecision::Reject { .. }
        ));
    }

    // ── Middleware integration test (relies on the DEFAULT R2_CAS_REGION="iad",
    //    so NO set_var — keeps the test parallel-safe; no other test in this
    //    crate mutates R2_CAS_REGION). Proves the layer rejects a cross-region
    //    request BEFORE the handler runs (zero handler/storage I/O). ──────────

    fn flag_router(reached: Arc<AtomicBool>) -> Router {
        Router::new()
            .route(
                "/v1/cas/{tenant}/{hash}",
                get(move || {
                    let r = reached.clone();
                    async move {
                        r.store(true, Ordering::SeqCst);
                        "ok"
                    }
                }),
            )
            .layer(axum::middleware::from_fn(residency_guard))
    }

    #[tokio::test]
    async fn weur_request_to_default_iad_container_is_409_with_no_handler_io() {
        // Default container colo is "iad" (R2_CAS_REGION unset). weur → lhr ≠ iad.
        let reached = Arc::new(AtomicBool::new(false));
        let app = flag_router(reached.clone());

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/v1/cas/tenant-1/abc")
                    .header(PRIMARY_REGION_HEADER, "weur")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        // The handler must NOT have run — zero S3/storage I/O on the reject path.
        assert!(
            !reached.load(Ordering::SeqCst),
            "handler must not be reached on a residency_violation"
        );
    }

    #[tokio::test]
    async fn absent_header_passes_through_to_handler() {
        // No residency header → local/IAD path → handler runs.
        let reached = Arc::new(AtomicBool::new(false));
        let app = flag_router(reached.clone());

        let resp = app
            .oneshot(
                HttpRequest::builder()
                    .uri("/v1/cas/tenant-1/abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(reached.load(Ordering::SeqCst));
    }
}
