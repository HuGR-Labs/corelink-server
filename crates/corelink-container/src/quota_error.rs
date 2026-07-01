//! Centralized STRUCTURED-JSON bodies for the per-tenant monthly **$-ceiling**
//! quota rejects (ADR-0068).
//!
//! # Why a shared module
//!
//! Every quota-reject site — the native gate ([`crate::tenant_quota::QuotaGuard`]),
//! the Bazel REAPI `quota_reject` helpers, and the OCI `oci_quota_gate`
//! middleware — used to return a bare plain-text body via
//! `(StatusCode, "…").into_response()`. An arbitrary user (or the githugr UX)
//! then got an OPAQUE string, not a machine-parseable "you are over quota +
//! here is the path" signal. This module is the single source of truth for the
//! two quota-reject response BODIES so every surface emits an identical,
//! parseable JSON envelope.
//!
//! # What is (and is NOT) changed
//!
//! ONLY the response body + `Content-Type` change. The STATUS CODES are
//! byte-identical to before — `402 Payment Required` over the ceiling, `503
//! Service Unavailable` on the fail-CLOSED (store/clock/accrual) paths — and no
//! quota LOGIC (accrue/check/lease/fail-closed semantics) is touched.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

/// Public docs URL surfaced in the over-ceiling body so a client can point the
/// user at the remediation path (raise the cap / wait for the cycle).
const DOCS_URL: &str = "https://corelink-api.humangr.com/";

/// `402 Payment Required` — the tenant's monthly `$`-ceiling was reached.
///
/// Returns a JSON envelope
/// `{"error":"quota_exceeded","message":…,"docs_url":…,"retriable":false}`
/// with `Content-Type: application/json`. This is a hard business reject, so
/// `retriable` is `false` (the same op will keep failing until the cap is
/// raised or the cycle resets).
#[must_use]
pub fn quota_exceeded_response() -> Response {
    let body = serde_json::json!({
        "error": "quota_exceeded",
        "message": "Monthly usage ceiling reached. Raise the cap in the CoreLink \
                    dashboard billing settings, or wait for the monthly cycle to reset.",
        "docs_url": DOCS_URL,
        "retriable": false,
    })
    .to_string();

    (
        StatusCode::PAYMENT_REQUIRED,
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}

/// `503 Service Unavailable` — a fail-CLOSED quota reject (store/clock/accrual
/// unavailable), where `reason` is the existing static marker string (e.g.
/// `"quota store unavailable"`).
///
/// Returns a JSON envelope
/// `{"error":"quota_unavailable","message":…,"reason":<reason>,"retriable":true}`
/// with `Content-Type: application/json`. `reason` is preserved as a MACHINE
/// field (not the whole body) so callers keep the precise fail-CLOSED marker.
/// `retriable` is `true` — the metering path is expected to recover.
#[must_use]
pub fn quota_unavailable_response(reason: &'static str) -> Response {
    let body = serde_json::json!({
        "error": "quota_unavailable",
        "message": "Usage metering is temporarily unavailable; the request was \
                    refused fail-closed. Retry shortly.",
        "reason": reason,
        "retriable": true,
    })
    .to_string();

    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::CONTENT_TYPE, "application/json")],
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
    use axum::body::to_bytes;

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = to_bytes(resp.into_body(), 4096).await.expect("body");
        serde_json::from_slice(&bytes).expect("json body")
    }

    #[tokio::test]
    async fn quota_exceeded_is_402_json_with_docs_url() {
        let resp = quota_exceeded_response();
        assert_eq!(resp.status(), StatusCode::PAYMENT_REQUIRED);
        assert_eq!(
            resp.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json"),
        );
        let json = body_json(resp).await;
        assert_eq!(json.get("error").and_then(|v| v.as_str()), Some("quota_exceeded"));
        assert_eq!(json.get("retriable").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(json.get("docs_url").and_then(|v| v.as_str()), Some(DOCS_URL));
        assert!(
            json.get("message")
                .and_then(|v| v.as_str())
                .is_some_and(|m| !m.is_empty()),
            "402 body must carry a human message"
        );
    }

    #[tokio::test]
    async fn quota_unavailable_is_503_json_with_reason() {
        let resp = quota_unavailable_response("quota store unavailable");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            resp.headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json"),
        );
        let json = body_json(resp).await;
        assert_eq!(json.get("error").and_then(|v| v.as_str()), Some("quota_unavailable"));
        assert_eq!(
            json.get("reason").and_then(|v| v.as_str()),
            Some("quota store unavailable"),
        );
        assert_eq!(json.get("retriable").and_then(|v| v.as_bool()), Some(true));
    }
}
