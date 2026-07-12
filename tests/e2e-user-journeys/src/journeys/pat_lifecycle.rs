//! PAT-lifecycle journeys (S12) — `/v1/customer/keys` create / list / revoke.
//!
//! Black-box, grounded in the live container route
//! (`crates/corelink-container/src/routes/customer.rs`, captured 2026-06-20):
//!
//!   - `POST /v1/customer/keys` body `{name, scopes[]}` → 201
//!     `{pat:{pat_id,name,scopes,created_at,last_used_at,revoked_at}, token}`
//!   - `GET  /v1/customer/keys` → 200 `{pats:[…], byok}`
//!   - `POST /v1/customer/keys/{pat_id}/revoke` → 200 `{pat:{…,revoked_at}}`
//!
//! Key-MANAGEMENT (create/list/revoke) is an admin-grade op: the route requires
//! the caller to carry cache-write capability (Worker-trusted `x-corelink-scope`
//! / a `cas:rw` PAT / a Clerk dashboard `read-write` session). A read-only
//! (`cas:r`) principal is rejected **403** on every one of the three — this is
//! the rt-nuclear cycle-2 #7 priv-esc / credential-DoS gate.
//!
//! Personas (matrix S12 row):
//! Happy — P1 read+write: create (write scope) → list shows it → revoke → the
//! just-minted PAT, after revoke, is DENIED on a cache call (401/403).
//! Edge — P1: request a NARROWER scope subset (`cache:read` only); the created
//! PAT must carry exactly the requested (subset) scopes, never silently widened.
//! Adv. — P2 read-only: create / list / revoke must each be DENIED 403 (no
//! recon, no mint, no revoke); cross-tenant — tenant B revoking a tenant-A
//! `pat_id` must NOT succeed against A's key (isolation).
//!
//! Uses ONLY the harness URL builders + helpers + personas. No internal crates.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

use crate::harness::{
    bearer, expect_denied, expect_status, sha256_hex, unique_blob, url_cas, url_customer, Config,
    JourneyResult,
};
use crate::personas::Persona;

/// Run the PAT-lifecycle journeys (one per path-class: happy / edge / adversarial).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        happy_create_list_revoke_deny(cfg, client),
        edge_scope_subset(cfg, client),
        adversarial_readonly_and_isolation(cfg, client),
    ]
}

/// URL of the keys collection: `POST`/`GET /v1/customer/keys`.
fn keys_url(cfg: &Config) -> String {
    url_customer(cfg, "keys")
}

/// URL of the revoke action for a given pat id: `POST /v1/customer/keys/{id}/revoke`.
fn revoke_url(cfg: &Config, pat_id: &str) -> String {
    url_customer(cfg, &format!("keys/{pat_id}/revoke"))
}

// ── Happy ─────────────────────────────────────────────────────────────────────

/// Happy path: create a write-scoped PAT → list shows it → revoke it → the newly
/// minted PAT, once revoked, is DENIED on a real cache call. The minted token is
/// returned by the create response, so this is a true end-to-end credential
/// lifecycle: mint → use-able → revoke → no-longer-use-able.
fn happy_create_list_revoke_deny(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "PAT lifecycle: create (write) → list shows it → revoke → revoked PAT denied";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // Management is gated on cache-write capability — drive it with the rw PAT.
    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    // The revoked-PAT-denied step exercises the native CAS surface, which needs
    // the tenant path segment. Gate (don't fail) when it is absent.
    if cfg.tenant.is_none() {
        return JourneyResult::gated(
            name,
            "CORELINK_E2E_TENANT not set — the revoked-PAT cache-deny step needs the tenant segment",
        );
    }
    let mgr = p1.token.expect("P1 always has a token");

    // 1) CREATE a write-scoped PAT.
    let create = match client
        .post(keys_url(cfg))
        .header(AUTHORIZATION, bearer(mgr))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({ "name": "e2e-lifecycle", "scopes": ["cache:read", "cache:write"] }))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST keys: {e}")),
    };
    if let Err(m) = expect_status("POST /v1/customer/keys", create.status().as_u16(), 201) {
        return JourneyResult::fail(name, ms(start), m);
    }
    let created: Value = match create.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("create body not JSON: {e}"))
        }
    };
    let pat_id = match created["pat"]["pat_id"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("create response missing pat.pat_id: {created}"),
            )
        }
    };
    let new_token = match created["token"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => {
            return JourneyResult::fail(
                name,
                ms(start),
                "create response missing plaintext token".to_string(),
            )
        }
    };

    // 2) LIST must now include the just-created pat_id, not yet revoked.
    let list = match client
        .get(keys_url(cfg))
        .header(AUTHORIZATION, bearer(mgr))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET keys: {e}")),
    };
    if let Err(m) = expect_status("GET /v1/customer/keys", list.status().as_u16(), 200) {
        return JourneyResult::fail(name, ms(start), m);
    }
    let listed: Value = match list.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("list body not JSON: {e}")),
    };
    let pats = match listed["pats"].as_array() {
        Some(a) => a,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("list response missing 'pats' array: {listed}"),
            )
        }
    };
    let row = pats.iter().find(|p| p["pat_id"].as_str() == Some(&pat_id));
    match row {
        Some(r) if !r["revoked_at"].is_null() => {
            return JourneyResult::fail(
                name,
                ms(start),
                "newly-created PAT already shows revoked_at in list".to_string(),
            )
        }
        Some(_) => {}
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("list does not contain just-created pat_id {pat_id}"),
            )
        }
    }

    // 3) REVOKE it.
    let revoke = match client
        .post(revoke_url(cfg, &pat_id))
        .header(AUTHORIZATION, bearer(mgr))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST revoke: {e}")),
    };
    if let Err(m) = expect_status(
        "POST /v1/customer/keys/{id}/revoke",
        revoke.status().as_u16(),
        200,
    ) {
        return JourneyResult::fail(name, ms(start), m);
    }
    let revoked: Value = match revoke.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("revoke body not JSON: {e}"))
        }
    };
    if revoked["pat"]["revoked_at"].is_null() {
        return JourneyResult::fail(
            name,
            ms(start),
            "revoke response did not set pat.revoked_at".to_string(),
        );
    }

    // 4) The just-revoked PAT must be DENIED on a real cache call. We probe the
    //    native CAS GET surface with the revoked plaintext token — a revoked
    //    credential must not authenticate (401/403). A 200/404 here would mean
    //    the revoke did not take effect on the cache plane.
    let probe_hash = sha256_hex(&unique_blob("e2e-revoked-probe"));
    let cas_url = url_cas(cfg, &p1.tenant, &probe_hash);
    let after = match client
        .get(&cas_url)
        .header(AUTHORIZATION, bearer(&new_token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("revoked-PAT probe: {e}")),
    };
    let after_status = after.status().as_u16();
    // A revoked PAT must be rejected at auth (401/403). 404 is NOT acceptable
    // here — that would mean the token authenticated and merely missed.
    if !matches!(after_status, 401 | 403) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "revoked PAT still authenticated: cache GET returned {after_status} (expected 401/403)"
            ),
        );
    }

    JourneyResult::pass(name, ms(start))
}

// ── Edge ────────────────────────────────────────────────────────────────────

/// Scope subset: request a NARROWER scope set (`cache:read` only) than the
/// manager holds. The minted PAT must carry exactly the requested (subset)
/// scopes — never silently widened to the caller's full capability.
fn edge_scope_subset(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "PAT lifecycle (edge): scope subset — request cache:read only → minted scopes == request";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let mgr = p1.token.expect("P1 always has a token");

    let requested = ["cache:read"];
    let create = match client
        .post(keys_url(cfg))
        .header(AUTHORIZATION, bearer(mgr))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({ "name": "e2e-scope-subset", "scopes": requested }))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST keys: {e}")),
    };
    if let Err(m) = expect_status(
        "POST /v1/customer/keys (subset)",
        create.status().as_u16(),
        201,
    ) {
        return JourneyResult::fail(name, ms(start), m);
    }
    let body: Value = match create.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("create body not JSON: {e}"))
        }
    };

    let minted = match body["pat"]["scopes"].as_array() {
        Some(a) => a
            .iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect::<Vec<_>>(),
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("create response missing pat.scopes array: {body}"),
            )
        }
    };

    // The minted scopes must NOT exceed the requested subset. A write scope
    // appearing here would be a silent privilege widening.
    let widened: Vec<&String> = minted
        .iter()
        .filter(|s| !requested.contains(&s.as_str()))
        .collect();
    if !widened.is_empty() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("minted PAT widened beyond requested {requested:?}: extra {widened:?}"),
        );
    }
    if !minted.iter().any(|s| s == "cache:read") {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("minted PAT dropped the requested cache:read scope: {minted:?}"),
        );
    }

    // Best-effort cleanup: revoke the edge key so we don't leak credentials.
    if let Some(id) = body["pat"]["pat_id"].as_str() {
        let _ = client
            .post(revoke_url(cfg, id))
            .header(AUTHORIZATION, bearer(mgr))
            .send();
    }

    JourneyResult::pass(name, ms(start))
}

// ── Adversarial ───────────────────────────────────────────────────────────────

/// Adversarial deny journey:
///   (a) P5 read-only: create / list / revoke must each be DENIED 403 — a
///       read-only cache token cannot recon, mint, or revoke credentials
///       (rt-nuclear cycle-2 #7 priv-esc / credential-DoS).
///   (b) Cross-tenant isolation: tenant B attempting to revoke a tenant-A
///       `pat_id` must NOT succeed against A's key — the route is tenant-scoped,
///       so B's call operates only in B's namespace (deny / not-found), never
///       touching A's credential.
fn adversarial_readonly_and_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "PAT lifecycle (adversarial): read-only create/list/revoke → 403; cross-tenant revoke isolated";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    // (a) Read-only persona — every management op must be denied 403.
    let ro = match Persona::P2ReadOnly.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let ro_token = ro.token.expect("P2 always has a token");

    // create → 403
    let ro_create = match client
        .post(keys_url(cfg))
        .header(AUTHORIZATION, bearer(ro_token))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({ "name": "e2e-ro-evil", "scopes": ["cache:write"] }))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("RO create probe: {e}")),
    };
    if let Err(m) = expect_status("RO create (must deny)", ro_create.status().as_u16(), 403) {
        return JourneyResult::fail(name, ms(start), m);
    }

    // list → 403
    let ro_list = match client
        .get(keys_url(cfg))
        .header(AUTHORIZATION, bearer(ro_token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("RO list probe: {e}")),
    };
    if let Err(m) = expect_status("RO list (must deny)", ro_list.status().as_u16(), 403) {
        return JourneyResult::fail(name, ms(start), m);
    }

    // revoke (some id) → 403
    let ro_revoke = match client
        .post(revoke_url(cfg, "any-pat-id"))
        .header(AUTHORIZATION, bearer(ro_token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("RO revoke probe: {e}")),
    };
    if let Err(m) = expect_status("RO revoke (must deny)", ro_revoke.status().as_u16(), 403) {
        return JourneyResult::fail(name, ms(start), m);
    }

    // (b) Cross-tenant isolation. Needs a write-capable PAT for tenant A (to
    // actually mint a victim key) AND a tenant-B PAT. Gate (don't fail) if
    // either prerequisite is absent — this sub-check is genuinely creds-bound.
    let a = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(_) => {
            // The read-only deny block already proved the core gate; record the
            // isolation half as gated rather than failing for missing creds.
            return JourneyResult::gated(
                name,
                "read-only deny verified; cross-tenant revoke skipped — P1 (tenant A rw) token absent",
            );
        }
    };
    let b = match Persona::P6TenantB.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => {
            return JourneyResult::gated(
                name,
                format!("read-only deny verified; cross-tenant revoke skipped — {reason}"),
            )
        }
    };
    let a_token = a.token.expect("P1 has a token");
    let b_token = b.token.expect("P6 has a token");

    // Tenant A mints a victim key in A's namespace.
    let mint = match client
        .post(keys_url(cfg))
        .header(AUTHORIZATION, bearer(a_token))
        .header(CONTENT_TYPE, "application/json")
        .json(&json!({ "name": "e2e-victim", "scopes": ["cache:read"] }))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("A mint victim: {e}")),
    };
    if let Err(m) = expect_status("A mint victim key", mint.status().as_u16(), 201) {
        return JourneyResult::fail(name, ms(start), m);
    }
    let victim: Value = match mint.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("victim body not JSON: {e}"))
        }
    };
    let victim_id = match victim["pat"]["pat_id"].as_str() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => return JourneyResult::fail(name, ms(start), "victim mint missing pat_id".to_string()),
    };

    // Tenant B tries to revoke tenant A's pat_id. The route is tenant-scoped:
    // B's call addresses B's namespace, so it must NOT revoke A's key. A 200 is
    // only acceptable if it is a no-op against B's (empty) namespace, never a
    // success that flips A's revoked_at — but a clean deny (403/404) is the
    // expected, unambiguous outcome.
    let cross = match client
        .post(revoke_url(cfg, &victim_id))
        .header(AUTHORIZATION, bearer(b_token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("B cross-revoke: {e}")),
    };
    let cross_status = cross.status().as_u16();
    if cross_status == 200 {
        // A 200 must NOT correspond to A's actual key. Verify A's key is still
        // un-revoked by listing as A — if it shows revoked_at, isolation broke.
        let recheck = client
            .get(keys_url(cfg))
            .header(AUTHORIZATION, bearer(a_token))
            .send();
        let leaked = match recheck.and_then(|r| r.json::<Value>()) {
            Ok(v) => v["pats"]
                .as_array()
                .map(|pats| {
                    pats.iter()
                        .find(|p| p["pat_id"].as_str() == Some(&victim_id))
                        .map(|p| !p["revoked_at"].is_null())
                        .unwrap_or(false)
                })
                .unwrap_or(false),
            Err(_) => true, // can't prove safety → treat as failure
        };
        // cleanup A's victim key regardless.
        let _ = client
            .post(revoke_url(cfg, &victim_id))
            .header(AUTHORIZATION, bearer(a_token))
            .send();
        if leaked {
            return JourneyResult::fail(
                name,
                ms(start),
                "ISOLATION: tenant B's revoke flipped tenant A's key revoked_at — cross-tenant credential mutation",
            );
        }
    } else if let Err(m) = expect_denied("B cross-tenant revoke of A's key", cross_status) {
        // cleanup A's victim key before returning.
        let _ = client
            .post(revoke_url(cfg, &victim_id))
            .header(AUTHORIZATION, bearer(a_token))
            .send();
        return JourneyResult::fail(name, ms(start), m);
    }

    // Best-effort cleanup of the victim key.
    let _ = client
        .post(revoke_url(cfg, &victim_id))
        .header(AUTHORIZATION, bearer(a_token))
        .send();

    JourneyResult::pass(name, ms(start))
}
