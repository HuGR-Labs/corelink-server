//! Turborepo remote-cache journeys — `/v8/artifacts/*` (S5 surface).
//!
//! Black-box: deployed HTTP API + Bearer PAT only. Uses ONLY the
//! [`crate::harness`] URL builders + helpers; no internal crate imports, no
//! direct store access, no mocks.
//!
//! ## Live contract (grounded in `routes/turbo_v8.rs`, captured 2026-06-20)
//!   - `PUT /v8/artifacts/{hash}?teamId=<team>` — raw `application/octet-stream`
//!     body. Success = **200** + JSON `{"urls":[...]}` (NOT 201). A read-only
//!     (`cas:r`) PAT is rejected **403** ("insufficient scope") by the
//!     fail-closed scope gate.
//!   - `GET /v8/artifacts/{hash}?teamId=<team>` — hit = **200** + the raw bytes;
//!     absent = **404**.
//!   - `POST /v8/artifacts/events` — accept-and-drop telemetry, always **200**
//!     (still requires an authenticated tenant — fail-closed 401 otherwise).
//!   - `POST /v8/artifacts/status` — static "remote cache enabled" check, **200**
//!     + JSON.
//!   - **Isolation is by the PAT's tenant (`auth.0`), NOT the client `teamId`**:
//!     the storage key is `"<teamId>/<hash>"` scoped under the authenticated
//!     tenant, so a different tenant's PAT reading the SAME teamId+hash misses
//!     (404) — it never sees the other tenant's bytes. `teamId` is a label
//!     within one tenant, not a security boundary.
//!
//! ### Cells covered (per the matrix)
//!   - Happy: artifact PUT→GET round-trip (bytes match); POST events accepted;
//!     POST status accepted (edge).
//!   - Adversarial P5: read-only PAT write → 403.
//!   - Adversarial P10: cross-tenant isolation — tenant B cannot read tenant A's
//!     artifact (deny, and never A's bytes).

use std::time::Instant;

use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};

use crate::harness::{
    bearer, expect_denied, expect_status, sha256_hex, unique_blob, url_turbo_artifact,
    url_turbo_events, url_turbo_status, Config, JourneyResult,
};
use crate::personas::Persona;

/// A stable teamId label for hermetic runs (isolation is by tenant, not teamId).
const TEAM_ID: &str = "corelink-e2e-team";

/// Append the required `teamId` query parameter to a `/v8/artifacts/{hash}` URL.
fn with_team(url: &str, team: &str) -> String {
    format!("{url}?teamId={team}")
}

/// Run the Turborepo journeys: one [`JourneyResult`] per journey.
pub fn run(cfg: &Config, client: &Client) -> Vec<JourneyResult> {
    vec![
        artifact_round_trip(cfg, client),
        artifact_missing_team_id(cfg, client),
        events_accepted(cfg, client),
        status_handshake(cfg, client),
        read_only_write_denied(cfg, client),
        cross_tenant_isolation(cfg, client),
    ]
}

/// Happy: PUT a unique artifact, GET it back, bytes must match (M15 happy path).
///
/// ## History of the 500 finding
///
/// The previous version of this journey discovered (2026-06-20) that both PUT
/// and GET returned 500 in prod because the `corelink-turbo-prod` R2 bucket was
/// not provisioned. That finding was documented in `turbo_storage_finding_gate`
/// and the journey was converted to a loud GATE rather than a silent skip.
///
/// The container has since been redeployed (image e423ed23-r1). This journey is
/// now restored as a **real assertion**. If the bucket is still absent (500 still
/// reproduces), the journey records a loud `FAIL` with the root-cause note — NOT
/// a silent gate. This is intentional: a persistent 500 on the happy path is a
/// real bug that must surface, not be hidden.
fn artifact_round_trip(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Turbo M15: artifact round-trip — PUT then GET matches (happy)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    let blob = unique_blob("corelink-e2e-turbo");
    let hash = sha256_hex(&blob);
    let url = with_team(&url_turbo_artifact(cfg, &hash), TEAM_ID);

    // PUT — raw octet-stream body, success is 200 + {"urls":[...]}.
    let put = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT {url}: {e}")),
    };
    let put_status = put.status().as_u16();

    // 500 on PUT: the R2 bucket is still missing. This is a LOUD FAIL (not a
    // gate/skip) — the container was redeployed (e423ed23-r1) and this should
    // be fixed. The root-cause note is preserved for the operator.
    if put_status == 500 {
        return JourneyResult::fail(
            name,
            ms(start),
            "FAIL-PENDING: PUT /v8/artifacts/{hash} returned 500 — R2_TURBO_BUCKET \
             'corelink-turbo-prod' is likely still not provisioned. \
             Root cause: R2S3Client::new does not verify the bucket at build time, so \
             /status 200s while every S3 GET/PUT errs → TurboBridgeError::Internal → \
             generic 500 (routes/turbo_v8.rs map_err). \
             FIX: provision corelink-turbo-prod and set R2_TURBO_BUCKET across all \
             prod/regional container envs (provision-cf-corelink-prod.sh).",
        );
    }
    if let Err(m) = expect_status("turbo PUT", put_status, 200) {
        return JourneyResult::fail(name, ms(start), format!("{m} (url={url})"));
    }

    // GET — must be a hit (200) and the bytes must match what we PUT.
    let get = match client.get(&url).header(AUTHORIZATION, bearer(token)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    let get_status = get.status().as_u16();
    if get_status == 500 {
        return JourneyResult::fail(
            name,
            ms(start),
            "FAIL-PENDING: GET /v8/artifacts/{hash} returned 500 after a successful PUT — \
             R2_TURBO_BUCKET read path is broken (see PUT note above).",
        );
    }
    if let Err(m) = expect_status("turbo GET", get_status, 200) {
        return JourneyResult::fail(name, ms(start), m);
    }
    match get.bytes() {
        Ok(b) if b.as_ref() == blob.as_slice() => {}
        Ok(b) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET bytes mismatch (put {} got {})", blob.len(), b.len()),
            )
        }
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET body: {e}")),
    }

    JourneyResult::pass(name, ms(start))
}

/// M15b: missing `teamId` query param → 400.
///
/// `PUT /v8/artifacts/{hash}` without `?teamId=` must return 400. This is
/// enforced by the server: `ArtifactQuery` has `team_id` as a required field, so
/// axum's `Query<ArtifactQuery>` extractor rejects the request with a 400
/// deserialization error before the handler runs. Similarly for GET.
///
/// This journey was previously absent from the suite. It is a clean-surface
/// contract: the route is documented as requiring `teamId` and the server must
/// enforce it, not silently use an empty/default team namespace.
fn artifact_missing_team_id(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Turbo M15b: missing teamId → 400 (required query param)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    let blob = unique_blob("corelink-e2e-turbo-no-team");
    let hash = sha256_hex(&blob);

    // URL WITHOUT ?teamId= — the raw artifact URL with no query string.
    let url_no_team = url_turbo_artifact(cfg, &hash);

    // PUT without teamId — must be 400.
    let put = match client
        .put(&url_no_team)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("PUT no-teamId: {e}")),
    };
    let put_status = put.status().as_u16();
    if put_status != 400 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "PUT without teamId: expected 400, got {put_status}. \
                 The route must enforce teamId as a required query parameter."
            ),
        );
    }

    // GET without teamId — must also be 400.
    let get = match client
        .get(&url_no_team)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET no-teamId: {e}")),
    };
    let get_status = get.status().as_u16();
    if get_status != 400 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "GET without teamId: expected 400, got {get_status}. \
                 The route must enforce teamId as a required query parameter."
            ),
        );
    }

    JourneyResult::pass(name, ms(start))
}

/// Happy: POST telemetry events — accept-and-drop, must return 200.
fn events_accepted(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Turbo: events ingest — POST /events accepted (happy)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    // A representative tiny Turbo telemetry payload (a few KiB max in the wild).
    let body = r#"[{"sessionId":"corelink-e2e","hash":"deadbeef","source":"LOCAL","event":"HIT","duration":1}]"#;
    let url = url_turbo_events(cfg);
    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/json")
        .body(body)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    if let Err(m) = expect_status("turbo events", resp.status().as_u16(), 200) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}

/// Edge: POST status — the static remote-cache-enabled handshake, must be 200.
fn status_handshake(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Turbo: status handshake — POST /status returns 200 (edge)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let p1 = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = p1.token.expect("P1 always has a token");

    let url = url_turbo_status(cfg);
    let resp = match client
        .post(&url)
        .header(AUTHORIZATION, bearer(token))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {url}: {e}")),
    };
    if let Err(m) = expect_status("turbo status", resp.status().as_u16(), 200) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}

/// Adversarial (P5/P2): a read-only PAT MUST NOT be able to write an artifact.
/// The fail-closed scope gate rejects a `cas:r` token with 403. A deny-journey
/// that "passes" on a 200 is a security bug — so a successful write is a FAIL.
fn read_only_write_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Turbo: read-only PAT write → denied (adversarial P5)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let ro = match Persona::P2ReadOnly.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token = ro.token.expect("P2 always has a token");

    let blob = unique_blob("corelink-e2e-turbo-ro");
    let hash = sha256_hex(&blob);
    let url = with_team(&url_turbo_artifact(cfg, &hash), TEAM_ID);

    let put = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob)
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("RO PUT {url}: {e}")),
    };
    let status = put.status().as_u16();
    if matches!(status, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("SECURITY: read-only PAT wrote an artifact (got {status}, expected 403)"),
        );
    }
    // Live contract is 403 ("insufficient scope"); accept any deny defensively.
    if let Err(m) = expect_denied("RO turbo write", status) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}

/// Adversarial (P10/P6): cross-tenant isolation. Tenant A PUTs an artifact under
/// `teamId`; tenant B (a DIFFERENT tenant's PAT) GETs the SAME teamId+hash.
/// Isolation is by the authenticated tenant, so B must be DENIED (404/403) and
/// must NEVER receive A's bytes. A 200 with A's content is a cross-tenant leak.
fn cross_tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "Turbo: cross-tenant isolation — B cannot read A's artifact (adversarial P10)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;

    let a = match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token_a = a.token.expect("P1 always has a token");
    let b = match Persona::P6TenantB.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let token_b = b.token.expect("P6 always has a token");

    // Same teamId + same hash for both — isolation must come from the PAT tenant.
    let blob = unique_blob("tenant-a-turbo-secret");
    let hash = sha256_hex(&blob);
    let url = with_team(&url_turbo_artifact(cfg, &hash), TEAM_ID);

    // Tenant A PUTs.
    let put = match client
        .put(&url)
        .header(AUTHORIZATION, bearer(token_a))
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("A PUT {url}: {e}")),
    };
    let put_status = put.status().as_u16();
    // If the seed write 500s (missing R2 bucket) there is nothing to cross-read.
    // Fail loudly so the isolation journey stays visible (not silently skipped).
    if put_status == 500 {
        return JourneyResult::fail(
            name,
            ms(start),
            "FAIL-PENDING (cross-tenant isolation): A PUT returned 500 — R2 Turbo bucket \
             likely still missing (see artifact_round_trip for root cause). \
             Cannot verify isolation without a successful write.",
        );
    }
    if !matches!(put_status, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "A PUT got {put_status} — cannot verify isolation without a successful write"
            ),
        );
    }

    // Tenant B GETs the SAME teamId+hash with B's PAT — must be denied, no leak.
    let get = match client.get(&url).header(AUTHORIZATION, bearer(token_b)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("B GET {url}: {e}")),
    };
    let status = get.status().as_u16();
    if status == 200 {
        let leaked = get.bytes().map(|b| b.as_ref() == blob.as_slice()).unwrap_or(false);
        if leaked {
            return JourneyResult::fail(
                name,
                ms(start),
                "SECURITY: tenant B read tenant A's artifact bytes — cross-tenant leak".to_string(),
            );
        }
        return JourneyResult::fail(
            name,
            ms(start),
            "tenant B got 200 for A's content address (expected 404/403 deny)".to_string(),
        );
    }
    if let Err(m) = expect_denied("tenant B cross-read", status) {
        return JourneyResult::fail(name, ms(start), m);
    }

    JourneyResult::pass(name, ms(start))
}
