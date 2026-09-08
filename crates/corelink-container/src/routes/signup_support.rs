use axum::http::HeaderMap;
use uuid::Uuid;

/// Extract the client IP for the per-IP rate-limit bucket.
pub(super) fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-corelink-client-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map_or_else(|| "_no_ip".to_owned(), str::to_owned)
}

/// Pre-auth signup requests share a nil tenant dimension and are bucketed by IP.
pub(super) const PRE_AUTH_TENANT: Uuid = Uuid::nil();

/// Truncate the raw token to its first 32 chars for the audit emit's
/// `token_id_or_prefix` field — the route MUST never log the full
/// signature (HMAC values are operator-internal forensic data).
pub(super) fn token_prefix(token: &str) -> String {
    token.chars().take(32).collect()
}
