//! `GET /health` end-to-end handler.
//!
//! This handler proves the full D1 + KV binding chain works end-to-end
//! from within a CF Worker. It is intentionally minimal — the goal is
//! to validate the binding surfaces, not to implement production logic.
//!
//! # Request
//!
//! `GET /health`
//!
//! # Response (200 OK)
//!
//! ```json
//! {
//!   "status": "ok",
//!   "kv_value": "2026-05-07T00:00:00Z",
//!   "d1_rowid": 42
//! }
//! ```
//!
//! `kv_value` is the last-seen timestamp stored under key
//! `clerk:health:last_seen` in the `CLERK_JWKS_KV` namespace, or the
//! string `"none"` if the key is absent.
//!
//! `d1_rowid` is the `last_insert_rowid` returned by D1 after
//! inserting a row into the `clerk_audit_health` table.
//!
//! # D1 schema
//!
//! The handler calls `CREATE TABLE IF NOT EXISTS clerk_audit_health ...`
//! on first use so no migration is required for the smoke test:
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS clerk_audit_health (
//!   id      INTEGER PRIMARY KEY AUTOINCREMENT,
//!   ts      TEXT    NOT NULL,
//!   note    TEXT    NOT NULL
//! );
//! ```
//!
//! # Trait surface finding — D1 `last_insert_rowid`
//!
//! `D1ResultMeta.last_row_id` is `Option<i64>`. After an INSERT the
//! field is `Some(row_id)`. We fall back to `0` when the meta is
//! absent (e.g. on a CREATE TABLE that produces no rows).

use serde::Serialize;
// `worker` re-exports `js_sys` and `wasm_bindgen` — use them from there
// so we don't need separate crate entries in Cargo.toml.
use worker::js_sys;
use worker::wasm_bindgen;
use worker::{kv::KvStore, D1Database, Response, Result as WorkerResult};

/// Response body for `GET /health`.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Always `"ok"` on success.
    pub status: &'static str,
    /// Value read from KV, or `"none"` if key absent.
    pub kv_value: String,
    /// Row ID of the D1 audit row inserted, or 0 if unavailable.
    pub d1_rowid: i64,
}

/// Handle a `GET /health` request.
///
/// `kv`  — a `KvStore` obtained from `env.kv("CLERK_JWKS_KV")`.
/// `d1`  — a `D1Database` obtained from `env.d1("CLERK_DB")`.
/// `now` — current timestamp string (RFC 3339 / ISO 8601); callers
///         may pass `js_sys::Date::new_0().to_iso_string().as_string()`
///         but we accept `&str` to avoid a direct `js-sys` dep in tests.
///
/// # Errors
///
/// Returns `worker::Error` on any KV / D1 failure. The Worker handler
/// should map this to a 500 response.
pub async fn handle_health(kv: &KvStore, d1: &D1Database, now: &str) -> WorkerResult<Response> {
    // 1. Read the KV counter / last-seen timestamp.
    let kv_value = kv
        .get("clerk:health:last_seen")
        .text()
        .await?
        .unwrap_or_else(|| "none".to_owned());

    // 2. Write the new timestamp back to KV (no TTL — this is a
    //    persistent health marker, not a JWKS cache slot).
    kv.put("clerk:health:last_seen", now)?
        .execute()
        .await?;

    // 3. Ensure the audit table exists.
    d1.exec(
        "CREATE TABLE IF NOT EXISTS clerk_audit_health \
         (id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, note TEXT NOT NULL)",
    )
    .await?;

    // 4. Insert an audit row.
    let stmt = d1
        .prepare("INSERT INTO clerk_audit_health (ts, note) VALUES (?1, ?2)")
        .bind(&[
            wasm_bindgen::JsValue::from_str(now),
            wasm_bindgen::JsValue::from_str("health-check"),
        ])?;
    let result = stmt.run().await?;

    // 5. Extract last_insert_rowid from meta.
    let d1_rowid = result
        .meta()
        .ok()
        .flatten()
        .and_then(|m| m.last_row_id)
        .unwrap_or(0);

    let body = HealthResponse {
        status: "ok",
        kv_value,
        d1_rowid,
    };
    let json = serde_json::to_string(&body)
        .map_err(|e| worker::Error::RustError(format!("json serialize: {e}")))?;
    Response::ok(json)
}

/// CF Workers `fetch` event entry point.
///
/// Wired via the `worker::event` macro. Routes `GET /health` to
/// [`handle_health`] and returns 404 for all other paths.
///
/// # Bindings expected in wrangler.toml
///
/// - `CLERK_JWKS_KV`: KV namespace binding for JWKS cache + health marker.
/// - `CLERK_DB`: D1 database binding for audit rows.
#[worker::event(fetch)]
pub async fn main(
    req: worker::Request,
    env: worker::Env,
    _ctx: worker::Context,
) -> WorkerResult<worker::Response> {
    let path = req.path();
    if path == "/health" && matches!(req.method(), worker::Method::Get) {
        let kv = env.kv("CLERK_JWKS_KV")?;
        let d1 = env.d1("CLERK_DB")?;
        // R2-10 — instantiate canonical adapters from `corelink-cf-bindings`
        // so the production binding wiring is exercised end-to-end. The
        // adapters wrap the same `worker::*` types we already pulled from
        // `env`, so this is zero-cost (no extra binding lookups). The
        // `corelink-cf-bindings` crate root carries
        // `#![cfg(target_arch = "wasm32")]` so this block is unreachable
        // (and the symbols invisible) on a native build — which is
        // correct because the enclosing `#[worker::event(fetch)]` only
        // runs on wasm32 anyway. We mirror that cfg gate on the use
        // sites so native `cargo build --workspace` does not see the
        // (empty) `corelink_cf_bindings::*` paths.
        //
        // Why no R2 binding in the clerk-cf wrangler.toml: this PoC
        // worker is scoped to JWKS cache + audit. The CAS_BUCKET binding
        // lives in the root `wrangler.toml` (main Worker). The adapter
        // types are still constructible from `env.bucket(...)` there
        // when the main Worker is wired in the next phase.
        #[cfg(target_arch = "wasm32")]
        let _kv_adapter = corelink_cf_bindings::CfKvNamespaceAdapter::new(kv.clone());
        #[cfg(target_arch = "wasm32")]
        let _d1_adapter = corelink_cf_bindings::CfD1DatabaseAdapter::new(
            // D1Database is !Clone — the adapter owns it. We rebuild
            // a second binding handle (cheap) for the adapter so the
            // raw `d1` ref remains usable by `handle_health` below.
            env.d1("CLERK_DB")?,
        );
        // Timestamp: use ISO-8601 from js_sys::Date on wasm, or a
        // placeholder string when running in native tests (not
        // applicable here — this handler only runs on wasm32).
        let now = js_sys::Date::new_0()
            .to_iso_string()
            .as_string()
            .unwrap_or_else(|| "1970-01-01T00:00:00.000Z".to_owned());
        handle_health(&kv, &d1, &now).await
    } else {
        worker::Response::error("not found", 404)
    }
}
