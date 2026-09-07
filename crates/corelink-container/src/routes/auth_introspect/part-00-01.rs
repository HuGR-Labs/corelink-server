        .and_then(|row| row.get("tier").and_then(|v| v.as_str()).map(str::to_owned))
    {
        if is_valid_tier(&tier) {
            return Ok(tier);
        }
    }

    // ── 2. tenant.tier default column ───────────────────────────────────────
    let tenant_rows = d1
        .query(
            TENANT_TIER_SQL,
            &[serde_json::Value::String(tenant_id.to_owned())],
        )
        .await?;
    if let Some(tier) = tenant_rows
        .into_iter()
        .next()
        .and_then(|row| row.get("tier").and_then(|v| v.as_str()).map(str::to_owned))
    {
        if is_valid_tier(&tier) {
            return Ok(tier);
        }
    }

    // ── 3. Hard default ─────────────────────────────────────────────────────
    Ok(DEFAULT_TIER.to_owned())
}

/// Whether a D1 tier string is one of the canonical wire tiers.
#[must_use]
fn is_valid_tier(value: &str) -> bool {
    VALID_TIERS.contains(&value)
}

// ──────────────────────────────────────────────────────────────────────────────
// Route handler
// ──────────────────────────────────────────────────────────────────────────────

/// Build the introspection router. Mount at the top level so
/// `/internal/v1/auth/introspect` is directly addressable.
pub fn router(state: AuthIntrospectRouteState) -> Router {
    Router::new()
        .route("/internal/v1/auth/introspect", post(handle_introspect))
        .route(
            "/internal/v1/auth/resolve-tenant",
            post(handle_resolve_tenant),
        )
        .with_state(state)
}

/// `POST /internal/v1/auth/introspect` handler.
///
/// The auth gate runs FIRST, on raw [`Bytes`] (the `HeaderMap` is a
/// `FromRequestParts` extractor, so it is evaluated before the body is
/// buffered/parsed): an unauthenticated caller is rejected with 401 WITHOUT the
/// body ever being JSON-parsed (denying parse CPU/heap to an attacker), exactly
/// as in `internal_pat`.
async fn handle_introspect(
    State(state): State<AuthIntrospectRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // ── 1. Dedicated-secret gate (constant-time; reused gate) ───────────────
    // Check the presented header against EVERY configured consumer key. We
    // OR-combine with `|=` (NOT short-circuiting `||`) so all keys are always
    // evaluated: the response time does not reveal WHICH consumer's key matched
    // (no consumer-identity oracle), and the number of constant-time comparisons
    // is independent of the outcome. A caller with no/ wrong key is rejected
    // identically regardless of how many consumers are configured.
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

    // ── 1b. Parse the body — ONLY after the auth gate passed ────────────────
    let req: IntrospectRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            // Never log the token; the body is rejected on shape only.
            tracing::warn!(error = %e, "auth_introspect: invalid request body");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid_body" })),
            )
                .into_response();
        }
    };

    // ── 2. Verify the PAT (HMAC + D1 liveness + Argon2id + scope) ───────────
    match state.verifier.verify(&req.token).await {
        Ok(tenant_id) => {
            // ── 3. Resolve the plan AND the runners entitlement.
            //
            // These are two INDEPENDENT D1 reads: `tier_for_tenant` reads
            // `tier_selection`, `runner_concurrency_for_tenant` reads
            // `runners_entitlement` (migrations 0070 + 0072), and neither
            // consumes the other's result — the M2 runners seam is a SEPARATE
            // axis, deliberately NOT derived from `plan`. They used to run
            // nested, so the route paid both round trips end to end. From the
            // container these are D1-over-HTTP, ~80-100 ms each measured, which
            // is the dominant cost of this handler; issuing them concurrently
            // halves the success path.
            //
            // FAIL-CLOSED is preserved exactly: either fault still yields 503,
            // and the two error arms keep their DISTINCT log lines, so an
            // operator can still tell which table faulted. `try_join!` returns
            // the first error, and each side tags its own before joining.
            //
            // ⚠️ Stated cost, not hidden: with a join, BOTH queries are issued
            // even when one is going to fault, where the nested form would have
            // short-circuited. That is one extra D1 read on the ERROR path, in
            // exchange for halving the SUCCESS path. The error path is the rare
            // one and it already ends in a 503.
            let plan_fut = async {
                tier_for_tenant(&state.d1, &tenant_id)
                    .await
                    .map_err(|e| ("tier", e))
            };
            let ent_fut = async {
                runner_concurrency_for_tenant(&state.d1, &tenant_id)
                    .await
                    .map_err(|e| ("runner entitlement", e))
            };
            match futures::try_join!(plan_fut, ent_fut) {
                Ok((plan, (max_concurrency, max_vcpu_h))) => (
                    StatusCode::OK,
                    Json(IntrospectResponse::valid(
                        tenant_id,
                        plan,
                        max_concurrency,
                        max_vcpu_h,
                    )),
                )
                    .into_response(),
                Err(("tier", e)) => {
                    tracing::error!(error = %e, "auth_introspect: tier resolution failed");
                    StatusCode::SERVICE_UNAVAILABLE.into_response()
                }
                Err((_, e)) => {
                    tracing::error!(error = %e, "auth_introspect: runner entitlement resolution failed");
                    StatusCode::SERVICE_UNAVAILABLE.into_response()
                }
            }
        }
        // Uniform invalid — no tenant_id, no reason.
        Err(VerifyError::InvalidPat) => {
            (StatusCode::OK, Json(IntrospectResponse::invalid())).into_response()
        }
        // Genuine backend fault OR a verifier load shed — the fabric maps
        // 503 → Err(Unreachable).
        //
        // ACCEPTED BEHAVIOUR CHANGE (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`):
        // the container's Argon2id shed is now symmetric across D1 row
        // existence, so a saturated verifier returns `Backend` for an
        // UNKNOWN token where it previously returned `InvalidPat`. For HuGR
        // Tools Mode B that flips this endpoint's answer for that narrow case
        // from 200 `{valid:false}` ("this token is invalid") to 503 ("the auth
        // service is unreachable, retry"). That is the HONEST answer — under
        // saturation the verifier never judged the credential — and it is
        // deliberately NOT special-cased here: re-splitting the two shed arms
        // by row existence at this layer would rebuild the exact row-existence
        // oracle the invariant exists to close. This route is internal-auth
        // gated, so the reclassification is not an external attack surface;
        // the caller's own retry (bounded, the shed clears in ~1 s) resolves it.
        Err(VerifyError::Backend(e)) => {
            tracing::error!(error = %e, "auth_introspect: verifier backend fault");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tenant-per-org resolution (githugr ADR-0007 Epic A primitive)
// ──────────────────────────────────────────────────────────────────────────────

/// JSON request body for `POST /internal/v1/auth/resolve-tenant`. The
/// `clerk_org_id` is the Clerk organization id (`org_...`) to resolve.
#[non_exhaustive]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveTenantRequest {
    /// The Clerk organization id to resolve to its isolated CoreLink tenant.
    pub clerk_org_id: String,
}

/// JSON response body for a SUCCESSFUL (`200`) tenant resolution.
#[non_exhaustive]
#[derive(Debug, Serialize, Deserialize)]
pub struct ResolveTenantResponse {
    /// The isolated CoreLink tenant UUID the org maps to.
    pub tenant_id: String,
}

/// SQL: the isolated tenant for a Clerk org from the `tenant_org_map` table
/// (migration 0083). The table is written ONLY by the provisioning authority;
/// this read is LOOKUP-ONLY (never auto-creates a mapping). A missing row is a
/// non-fault miss (the org is not yet provisioned).
///
/// # Ordering contract (A1, Option 2 — ratified)
///
/// Provisioning is the SOLE `tenant_org_map` writer — the resolver stays
/// lookup-only ON PURPOSE. Provision-in-resolver (Option 1) was REJECTED: the
/// `clerk_org_id` is an unverified, attacker-influenceable string, so writing a
/// mapping here would risk creating a WRONG/attacker-chosen tenant. The two
/// provisioning authorities are:
/// - **CoreLink-Clerk:** the `user.created` webhook (signup-worker) writes the
///   row (`INSERT OR IGNORE`, idempotent).
/// - **githugr-Clerk:** the in-worker Option-B token exchange writes the row.
///
/// Because the write happens out-of-band, a `resolve-tenant` read may race
/// AHEAD of provisioning (Svix webhook delivery lag): the row is simply not
/// there yet. That miss (`Ok(None)` → 404 `org_not_mapped`) is TRANSIENT during
/// the provisioning window, NOT a permanent "no such tenant". See
/// [`resolve_tenant_for_org`] / [`handle_resolve_tenant`] for the retry contract.
const RESOLVE_TENANT_SQL: &str =
    "SELECT tenant_id FROM tenant_org_map WHERE clerk_org_id = ?1 LIMIT 1";

/// Resolve a Clerk org id to its isolated CoreLink tenant via `tenant_org_map`
/// (migration 0083).
///
/// - Row present → `Ok(Some(tenant_id))` (the org is provisioned).
/// - No row → `Ok(None)` (the org is NOT mapped — the caller falls back to its
///   own unmapped-org behaviour; this endpoint NEVER auto-provisions).
///
/// # Fail-CLOSED
///
/// A genuine D1 backend fault surfaces as `Err(String)` so the route maps it to
/// **503** rather than guessing a tenant — the caller must never be handed a
/// WRONG tenant (which would break tenant isolation). A non-fault "no row" is
/// `Ok(None)`, not an error.
///
/// # Errors
///
/// Returns `Err(String)` only on a D1 backend fault (so the route can 503).
pub async fn resolve_tenant_for_org(
    d1: &D1HttpClient,
    clerk_org_id: &str,
) -> Result<Option<String>, String> {
    let rows = d1
        .query(
            RESOLVE_TENANT_SQL,
            &[serde_json::Value::String(clerk_org_id.to_owned())],
        )
        .await?;
    Ok(decode_resolved_tenant(&rows))
}

/// Pure decode of the `tenant_org_map` query result into the resolved tenant.
///
/// Split from [`resolve_tenant_for_org`] so the row→tenant mapping is
/// unit-testable without a network (mirrors [`decode_runner_cap`]): the I/O
/// wrapper does the keyed query, this maps rows → `Option<tenant_id>`.
///
/// - A row carrying a non-empty `tenant_id` string → `Some(tenant_id)`.
/// - No row (the org is not provisioned) → `None` (the handler maps this to a
///   404 `org_not_mapped` — never an auto-provision).
/// - A row whose `tenant_id` is missing / non-string / empty → `None` (treated
///   as "no mapping": a malformed row must never resolve to a wrong/blank
///   tenant — fail to a clean 404, never serve a bad isolation boundary).
fn decode_resolved_tenant(rows: &[crate::storage::d1_http::D1Row]) -> Option<String> {
    rows.first()
        .and_then(|row| row.get("tenant_id"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
