//! Adversarial SECURITY matrix — the protection moat's deny-path half.
//!
//! Black-box only: the deployed HTTP API + Bearer PATs, via the
//! [`crate::harness`] URL builders + the [`crate::personas`] persona/token map.
//! No `corelink-*` crate import, no mocks, no internal-state reads.
//!
//! Where the per-surface journey modules (`cas`, `ac`, …) prove the HAPPY path
//! and one or two adversarial cells, THIS module is the exhaustive negative
//! matrix a real attacker walks: for every applicable cache surface it asserts
//! that the documented deny-paths actually deny. A deny-journey that PASSES on a
//! 2xx is a launch-blocking bug, so every cell that observes a 200 on a write or
//! a cross-tenant read returns FAIL with a `SECURITY:` detail.
//!
//! Surfaces probed: native CAS (`/v1/cas/{tenant}/{hash}`) and native AC
//! (`/v1/ac/{tenant}/{digest}`) — the two path-tenant surfaces whose route
//! templates the harness owns. Each surface is exercised through one shared
//! helper so the matrix stays uniform.
//!
//! Cells (each a PASS/GATED/FAIL [`JourneyResult`]):
//!   - **P2 ReadOnly write-deny** — read 200, but PUT is denied (NOT 200).
//!   - **P4 Revoked op-deny** — every op denied (401/403/404).
//!   - **P5 Expired op-deny** — every op denied.
//!   - **P12 Anonymous op-deny** — every op denied (no Authorization header).
//!   - **Cross-tenant isolation (P1 vs P6)** — P1 cannot read/write/delete/list
//!     content under P6's tenant path (the single most important launch
//!     guarantee).
//!   - **Tenant-path spoofing** — a PAT for tenant A presenting tenant B's path
//!     segment is denied (no path-tenant trust).
//!   - **Scope non-escalation** — a read-only token cannot acquire write
//!     capability (a write op with the RO PAT is denied, on EVERY surface).
//!
//! GATED whenever the persona's token env var (or the tenant segment) is absent
//! — recorded, never a silent skip, never a hard panic.

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::harness::{
    bearer, blake3_hex, expect_denied, expect_gate_denied, sha256_hex, unique_blob, url_ac,
    url_ac_list, url_cas, url_cas_list, Config, JourneyResult,
};
use crate::personas::Persona;

/// Milliseconds elapsed since `start`.
fn ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// A path-tenant cache surface the matrix probes uniformly. Each variant knows
/// how to address a single object, the surface's list endpoint, and how to
/// compute a valid content address for a given body (BLAKE3 for native CAS —
/// the server verifies it; an opaque SHA-256 for native AC).
#[derive(Debug, Clone, Copy)]
enum Surface {
    /// Native CAS — `/v1/cas/{tenant}/{hash}`; the server VERIFIES the body's
    /// BLAKE3 against the URL hash, so the address must be the real digest.
    Cas,
    /// Native AC — `/v1/ac/{tenant}/{digest}`; the digest is an OPAQUE key, so
    /// any fresh 64-hex string is a valid address.
    Ac,
}

impl Surface {
    /// Human label for journey names.
    fn label(self) -> &'static str {
        match self {
            Surface::Cas => "CAS",
            Surface::Ac => "AC",
        }
    }

    /// All surfaces the matrix walks, in a stable order.
    fn all() -> [Surface; 2] {
        [Surface::Cas, Surface::Ac]
    }

    /// The single-object URL for `tenant` + `addr` on this surface.
    fn url_obj(self, cfg: &Config, tenant: &str, addr: &str) -> String {
        match self {
            Surface::Cas => url_cas(cfg, tenant, addr),
            Surface::Ac => url_ac(cfg, tenant, addr),
        }
    }

    /// The list URL for `tenant` on this surface.
    fn url_list(self, cfg: &Config, tenant: &str) -> String {
        match self {
            Surface::Cas => url_cas_list(cfg, tenant),
            Surface::Ac => url_ac_list(cfg, tenant),
        }
    }

    /// A valid content address for `body` on this surface (BLAKE3 for CAS, an
    /// opaque SHA-256 for AC).
    fn addr(self, body: &[u8]) -> String {
        match self {
            Surface::Cas => blake3_hex(body),
            Surface::Ac => sha256_hex(body),
        }
    }
}

/// A PUT helper that returns the status code (or a transport-error message).
fn put_body(client: &Client, url: &str, token: Option<&str>, body: Vec<u8>) -> Result<u16, String> {
    let mut req = client
        .put(url)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(body);
    if let Some(t) = token {
        req = req.header(AUTHORIZATION, bearer(t));
    }
    req.send()
        .map(|r| r.status().as_u16())
        .map_err(|e| format!("PUT {url}: {e}"))
}

/// A GET helper that returns the status code (or a transport-error message).
fn get_status(client: &Client, url: &str, token: Option<&str>) -> Result<u16, String> {
    let mut req = client.get(url);
    if let Some(t) = token {
        req = req.header(AUTHORIZATION, bearer(t));
    }
    req.send()
        .map(|r| r.status().as_u16())
        .map_err(|e| format!("GET {url}: {e}"))
}

/// A DELETE helper that returns the status code (or a transport-error message).
fn delete_status(client: &Client, url: &str, token: Option<&str>) -> Result<u16, String> {
    let mut req = client.delete(url);
    if let Some(t) = token {
        req = req.header(AUTHORIZATION, bearer(t));
    }
    req.send()
        .map(|r| r.status().as_u16())
        .map_err(|e| format!("DELETE {url}: {e}"))
}

/// Run the full adversarial security matrix.
///
/// The matrix is `{per-surface deny cells} ∪ {cross-cutting cells}`: the deny
/// personas (P2/P4/P5/P12) and scope-escalation are asserted per surface; the
/// cross-tenant isolation and tenant-path-spoofing cells are likewise asserted
/// per surface (they ARE per-surface launch guarantees).
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    let mut out = Vec::new();
    for s in Surface::all() {
        out.push(read_only_write_denied(cfg, client, s));
        out.push(revoked_every_op_denied(cfg, client, s));
        out.push(expired_every_op_denied(cfg, client, s));
        out.push(anonymous_every_op_denied(cfg, client, s));
        out.push(cross_tenant_isolation(cfg, client, s));
        out.push(tenant_path_spoofing(cfg, client, s));
        out.push(scope_no_escalation(cfg, client, s));
    }
    // M18: header-injection strip (cross-cutting, run once, not per-surface).
    out.push(forged_tenant_header_ignored(cfg, client));
    out
}

/// **P2 ReadOnly:** reads succeed (the RO PAT can GET an existing object) but
/// writes are DENIED — a PUT must NOT return 200/201. The journey first writes a
/// fresh object with the RW PAT (so a read CAN succeed), confirms the RO PAT
/// reads it, then asserts the RO PAT's PUT to a fresh address is denied.
fn read_only_write_denied(cfg: &Config, client: &Client, s: Surface) -> JourneyResult {
    let name: &'static str = match s {
        Surface::Cas => "SEC CAS: P2 read-only — read 200, write DENIED (no PUT bypass)",
        Surface::Ac => "SEC AC: P2 read-only — read 200, write DENIED (no PUT bypass)",
    };
    let start = Instant::now();

    let rw = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let ro = match Persona::P2ReadOnly.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token_rw = rw.token.expect("P1 has a token");
    let token_ro = ro.token.expect("P2 has a token");

    // Seed a readable object with the RW PAT so the RO read CAN succeed.
    let seed = unique_blob("sec-ro-readable");
    let addr = s.addr(&seed);
    let url = s.url_obj(cfg, &rw.tenant, &addr);
    match put_body(client, &url, Some(token_rw), seed.clone()) {
        Ok(200 | 201) => {}
        Ok(st) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("seed PUT got {st} — cannot verify RO read without a written object"),
            )
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // RO read must succeed (read capability present).
    match get_status(client, &url, Some(token_ro)) {
        Ok(200) => {}
        Ok(st) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("RO GET of an existing object got {st} (expected 200 — read should work)"),
            )
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // RO write to a FRESH address must be denied — and crucially NOT a 200/201.
    let attempt = unique_blob("sec-ro-write-attempt");
    let attempt_addr = s.addr(&attempt);
    let attempt_url = s.url_obj(cfg, &ro.tenant, &attempt_addr);
    let st = match put_body(client, &attempt_url, Some(token_ro), attempt) {
        Ok(st) => st,
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    };
    if matches!(st, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "SECURITY: read-only PAT WROTE (got {st}) — scope bypass on {}",
                s.label()
            ),
        );
    }
    if let Err(m) = expect_gate_denied("RO write", st) {
        return JourneyResult::fail(name, ms(start), m);
    }
    JourneyResult::pass(name, ms(start))
}

/// **P4 Revoked:** every op (GET / PUT / DELETE / list) must be denied. A
/// revoked PAT is an attacker replaying a leaked-then-revoked token; the server
/// must ACTIVELY reject it on every verb. These probes target the caller's OWN
/// primary-tenant path, so the route is guaranteed to exist — the deny must be
/// an authorization rejection (401/403), NEVER a 404. A 404 here would mean the
/// route is absent/renamed (the auth gate never ran), which masks a real
/// regression as "secure" (H18); so `expect_gate_denied` is used, matching the
/// sibling `pat_lifecycle` revoke-deny standard.
fn revoked_every_op_denied(cfg: &Config, client: &Client, s: Surface) -> JourneyResult {
    let name: &'static str = match s {
        Surface::Cas => "SEC CAS: P4 revoked PAT — GET/PUT/DELETE/list all DENIED",
        Surface::Ac => "SEC AC: P4 revoked PAT — GET/PUT/DELETE/list all DENIED",
    };
    every_op_denied(cfg, client, s, name, Persona::P4Revoked)
}

/// **P5 Expired:** every op must be denied (an expired PAT is past its TTL — the
/// server must not honour it on any verb).
fn expired_every_op_denied(cfg: &Config, client: &Client, s: Surface) -> JourneyResult {
    let name: &'static str = match s {
        Surface::Cas => "SEC CAS: P5 expired PAT — GET/PUT/DELETE/list all DENIED",
        Surface::Ac => "SEC AC: P5 expired PAT — GET/PUT/DELETE/list all DENIED",
    };
    every_op_denied(cfg, client, s, name, Persona::P5Expired)
}

/// **P12 Anonymous:** every op must be denied with NO Authorization header at
/// all (an unauthenticated probe).
fn anonymous_every_op_denied(cfg: &Config, client: &Client, s: Surface) -> JourneyResult {
    let name: &'static str = match s {
        Surface::Cas => "SEC CAS: P12 anonymous (no auth) — GET/PUT/DELETE/list all DENIED",
        Surface::Ac => "SEC AC: P12 anonymous (no auth) — GET/PUT/DELETE/list all DENIED",
    };
    every_op_denied(cfg, client, s, name, Persona::P12Anonymous)
}

/// Shared deny-every-verb assertion for a Denied-expectation persona (revoked /
/// expired / anonymous). Walks GET, PUT, DELETE and list against the PRIMARY
/// tenant path; each must return a deny and PUT must never be a 200/201.
fn every_op_denied(
    cfg: &Config,
    client: &Client,
    s: Surface,
    name: &'static str,
    persona: Persona,
) -> JourneyResult {
    let start = Instant::now();

    let actor = match persona.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    // Anonymous resolves with token == None (no header); the others carry the
    // backing token. Either way we address the PRIMARY tenant path.
    let token = actor.token; // Option<&str> — None only for P12.

    let body = unique_blob("sec-deny-op");
    let addr = s.addr(&body);
    let url = s.url_obj(cfg, &actor.tenant, &addr);
    let list_url = s.url_list(cfg, &actor.tenant);

    // GET an object — denied.
    match get_status(client, &url, token) {
        Ok(st) => {
            if st == 200 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("SECURITY: denied-persona GET returned 200 on {}", s.label()),
                );
            }
            if let Err(m) = expect_gate_denied("denied GET", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // PUT an object — denied, and never a 200/201 (would be a write bypass).
    match put_body(client, &url, token, body) {
        Ok(st) => {
            if matches!(st, 200 | 201) {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!("SECURITY: denied-persona WROTE (got {st}) on {}", s.label()),
                );
            }
            if let Err(m) = expect_gate_denied("denied PUT", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // DELETE an object — denied.
    match delete_status(client, &url, token) {
        Ok(st) => {
            if matches!(st, 200 | 202 | 204) {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: denied-persona DELETE accepted (got {st}) on {}",
                        s.label()
                    ),
                );
            }
            if let Err(m) = expect_gate_denied("denied DELETE", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // LIST the tenant — denied (must not enumerate another's keys).
    match get_status(client, &list_url, token) {
        Ok(st) => {
            if st == 200 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: denied-persona LIST returned 200 on {}",
                        s.label()
                    ),
                );
            }
            if let Err(m) = expect_gate_denied("denied LIST", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    JourneyResult::pass(name, ms(start))
}

/// **Cross-tenant isolation (P1 vs P6 TenantB):** P1 must NOT be able to
/// read / write / delete / list content under P6's tenant path. The single most
/// important launch guarantee. P6 seeds a real secret object under its own path
/// (so a leak would expose real bytes); then P1 — a fully valid PAT for a
/// DIFFERENT tenant — attacks every verb against P6's path and must be denied on
/// all of them (never a 200 carrying P6's bytes).
fn cross_tenant_isolation(cfg: &Config, client: &Client, s: Surface) -> JourneyResult {
    let name: &'static str = match s {
        Surface::Cas => "SEC CAS: cross-tenant (P1→P6 path) — read/write/delete/list all DENIED",
        Surface::Ac => "SEC AC: cross-tenant (P1→P6 path) — read/write/delete/list all DENIED",
    };
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let p6 = match Persona::P6TenantB.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token_1 = p1.token.expect("P1 has a token");
    let token_6 = p6.token.expect("P6 has a token");

    // P6 seeds a secret object under P6's own tenant path.
    let secret = unique_blob("tenantB-secret-bytes");
    let addr = s.addr(&secret);
    let url_b = s.url_obj(cfg, &p6.tenant, &addr);
    match put_body(client, &url_b, Some(token_6), secret.clone()) {
        Ok(200 | 201) => {}
        Ok(st) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("P6 seed PUT got {st} — cannot verify isolation without P6's object"),
            )
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // P1 attacks P6's object path with P1's (different-tenant) PAT.
    // READ — must be denied and must NEVER return P6's secret bytes.
    {
        let resp = match client
            .get(&url_b)
            .header(AUTHORIZATION, bearer(token_1))
            .send()
        {
            Ok(r) => r,
            Err(e) => return JourneyResult::fail(name, ms(start), format!("P1 GET {url_b}: {e}")),
        };
        let st = resp.status().as_u16();
        if st == 200 {
            let body = resp.bytes().map(|b| b.to_vec()).unwrap_or_default();
            if body.as_slice() == secret.as_slice() {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: P1 read P6's {} bytes ({} bytes) — cross-tenant LEAK",
                        s.label(),
                        body.len()
                    ),
                );
            }
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "SECURITY: P1 got 200 on P6's {} path — must be denied",
                    s.label()
                ),
            );
        }
        if let Err(m) = expect_denied("P1 cross-read", st) {
            return JourneyResult::fail(name, ms(start), m);
        }
    }

    // WRITE — P1 cannot write a fresh object under P6's path.
    let attack = unique_blob("p1-poison-into-tenantB");
    let attack_addr = s.addr(&attack);
    let attack_url = s.url_obj(cfg, &p6.tenant, &attack_addr);
    match put_body(client, &attack_url, Some(token_1), attack) {
        Ok(st) => {
            if matches!(st, 200 | 201) {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: P1 WROTE under P6's {} path (got {st}) — poisoning",
                        s.label()
                    ),
                );
            }
            if let Err(m) = expect_denied("P1 cross-write", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // DELETE — P1 cannot delete P6's seeded object.
    match delete_status(client, &url_b, Some(token_1)) {
        Ok(st) => {
            if matches!(st, 200 | 202 | 204) {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: P1 DELETED under P6's {} path (got {st})",
                        s.label()
                    ),
                );
            }
            if let Err(m) = expect_denied("P1 cross-delete", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // LIST — P1 cannot enumerate P6's tenant.
    let list_b = s.url_list(cfg, &p6.tenant);
    match get_status(client, &list_b, Some(token_1)) {
        Ok(st) => {
            if st == 200 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: P1 LISTED P6's {} tenant (got 200) — enumeration",
                        s.label()
                    ),
                );
            }
            if let Err(m) = expect_denied("P1 cross-list", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    JourneyResult::pass(name, ms(start))
}

/// **Tenant-path spoofing:** a PAT for tenant A presenting tenant B's id in the
/// path segment must be denied — the server derives/validates the tenant from
/// the PAT, NOT from the URL (no path-tenant trust). Distinct from cross-tenant
/// isolation in that the attacker's token belongs to the PRIMARY tenant and the
/// spoofed segment is tenant B's id; if the server trusted the path it would
/// serve B's namespace under A's credential.
fn tenant_path_spoofing(cfg: &Config, client: &Client, s: Surface) -> JourneyResult {
    let name: &'static str = match s {
        Surface::Cas => "SEC CAS: tenant-path spoof — A's PAT on B's path segment DENIED",
        Surface::Ac => "SEC AC: tenant-path spoof — A's PAT on B's path segment DENIED",
    };
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    // The spoofed path segment is tenant B's id; gate if it's absent.
    let tenant_b = match cfg.tenant_b.as_deref() {
        Some(t) => t,
        None => return JourneyResult::gated(name, "CORELINK_E2E_TENANT_B not set"),
    };
    let token_1 = p1.token.expect("P1 has a token");

    // A read with A's PAT against B's path segment must NOT serve B's namespace.
    let probe = unique_blob("sec-spoof-probe");
    let addr = s.addr(&probe);
    let spoof_url = s.url_obj(cfg, tenant_b, &addr);
    match get_status(client, &spoof_url, Some(token_1)) {
        Ok(st) => {
            if st == 200 {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: A's PAT GET 200 on B's {} path segment — path-tenant trust",
                        s.label()
                    ),
                );
            }
            if let Err(m) = expect_denied("spoof GET", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // A write with A's PAT against B's path segment must be denied (no
    // path-asserted write into another tenant's namespace).
    let payload = unique_blob("sec-spoof-write");
    let waddr = s.addr(&payload);
    let spoof_wurl = s.url_obj(cfg, tenant_b, &waddr);
    match put_body(client, &spoof_wurl, Some(token_1), payload) {
        Ok(st) => {
            if matches!(st, 200 | 201) {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: A's PAT WROTE on B's {} path segment (got {st}) — spoof bypass",
                        s.label()
                    ),
                );
            }
            if let Err(m) = expect_denied("spoof PUT", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    JourneyResult::pass(name, ms(start))
}

/// **Scope non-escalation:** the documented downscope holds — a read-only token
/// cannot obtain a write capability. A read-only PAT is the least-privileged
/// cache credential; it must not be able to mutate state on ANY surface. This
/// asserts the RO PAT's write op is denied (and never a 200/201) — the inverse
/// of the read_only_write_denied read+write split, focused purely on the
/// no-escalation invariant across both verbs that mutate (PUT, DELETE).
fn scope_no_escalation(cfg: &Config, client: &Client, s: Surface) -> JourneyResult {
    let name: &'static str = match s {
        Surface::Cas => "SEC CAS: scope non-escalation — RO PAT PUT+DELETE both DENIED",
        Surface::Ac => "SEC AC: scope non-escalation — RO PAT PUT+DELETE both DENIED",
    };
    let start = Instant::now();

    let ro = match Persona::P2ReadOnly.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let token_ro = ro.token.expect("P2 has a token");

    // RO PUT — must be denied, never 200/201 (no write escalation).
    let body = unique_blob("sec-escalation-write");
    let addr = s.addr(&body);
    let url = s.url_obj(cfg, &ro.tenant, &addr);
    match put_body(client, &url, Some(token_ro), body) {
        Ok(st) => {
            if matches!(st, 200 | 201) {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: RO PAT escalated to WRITE (got {st}) on {}",
                        s.label()
                    ),
                );
            }
            if let Err(m) = expect_gate_denied("RO escalation PUT", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    // RO DELETE — must be denied, never an accepted-delete (no destroy
    // escalation). The address need not exist; a deny is the contract.
    match delete_status(client, &url, Some(token_ro)) {
        Ok(st) => {
            if matches!(st, 200 | 202 | 204) {
                return JourneyResult::fail(
                    name,
                    ms(start),
                    format!(
                        "SECURITY: RO PAT escalated to DELETE (got {st}) on {}",
                        s.label()
                    ),
                );
            }
            if let Err(m) = expect_gate_denied("RO escalation DELETE", st) {
                return JourneyResult::fail(name, ms(start), m);
            }
        }
        Err(m) => return JourneyResult::fail(name, ms(start), m),
    }

    JourneyResult::pass(name, ms(start))
}

/// **M18 — Forged tenant-id / scope headers ignored (header-injection strip).**
///
/// The Worker strips `x-corelink-tenant-id` and `x-corelink-scope` before
/// forwarding any request to the container (`worker/src/index.ts`). A customer
/// PAT for tenant A that ALSO sends forged `x-corelink-tenant-id: <tenantB-id>`
/// and `x-corelink-scope: cas:admin` headers must NOT:
///   - gain access to tenant B's namespace (the request must be denied or serve
///     only tenant A's data — never tenant B's),
///   - receive elevated scope (the operation must succeed only within P1's
///     actual read-write scope, not any claimed-admin scope).
///
/// Proof strategy: P1 (tenant A) PUTs a small secret blob under tenant B's path
/// AND sends the forged headers so the server sees the PAT's REAL tenant (A) in
/// both the Bearer AND the injected id header. If the worker correctly strips
/// them, the server derives the tenant ONLY from the PAT — which belongs to A —
/// so a write to B's path must be denied (can't write into another tenant's
/// namespace regardless of what the headers claim). A 200/201 means the header
/// was honoured instead of the PAT (priv-esc). The gate actively denies cross-
/// tenant writes, so `expect_gate_denied` (401/403) is the correct assert: a
/// 404 would mean the route is absent, not that the strip held. M18.
///
/// We probe a GET as well: P1 with `x-corelink-tenant-id: <tenantB>` against
/// B's path must NOT return a 200 carrying B's data.
fn forged_tenant_header_ignored(cfg: &Config, client: &Client) -> JourneyResult {
    let name =
        "SEC: forged x-corelink-tenant-id / x-corelink-scope headers ignored (header-injection strip)";
    let start = Instant::now();

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    if cfg.tenant.is_none() {
        return JourneyResult::gated(name, "CORELINK_E2E_TENANT not set");
    }
    let tenant_b_id = match cfg.tenant_b.as_deref() {
        Some(t) => t,
        None => return JourneyResult::gated(name, "CORELINK_E2E_TENANT_B not set — header-injection probe needs a second tenant id to forge"),
    };
    let token_a = p1.token.expect("P1 has a token");

    // A fresh blob/address for the injection probe — we PUT under tenant B's
    // path with A's PAT AND the forged tenant-id header (the attacker's attempt
    // to claim they own B's namespace). The write must NOT succeed.
    let payload = unique_blob("m18-header-inject-write");
    let addr = blake3_hex(&payload);
    let url_b_write = url_cas(cfg, tenant_b_id, &addr);

    let write_resp = match client
        .put(&url_b_write)
        .header(AUTHORIZATION, bearer(token_a))
        .header(CONTENT_TYPE, "application/octet-stream")
        // Forged headers the attacker injects hoping the server trusts them
        // over the PAT. The Worker strips these; the container must never see
        // them with attacker-supplied values.
        .header("x-corelink-tenant-id", tenant_b_id)
        .header("x-corelink-scope", "cas:admin")
        .body(payload)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("header-injection PUT {url_b_write}: {e}"),
            )
        }
    };
    let write_st = write_resp.status().as_u16();
    if matches!(write_st, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "SECURITY: forged x-corelink-tenant-id header ACCEPTED — tenant A's PAT wrote \
                 (got {write_st}) under tenant B's path; the header-injection strip did NOT hold"
            ),
        );
    }
    if write_st >= 500 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "header-injection PUT got {write_st} 5xx — the gate must deny (401/403), not crash"
            ),
        );
    }
    // The cross-tenant-write gate must actively deny (401/403). A 404 here means
    // the CAS route is absent/renamed, leaving the injection-strip property
    // unproven. M18.
    if let Err(m) = expect_gate_denied("forged-header PUT to B's path", write_st) {
        return JourneyResult::fail(name, ms(start), m);
    }

    // A GET read probe: P1 with forged x-corelink-tenant-id: B against B's path
    // must NOT return a 200 carrying B's data. Here a 404 is acceptable (the
    // object may not exist, or the server returns not-found as a safe deny) —
    // but a 200 carrying any bytes is a cross-tenant leak. We use expect_denied
    // (401/403/404) because a not-found-safe-deny on a read is legitimate when
    // the route IS mounted; what we're guarding against is a 200.
    let read_probe = unique_blob("m18-header-inject-read");
    let read_addr = blake3_hex(&read_probe);
    let url_b_read = url_cas(cfg, tenant_b_id, &read_addr);

    let read_resp = match client
        .get(&url_b_read)
        .header(AUTHORIZATION, bearer(token_a))
        .header("x-corelink-tenant-id", tenant_b_id)
        .header("x-corelink-scope", "cas:admin")
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("header-injection GET {url_b_read}: {e}"),
            )
        }
    };
    let read_st = read_resp.status().as_u16();
    if read_st == 200 {
        let body_len = read_resp.bytes().map(|b| b.len()).unwrap_or(0);
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "SECURITY: forged x-corelink-tenant-id GET returned 200 ({body_len} bytes) from \
                 tenant B's path using tenant A's PAT — cross-tenant read via header injection"
            ),
        );
    }
    if read_st >= 500 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("header-injection GET got {read_st} 5xx — must deny cleanly, not crash"),
        );
    }
    if let Err(m) = expect_denied("forged-header GET of B's path", read_st) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}
