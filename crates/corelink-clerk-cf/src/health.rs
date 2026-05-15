//! `GET /health` end-to-end handler — production wiring on the 4 real
//! CF binding wrappers.
//!
//! This handler exercises all four real-binding adapters
//! (`CfR2BucketReal`, `CfD1DatabaseReal`, `CfKvNamespaceReal`,
//! `CfDurableObjectReal`) from `corelink-cf-bindings`. Each adapter is
//! constructed via [`crate::prod_wiring::build_real_bindings`] at request
//! entry and threaded into this handler as a single [`CfRealBindings`]
//! bundle.
//!
//! # Request
//!
//! `GET /health` with `x-corelink-tenant: <tenant-prefix>` header. The
//! tenant prefix is the JWT-validated principal claim resolved upstream
//! (e.g. by the canonical `corelink-clerk` issuer-bound JWKS verifier).
//!
//! # Response (200 OK)
//!
//! ```json
//! { "status": "ok", "kv_value": "...", "d1_rowid": 42, "r2_present": true, "do_resolved": true }
//! ```
//!
//! # Binding-surface ops exercised
//!
//! - **KV**: `kv.get_bytes("health:last_seen")` → effective key
//!   `<tenant>:health:last_seen`. Then `kv.put_bytes(...)` with no TTL.
//! - **D1**: `INSERT INTO clerk_audit_health (tenant_id, ts, note) VALUES (?1, ?2, ?3)`;
//!   bind `[tenant_id, now, "health-check"]`. The wrapper rejects the
//!   query if `tenant_id` is missing from the column list and rejects
//!   the bind if the first positional parameter is not the anchored
//!   tenant-id (CT-eq check).
//! - **R2**: `head("health/probe")` → effective key `<tenant>/health/probe`.
//!   No write — health probe is read-only on R2 to avoid creating
//!   per-request garbage objects.
//! - **DO**: `stub_by_name("health")` → tenant-scoped name
//!   `tenant:<id>:health`. Resolution is a no-op probe of the namespace
//!   binding; no fetch is issued from the health handler.
//!
//! # D1 schema (one-time migration)
//!
//! Run once via `wrangler d1 execute`:
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS clerk_audit_health (
//!   id        INTEGER PRIMARY KEY AUTOINCREMENT,
//!   tenant_id TEXT    NOT NULL,
//!   ts        TEXT    NOT NULL,
//!   note      TEXT    NOT NULL
//! );
//! ```
//!
//! The CREATE TABLE statement is NOT executed from the per-request hot
//! path — the D1 real wrapper only permits SELECT/UPDATE/DELETE/INSERT
//! verbs, by design.

use serde::Serialize;
#[cfg(target_arch = "wasm32")]
use worker::Response;

use crate::prod_wiring::CfRealBindings;

/// Response body for `GET /health`.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Always `"ok"` on success.
    pub status: &'static str,
    /// Value read from KV (tenant-scoped key), or `"none"` if absent.
    pub kv_value: String,
    /// Row id of the D1 audit row inserted, or `0` if unavailable.
    pub d1_rowid: i64,
    /// `true` if the R2 health probe key currently exists in the bucket.
    pub r2_present: bool,
    /// `true` if the DO stub resolved successfully.
    pub do_resolved: bool,
}

/// Error type for `handle_health`. Wraps the per-binding error variants
/// so the handler can return a single `Result` to the entry point.
///
/// `#[non_exhaustive]` — adding a binding (Queues, Vectorize, ...)
/// must not break downstream pattern-matching consumers.
#[non_exhaustive]
#[derive(Debug)]
pub enum HealthError {
    /// A KV operation failed at the wrapper layer.
    Kv(String),
    /// A D1 operation failed at the wrapper layer.
    D1(String),
    /// An R2 operation failed at the wrapper layer.
    R2(String),
    /// A DO operation failed at the wrapper layer.
    Do(String),
    /// JSON serialisation of the response body failed.
    Serialize(String),
}

impl std::fmt::Display for HealthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kv(m) => write!(f, "kv: {m}"),
            Self::D1(m) => write!(f, "d1: {m}"),
            Self::R2(m) => write!(f, "r2: {m}"),
            Self::Do(m) => write!(f, "do: {m}"),
            Self::Serialize(m) => write!(f, "serialize: {m}"),
        }
    }
}

impl std::error::Error for HealthError {}

/// Handle a `GET /health` request against the 4 real-binding wrappers.
///
/// Every binding access flows through the audit-fenced + tenant-prefix-
/// enforced adapter; no raw `worker::*` lookups happen in this function.
///
/// # Errors
///
/// Returns `HealthError::*` on any binding failure or response
/// serialisation failure. The CF Worker entry point maps this to a
/// `5xx` response.
#[cfg(target_arch = "wasm32")]
pub async fn handle_health_real(
    bindings: &CfRealBindings,
    now: &str,
) -> Result<Response, HealthError> {
    // 1. KV read — tenant-scoped key `<tenant>:health:last_seen`.
    let kv_value_bytes = bindings
        .kv
        .get_bytes("health:last_seen")
        .await
        .map_err(|e| HealthError::Kv(e.to_string()))?;
    let kv_value = match kv_value_bytes {
        Some(bytes) => String::from_utf8(bytes)
            .map_err(|e| HealthError::Kv(format!("utf8: {e}")))?,
        None => "none".to_owned(),
    };

    // 2. KV write — same scoped key, new timestamp. Audit-fenced.
    bindings
        .kv
        .put_bytes("health:last_seen", now.as_bytes(), None)
        .await
        .map_err(|e| HealthError::Kv(e.to_string()))?;

    // 3. R2 head probe — tenant-scoped `<tenant>/health/probe`.
    let r2_present = bindings
        .r2
        .head("health/probe")
        .await
        .map_err(|e| HealthError::R2(e.to_string()))?;

    // 4. D1 insert with explicit tenant_id column + bind anchor.
    let scoped = bindings
        .d1
        .scoped_query(
            "INSERT INTO clerk_audit_health (tenant_id, ts, note) VALUES (?1, ?2, ?3)",
        )
        .map_err(|e| HealthError::D1(e.to_string()))?;
    let stmt = bindings
        .d1
        .prepare(&scoped)
        .map_err(|e| HealthError::D1(e.to_string()))?;
    let tenant_id_param = bindings.d1.tenant().as_str().to_owned();
    let bound = bindings
        .d1
        .bind(stmt, &[tenant_id_param.as_str(), now, "health-check"])
        .map_err(|e| HealthError::D1(e.to_string()))?;
    let result = bindings
        .d1
        .run(&bound)
        .await
        .map_err(|e| HealthError::D1(e.to_string()))?;
    let d1_rowid = result
        .meta()
        .ok()
        .flatten()
        .and_then(|m| m.last_row_id)
        .unwrap_or(0);

    // 5. DO probe — resolve the tenant-scoped `health` stub. We do not
    //    issue a fetch from the health handler (would create traffic on
    //    the actor for every probe); resolution alone exercises the
    //    binding and emits the audit event.
    let do_resolved = match bindings.do_.stub_by_name("health") {
        Ok(_stub) => true,
        Err(e) => return Err(HealthError::Do(e.to_string())),
    };

    let body = HealthResponse {
        status: "ok",
        kv_value,
        d1_rowid,
        r2_present,
        do_resolved,
    };
    let json = serde_json::to_string(&body).map_err(|e| HealthError::Serialize(e.to_string()))?;
    Response::ok(json).map_err(|e| HealthError::Serialize(e.to_string()))
}

/// Native test surface — exercises the same per-binding contract calls
/// against the `stub_for_native_tests` wrappers. The stubs return
/// `*Error::Backend("WasmOnly: ...")` after the validation + audit
/// pre-emission have run, so this function lets tests assert that the
/// full audit/validation contract fires on host CI without the wasm32
/// toolchain.
///
/// Returns the captured "WasmOnly" diagnostic for each surface so the
/// integration test can assert on the contract.
///
/// # Errors
///
/// Returns `HealthError::*` if the wrapper layer rejects the call
/// **before** the WasmOnly stub fires (e.g. tenant-scope validation
/// failure on a probe). Test code uses this to detect cross-tenant
/// rejection scenarios.
#[cfg(not(target_arch = "wasm32"))]
pub async fn handle_health_real(
    bindings: &CfRealBindings,
    now: &str,
) -> Result<HealthProbe, HealthError> {
    // Drive each binding wrapper through its native stub. The stub runs
    // the SAME validation + audit emission as the wasm32 production
    // path before returning `Backend("WasmOnly: …")` — we capture the
    // diagnostic string and assert on it in the integration test.
    //
    // NOTE: the native stubs intentionally return Err on every op
    // (there is no host-side `worker::*` shim). The validation and
    // audit code path runs FIRST, so a captured "WasmOnly" diagnostic
    // is positive evidence the contract fired.

    let kv_get_err = bindings
        .kv
        .get_bytes("health:last_seen")
        .await
        .err()
        .map(|e| e.to_string());

    let kv_put_err = bindings
        .kv
        .put_bytes("health:last_seen", now.as_bytes(), None)
        .await
        .err()
        .map(|e| e.to_string());

    // Use `delete` for the native probe — it is audit-fenced on both
    // wasm32 and native (the `head` native stub only validates prefix
    // and skips audit emission, an asymmetry with the wasm32 path).
    let r2_head_err = bindings
        .r2
        .delete("health/probe")
        .await
        .err()
        .map(|e| e.to_string());

    // D1: scoped_query → prepare → bind. On native the `prepare` stub
    // returns Err(WasmOnly: prepare) AFTER auditing — we accept that
    // as positive evidence of the contract firing and then test `bind`
    // independently against the anchored tenant-id.
    let d1_scope_err = bindings
        .d1
        .scoped_query("INSERT INTO clerk_audit_health (tenant_id, ts, note) VALUES (?1, ?2, ?3)")
        .err()
        .map(|e| e.to_string());
    let d1_prepare_err = match bindings
        .d1
        .scoped_query("INSERT INTO clerk_audit_health (tenant_id, ts, note) VALUES (?1, ?2, ?3)")
    {
        Ok(scoped) => bindings.d1.prepare(&scoped).err().map(|e| e.to_string()),
        Err(_) => None,
    };
    let tenant_id_param = bindings.d1.tenant().as_str().to_owned();
    let d1_bind_err = bindings
        .d1
        .bind(&[tenant_id_param.as_str(), now, "health-check"])
        .err()
        .map(|e| e.to_string());

    let do_resolve_err = bindings
        .do_
        .stub_by_name("health")
        .err()
        .map(|e| e.to_string());

    Ok(HealthProbe {
        kv_get_err,
        kv_put_err,
        r2_head_err,
        d1_scope_err,
        d1_prepare_err,
        d1_bind_err,
        do_resolve_err,
    })
}

/// Native-only diagnostic struct returned by [`handle_health_real`] on
/// the host target. Each field carries the wrapper-layer error string
/// (always `Some("WasmOnly: ...")` when validation/audit succeed; `None`
/// is the success path that only the wasm32 production target reaches).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub struct HealthProbe {
    /// Error captured from `kv.get_bytes` (None = success path).
    pub kv_get_err: Option<String>,
    /// Error captured from `kv.put_bytes` (None = success path).
    pub kv_put_err: Option<String>,
    /// Error captured from `r2.head` (None = success path).
    pub r2_head_err: Option<String>,
    /// Error captured from `d1.scoped_query` (None = success path).
    pub d1_scope_err: Option<String>,
    /// Error captured from `d1.prepare` (None = success path; native
    /// stub always returns `WasmOnly: prepare` after auditing).
    pub d1_prepare_err: Option<String>,
    /// Error captured from `d1.bind` (None = success path; the bind
    /// validator + audit runs even on native).
    pub d1_bind_err: Option<String>,
    /// Error captured from `do_.stub_by_name` (None = success path).
    pub do_resolve_err: Option<String>,
}

/// HTTP header name carrying the JWT-validated tenant identifier.
///
/// Upstream gateway is responsible for verifying the JWT and projecting
/// the principal's tenant claim into this header before the CF Worker
/// dispatches to this handler. This crate trusts the header value
/// (re-validates shape at each binding-anchor boundary — see
/// [`crate::prod_wiring::TenantContext::from_header_value`]).
pub const TENANT_HEADER: &str = "x-corelink-tenant";

/// CF Workers `fetch` event entry point.
///
/// Wired via the `worker::event` macro. Resolves the tenant context from
/// the `x-corelink-tenant` header, builds the four real-binding wrappers
/// via [`crate::prod_wiring::build_real_bindings`], and dispatches
/// `GET /health` to [`handle_health_real`]. Returns 404 for all other
/// paths and 400 if the tenant header is missing / malformed.
///
/// # Bindings expected in wrangler.toml
///
/// - `CAS_BUCKET`     — R2 bucket binding (content-addressed store).
/// - `CLERK_DB`       — D1 database binding (audit rows).
/// - `CLERK_JWKS_KV`  — KV namespace binding (JWKS cache + markers).
/// - `CLERK_DO`       — Durable Object namespace binding (per-tenant actors).
#[cfg(target_arch = "wasm32")]
#[worker::event(fetch)]
pub async fn main(
    req: worker::Request,
    env: worker::Env,
    _ctx: worker::Context,
) -> worker::Result<worker::Response> {
    let path = req.path();
    if !(path == "/health" && matches!(req.method(), worker::Method::Get)) {
        return worker::Response::error("not found", 404);
    }

    // Resolve tenant from the validated header.
    let headers = req.headers();
    let tenant_raw = match headers.get(TENANT_HEADER) {
        Ok(Some(v)) => v,
        _ => {
            return worker::Response::error(
                format!("missing {TENANT_HEADER} header"),
                400,
            );
        }
    };
    let tenant = match crate::prod_wiring::TenantContext::from_header_value(&tenant_raw) {
        Ok(t) => t,
        Err(e) => return worker::Response::error(format!("tenant: {e}"), 400),
    };
    let bindings = match crate::prod_wiring::build_real_bindings(&env, &tenant) {
        Ok(b) => b,
        Err(e) => return worker::Response::error(format!("wiring: {e}"), 500),
    };

    let now = worker::js_sys::Date::new_0()
        .to_iso_string()
        .as_string()
        .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_owned());

    match handle_health_real(&bindings, &now).await {
        Ok(resp) => Ok(resp),
        Err(e) => worker::Response::error(format!("health: {e}"), 500),
    }
}
