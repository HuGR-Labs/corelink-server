/// Build the route state from env vars at binary boot time.
///
/// - `CORELINK_PAT_MINT_AUTH_KEY` — the **dedicated, REQUIRED** secret for the
///   mint auth header gate (DD HIGH remediation). It is read on its own and
///   **MUST NOT** fall back to the shared `CORELINK_INTERNAL_AUTH_KEY`: this
///   surface can mint ANY tenant's PAT (including `SCOPE_ADMIN_ALL`), so a leak
///   of the broad shared secret must never, by itself, exercise it. The key
///   must be at least 32 chars; if it is unset/blank/too-short the route is
///   NOT mounted (fail-CLOSED — the endpoint is unavailable rather than gated
///   only by the shared key). The secrets-checklist instructs
///   `openssl rand -hex 32` (64 chars); anything shorter is rejected. The
///   dedicated key is REQUIRED in prod (no shared-key fallback).
/// - `PAT_SIGNING_KEY` — hex-encoded HMAC signing key (≥ 32 bytes decoded).
///   Missing → route returns 503 (same fail-CLOSED policy).
///
/// Returns `None` when either key is absent or invalid; the caller logs
/// a warning and skips mounting the route (dev/CI without secrets).
pub fn build_state_from_env() -> Option<InternalPatRouteState> {
    // DD HIGH remediation: the mint gate requires the DEDICATED
    // `CORELINK_PAT_MINT_AUTH_KEY` and MUST NOT fall back to the shared
    // `CORELINK_INTERNAL_AUTH_KEY`. Unset/blank/< 32 chars ⇒ fail CLOSED
    // (route NOT mounted, endpoint unavailable — never silently widened to
    // the broad shared secret).
    let auth_key =
        resolve_mint_auth_key(std::env::var("CORELINK_PAT_MINT_AUTH_KEY").ok().as_deref())
            .or_else(|| {
                tracing::warn!(
                    "CORELINK_PAT_MINT_AUTH_KEY unset/blank/< 32 chars; \
             /_internal/pat/mint route NOT mounted (fail-CLOSED — NO fallback to \
             the shared CORELINK_INTERNAL_AUTH_KEY; DD HIGH remediation; \
             use `openssl rand -hex 32`)"
                );
                None
            })?;

    let signing_key_hex = std::env::var("PAT_SIGNING_KEY").ok()?;
    let key_bytes = hex_decode(&signing_key_hex)?;
    let signing_key = PatSigningKey::from_bytes(key_bytes)
        .map_err(|e| {
            tracing::warn!(error = %e, "PAT_SIGNING_KEY invalid; /_internal/pat/mint NOT mounted");
        })
        .ok()?;

    Some(InternalPatRouteState {
        internal_auth_key: auth_key,
        signing_key: Arc::new(signing_key),
        signing_key_id: 1,
        inflight: Arc::new(MintInflightLimiter::from_env()),
        rate: Arc::new(MintRateLimiter::from_env()),
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────
