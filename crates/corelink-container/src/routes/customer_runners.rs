//! Customer self-serve **Runners** READ routes: `/v1/customer/runners/*`.
//!
//! Backs the admin-ui Runners screen (W9), whose client method
//! `getRunnerEntitlement` currently throws `NotWiredError("BE-10 …")` so every
//! panel renders a teaching `EmptyState`. These endpoints expose the tenant's
//! runner **entitlement**, **repo allowlist**, and **recent runs** as honest,
//! tenant-scoped reads over the ALREADY-DEPLOYED D1 tables — no new migration.
//!
//! # Endpoints (all tenant-scoped GET, fail-CLOSED)
//!
//! ```text
//! GET /v1/customer/runners/entitlement  → CustomerRunnerEntitlement (FE type)
//! GET /v1/customer/runners/allowlist    → { repos: string[] }
//! GET /v1/customer/runners/runs         → { runs: [] }  (honest empty — no runs table)
//! ```
//!
//! # Auth model
//!
//! Identical to [`super::customer`]: the tenant is DERIVED from the
//! Worker-injected `x-corelink-tenant-id` header (never a path/body param); a
//! missing/sentinel tenant fails CLOSED (401). The native-PAT possession
//! backstop (`pat_gate`) re-verifies a bearer PAT against the claimed tenant
//! (skipped for edge-verified Clerk-session callers and in dev/CI). Reads are
//! non-financial operational data, so — like `GET /v1/customer/team` — they
//! carry no additional cache-write / billing-PII scope gate.
//!
//! # Data sources (READ-ONLY; no writes, no new tables)
//!
//! | field / route          | table (migration) |
//! |------------------------|-------------------|
//! | `max_concurrency`, `max_vcpu_h`, `sku` | `runners_entitlement` (0070/0072) |
//! | `sku` (preferred)      | `runner_billing` (0087) — the Stripe tier label |
//! | `repo_allowlist`, `/allowlist` | `runner_repo_allowlist` (0085) |
//! | `consumed_vcpu_h`      | [stub] `0` — no per-run consumption table exists |
//! | `/runs`               | [honest empty] — no runner-runs table exists |
//!
//! # Pattern
//!
//! Mirrors [`super::customer`]: one route-state struct holding the D1 query
//! seam ([`crate::customer_d1::CustomerD1`]) + the optional `pat_gate`, one
//! `build_state_from_env()` factory (fail-CLOSED to honest-empty in dev/CI when
//! the D1 env is absent), one `router(state)`.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};

use crate::customer_d1::CustomerD1;
use crate::storage::d1_http::D1Row;

// ─── Route state ─────────────────────────────────────────────────────────────

/// Shared route state for the customer Runners READ surface.
///
/// `db` is the D1 query seam ([`crate::customer_d1::CustomerD1`]); `None` in
/// dev/CI when the storage env is absent — the handlers then return an honest
/// **not-entitled / empty** response (200) rather than 500, mirroring how
/// `customer::build_handlers_from_env` falls back for dev/CI. `pat_gate` is the
/// native-PAT possession backstop, wired by `routes.rs` exactly as it wires
/// `customer_state.pat_gate` (`None` here keeps the factory env-pure).
#[derive(Clone)]
pub struct CustomerRunnersRouteState {
    /// D1 row source (`None` in dev/CI ⇒ honest-empty responses).
    pub db: Option<Arc<dyn CustomerD1>>,
    /// Native PAT possession backstop; `Some` in production (wired by
    /// `routes.rs`), `None` in dev/CI (skipped). Mirrors
    /// [`super::customer::CustomerRouteState::pat_gate`].
    pub pat_gate: Option<Arc<crate::native_pat_gate::NativePatGate>>,
}

impl core::fmt::Debug for CustomerRunnersRouteState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CustomerRunnersRouteState")
            .finish_non_exhaustive()
    }
}

// ─── Handler factory ─────────────────────────────────────────────────────────

/// Build the production `CustomerRunnersRouteState` from process env.
///
/// When the D1 config ([`crate::storage::StorageEnv`]) is present the handlers
/// read the live `runners_entitlement` / `runner_repo_allowlist` /
/// `runner_billing` tables; otherwise dev/CI gets a `None` db and the handlers
/// return honest not-entitled / empty responses. Mirrors
/// [`super::customer::build_handlers_from_env`]'s env-gate. `pat_gate` is left
/// `None` and wired by `routes.rs` (same as the customer state).
#[must_use]
pub fn build_state_from_env() -> CustomerRunnersRouteState {
    let db = crate::customer_d1::D1HttpCustomerDb::from_env()
        .map(|d| Arc::new(d) as Arc<dyn CustomerD1>);
    if db.is_none() {
        tracing::warn!(
            "StorageEnv unset/invalid; /v1/customer/runners/* returns honest \
             not-entitled/empty (dev/CI mode)"
        );
    }
    CustomerRunnersRouteState { db, pat_gate: None }
}

// ─── Router ──────────────────────────────────────────────────────────────────

/// Build the axum `Router` mounting the `/v1/customer/runners/*` READ routes.
pub fn router(state: CustomerRunnersRouteState) -> Router {
    Router::new()
        .route("/v1/customer/runners/entitlement", get(handle_entitlement))
        .route("/v1/customer/runners/allowlist", get(handle_allowlist))
        .route("/v1/customer/runners/runs", get(handle_runs))
        .with_state(state)
}

// ─── Auth helpers (mirror `super::customer`) ─────────────────────────────────

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant.
/// Mirrors `customer::TENANT_SENTINELS` / `auth_tenant::AuthTenant`.
const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];

/// The `x-corelink-token-prefix` the Worker stamps for a Clerk-session caller
/// (already edge-verified, carries no bearer — the PAT backstop is skipped for
/// it). Mirrors `customer::CLERK_TOKEN_PREFIX`.
const CLERK_TOKEN_PREFIX: &str = "clerk";

/// Read the authenticated `x-corelink-tenant-id`, **fail-CLOSED**: a
/// missing/empty/sentinel value is an `Err(())` the handler maps to 401.
/// Mirrors `customer::tenant`.
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

/// Read `x-corelink-token-prefix`; fail-CLOSED to `"_unknown"`. Mirrors
/// `customer::principal`.
fn principal(headers: &HeaderMap) -> String {
    headers
        .get("x-corelink-token-prefix")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "_unknown".to_owned())
}

/// Run the native PAT possession backstop when wired, EXCEPT for Clerk-session
/// callers. `Some(resp)` ⇒ REJECT; `None` ⇒ proceed. Mirrors
/// `customer::pat_gate_reject` (called AFTER the fail-CLOSED tenant resolution,
/// BEFORE any storage access).
async fn pat_gate_reject(
    state: &CustomerRunnersRouteState,
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

/// Canonical fail-CLOSED 500 for a D1 / transport fault (never a silent empty).
fn d1_fault(context: &str, err: &str, tenant: &str) -> axum::response::Response {
    tracing::error!(error = %err, tenant = %tenant, "customer runners: {context}");
    (StatusCode::INTERNAL_SERVER_ERROR, "runner read failed").into_response()
}

// ─── D1 column helpers ───────────────────────────────────────────────────────

/// Read a nullable string column from a row (mirrors `customer_d1`'s helper).
fn col_str(row: &D1Row, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Read a non-negative integer column as `u64`, clamping a negative / oversized
/// value to `0` (out-of-contract rows never panic; the FE fields are `number`).
fn col_u64(row: &D1Row, key: &str) -> u64 {
    row.get(key)
        .and_then(Value::as_i64)
        .filter(|v| *v >= 0)
        .map_or(0, |v| u64::try_from(v).unwrap_or(0))
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// `GET /v1/customer/runners/entitlement` → [`CustomerRunnerEntitlement`] shape.
///
/// Returns EXACTLY the admin-ui `CustomerRunnerEntitlement` type
/// (`apps/admin-ui/src/lib/customer-types.ts`):
/// `{ sku, max_concurrency, max_vcpu_h, consumed_vcpu_h, install_status,
/// repo_allowlist }`.
///
/// Data mapping (READ-ONLY):
/// - `max_concurrency` / `max_vcpu_h` ← `runners_entitlement` (0070/0072); a
///   missing entitlement row ⇒ the honest **not-entitled** state
///   (`sku:null, max_concurrency:0, max_vcpu_h:0, install_status:"not_installed"`)
///   — never a 500.
/// - `sku` ← `runner_billing.plan` (the Stripe tier label, 0087) preferred,
///   falling back to `runners_entitlement.plan`; `null` when neither exists.
/// - `install_status` ← `"installed"` when an entitlement row exists (the honest
///   proxy available from the allowed tables — the container has no GitHub-App
///   install table to read here), else `"not_installed"`.
/// - `repo_allowlist` ← `runner_repo_allowlist` (0085).
/// - `consumed_vcpu_h` ← `0` [stub]: no per-run consumption table exists yet.
async fn handle_entitlement(
    State(state): State<CustomerRunnersRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }

    // Dev/CI (no D1) → honest not-entitled state (200, never 500).
    let Some(db) = state.db.as_ref() else {
        return (StatusCode::OK, Json(not_entitled(&[]))).into_response();
    };

    // 1) Entitlement row (concurrency + vCPU-h ceiling + informational plan).
    let ent_rows = match db.query(
        "SELECT max_concurrency, max_vcpu_h, plan FROM runners_entitlement \
         WHERE tenant_id = ?1 LIMIT 1",
        vec![json!(t)],
    ) {
        Ok(rows) => rows,
        Err(e) => return d1_fault("entitlement lookup failed", &e, &t),
    };

    // 2) Billing tier label (the customer-facing SKU), newest row.
    let billing_rows = match db.query(
        "SELECT plan FROM runner_billing WHERE tenant_id = ?1 \
         ORDER BY updated_at_ms DESC LIMIT 1",
        vec![json!(t)],
    ) {
        Ok(rows) => rows,
        Err(e) => return d1_fault("runner_billing lookup failed", &e, &t),
    };

    // 3) Repo allowlist.
    let repos = match allowlist_repos(db.as_ref(), &t) {
        Ok(repos) => repos,
        Err(e) => return d1_fault("allowlist lookup failed", &e, &t),
    };

    let Some(ent) = ent_rows.first() else {
        // No entitlement row → not entitled, but still surface any allowlist.
        return (StatusCode::OK, Json(not_entitled(&repos))).into_response();
    };

    let max_concurrency = col_u64(ent, "max_concurrency");
    let max_vcpu_h = col_u64(ent, "max_vcpu_h");
    // Prefer the Stripe tier label; fall back to the entitlement's own plan.
    let sku = billing_rows
        .first()
        .and_then(|r| col_str(r, "plan"))
        .or_else(|| col_str(ent, "plan"));

    let body = json!({
        "sku": sku,                       // string | null
        "max_concurrency": max_concurrency,
        "max_vcpu_h": max_vcpu_h,
        "consumed_vcpu_h": 0,             // [stub] no per-run consumption table
        "install_status": "installed",    // entitlement row present
        "repo_allowlist": repos,
    });
    (StatusCode::OK, Json(body)).into_response()
}

/// The honest **not-entitled** [`CustomerRunnerEntitlement`] value (no
/// `runners_entitlement` row): zeroed caps, `null` sku, `not_installed`. Any
/// `repos` allowlisted independently of an entitlement are still surfaced.
fn not_entitled(repos: &[String]) -> Value {
    json!({
        "sku": Value::Null,
        "max_concurrency": 0,
        "max_vcpu_h": 0,
        "consumed_vcpu_h": 0,
        "install_status": "not_installed",
        "repo_allowlist": repos,
    })
}

/// `GET /v1/customer/runners/allowlist` → `{ repos: string[] }` from
/// `runner_repo_allowlist` (0085), tenant-scoped.
async fn handle_allowlist(
    State(state): State<CustomerRunnersRouteState>,
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
        return (StatusCode::OK, Json(json!({ "repos": [] }))).into_response();
    };
    match allowlist_repos(db.as_ref(), &t) {
        Ok(repos) => (StatusCode::OK, Json(json!({ "repos": repos }))).into_response(),
        Err(e) => d1_fault("allowlist lookup failed", &e, &t),
    }
}

/// Read the tenant's allowlisted `repo_full_name`s (ordered), fail-CLOSED on a
/// D1 fault. Shared by the entitlement + allowlist handlers.
fn allowlist_repos(db: &dyn CustomerD1, tenant: &str) -> Result<Vec<String>, String> {
    let rows = db.query(
        "SELECT repo_full_name FROM runner_repo_allowlist WHERE tenant_id = ?1 \
         ORDER BY repo_full_name",
        vec![json!(tenant)],
    )?;
    Ok(rows
        .iter()
        .filter_map(|r| col_str(r, "repo_full_name"))
        .collect())
}

/// `GET /v1/customer/runners/runs` → `{ runs: [] }`.
///
/// **Honest empty**: no runner-runs / runner-jobs table exists in D1 today
/// (verified against `migrations/d1/`), so there is no run history to surface.
/// The endpoint returns an empty list (never a fabricated run) so the FE renders
/// a teaching EmptyState rather than throwing `NotWiredError`. When a real runs
/// table lands, this handler reads it; the shape is stable.
async fn handle_runs(
    State(state): State<CustomerRunnersRouteState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let t = match tenant(&headers) {
        Ok(t) => t,
        Err(()) => return unauthenticated_tenant(),
    };
    if let Some(resp) = pat_gate_reject(&state, &t, &headers).await {
        return resp;
    }
    (StatusCode::OK, Json(json!({ "runs": [] }))).into_response()
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

    /// Hermetic D1 mock: canned rows keyed by an SQL fragment; records every
    /// (sql, binds) call so a test can assert the tenant-scoping bind (`?1`).
    #[derive(Debug, Default)]
    struct MockRunnersD1 {
        entitlement_rows: Vec<D1Row>,
        billing_rows: Vec<D1Row>,
        allowlist_rows: Vec<D1Row>,
        calls: Mutex<Vec<(String, Vec<Value>)>>,
    }

    impl CustomerD1 for MockRunnersD1 {
        fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
            self.calls.lock().unwrap().push((sql.to_owned(), binds));
            if sql.contains("FROM runners_entitlement") {
                return Ok(self.entitlement_rows.clone());
            }
            if sql.contains("FROM runner_billing") {
                return Ok(self.billing_rows.clone());
            }
            if sql.contains("FROM runner_repo_allowlist") {
                return Ok(self.allowlist_rows.clone());
            }
            Ok(Vec::new())
        }
    }

    fn d1row(pairs: &[(&str, Value)]) -> D1Row {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
    }

    fn state_no_db() -> CustomerRunnersRouteState {
        CustomerRunnersRouteState { db: None, pat_gate: None }
    }

    fn state_with(db: MockRunnersD1) -> (CustomerRunnersRouteState, Arc<MockRunnersD1>) {
        let db = Arc::new(db);
        (
            CustomerRunnersRouteState {
                db: Some(db.clone() as Arc<dyn CustomerD1>),
                pat_gate: None,
            },
            db,
        )
    }

    async fn get(state: CustomerRunnersRouteState, uri: &str, tenant: Option<&str>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(http::Method::GET).uri(uri);
        if let Some(t) = tenant {
            req = req.header("x-corelink-tenant-id", t);
        }
        let resp = router(state)
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, body)
    }

    /// Missing / sentinel tenant fails CLOSED (401) on every read — before any
    /// storage access.
    #[tokio::test]
    async fn unauthenticated_is_401() {
        for uri in [
            "/v1/customer/runners/entitlement",
            "/v1/customer/runners/allowlist",
            "/v1/customer/runners/runs",
        ] {
            let (missing, _) = get(state_no_db(), uri, None).await;
            assert_eq!(missing, StatusCode::UNAUTHORIZED, "{uri}: missing tenant");
            let (sentinel, _) = get(state_no_db(), uri, Some("_anonymous")).await;
            assert_eq!(sentinel, StatusCode::UNAUTHORIZED, "{uri}: sentinel tenant");
        }
    }

    /// Dev/CI (no D1) → honest not-entitled entitlement + empty allowlist/runs
    /// (200, never 500).
    #[tokio::test]
    async fn dev_no_db_is_honest_empty() {
        let (s, ent) = get(state_no_db(), "/v1/customer/runners/entitlement", Some("t1")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(ent["sku"], Value::Null);
        assert_eq!(ent["max_concurrency"], json!(0));
        assert_eq!(ent["install_status"], json!("not_installed"));
        assert_eq!(ent["repo_allowlist"], json!([]));

        let (s, al) = get(state_no_db(), "/v1/customer/runners/allowlist", Some("t1")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(al["repos"], json!([]));

        let (s, runs) = get(state_no_db(), "/v1/customer/runners/runs", Some("t1")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(runs["runs"], json!([]));
    }

    /// A real entitlement row projects to the FROZEN FE shape, prefers the
    /// Stripe billing SKU, and every query is tenant-scoped on `?1`.
    #[tokio::test]
    async fn entitlement_maps_rows_and_is_tenant_scoped() {
        let (state, db) = state_with(MockRunnersD1 {
            entitlement_rows: vec![d1row(&[
                ("max_concurrency", json!(4)),
                ("max_vcpu_h", json!(100)),
                ("plan", json!("runners_entitlement_fallback")),
            ])],
            billing_rows: vec![d1row(&[("plan", json!("runner_pro"))])],
            allowlist_rows: vec![
                d1row(&[("repo_full_name", json!("acme/api"))]),
                d1row(&[("repo_full_name", json!("acme/web"))]),
            ],
            ..MockRunnersD1::default()
        });
        let (s, ent) = get(state, "/v1/customer/runners/entitlement", Some("tenant-xyz")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(ent["sku"], json!("runner_pro"), "prefers the Stripe billing SKU");
        assert_eq!(ent["max_concurrency"], json!(4));
        assert_eq!(ent["max_vcpu_h"], json!(100));
        assert_eq!(ent["consumed_vcpu_h"], json!(0));
        assert_eq!(ent["install_status"], json!("installed"));
        assert_eq!(ent["repo_allowlist"], json!(["acme/api", "acme/web"]));

        // INV-TENANT-ISOLATION: every statement binds the header tenant as ?1.
        let calls = db.calls.lock().unwrap();
        assert!(!calls.is_empty());
        for (_, binds) in calls.iter() {
            assert_eq!(binds.first(), Some(&json!("tenant-xyz")), "each query is tenant-scoped");
        }
    }

    /// No entitlement row → honest not-entitled, but an independently-allowlisted
    /// repo set is still surfaced.
    #[tokio::test]
    async fn no_entitlement_row_surfaces_allowlist() {
        let (state, _) = state_with(MockRunnersD1 {
            allowlist_rows: vec![d1row(&[("repo_full_name", json!("acme/api"))])],
            ..MockRunnersD1::default()
        });
        let (s, ent) = get(state, "/v1/customer/runners/entitlement", Some("t1")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(ent["sku"], Value::Null);
        assert_eq!(ent["install_status"], json!("not_installed"));
        assert_eq!(ent["repo_allowlist"], json!(["acme/api"]));
    }
}
