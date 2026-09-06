use super::*;

pub(super) fn oci_host_reachable(cfg: &Config, client: &Client) -> JourneyResult {
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
pub(super) fn p1_pat<'a>(cfg: &'a Config, name: &'static str) -> Result<&'a str, JourneyResult> {
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
pub(super) fn j1_v2_challenge(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #1: GET /v2/ → 401 + Bearer realm/service challenge";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    let resp = match client.get(format!("{base}/v2/")).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}"))
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
            )
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
pub(super) fn j2_token_get(cfg: &Config, client: &Client) -> JourneyResult {
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
pub(super) fn j3_token_post(cfg: &Config, client: &Client) -> JourneyResult {
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
            return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}"))
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
            return JourneyResult::fail(name, ms(start), format!("POST /token not JSON: {e}"))
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
pub(super) fn j4_token_empty_scope(cfg: &Config, client: &Client) -> JourneyResult {
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
            return JourneyResult::fail(name, ms(start), format!("GET /v2/ with login bearer: {e}"))
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
pub(super) fn j5_no_numeric_issued_at(cfg: &Config, client: &Client) -> JourneyResult {
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
            )
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
pub(super) fn j6_blob_head_specific_challenge(cfg: &Config, client: &Client) -> JourneyResult {
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
            return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}"))
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
            )
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

// ── #7 — a push-scoped bearer is accepted on a pull op ───────────────────────

/// A bearer minted for `repository:<repo>:pull,push` (push superset) must be
/// ACCEPTED on a pull op: `HEAD /v2/<repo>/blobs/<absent-digest>` → 404 (miss),
/// NOT 401. i.e. the scope check treats push⇒pull (a push grant subsumes pull),
/// so `docker push`'s mount/exists probes don't bounce.
pub(super) fn j7_push_scoped_bearer_pulls(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #7: push-scoped bearer accepted on pull (HEAD blob → 404, not 401)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let scope = format!("repository:{repo}:pull,push");
    let bearer = match mint_bearer(client, &base, pat, &scope) {
        Ok(t) => t,
        Err(e) => {
            // A read-only PAT cannot mint a push bearer; that is a credential
            // mismatch, not a contract break we can attribute — gate it.
            return JourneyResult::gated(
                name,
                format!("could not mint a push-scoped bearer (RW PAT required): {e}"),
            );
        }
    };
    let digest = oci_digest(b"corelink-e2e-oci-push-then-pull-absent");
    let url = format!("{base}/v2/{repo}/blobs/{digest}");
    let resp = match client
        .head(&url)
        .header(AUTHORIZATION, format!("Bearer {bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("HEAD {url}: {e}")),
    };
    let code = resp.status().as_u16();
    if code == 401 || code == 403 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "push-scoped bearer was REJECTED on a pull op (got {code}) — push⇏pull regression"
            ),
        );
    }
    if code != 404 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("absent blob HEAD with push bearer got {code} (expected 404 miss)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #8 — insufficient (empty-scope) bearer re-challenges with required scope ──

/// An empty-scope (login) bearer on a blob HEAD is INSUFFICIENT → 401, and the
/// re-challenge must re-advertise the REQUIRED `repository:<repo>:pull` scope
/// (NOT an empty scope), so docker re-fetches a token for the RIGHT scope
/// instead of looping. This is the scope-re-challenge fix.
pub(super) fn j8_insufficient_bearer_rechallenge(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #8: empty-scope bearer on blob HEAD → 401 re-advertising repo:pull";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let bearer = match mint_bearer(client, &base, pat, "") {
        Ok(t) => t,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("login-bearer mint: {e}")),
    };
    let repo = run_repo();
    let digest = oci_digest(b"corelink-e2e-oci-rechallenge-probe");
    let url = format!("{base}/v2/{repo}/blobs/{digest}");
    let resp = match client
        .head(&url)
        .header(AUTHORIZATION, format!("Bearer {bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("HEAD {url}: {e}")),
    };
    let code = resp.status().as_u16();
    if code != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("empty-scope bearer on blob HEAD got {code} (expected 401 insufficient-scope)"),
        );
    }
    let wa = match www_auth(&resp) {
        Some(w) => w,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "insufficient-scope 401 missing the Www-Authenticate re-challenge".to_string(),
            )
        }
    };
    let want = format!(r#"scope="repository:{repo}:pull""#);
    if !wa.contains(&want) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("re-challenge must re-advertise {want} (not empty/wildcard) — got: {wa}"),
        );
    }
    if wa.contains(r#"scope="""#) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("re-challenge advertised an EMPTY scope (docker loop): {wa}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #9 — manifest HEAD after a full push → 200 + real Content-Length ─────────

/// Do a full curl-style monolithic push (POST upload → PUT ?digest= → 201 → PUT
/// manifest → 201), then `HEAD /v2/<repo>/manifests/<tag>` → 200 with a
/// `Content-Length` equal to the manifest's REAL byte size (NOT 0) and a
/// `Content-Type`. A `Content-Length: 0` HEAD made docker reject the descriptor.
pub(super) fn j9_manifest_head_content_length(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #9: full push then HEAD manifest → 200 + real Content-Length";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let tag = "e2e";
    let scope = format!("repository:{repo}:pull,push");
    let bearer = match mint_bearer(client, &base, pat, &scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("could not mint a push bearer (RW PAT required): {e}"),
            )
        }
    };
    let auth = format!("Bearer {bearer}");

    // 1) A config blob (small JSON) + the image config descriptor digest.
    let config_blob = format!(
        r#"{{"architecture":"amd64","os":"linux","rootfs":{{"type":"layers","diff_ids":[]}},"e2e":"{}"}}"#,
        uuid::Uuid::new_v4().simple()
    )
    .into_bytes();
    let config_digest = oci_digest(&config_blob);

    if let Err(m) = push_blob(
        client,
        &base,
        &auth,
        &repo,
        &config_digest,
        config_blob.clone(),
    ) {
        return JourneyResult::fail(name, ms(start), m);
    }

    // 2) Build a minimal OCI image manifest referencing the config blob (no
    //    layers — a valid empty-layer manifest is enough for HEAD/Content-Length).
    let manifest = format!(
        r#"{{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{{"mediaType":"application/vnd.oci.image.config.v1+json","digest":"{}","size":{}}},"layers":[]}}"#,
        config_digest,
        config_blob.len()
    )
    .into_bytes();
    let manifest_len = manifest.len();
    let manifest_media = "application/vnd.oci.image.manifest.v1+json";

    // 3) PUT the manifest under the tag.
    let put_url = format!("{base}/v2/{repo}/manifests/{tag}");
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, manifest_media)
        .body(manifest.clone())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("PUT manifest {put_url}: {e}"))
        }
    };
    let pc = put.status().as_u16();
    if !matches!(pc, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PUT manifest got {pc} (expected 201). url={put_url}"),
        );
    }

    // 4) HEAD the manifest — the regression assertion.
    let head_url = format!("{base}/v2/{repo}/manifests/{tag}");
    let head = match client
        .head(&head_url)
        .header(AUTHORIZATION, &auth)
        .header("Accept", manifest_media)
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("HEAD manifest {head_url}: {e}"))
        }
    };
    let hc = head.status().as_u16();
    if hc != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("HEAD manifest got {hc} (expected 200 after a successful push)"),
        );
    }
    // Content-Length must equal the real manifest size, never 0.
    let cl = head
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    match cl {
        Some(n) if n == manifest_len => {}
        Some(0) => {
            return JourneyResult::fail(
                name,
                ms(start),
                "HEAD manifest Content-Length is 0 — docker rejects the descriptor (regression)"
                    .to_string(),
            )
        }
        Some(n) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("HEAD manifest Content-Length {n} ≠ real manifest size {manifest_len}"),
            )
        }
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "HEAD manifest missing Content-Length (empty body would default to 0)".to_string(),
            )
        }
    }
    if head.headers().get(CONTENT_TYPE).is_none() {
        return JourneyResult::fail(
            name,
            ms(start),
            "HEAD manifest missing Content-Type (docker needs the manifest mediaType)".to_string(),
        );
    }
    JourneyResult::pass(name, ms(start))
}
