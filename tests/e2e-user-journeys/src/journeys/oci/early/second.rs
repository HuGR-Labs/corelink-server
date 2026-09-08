use super::*;

// ── #7 — a push-scoped bearer is accepted on a pull op ───────────────────────

/// A bearer minted for `repository:<repo>:pull,push` (push superset) must be
/// ACCEPTED on a pull op: `HEAD /v2/<repo>/blobs/<absent-digest>` → 404 (miss),
/// NOT 401. i.e. the scope check treats push⇒pull (a push grant subsumes pull),
/// so `docker push`'s mount/exists probes don't bounce.
pub(in crate::journeys::oci) fn j7_push_scoped_bearer_pulls(
    cfg: &Config,
    client: &Client,
) -> JourneyResult {
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
pub(in crate::journeys::oci) fn j8_insufficient_bearer_rechallenge(
    cfg: &Config,
    client: &Client,
) -> JourneyResult {
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
            );
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
pub(in crate::journeys::oci) fn j9_manifest_head_content_length(
    cfg: &Config,
    client: &Client,
) -> JourneyResult {
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
            );
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
            return JourneyResult::fail(name, ms(start), format!("PUT manifest {put_url}: {e}"));
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
            return JourneyResult::fail(name, ms(start), format!("HEAD manifest {head_url}: {e}"));
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
            );
        }
        Some(n) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("HEAD manifest Content-Length {n} ≠ real manifest size {manifest_len}"),
            );
        }
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "HEAD manifest missing Content-Length (empty body would default to 0)".to_string(),
            );
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
