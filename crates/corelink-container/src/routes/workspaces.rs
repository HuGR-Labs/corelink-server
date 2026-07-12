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
    WorkspacesRouteState { db, pat_gate: None }
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Build the axum `Router` mounting all `/v1/customer/workspaces*` routes.
pub fn router(state: WorkspacesRouteState) -> Router {
    Router::new()
        .route(
            "/v1/customer/workspaces",
            get(handle_list).post(handle_create),
        )
        .route(
            "/v1/customer/workspaces/{workspace_id}",
            delete(handle_delete),
        )
        .route(
            "/v1/customer/workspaces/{workspace_id}/pin",
            post(handle_pin),
        )
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
    let created_at_ms = row
        .get("created_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
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
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "workspaces backend error",
    )
        .into_response()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{self, Request};
    use std::sync::Mutex;
    use tower::ServiceExt;

    /// Hermetic D1 mock: returns `list_rows` for the list query and
    /// `readback_row` for the pin read-back (LIMIT 1); records every call so a
    /// test can assert the tenant-scoping bind (`?1`).
    #[derive(Debug, Default)]
    struct MockWsD1 {
        list_rows: Vec<crate::storage::d1_http::D1Row>,
        readback_row: Vec<crate::storage::d1_http::D1Row>,
        calls: Mutex<Vec<(String, Vec<Value>)>>,
    }

    impl CustomerD1 for MockWsD1 {
        fn query(
            &self,
            sql: &str,
            binds: Vec<Value>,
        ) -> Result<Vec<crate::storage::d1_http::D1Row>, String> {
            self.calls.lock().unwrap().push((sql.to_owned(), binds));
            if sql.contains("FROM workspaces") && sql.contains("LIMIT 1") {
                return Ok(self.readback_row.clone());
            }
            if sql.contains("FROM workspaces") && sql.contains("ORDER BY") {
                return Ok(self.list_rows.clone());
            }
            // INSERT / DELETE / UPDATE → affected-rows semantics, no rows back.
            Ok(Vec::new())
        }
    }

    fn d1row(pairs: &[(&str, Value)]) -> crate::storage::d1_http::D1Row {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    fn state_no_db() -> WorkspacesRouteState {
        WorkspacesRouteState {
            db: None,
            pat_gate: None,
        }
    }

    fn state_with(db: MockWsD1) -> (WorkspacesRouteState, Arc<MockWsD1>) {
        let db = Arc::new(db);
        (
            WorkspacesRouteState {
                db: Some(db.clone() as Arc<dyn CustomerD1>),
                pat_gate: None,
            },
            db,
        )
    }

    /// One request builder: method + uri + optional tenant + optional write
    /// scope + optional JSON body.
    async fn send(
        state: WorkspacesRouteState,
        method: http::Method,
        uri: &str,
        tenant: Option<&str>,
        scope: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = tenant {
            req = req.header("x-corelink-tenant-id", t);
        }
        if let Some(s) = scope {
            req = req.header(crate::scope::SCOPE_HEADER, s);
        }
        let req = match body {
            Some(v) => req
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let resp = router(state).oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, parsed)
    }

    /// Missing / sentinel tenant fails CLOSED (401) on every route — before any
    /// storage access (and before the write-scope gate).
    #[tokio::test]
    async fn unauthenticated_is_401() {
        let cases = [
            (http::Method::GET, "/v1/customer/workspaces", None),
            (
                http::Method::POST,
                "/v1/customer/workspaces",
                Some(json!({"name": "w"})),
            ),
            (http::Method::DELETE, "/v1/customer/workspaces/abc", None),
            (http::Method::POST, "/v1/customer/workspaces/abc/pin", None),
        ];
        for (m, uri, b) in cases {
            let (missing, _) = send(
                state_no_db(),
                m.clone(),
                uri,
                None,
                Some("cas:rw"),
                b.clone(),
            )
            .await;
            assert_eq!(
                missing,
                StatusCode::UNAUTHORIZED,
                "{m} {uri}: missing tenant"
            );
            let (sentinel, _) = send(
                state_no_db(),
                m.clone(),
                uri,
                Some("_system"),
                Some("cas:rw"),
                b,
            )
            .await;
            assert_eq!(
                sentinel,
                StatusCode::UNAUTHORIZED,
                "{m} {uri}: sentinel tenant"
            );
        }
    }

    /// A read-only (`cas:r`) or scope-less caller cannot mutate workspaces (403),
    /// even with a valid tenant — the create/delete/pin write gate.
    #[tokio::test]
    async fn mutations_require_write_scope() {
        let mutations = [
            (
                http::Method::POST,
                "/v1/customer/workspaces",
                Some(json!({"name": "w"})),
            ),
            (http::Method::DELETE, "/v1/customer/workspaces/abc", None),
            (http::Method::POST, "/v1/customer/workspaces/abc/pin", None),
        ];
        for (m, uri, b) in mutations {
            // Read-only scope → 403.
            let (ro, _) = send(
                state_no_db(),
                m.clone(),
                uri,
                Some("t1"),
                Some("cas:r"),
                b.clone(),
            )
            .await;
            assert_eq!(
                ro,
                StatusCode::FORBIDDEN,
                "{m} {uri}: cas:r must not mutate"
            );
            // No scope header → 403.
            let (none, _) = send(state_no_db(), m.clone(), uri, Some("t1"), None, b).await;
            assert_eq!(
                none,
                StatusCode::FORBIDDEN,
                "{m} {uri}: no scope must not mutate"
            );
        }
    }

    /// Dev/CI (no D1): list is honest-empty (200); mutations fail CLOSED (503) —
    /// never a silent success we cannot persist.
    #[tokio::test]
    async fn dev_no_db_read_empty_write_failclosed() {
        let (s, list) = send(
            state_no_db(),
            http::Method::GET,
            "/v1/customer/workspaces",
            Some("t1"),
            None,
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(list["workspaces"], json!([]));

        let (create, _) = send(
            state_no_db(),
            http::Method::POST,
            "/v1/customer/workspaces",
            Some("t1"),
            Some("cas:rw"),
            Some(json!({"name": "w"})),
        )
        .await;
        assert_eq!(create, StatusCode::SERVICE_UNAVAILABLE);
        let (del, _) = send(
            state_no_db(),
            http::Method::DELETE,
            "/v1/customer/workspaces/abc",
            Some("t1"),
            Some("cas:rw"),
            None,
        )
        .await;
        assert_eq!(del, StatusCode::SERVICE_UNAVAILABLE);
    }

    /// A create with a blank name is a 400 (before any storage write).
    #[tokio::test]
    async fn create_blank_name_is_400() {
        let (state, _) = state_with(MockWsD1::default());
        let (s, _) = send(
            state,
            http::Method::POST,
            "/v1/customer/workspaces",
            Some("t1"),
            Some("cas:rw"),
            Some(json!({"name": "   "})),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }

    /// List projects each row into the FROZEN `CustomerWorkspace` shape and is
    /// tenant-scoped on `?1`.
    #[tokio::test]
    async fn list_projects_rows_tenant_scoped() {
        let (state, db) = state_with(MockWsD1 {
            list_rows: vec![d1row(&[
                ("workspace_id", json!("ws-1")),
                ("name", json!("nightly")),
                ("size_bytes", json!(2048)),
                ("pinned", json!(1)),
                ("created_at_ms", json!(1_700_000_000_000_i64)),
            ])],
            ..MockWsD1::default()
        });
        let (s, body) = send(
            state,
            http::Method::GET,
            "/v1/customer/workspaces",
            Some("tenant-xyz"),
            None,
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let ws = &body["workspaces"][0];
        assert_eq!(ws["workspace_id"], json!("ws-1"));
        assert_eq!(ws["name"], json!("nightly"));
        assert_eq!(ws["size_bytes"], json!(2048));
        assert_eq!(ws["pinned"], json!(true));
        assert!(
            ws["created_at"].as_str().unwrap().starts_with("20"),
            "ISO-8601 created_at"
        );

        let calls = db.calls.lock().unwrap();
        assert_eq!(
            calls[0].1.first(),
            Some(&json!("tenant-xyz")),
            "list is tenant-scoped"
        );
    }

    /// Create mints a workspace, returns 201 + the frozen shape, and binds the
    /// header tenant as `?1` on the INSERT.
    #[tokio::test]
    async fn create_returns_201_tenant_scoped() {
        let (state, db) = state_with(MockWsD1::default());
        let (s, body) = send(
            state,
            http::Method::POST,
            "/v1/customer/workspaces",
            Some("tenant-xyz"),
            Some("cas:rw"),
            Some(json!({"name": "  release  "})),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        assert_eq!(body["name"], json!("release"), "name is trimmed");
        assert_eq!(body["size_bytes"], json!(0));
        assert_eq!(body["pinned"], json!(false));
        assert_eq!(body["workspace_id"].as_str().unwrap().len(), 36, "UUID id");

        let calls = db.calls.lock().unwrap();
        let insert = calls
            .iter()
            .find(|(sql, _)| sql.contains("INSERT INTO workspaces"))
            .unwrap();
        assert_eq!(insert.1[0], json!("tenant-xyz"), "INSERT is tenant-scoped");
    }

    /// Delete is a tenant-scoped `{ ok: true }` (idempotent) with both binds.
    #[tokio::test]
    async fn delete_ok_tenant_scoped() {
        let (state, db) = state_with(MockWsD1::default());
        let (s, body) = send(
            state,
            http::Method::DELETE,
            "/v1/customer/workspaces/ws-9",
            Some("tenant-xyz"),
            Some("cas:rw"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body["ok"], json!(true));
        let calls = db.calls.lock().unwrap();
        let del = calls
            .iter()
            .find(|(sql, _)| sql.contains("DELETE FROM workspaces"))
            .unwrap();
        assert_eq!(del.1[0], json!("tenant-xyz"));
        assert_eq!(del.1[1], json!("ws-9"));
    }

    /// Pin toggles then reads the fresh row back; a present row is 200 + shape.
    #[tokio::test]
    async fn pin_toggles_and_reads_back() {
        let (state, _) = state_with(MockWsD1 {
            readback_row: vec![d1row(&[
                ("workspace_id", json!("ws-1")),
                ("name", json!("nightly")),
                ("size_bytes", json!(0)),
                ("pinned", json!(1)),
                ("created_at_ms", json!(1_700_000_000_000_i64)),
            ])],
            ..MockWsD1::default()
        });
        let (s, body) = send(
            state,
            http::Method::POST,
            "/v1/customer/workspaces/ws-1/pin",
            Some("t1"),
            Some("cas:rw"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(body["workspace_id"], json!("ws-1"));
        assert_eq!(body["pinned"], json!(true));
    }

    /// Pin of a missing id ⇒ 404 (empty read-back).
    #[tokio::test]
    async fn pin_missing_is_404() {
        let (state, _) = state_with(MockWsD1::default());
        let (s, _) = send(
            state,
            http::Method::POST,
            "/v1/customer/workspaces/nope/pin",
            Some("t1"),
            Some("cas:rw"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }
}
