use super::*;

pub(in crate::journeys::oci) fn oci_host_reachable(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI: host reachable (scale-to-zero warmup)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    if warm_oci(cfg, client) {
        JourneyResult::pass(name, ms(start))
    } else {
        JourneyResult::fail(
            name,
            ms(start),
            "OCI host GET /v2/ never answered (<500) within the warmup budget — the registry \
             is unreachable; the OCI conformance journeys below cannot run"
                .to_string(),
        )
    }
}

/// Resolve P1's PAT or a gate reason.
pub(in crate::journeys::oci) fn p1_pat<'a>(cfg: &'a Config, name: &'static str) -> Result<&'a str, JourneyResult> {
    match Persona::P1ReadWrite.resolve(cfg) {
        Ok(p) => Ok(p.token.expect("P1 always has a token")),
        Err(reason) => Err(JourneyResult::gated(name, reason)),
    }
}

// ── #1 — `GET /v2/` → 401 + Bearer realm challenge ───────────────────────────

/// `GET /v2/` with no auth → 401 carrying a `Www-Authenticate: Bearer
/// realm="https://corelink-oci.humangr.com/token",service="corelink-oci",…`.
/// This is the entrypoint docker uses to discover the token endpoint; a missing
/// or malformed challenge breaks `docker login` before it starts. Runs on
/// connectivity alone (no creds needed to assert the challenge).
pub(in crate::journeys::oci) fn j1_v2_challenge(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #1: GET /v2/ → 401 + Bearer realm/service challenge";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    let resp = match client.get(format!("{base}/v2/")).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}"));
        }
    };
    let code = resp.status().as_u16();
    if code != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /v2/ got {code} (expected 401 to advertise the token realm)"),
        );
    }
    let wa = match www_auth(&resp) {
        Some(w) => w,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "401 on /v2/ missing the Www-Authenticate header".to_string(),
            );
        }
    };
    if !wa.starts_with("Bearer ") {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("Www-Authenticate is not a Bearer challenge: {wa}"),
        );
    }
    let want_realm = r#"realm="https://corelink-oci.humangr.com/token""#;
    if !wa.contains(want_realm) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("challenge realm wrong — expected {want_realm} in: {wa}"),
        );
    }
    if !wa.contains(&format!(r#"service="{OCI_SERVICE}""#)) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("challenge missing service=\"{OCI_SERVICE}\": {wa}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #2 — GET /token with Basic(PAT) → 200 + bearer ───────────────────────────

/// `GET /token?service=…&scope=repository:<repo>:pull` with Basic(tenant:PAT) →
/// 200 + a non-empty `token`. The classic docker-pull token leg.
pub(in crate::journeys::oci) fn j2_token_get(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #2: GET /token Basic(PAT) pull-scope → 200 + bearer";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let scope = format!("repository:{repo}:pull");
    match mint_bearer(client, &base, pat, &scope) {
        Ok(tok) if !tok.is_empty() => JourneyResult::pass(name, ms(start)),
        Ok(_) => JourneyResult::fail(
            name,
            ms(start),
            "GET /token returned an empty token".to_string(),
        ),
        Err(e) => JourneyResult::fail(name, ms(start), e),
    }
}

// ── #3 — POST /token (form) → 200 + bearer (NOT 405) ─────────────────────────

/// `POST /token` with `application/x-www-form-urlencoded`
/// `grant_type=password&username=…&password=<PAT>&scope=…&service=…` → 200 +
/// bearer. Real `docker push` uses the OAuth2 POST form, NOT the GET leg — a 405
/// here breaks every push. This is the load-bearing "docker push uses POST" fix.
pub(in crate::journeys::oci) fn j3_token_post(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #3: POST /token form grant → 200 + bearer (not 405)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let scope = format!("repository:{repo}:pull,push");
    let form = format!(
        "grant_type=password&service={}&username=oci&password={}&scope={}",
        OCI_SERVICE,
        urlencode(pat),
        urlencode(&scope),
    );
    let resp = match client
        .post(format!("{base}/token"))
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(form)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}"));
        }
    };
    let code = resp.status().as_u16();
    if code == 405 {
        return JourneyResult::fail(
            name,
            ms(start),
            "POST /token got 405 — docker push's OAuth2 token leg is rejected (regression)"
                .to_string(),
        );
    }
    if code != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("POST /token got {code} (expected 200)"),
        );
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("POST /token not JSON: {e}"));
        }
    };
    match body["token"].as_str() {
        Some(t) if !t.is_empty() => JourneyResult::pass(name, ms(start)),
        _ => JourneyResult::fail(
            name,
            ms(start),
            format!("POST /token response missing non-empty `token`: {body}"),
        ),
    }
}

// ── #4 — /token empty scope (docker login) → 200, bearer round-trips ─────────

/// `/token` with an EMPTY scope (docker login's credential-check token) → 200
/// and the minted bearer round-trips on `GET /v2/` (→ 200) even though it grants
/// no repository scope. A non-200 here breaks `docker login`.
pub(in crate::journeys::oci) fn j4_token_empty_scope(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #4: /token empty scope (docker login) → 200, bearer ok on /v2/";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let bearer = match mint_bearer(client, &base, pat, "") {
        Ok(t) => t,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("empty-scope mint: {e}")),
    };
    // The login token grants no repo scope but MUST pass the /v2/ base recheck.
    let resp = match client
        .get(format!("{base}/v2/"))
        .header(AUTHORIZATION, format!("Bearer {bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET /v2/ with login bearer: {e}"),
            );
        }
    };
    let code = resp.status().as_u16();
    if code != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("login bearer did not round-trip on /v2/ — got {code} (expected 200)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #5 — /token JSON has NO numeric `issued_at` ──────────────────────────────

/// The `/token` response must NOT carry a numeric `issued_at` field. docker's
/// JSON decoder expects `issued_at` to be an RFC3339 *string*; a numeric one
/// (epoch int) makes docker fail to decode the token envelope. The adapter omits
/// it entirely — assert it is either absent or, if present, a string.
pub(in crate::journeys::oci) fn j5_no_numeric_issued_at(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #5: /token JSON has no numeric issued_at (docker decode)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let url = format!(
        "{base}/token?service={OCI_SERVICE}&scope={}",
        urlencode(&format!("repository:{repo}:pull"))
    );
    let resp = match client.get(&url).header(AUTHORIZATION, basic(pat)).send() {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {url}: {e}")),
    };
    if resp.status().as_u16() != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /token got {} (expected 200)", resp.status()),
        );
    }
    let body: Value = match resp.json() {
        Ok(v) => v,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("/token not JSON: {e}")),
    };
    match &body["issued_at"] {
        Value::Null => {}
        Value::String(_) => {} // RFC3339 string is spec-legal
        other => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("issued_at must be absent or an RFC3339 string, got non-string: {other}"),
            );
        }
    }
    JourneyResult::pass(name, ms(start))
}

// ── #6 — blob HEAD no-auth → 401 with SPECIFIC scope challenge ───────────────

/// `HEAD /v2/<repo>/blobs/<digest>` with NO auth → 401 whose `Www-Authenticate`
/// names the SPECIFIC `repository:<repo>:pull` scope, NOT the wildcard
/// `repository:*:pull`. The wildcard made docker request a `*`-scoped token that
/// the exact-match `OciScope::allows` then rejected → an endless push 401-loop.
/// The fix parses the path BEFORE the auth check so the challenge is specific.
/// Runs on connectivity alone (the 401 challenge needs no creds).
pub(in crate::journeys::oci) fn j6_blob_head_specific_challenge(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #6: blob HEAD no-auth → 401 with specific repo:pull scope (not *)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    let repo = run_repo();
    let digest = oci_digest(b"corelink-e2e-oci-absent-blob-probe");
    let url = format!("{base}/v2/{repo}/blobs/{digest}");
    let resp = match client.head(&url).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}"));
        }
    };
    let code = resp.status().as_u16();
    if code != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("unauth blob HEAD got {code} (expected 401 + scope challenge)"),
        );
    }
    let wa = match www_auth(&resp) {
        Some(w) => w,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "401 on blob HEAD missing the Www-Authenticate challenge".to_string(),
            );
        }
    };
    let want = format!(r#"scope="repository:{repo}:pull""#);
    if !wa.contains(&want) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("challenge must name the SPECIFIC scope {want} — got: {wa}"),
        );
    }
    if wa.contains(r#"scope="repository:*"#) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("challenge advertised the wildcard repository:* scope (push 401-loop): {wa}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}
