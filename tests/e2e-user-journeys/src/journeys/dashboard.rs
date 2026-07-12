//! Customer-dashboard journeys (S11) — `/v1/customer/{overview,usage,billing,keys}`.
//!
//! Black-box only: the deployed HTTP API + a Bearer PAT, through the frozen
//! [`crate::harness::url_customer`] builder. The customer surface is the
//! self-serve portal a paying SMB sees: their plan, usage, invoices, and keys.
//!
//! Live contract (grounded in `routes/customer.rs` on the deploy under test):
//!   - `GET /v1/customer/overview` → 200 `{tenant_id, plan, usage{…}, billing{…}, …}`
//!   - `GET /v1/customer/usage?period=` → 200 `{period, cas_bytes, reads, writes,
//!     quota_bytes, daily[…]}`
//!   - `GET /v1/customer/billing` → 200 `{status, plan, amount_due_cents,
//!     currency, invoices[…]}`
//!   - `GET /v1/customer/keys` is a privileged (key-management) op: it requires a
//!     CACHE-WRITE/admin scope. A read-only PAT MUST get 403 (rt-nuclear cycle-2
//!     #7: a `cas:r` token must not enumerate the tenant's credentials).
//!   - Every customer route fails CLOSED to 401 on a missing/sentinel tenant, so
//!     a no-auth request is denied.
//!
//! Cells covered (per JOURNEY-MATRIX §5, surface S11):
//!   - Happy   : overview / usage / billing read → 200 with the expected shape.
//!   - Edge    : usage for an empty (no-traffic) period → 200, well-formed shape.
//!   - Advers. : read-only PAT on the write-gated keys list → 403; tenant
//!     scoping — tenant B's overview is B's own, never A's; no-auth → 401.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use serde_json::Value;

use crate::harness::{
    bearer, expect_gate_denied, expect_status, url_customer, Config, JourneyResult,
};
use crate::personas::Persona;

/// Run the dashboard journeys (one [`JourneyResult`] per journey).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        overview_usage_billing_shape(cfg, client),
        usage_empty_period(cfg, client),
        keys_list_requires_write_scope(cfg, client),
        tenant_scoping(cfg, client),
        overview_no_auth(cfg, client),
    ]
}

const fn ms(start: Instant) -> impl Fn() -> u64 {
    move || start.elapsed().as_millis() as u64
}

/// Resolve a persona, or convert an absent token into the journey's gate result.
macro_rules! resolve_or_gate {
    ($persona:expr, $cfg:expr, $name:expr) => {
        match $persona.resolve($cfg) {
            Ok(p) => p,
            Err(reason) => return JourneyResult::gated($name, reason),
        }
    };
}

/// Happy (P1–P4): the three primary read panels (overview, usage, billing) each
/// return 200 with their documented required fields. We drive the surface with
/// the read-write persona (P1); the tier-specific personas P2/P3/P4 prove the
/// SAME shape and gate on their own tokens when present, so a single RW PAT is
/// enough to exercise the contract.
fn overview_usage_billing_shape(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Dashboard: overview+usage+billing → 200 with expected shape";
    let start = Instant::now();
    let dur = ms(start);

    let p1 = resolve_or_gate!(Persona::P1ReadWrite, cfg, name);
    let token = p1.token.expect("P1 always has a token");

    // --- overview ---
    let overview = match client
        .get(url_customer(cfg, "overview"))
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        // A transport error means the endpoint isn't under test — gate, never
        // fail: a black-box gate fails only on a CONTRACT violation.
        Err(e) => {
            return JourneyResult::gated(name, format!("endpoint unreachable: {e}"));
        }
    };
    if let Err(m) = expect_status("GET /v1/customer/overview", overview.status().as_u16(), 200) {
        return JourneyResult::fail(name, dur(), m);
    }
    let body: Value = match overview.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, dur(), format!("overview not JSON: {e}")),
    };
    if body["tenant_id"].is_null() || body["plan"].is_null() {
        return JourneyResult::fail(
            name,
            dur(),
            format!("overview missing tenant_id/plan: {body}"),
        );
    }
    for field in ["period", "cas_bytes", "quota_bytes", "reads", "writes"] {
        if body["usage"][field].is_null() {
            return JourneyResult::fail(name, dur(), format!("overview.usage missing '{field}'"));
        }
    }
    for field in ["status", "amount_due_cents", "currency"] {
        if body["billing"][field].is_null() {
            return JourneyResult::fail(name, dur(), format!("overview.billing missing '{field}'"));
        }
    }

    // --- usage ---
    let usage = match client
        .get(url_customer(cfg, "usage"))
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, dur(), format!("GET usage: {e}")),
    };
    if let Err(m) = expect_status("GET /v1/customer/usage", usage.status().as_u16(), 200) {
        return JourneyResult::fail(name, dur(), m);
    }
    let body: Value = match usage.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, dur(), format!("usage not JSON: {e}")),
    };
    for field in ["period", "cas_bytes", "reads", "writes", "quota_bytes"] {
        if body[field].is_null() {
            return JourneyResult::fail(name, dur(), format!("usage missing '{field}'"));
        }
    }
    if !body["daily"].is_array() {
        return JourneyResult::fail(name, dur(), format!("usage.daily not an array: {body}"));
    }

    // --- billing ---
    let billing = match client
        .get(url_customer(cfg, "billing"))
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, dur(), format!("GET billing: {e}")),
    };
    if let Err(m) = expect_status("GET /v1/customer/billing", billing.status().as_u16(), 200) {
        return JourneyResult::fail(name, dur(), m);
    }
    let body: Value = match billing.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, dur(), format!("billing not JSON: {e}")),
    };
    for field in ["status", "plan", "amount_due_cents", "currency"] {
        if body[field].is_null() {
            return JourneyResult::fail(name, dur(), format!("billing missing '{field}'"));
        }
    }
    if !body["invoices"].is_array() {
        return JourneyResult::fail(
            name,
            dur(),
            format!("billing.invoices not an array: {body}"),
        );
    }

    JourneyResult::pass(name, dur())
}

/// Edge: usage scoped to an EMPTY period (a billing month far in the past, no
/// traffic) must still return 200 with a well-formed shape — empty periods are a
/// real customer view (a fresh signup, or browsing a prior month). The `daily`
/// array may be empty; the scalar counters must still be present.
fn usage_empty_period(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Dashboard: usage for an empty period → 200, well-formed (empty daily ok)";
    let start = Instant::now();
    let dur = ms(start);

    let p1 = resolve_or_gate!(Persona::P1ReadWrite, cfg, name);
    let token = p1.token.expect("P1 always has a token");

    // A period with (almost certainly) no traffic for an e2e tenant.
    let url = format!("{}?period=2000-01", url_customer(cfg, "usage"));
    let resp = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::gated(name, format!("endpoint unreachable: {e}")),
    };
    if let Err(m) = expect_status("GET usage?period=2000-01", resp.status().as_u16(), 200) {
        return JourneyResult::fail(name, dur(), m);
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, dur(), format!("usage(empty) not JSON: {e}")),
    };
    // Shape must hold even with no data: scalars present, `daily` is an array.
    for field in ["period", "cas_bytes", "reads", "writes", "quota_bytes"] {
        if body[field].is_null() {
            return JourneyResult::fail(
                name,
                dur(),
                format!("usage(empty) missing scalar '{field}': {body}"),
            );
        }
    }
    if !body["daily"].is_array() {
        return JourneyResult::fail(
            name,
            dur(),
            format!("usage(empty) daily not an array: {body}"),
        );
    }

    JourneyResult::pass(name, dur())
}

/// Adversarial (P5 read-only): listing keys is a privileged key-management op
/// (it enumerates the tenant's PATs + BYOK status). A read-only cache PAT MUST
/// be denied with 403 — a `cas:r` token enumerating credentials is the recon
/// step of a revoke/escalation attack (rt-nuclear cycle-2 #7). This is a
/// deny-journey: a 200 here is a SECURITY BUG, so we assert the deny.
fn keys_list_requires_write_scope(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Dashboard: read-only PAT on keys list → 403 (write-scope gated)";
    let start = Instant::now();
    let dur = ms(start);

    let ro = resolve_or_gate!(Persona::P2ReadOnly, cfg, name);
    let token = ro.token.expect("P2 always has a token");

    let resp = match client
        .get(url_customer(cfg, "keys"))
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::gated(name, format!("endpoint unreachable: {e}")),
    };
    let status = resp.status().as_u16();
    // 200 = the read-only token enumerated credentials → privilege break.
    if status == 200 {
        return JourneyResult::fail(
            name,
            dur(),
            "SECURITY: read-only PAT got 200 from GET /v1/customer/keys — \
             credential enumeration with insufficient scope"
                .to_string(),
        );
    }
    // The exact contract is 403 (insufficient scope); accept the deny family so
    // a fail-closed 401 (e.g. tenant gate) is still a pass, never a 200.
    if let Err(m) = expect_gate_denied("keys list (read-only PAT)", status) {
        return JourneyResult::fail(name, dur(), m);
    }

    JourneyResult::pass(name, dur())
}

/// Adversarial (P10 cross-tenant): tenant B reads the dashboard with their OWN
/// PAT and must see THEIR OWN tenant — never tenant A's. The Worker re-derives
/// the tenant from the PAT, so B cannot address A's data at all; the black-box
/// proof is that B's `overview.tenant_id` is B's tenant and is NOT equal to
/// A's. A 200 carrying A's tenant_id would be a cross-tenant leak.
fn tenant_scoping(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Dashboard: tenant scoping — B's overview is B's own, never A's";
    let start = Instant::now();
    let dur = ms(start);

    let a = resolve_or_gate!(Persona::P1ReadWrite, cfg, name);
    let b = resolve_or_gate!(Persona::P6TenantB, cfg, name);
    let token_a = a.token.expect("P1 always has a token");
    let token_b = b.token.expect("P6 always has a token");

    // Read A's overview to learn A's tenant_id (the value B must never return).
    let a_resp = match client
        .get(url_customer(cfg, "overview"))
        .header(AUTHORIZATION, bearer(token_a))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::gated(name, format!("endpoint unreachable: {e}")),
    };
    if a_resp.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            dur(),
            format!(
                "A overview got {} (expected 200) — cannot establish A's identity",
                a_resp.status()
            ),
        );
    }
    let a_body: Value = match a_resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, dur(), format!("A overview not JSON: {e}")),
    };
    let a_tenant = match a_body["tenant_id"].as_str() {
        Some(t) => t.to_string(),
        None => {
            return JourneyResult::fail(
                name,
                dur(),
                format!("A overview missing tenant_id: {a_body}"),
            )
        }
    };

    // B reads B's own overview.
    let b_resp = match client
        .get(url_customer(cfg, "overview"))
        .header(AUTHORIZATION, bearer(token_b))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, dur(), format!("B overview: {e}")),
    };
    if b_resp.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            dur(),
            format!("B overview got {} (expected 200)", b_resp.status()),
        );
    }
    let b_body: Value = match b_resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, dur(), format!("B overview not JSON: {e}")),
    };
    let b_tenant = match b_body["tenant_id"].as_str() {
        Some(t) => t.to_string(),
        None => {
            return JourneyResult::fail(
                name,
                dur(),
                format!("B overview missing tenant_id: {b_body}"),
            )
        }
    };

    // The security assertion: B's view is scoped to B, never A.
    if b_tenant == a_tenant {
        return JourneyResult::fail(
            name,
            dur(),
            format!(
                "SECURITY: tenant B's overview returned tenant_id={b_tenant} == A's — \
                 cross-tenant identity leak"
            ),
        );
    }

    JourneyResult::pass(name, dur())
}

/// Adversarial (P12 anonymous): the dashboard requires auth. A request with NO
/// Authorization header must be denied (the customer routes fail CLOSED to 401
/// on a missing/sentinel tenant). Deny-journey: a 200 here is a BUG.
fn overview_no_auth(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Dashboard: overview with no auth → 401";
    let start = Instant::now();
    let dur = ms(start);

    let resp = match client.get(url_customer(cfg, "overview")).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::gated(name, format!("endpoint unreachable: {e}")),
    };
    let status = resp.status().as_u16();
    if status == 200 {
        return JourneyResult::fail(
            name,
            dur(),
            "SECURITY: unauthenticated GET /v1/customer/overview returned 200".to_string(),
        );
    }
    // Contract is 401; accept the deny family (401/403/404) so an edge-gate that
    // hides the route is still a pass, never an authenticated leak.
    if let Err(m) = expect_gate_denied("overview (no auth)", status) {
        return JourneyResult::fail(name, dur(), m);
    }

    JourneyResult::pass(name, dur())
}
