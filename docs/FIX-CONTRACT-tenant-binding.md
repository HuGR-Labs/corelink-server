# FROZEN FIX CONTRACT — container tenant-binding (auth_tenant)

> Security-critical. The lead has pre-decided every interface below; agents
> EXECUTE, they do not redesign. Mirror the Bazel exemplar
> (`routes/bazel_v2.rs`: header-sourced tenant + `instance == header → else 403`).
> axum 0.7. Each agent edits ONLY its assigned file(s). Do NOT compile. Do NOT
> add deps. Cold review by the lead before any merge.

## Threat being closed (verified)
The DO forwards path+query unchanged and only injects
`x-corelink-tenant-id: <PAT-resolved tenant>` as a header. CAS/AC handlers key
isolation off the client-controllable URL `:tenant`; turbo keys off the client
`?teamId=`. Both ignore the header ⇒ any authenticated caller reaches any
tenant. Fix: the **header is the sole isolation tenant**; client-supplied
tenant-shaped values are either validated to equal it (CAS/AC) or demoted to a
non-security label (turbo teamId).

## Artifact 1 — the shared extractor (NEW FILE) — `crates/corelink-container/src/auth_tenant.rs`
Frozen public API (write EXACTLY this behavior):
```rust
//! Authenticated-tenant extractor. The ONLY trustworthy tenant source inside
//! the container is the DO-injected `x-corelink-tenant-id` header (the Worker
//! resolves it from the PAT; the DO forwards path/query unchanged). Fail-CLOSED.
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// The PAT-resolved tenant for this request. Construction is only possible from
/// a concrete, non-sentinel `x-corelink-tenant-id`; handlers that take this as
/// an argument cannot run without an authenticated tenant.
#[derive(Debug, Clone)]
pub struct AuthTenant(pub String);

/// Sentinels the Worker/DO use for non-tenant traffic — never a real tenant.
const SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending", ""];

#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AuthTenant {
    type Rejection = Response;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get("x-corelink-tenant-id")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .unwrap_or("");
        if raw.is_empty() || SENTINELS.contains(&raw) {
            // Fail CLOSED: no authenticated tenant ⇒ deny. Do not leak which.
            return Err((StatusCode::UNAUTHORIZED, "authenticated tenant required").into_response());
        }
        Ok(AuthTenant(raw.to_owned()))
    }
}
```
Also: register the module in `crates/corelink-container/src/lib.rs` (or the crate
root that exposes `routes`) with `pub mod auth_tenant;` — the agent owning this
artifact adds that one line and states where.

Unit tests (same file, `#[cfg(test)]`): build `Parts` via
`axum::http::Request::builder().header(...).body(axum::body::Body::empty())` then
`.into_parts().0`; assert: concrete header → `Ok(AuthTenant(v))`; each sentinel
and missing header → `Err` with `StatusCode::UNAUTHORIZED`. (`#[tokio::test]`.)

## Artifact 2 — CAS route — `crates/corelink-container/src/routes/cas.rs`
Keep the route path `/v1/cas/:tenant/:hash`. In `handle_read` and `handle_write`:
- add the extractor arg `auth: AuthTenant` (i.e. `crate::auth_tenant::AuthTenant`).
- BIND: the path `:tenant` must equal `auth.0`; if not, return **403** (mirror
  Bazel cross-tenant deny) BEFORE any storage access — quote no tenant in the body.
- Use `auth.0` (the authenticated tenant) as BOTH the request tenant and
  caller_tenant passed to the handler (replace the current
  `tenant.clone()`/`tenant` path values). The path tenant is now only a
  client-echo that must match; the isolation key is the authenticated tenant.
- Update the module/handler comments that say "demo … production wiring threads
  the authenticated tenant from a tower middleware" — that wiring is now here.

## Artifact 3 — AC route — `crates/corelink-container/src/routes/ac.rs`
Identical transformation to CAS, on `handle_lookup` and `handle_update`
(route `/v1/ac/:tenant/:action_digest`): add `auth: AuthTenant`, enforce path
`:tenant == auth.0` else **403**, use `auth.0` as request+caller tenant.

## Artifact 4 — Turbo route + bridge — `routes/turbo_v8.rs` (+ `corelink-turbo-bridge`)
Turbo has NO path tenant; `teamId` is a Turborepo team label, NOT the CoreLink
tenant, so they are NOT expected to be equal.
- In `handle_get`/`handle_put` (turbo_v8.rs): add `auth: AuthTenant`. The
  **isolation/storage tenant becomes `auth.0`** (the authenticated tenant), NOT
  `params.team_id`. Pass `auth.0` as `caller_tenant`.
- `teamId` is demoted to a logical SUB-NAMESPACE inside the tenant. Concretely,
  the storage `key` handed to the Cas*Store becomes `format!("{}/{}", team_id, hash)`
  (so two teams under one tenant stay partitioned) while the **tenant** dimension
  is `auth.0`. Confirm the exact threading by reading
  `corelink-turbo-bridge/src/handler.rs` + `adapter.rs`: the `write(tenant, key, …)`
  / `read(tenant, key, …)` calls must receive `tenant = auth.0` and
  `key = "<team_id>/<hash>"`.
- REMOVE the now-wrong tautology: the `team_id == caller_tenant` check
  (handler.rs `check_tenant`) must go (teamId is not the tenant). Isolation is
  now provided solely by `tenant = auth.0`.
- If the bridge request types conflate team_id with the storage tenant, add/ья
  thread a distinct `caller_tenant` field so the route can pass `auth.0`
  separately from `team_id`. Keep the change minimal and quote every edit.

## Out of scope (flag, do not change): npm/pip/cargo/brew/oci/admin/customer
Note in your card whether each (if mounted in the container) takes a path/query
tenant and might share the defect — for a follow-up audit. Do NOT edit them.

## Return shape (every agent — COMPACT card, no code dump)
`<file(s)> · <what changed in 1-2 lines> · <403/401 paths added> · <open questions / risks for lead review>`
