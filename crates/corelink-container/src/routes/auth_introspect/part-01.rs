/// `POST /internal/v1/auth/resolve-tenant` handler.
///
/// Mirrors [`handle_introspect`]'s posture: the dedicated-secret auth gate runs
/// FIRST on raw [`Bytes`] (before the body is parsed), so an unauthenticated
/// caller is rejected with 401 without any JSON parse. A mapped org → 200
/// `{ "tenant_id": ... }`; an unmapped org → 404 `{ "error": "org_not_mapped" }`
/// (provisioning is a SEPARATE step — never auto-created here); a D1 fault
/// → 503 (fail-CLOSED: never serve a wrong tenant).
///
/// # Retry contract (A1 ordering, Option 2 — ratified)
///
/// Provisioning (the `user.created` webhook for CoreLink-Clerk; the in-worker
/// token exchange for githugr-Clerk) is the SOLE `tenant_org_map` writer; this
/// handler NEVER writes. A `404 org_not_mapped` therefore means
/// "not-yet-provisioned", which during Svix webhook-delivery lag is a
/// **TRANSIENT** condition, not a permanent answer. Consumers MUST treat 404 as
/// **retryable** and re-poll with bounded backoff until the provisioning write
/// lands (an arbitrary new user then resolves reliably). `503` is distinct: it
/// is the fail-CLOSED D1-fault signal (also retryable, but backend-fault, not
/// provisioning-lag). Endpoint behaviour is UNCHANGED — this is a contract
/// clarification only; see `docs/integrations/resolve-tenant-contract.md`.
async fn handle_resolve_tenant(
    State(state): State<AuthIntrospectRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // ── 1. Dedicated-secret gate (constant-time; the SAME key set introspect
    //       uses) — non-short-circuiting OR so timing reveals no consumer. ────
    let mut auth_ok = false;
    for key in &state.internal_auth_keys {
        auth_ok |= internal_auth_ok(key.as_bytes(), &headers);
    }
    if !auth_ok {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "unauthorized" })),
        )
            .into_response();
    }

    // ── 1b. Parse the body — ONLY after the auth gate passed. ───────────────
    let req: ResolveTenantRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "resolve_tenant: invalid request body");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_body" })),
            )
                .into_response();
        }
    };
    // Bound the attacker-controlled lookup key and return the same miss shape
    // for malformed identifiers. This avoids turning parser/encoding quirks
    // into a second topology oracle and keeps D1 work strictly bounded.
    if req.clerk_org_id.is_empty()
        || req.clerk_org_id.len() > 128
        || !req
            .clerk_org_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "org_not_mapped" })),
        )
            .into_response();
    }

    // ── 2. D1 read of tenant_org_map (lookup-only, never auto-create). ──────
    match resolve_tenant_for_org(&state.d1, &req.clerk_org_id).await {
        Ok(Some(tenant_id)) => {
            (StatusCode::OK, Json(ResolveTenantResponse { tenant_id })).into_response()
        }
        // Unmapped org → 404 (the showcase-tenant fallback path is the caller's;
        // this endpoint NEVER provisions).
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "org_not_mapped" })),
        )
            .into_response(),
        // D1 fault → fail-CLOSED 503 (never serve a guessed/wrong tenant).
        Err(e) => {
            tracing::error!(error = %e, "resolve_tenant: D1 lookup failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tenant-per-installation resolution + repo allowlist (cf-multitenant WP3)
// ──────────────────────────────────────────────────────────────────────────────

/// SQL: the isolated tenant for a GitHub App installation from the
/// `tenant_gh_installation_map` table (migration 0084). The table is written
/// ONLY by the provisioning authority; this read is LOOKUP-ONLY (never
/// auto-creates a mapping). A missing row is a non-fault miss (the installation
/// is not yet mapped to a tenant).
///
/// # Ordering contract (mirrors [`RESOLVE_TENANT_SQL`])
///
/// Provisioning is the SOLE `tenant_gh_installation_map` writer — the fabric
/// resolver stays lookup-only ON PURPOSE. The fabric plane resolves a GitHub App
/// installation id → isolated tenant against the SAME D1 table the Worker mint
/// reads (single source of truth, no divergent copy). Because the mapping write
/// happens out-of-band, a resolve read may race AHEAD of provisioning: the row is
/// simply not there yet. That miss (`Ok(None)` → 404 `installation_not_mapped`)
/// is TRANSIENT during the provisioning window, NOT a permanent "no such tenant"
/// (analogous to the org resolver's `org_not_mapped`). See
/// [`resolve_tenant_for_installation`].
const RESOLVE_TENANT_FOR_INSTALLATION_SQL: &str =
    "SELECT tenant_id FROM tenant_gh_installation_map WHERE installation_id = ?1 LIMIT 1";

/// SQL: whether a repo is on a tenant's runner allowlist
/// (`runner_repo_allowlist`, migration 0085). A present row (`SELECT 1`) means
/// the `repo_full_name` is explicitly allowed for `tenant_id`; the absence of a
/// row means NOT allowed. This is the fabric-plane's allowlist check, reading the
/// SAME table the Worker mint reads (single source of truth). See
/// [`repo_on_tenant_allowlist`].
const REPO_ON_TENANT_ALLOWLIST_SQL: &str =
    "SELECT 1 FROM runner_repo_allowlist WHERE tenant_id = ?1 AND repo_full_name = ?2 LIMIT 1";

/// Resolve a GitHub App installation id to its isolated CoreLink tenant via
/// `tenant_gh_installation_map` (migration 0084).
///
/// - Row present → `Ok(Some(tenant_id))` (the installation is mapped).
/// - No row → `Ok(None)` (the installation is NOT mapped — the caller falls back
///   to its own unmapped behaviour; this NEVER auto-provisions).
///
/// # Fail-CLOSED
///
/// A genuine D1 backend fault surfaces as `Err(String)` so the caller maps it to
/// **503** rather than guessing a tenant — the fabric must never be handed a
/// WRONG tenant (which would break tenant isolation). A non-fault "no row" is
/// `Ok(None)`, not an error. Mirrors [`resolve_tenant_for_org`].
///
/// # Errors
///
/// Returns `Err(String)` only on a D1 backend fault (so the caller can 503).
pub async fn resolve_tenant_for_installation(
    d1: &D1HttpClient,
    installation_id: &str,
) -> Result<Option<String>, String> {
    let rows = d1
        .query(
            RESOLVE_TENANT_FOR_INSTALLATION_SQL,
            &[serde_json::Value::String(installation_id.to_owned())],
        )
        .await?;
    Ok(decode_resolved_installation_tenant(&rows))
}

/// Pure decode of the `tenant_gh_installation_map` query result into the
/// resolved tenant.
///
/// Split from [`resolve_tenant_for_installation`] so the row→tenant mapping is
/// unit-testable without a network (mirrors [`decode_resolved_tenant`]): the I/O
/// wrapper does the keyed query, this maps rows → `Option<tenant_id>`.
///
/// - A row carrying a non-empty `tenant_id` string → `Some(tenant_id)`.
/// - No row (the installation is not mapped) → `None` (the caller maps this to a
///   404 `installation_not_mapped` — never an auto-provision).
/// - A row whose `tenant_id` is missing / non-string / empty → `None` (treated
///   as "no mapping": a malformed row must never resolve to a wrong/blank tenant
///   — fail to a clean 404, never serve a bad isolation boundary).
fn decode_resolved_installation_tenant(rows: &[crate::storage::d1_http::D1Row]) -> Option<String> {
    rows.first()
        .and_then(|row| row.get("tenant_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Whether `repo_full_name` is on `tenant_id`'s runner allowlist
/// (`runner_repo_allowlist`, migration 0085) — the fabric-plane's allowlist
/// check, reading the SAME table the Worker mint reads.
///
/// - A row present → `Ok(true)` (the repo is explicitly allowed for the tenant).
/// - No row → `Ok(false)` (NOT allowed — the caller denies the mint).
///
/// # Fail-CLOSED
///
/// A genuine D1 backend fault surfaces as `Err(String)` so the caller denies the
/// mint (never grants on a backend fault). This is a lookup-only read; it NEVER
/// writes the allowlist.
///
/// # Errors
///
/// Returns `Err(String)` only on a D1 backend fault (so the caller can deny /
/// 503 fail-CLOSED).
pub async fn repo_on_tenant_allowlist(
    d1: &D1HttpClient,
    tenant_id: &str,
    repo_full_name: &str,
) -> Result<bool, String> {
    let rows = d1
        .query(
            REPO_ON_TENANT_ALLOWLIST_SQL,
            &[
                serde_json::Value::String(tenant_id.to_owned()),
                serde_json::Value::String(repo_full_name.to_owned()),
            ],
        )
        .await?;
    Ok(decode_repo_on_allowlist(&rows))
}

/// Pure decode of the `runner_repo_allowlist` existence query into a bool.
///
/// Split from [`repo_on_tenant_allowlist`] so the row→bool mapping is
/// unit-testable without a network (mirrors [`decode_resolved_tenant`]): the I/O
/// wrapper does the keyed query, this maps rows → presence. The `SELECT 1`
/// projection means a row's mere PRESENCE is the answer — `true` iff at least one
/// row came back.
fn decode_repo_on_allowlist(rows: &[crate::storage::d1_http::D1Row]) -> bool {
    !rows.is_empty()
}

// ──────────────────────────────────────────────────────────────────────────────
// State builder
// ──────────────────────────────────────────────────────────────────────────────

/// Minimum length (chars) of the dedicated fabric introspection secret.
const MIN_FABRIC_AUTH_KEY_LEN: usize = 32;

/// Build the route state from env at binary boot.
///
/// - `FABRIC_INTROSPECT_AUTH_KEY` — DEDICATED shared secret for the auth-header
///   gate (NOT `CORELINK_INTERNAL_AUTH_KEY`). Must be ≥ 32 chars. Absent / too
///   short → returns `None` (route NOT mounted; warn log) — fail-CLOSED.
/// - The PAT verifier + D1 client are built from the same env as the cache
///   adapters via [`PatVerifier::from_env`] / [`D1HttpClient`]; absent →
///   `None`.
///
/// Returns `None` when any required input is missing/invalid; the caller logs a
/// warning and skips mounting the route (dev/CI without secrets).
#[must_use]
pub fn build_state_from_env() -> Option<AuthIntrospectRouteState> {
    let auth_key = std::env::var("FABRIC_INTROSPECT_AUTH_KEY").ok()?;
    if auth_key.len() < MIN_FABRIC_AUTH_KEY_LEN {
        tracing::warn!(
            "FABRIC_INTROSPECT_AUTH_KEY absent or too short (< 32 chars); \
             /internal/v1/auth/introspect route NOT mounted"
        );
        return None;
    }

    let verifier = PatVerifier::from_env()?;

    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = D1HttpClient::new(&storage_env)
        .map_err(|e| {
            tracing::warn!(error = %e, "auth_introspect: D1 client init failed; route NOT mounted");
        })
        .ok()?;

    let state = AuthIntrospectRouteState::new(
        Arc::from(auth_key.as_str()),
        Arc::new(verifier),
        Arc::new(d1),
    );

    // Deliberately do not accept a secondary consumer key here. Tenant
    // resolution is an isolation-sensitive lookup, not a general fabric
    // introspection surface; accepting every configured fabric key made it a
    // post-auth tenant-existence/topology oracle. The single dedicated key is
    // the only authority for both endpoints.

    Some(state)
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("tests-00-00.rs");
    include!("tests-00-01.rs");
}
