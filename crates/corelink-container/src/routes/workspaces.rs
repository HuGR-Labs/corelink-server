//! Customer self-serve Workspaces HTTP routes: `/v1/customer/workspaces*`.
//!
//! Backs the dashboard Workspaces screen (`apps/admin-ui`, the
//! `CustomerWorkspace` type), which was all prose while `listWorkspaces` threw
//! `NotWiredError` (BE-11). A **workspace** is a named snapshot of a tenant's
//! cached state; a **pinned** workspace is kept warm (never GC-evicted) so a
//! restore is instant.
//!
//! # Endpoints
//!
//! ```text
//! GET    /v1/customer/workspaces               → { workspaces: CustomerWorkspace[] }
//! POST   /v1/customer/workspaces   { name }    → CustomerWorkspace (201)
//! DELETE /v1/customer/workspaces/:workspace_id → { ok: true }
//! POST   /v1/customer/workspaces/:workspace_id/pin → CustomerWorkspace (toggled)
//! ```
//!
//! # Auth model
//!
//! Mirrors [`super::customer`] EXACTLY: the Worker injects
//! `x-corelink-tenant-id` (+ `x-corelink-token-prefix`) post-auth; these routes
//! trust those headers exclusively and NEVER accept a client-supplied tenant.
//! The tenant is resolved fail-CLOSED (missing/sentinel ⇒ 401) and rides
//! `WHERE tenant_id = ?` on every statement (INV-TENANT-ISOLATION). The native
//! PAT possession backstop (cluster A) runs at the top of every handler except
//! for edge-verified Clerk callers.
//!
//! # Fail-CLOSED posture
//!
//! - Missing/sentinel tenant ⇒ 401 (before any storage access).
//! - Forged-HMAC / wrong-tenant PAT ⇒ 401 (native PAT backstop).
//! - Mutations (create / delete / pin) require the cache-write capability
//!   (Worker-trusted `x-corelink-scope`) — a read-only (`cas:r`) cache token
//!   must not mutate workspaces. Mirrors the `customer.rs` keys-create gate.
//! - `db` unwired (dev/CI): reads return an honest empty list; mutations
//!   fail CLOSED (503) rather than pretending to persist.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::customer_d1::{ms_to_iso8601, CustomerD1};

// ─── Route state ─────────────────────────────────────────────────────────────

/// Shared route state for the Workspaces surface.
///
/// Reuses the [`CustomerD1`] row-source seam (same sync↔async D1-over-HTTP
/// bridge the customer dashboard uses) so unit tests exercise the handlers with
/// a hermetic mock. `db` is `None` in dev/CI (StorageEnv unset) → reads return
/// an honest empty list and mutations fail CLOSED (503). `pat_gate` is wired by
/// `routes.rs` (mirroring the customer/cas states); `None` skips the backstop
/// in dev/CI.
#[derive(Clone)]
pub struct WorkspacesRouteState {
    /// D1 row source (production: [`crate::customer_d1::D1HttpCustomerDb`]);
    /// `None` in dev/CI (no StorageEnv).
    pub db: Option<Arc<dyn CustomerD1>>,
    /// Native PAT possession backstop (cluster A). `Some` in production
    /// (`PAT_SIGNING_KEY` + D1); `None` in dev/CI. Wired by `routes.rs` from
    /// `native_pat_gate_from_env()`. Mirrors
    /// [`super::customer::CustomerRouteState::pat_gate`].
    pub pat_gate: Option<Arc<crate::native_pat_gate::NativePatGate>>,
}

impl core::fmt::Debug for WorkspacesRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WorkspacesRouteState")
            .field("db", &self.db.as_ref().map(|_| "[CustomerD1]"))
            .finish_non_exhaustive()
    }
}

// ─── Handler factory ─────────────────────────────────────────────────────────

/// Build the production `WorkspacesRouteState` from process env. When the D1
/// config ([`crate::storage::StorageEnv`]) is present the surface is backed by
/// a real [`crate::customer_d1::D1HttpCustomerDb`]; otherwise dev/CI runs with
/// `db: None` (honest-empty reads, fail-CLOSED mutations). Mirrors
/// [`super::customer::build_handlers_from_env`]'s fail-closed env-gate.
///
/// `pat_gate` is `None` here so the factory stays env-pure; `routes.rs`
/// overwrites it with `native_pat_gate_from_env()` (exactly as it does for the
/// customer/cas states).
#[must_use]
pub fn build_handlers_from_env() -> WorkspacesRouteState {
    let db: Option<Arc<dyn CustomerD1>> = crate::customer_d1::D1HttpCustomerDb::from_env()
        .map(|d| Arc::new(d) as Arc<dyn CustomerD1>);
    if db.is_none() {
        tracing::warn!(
            "StorageEnv unset/invalid; /v1/customer/workspaces reads = empty, \
             writes fail-CLOSED (dev/CI mode)"
        );
    }
    WorkspacesRouteState {
        db,
        pat_gate: None,
    }
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Build the axum `Router` mounting all `/v1/customer/workspaces*` routes.
pub fn router(state: WorkspacesRouteState) -> Router {
    Router::new()
        .route(
            "/v1/customer/workspaces",
            get(handle_list).post(handle_create),
        )
        .route("/v1/customer/workspaces/:workspace_id", delete(handle_delete))
        .route("/v1/customer/workspaces/:workspace_id/pin", post(handle_pin))
        .with_state(state)
}

// ─── Header / auth helpers (mirror `customer.rs` EXACTLY) ─────────────────────

/// Read `name` from `headers`; return `default` when absent/empty/non-ASCII.
fn header_or(headers: &HeaderMap, name: &str, default: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant.
/// Mirrors `auth_tenant::AuthTenant` + `customer.rs`.
const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];

/// The `x-corelink-token-prefix` value the Worker stamps for a Clerk-session
/// caller (edge-verified, carries NO bearer). Mirrors `customer.rs`.
const CLERK_TOKEN_PREFIX: &str = "clerk";

/// Read the authenticated `x-corelink-tenant-id`, **fail-CLOSED**: a
/// missing/empty/sentinel value is an `Err(())` the handler maps to 401.
fn tenant(headers: &HeaderMap) -> Result<String, ()> {
    let raw = headers
        .get("x-corelink-tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    if raw.is_empty() || TENANT_SENTINELS.contains(&raw) {
        return Err(());
    }
    Ok(raw.to_owned())
}

/// Canonical fail-CLOSED 401 for a missing/sentinel authenticated tenant.
fn unauthenticated_tenant() -> axum::response::Response {
    (StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response()
}

/// Read `x-corelink-token-prefix`; fail-CLOSED to `"_unknown"`.
fn principal(headers: &HeaderMap) -> String {
    header_or(headers, "x-corelink-token-prefix", "_unknown")
}

/// Run the native PAT possession backstop (cluster A) when wired, EXCEPT for
/// edge-verified Clerk-session callers (they carry no bearer). `Some(resp)` ⇒
/// REJECT; `None` ⇒ proceed. Mirrors `customer.rs::pat_gate_reject`.
async fn pat_gate_reject(
    state: &WorkspacesRouteState,
    tenant: &str,
    headers: &HeaderMap,
) -> Option<axum::response::Response> {
    let gate = state.pat_gate.as_ref()?;
    if principal(headers) == CLERK_TOKEN_PREFIX {
        return None;
    }
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    gate.verify(tenant, bearer).await.err()
}

/// Mutation gate: creating / deleting / pinning a workspace is a write op — a
/// read-only (`cas:r`) cache token must not perform it. Gate on the
/// Worker-trusted `x-corelink-scope` cache-write capability (the same one
/// `customer.rs` keys-create/revoke enforce). Dashboard Clerk callers carry the
/// `read-write` scope (Worker-set) and pass; `cas:rw` passes; `cas:r` → 403.
/// `Some(resp)` ⇒ REJECT 403; `None` ⇒ proceed.
fn write_scope_gate_reject(headers: &HeaderMap) -> Option<axum::response::Response> {
    let caller_scope = header_or(headers, crate::scope::SCOPE_HEADER, "");
    if crate::scope::requires_cache_write(&caller_scope) {
        return None;
    }
    Some(
        (
            StatusCode::FORBIDDEN,
            "insufficient scope to modify workspaces",
        )
            .into_response(),
    )
}

/// Real wall-clock unix-ms (the `created_at` anchor). The customer routes pass a
/// logical clock of 0; workspaces mint a genuine creation timestamp here.
fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(i64::MAX)
}

// ─── POST body ────────────────────────────────────────────────────────────────

/// `POST /v1/customer/workspaces` request body.
#[derive(Debug, Deserialize)]
pub struct CreateWorkspaceBody {
    /// Human-readable label for the new workspace.
    pub name: String,
}

// ─── Row → CustomerWorkspace projection ────────────────────────────────────────

/// Project one D1 row into the FROZEN `CustomerWorkspace` wire shape
/// (`apps/admin-ui/src/lib/customer-types.ts`): `workspace_id`, `name`,
/// `size_bytes`, `created_at` (ISO-8601, derived from `created_at_ms`),
/// `pinned` (bool from the `INTEGER` column).
fn row_to_workspace(row: &crate::storage::d1_http::D1Row) -> Value {
    let created_at_ms = row.get("created_at_ms").and_then(Value::as_i64).unwrap_or(0);
    json!({
        "workspace_id": row.get("workspace_id").and_then(Value::as_str).unwrap_or_default(),
        "name":         row.get("name").and_then(Value::as_str).unwrap_or_default(),
        "size_bytes":   row.get("size_bytes").and_then(Value::as_i64).unwrap_or(0),
        "created_at":   ms_to_iso8601(created_at_ms),
        "pinned":       row.get("pinned").and_then(Value::as_i64).unwrap_or(0) != 0,
    })
}

// ─── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /v1/customer/workspaces` → `{ workspaces: CustomerWorkspace[] }`.
///
/// Tenant-scoped, newest-first. An unwired `db` (dev/CI) or a tenant with no
/// rows returns an honest empty list (never a 404).
async fn handle_list(
    State(state): State<WorkspacesRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    let Some(db) = state.db.as_ref() else {
        // Honest-empty (dev/CI): no D1 → no workspaces for this tenant.
        return (StatusCode::OK, Json(json!({ "workspaces": [] }))).into_response();
    };
    match db.query(
        "SELECT workspace_id, name, size_bytes, snapshot_ref, pinned, created_at_ms \
         FROM workspaces WHERE tenant_id = ?1 ORDER BY created_at_ms DESC",
        vec![json!(t)],
    ) {
        Ok(rows) => {
            let workspaces: Vec<Value> = rows.iter().map(row_to_workspace).collect();
            (StatusCode::OK, Json(json!({ "workspaces": workspaces }))).into_response()
        }
        Err(e) => internal(&t, "workspaces list", &e),
    }
}

/// `POST /v1/customer/workspaces` `{ name }` → the new `CustomerWorkspace` (201).
///
/// Mints a server-side `workspace_id` (UUID); `size_bytes` = 0 and `pinned` =
/// false until snapshot sizing/pinning land. Fail-CLOSED: an unwired `db` ⇒ 503.
async fn handle_create(
    State(state): State<WorkspacesRouteState>,
    headers: HeaderMap,
    Json(body): Json<CreateWorkspaceBody>,
) -> impl IntoResponse {
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    if let Some(resp) = write_scope_gate_reject(&headers) {
        return resp;
    }
    let name = body.name.trim().to_owned();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, "workspace name required").into_response();
    }
    let Some(db) = state.db.as_ref() else {
        return workspaces_unconfigured();
    };
    let workspace_id = uuid::Uuid::new_v4().to_string();
    let created_at_ms = now_ms();
    match db.query(
        "INSERT INTO workspaces \
         (tenant_id, workspace_id, name, size_bytes, snapshot_ref, pinned, created_at_ms) \
         VALUES (?1, ?2, ?3, 0, '', 0, ?4)",
        vec![
            json!(t),
            json!(workspace_id),
            json!(name),
            json!(created_at_ms),
        ],
    ) {
        Ok(_) => {
            let body = json!({
                "workspace_id": workspace_id,
                "name":         name,
                "size_bytes":   0,
                "created_at":   ms_to_iso8601(created_at_ms),
                "pinned":       false,
            });
            (StatusCode::CREATED, Json(body)).into_response()
        }
        Err(e) => internal(&t, "workspace create", &e),
    }
}

/// `DELETE /v1/customer/workspaces/:workspace_id` → `{ ok: true }`.
///
/// Tenant-scoped delete (`WHERE tenant_id = ?1 AND workspace_id = ?2` — a
/// caller can never delete another tenant's workspace). Idempotent: deleting a
/// missing/already-deleted id is a 200 no-op. Fail-CLOSED: unwired `db` ⇒ 503.
async fn handle_delete(
    State(state): State<WorkspacesRouteState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> impl IntoResponse {
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    if let Some(resp) = write_scope_gate_reject(&headers) {
        return resp;
    }
    let Some(db) = state.db.as_ref() else {
        return workspaces_unconfigured();
    };
    match db.query(
        "DELETE FROM workspaces WHERE tenant_id = ?1 AND workspace_id = ?2",
        vec![json!(t), json!(workspace_id)],
    ) {
        Ok(_) => (StatusCode::OK, Json(json!({ "ok": true }))).into_response(),
        Err(e) => internal(&t, "workspace delete", &e),
    }
}

/// `POST /v1/customer/workspaces/:workspace_id/pin` → the updated
/// `CustomerWorkspace`.
///
/// Toggles the `pinned` flag (`pinned = 1 - pinned`), tenant-scoped, then
/// returns the fresh row. A missing id ⇒ 404. Fail-CLOSED: unwired `db` ⇒ 503.
async fn handle_pin(
    State(state): State<WorkspacesRouteState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> impl IntoResponse {
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    if let Some(resp) = write_scope_gate_reject(&headers) {
        return resp;
    }
    let Some(db) = state.db.as_ref() else {
        return workspaces_unconfigured();
    };
    // Toggle, tenant-scoped.
    if let Err(e) = db.query(
        "UPDATE workspaces SET pinned = 1 - pinned \
         WHERE tenant_id = ?1 AND workspace_id = ?2",
        vec![json!(t), json!(workspace_id)],
    ) {
        return internal(&t, "workspace pin", &e);
    }
    // Read the fresh row back (tenant-scoped).
    match db.query(
        "SELECT workspace_id, name, size_bytes, snapshot_ref, pinned, created_at_ms \
         FROM workspaces WHERE tenant_id = ?1 AND workspace_id = ?2 LIMIT 1",
        vec![json!(t), json!(workspace_id)],
    ) {
        Ok(rows) => match rows.first() {
            Some(row) => (StatusCode::OK, Json(row_to_workspace(row))).into_response(),
            None => (StatusCode::NOT_FOUND, "workspace not found").into_response(),
        },
        Err(e) => internal(&t, "workspace pin read-back", &e),
    }
}

// ─── Error helpers ──────────────────────────────────────────────────────────────

/// Fail-CLOSED 503 when the D1 backend is unwired (dev/CI) — never a silent
/// success for a mutation we cannot durably persist.
fn workspaces_unconfigured() -> axum::response::Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "workspaces backend not configured",
    )
        .into_response()
}

/// Fail-CLOSED 500 on any D1 transport/decode fault (never fabricated data).
fn internal(tenant: &str, op: &str, err: &str) -> axum::response::Response {
    tracing::error!(tenant = %tenant, op = %op, error = %err, "workspaces D1 error");
    (StatusCode::INTERNAL_SERVER_ERROR, "workspaces backend error").into_response()
}
