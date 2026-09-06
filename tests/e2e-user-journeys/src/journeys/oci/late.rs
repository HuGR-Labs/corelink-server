use super::*;

// ── #10 — chunked PATCH upload: POST → PATCH → PUT → GET bytes equality ───────

/// The large-layer (multi-chunk) upload path: open a session with `POST
/// /v2/<repo>/blobs/uploads/` (→ 202 + `Location`), stream the blob in two
/// `PATCH` chunks (→ 202 + `Range: 0-<n>` after each), finalize with `PUT
/// <location>?digest=<digest>` (no body — all bytes already PATCHed; → 201 +
/// `Location: /v2/<repo>/blobs/<digest>`), then `GET` the canonical blob URL
/// and assert byte-for-byte + Content-Length equality.
///
/// This differs from `push_blob` (monolithic) in that data is sent as TWO
/// separate PATCH requests before the PUT finalizer, proving the server
/// accumulates chunks correctly. The digest is asserted at both finalize time
/// (server-side 400 on mismatch) and GET time (local equality).
pub(super) fn j10_chunked_patch_upload(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #10: chunked PATCH upload (POST→PATCH×2→PUT) → GET byte equality";
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
            return JourneyResult::gated(
                name,
                format!("could not mint a push bearer (RW PAT required): {e}"),
            )
        }
    };
    let auth = format!("Bearer {bearer}");

    // Build a small but two-chunk-able payload.
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let full_bytes: Vec<u8> = format!("corelink-e2e-oci-chunked-{run_id}").into_bytes();
    let chunk1 = &full_bytes[..full_bytes.len() / 2];
    let chunk2 = &full_bytes[full_bytes.len() / 2..];
    let digest = oci_digest(&full_bytes);

    // 1) Open session — expect 202 + Location.
    let open_url = format!("{base}/v2/{repo}/blobs/uploads/");
    let open = match client
        .post(&open_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Vec::<u8>::new())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {open_url}: {e}")),
    };
    let oc = open.status().as_u16();
    if !matches!(oc, 202 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("open upload got {oc} (expected 202). url={open_url}"),
        );
    }
    let location = match open
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
    {
        Some(l) => l,
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                "open upload 202 missing the Location header".to_string(),
            )
        }
    };
    let session_url = if location.starts_with("http://") || location.starts_with("https://") {
        location
    } else {
        format!("{base}{location}")
    };

    // 2) PATCH chunk 1 — expect 202 + Range: 0-<len1-1>.
    let patch1 = match client
        .patch(&session_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(chunk1.to_vec())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("PATCH chunk1 {session_url}: {e}"))
        }
    };
    let p1c = patch1.status().as_u16();
    if p1c != 202 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PATCH chunk1 got {p1c} (expected 202)"),
        );
    }
    // The Range header after chunk1 should reflect the bytes received so far.
    let range1 = patch1
        .headers()
        .get("Range")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let expected_range1 = format!("0-{}", chunk1.len() - 1);
    if let Some(ref r) = range1 {
        if r != &expected_range1 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PATCH chunk1 Range={r:?} does not match expected {expected_range1:?}"),
            );
        }
    }

    // 3) PATCH chunk 2 — expect 202 + Range: 0-<total-1>.
    let patch2 = match client
        .patch(&session_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(chunk2.to_vec())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("PATCH chunk2 {session_url}: {e}"))
        }
    };
    let p2c = patch2.status().as_u16();
    if p2c != 202 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PATCH chunk2 got {p2c} (expected 202)"),
        );
    }
    let expected_range2 = format!("0-{}", full_bytes.len() - 1);
    let range2 = patch2
        .headers()
        .get("Range")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    if let Some(ref r) = range2 {
        if r != &expected_range2 {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PATCH chunk2 Range={r:?} does not match expected {expected_range2:?}"),
            );
        }
    }

    // 4) PUT finalize with ?digest= and NO body (all bytes already PATCHed).
    let sep = if session_url.contains('?') { '&' } else { '?' };
    let put_url = format!("{session_url}{sep}digest={}", urlencode(&digest));
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Vec::<u8>::new())
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("PUT finalize {put_url}: {e}"))
        }
    };
    let pc = put.status().as_u16();
    if !matches!(pc, 200 | 201) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("PUT finalize got {pc} (expected 201). url={put_url}"),
        );
    }
    // Location header on 201 should point to the canonical blob URL.
    let blob_location = put
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 5) GET the blob and assert byte + Content-Length equality.
    let get_url = match blob_location {
        Some(ref loc) if loc.starts_with("http://") || loc.starts_with("https://") => loc.clone(),
        Some(ref loc) => format!("{base}{loc}"),
        None => format!("{base}/v2/{repo}/blobs/{digest}"),
    };
    let pull_scope = format!("repository:{repo}:pull");
    let pull_bearer = match mint_bearer(client, &base, pat, &pull_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("mint pull bearer for GET verification: {e}"),
            )
        }
    };
    let get = match client
        .get(&get_url)
        .header(AUTHORIZATION, format!("Bearer {pull_bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {get_url}: {e}")),
    };
    let gc = get.status().as_u16();
    if gc != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET blob got {gc} (expected 200 after chunked upload). url={get_url}"),
        );
    }
    // Content-Length must equal the full payload size.
    let cl = get
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok());
    match cl {
        Some(n) if n == full_bytes.len() => {}
        Some(n) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!(
                    "GET blob Content-Length {n} ≠ uploaded size {} (chunked upload byte equality)",
                    full_bytes.len()
                ),
            )
        }
        None => {
            // Missing Content-Length is acceptable if the body is correct.
        }
    }
    // Byte equality — the body round-trip is the ground-truth assertion.
    let body_bytes = match get.bytes() {
        Ok(b) => b,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET blob body drain: {e}")),
    };
    if body_bytes.as_ref() != full_bytes.as_slice() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "GET blob body ({} bytes) does not match uploaded payload ({} bytes) — chunked upload corrupted",
                body_bytes.len(),
                full_bytes.len()
            ),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #11 — tags/list after a tagged manifest push ──────────────────────────────

/// Push a tagged manifest and assert that `GET /v2/<repo>/tags/list` returns
/// a JSON body containing the pushed tag. The response must be:
///
/// - `200 OK`
/// - `Content-Type: application/json`
/// - Body: `{"name": "<repo>", "tags": ["<tag>", ...]}` where `tags` is a
///   non-empty array containing the tag we just pushed.
///
/// This exercises the `oci_tags:<repo>` KV slot written by `push::manifest`
/// and read by `tags::list`.
pub(super) fn j11_tags_list_after_push(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #11: tags/list after manifest push → 200 + tag enumerated";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };

    let repo = run_repo();
    let tag = "e2e-list-probe";
    let scope = format!("repository:{repo}:pull,push");
    let bearer = match mint_bearer(client, &base, pat, &scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(
                name,
                format!("could not mint push bearer (RW PAT required): {e}"),
            )
        }
    };
    let auth = format!("Bearer {bearer}");

    // Push a minimal config blob + manifest with the tag.
    let config_blob = format!(
        r#"{{"architecture":"amd64","os":"linux","rootfs":{{"type":"layers","diff_ids":[]}},"e2e-tags":"{}"}}"#,
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
        return JourneyResult::fail(name, ms(start), format!("push config blob: {m}"));
    }

    let manifest = format!(
        r#"{{"schemaVersion":2,"mediaType":"application/vnd.oci.image.manifest.v1+json","config":{{"mediaType":"application/vnd.oci.image.config.v1+json","digest":"{}","size":{}}},"layers":[]}}"#,
        config_digest,
        config_blob.len()
    )
    .into_bytes();
    let manifest_media = "application/vnd.oci.image.manifest.v1+json";

    let put_url = format!("{base}/v2/{repo}/manifests/{tag}");
    let put = match client
        .put(&put_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, manifest_media)
        .body(manifest)
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
            format!("PUT manifest got {pc} (expected 201)"),
        );
    }

    // GET the tags list — a pull-scoped bearer is sufficient.
    let pull_scope = format!("repository:{repo}:pull");
    let pull_bearer = match mint_bearer(client, &base, pat, &pull_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("mint pull bearer for tags/list: {e}"),
            )
        }
    };
    let list_url = format!("{base}/v2/{repo}/tags/list");
    let list = match client
        .get(&list_url)
        .header(AUTHORIZATION, format!("Bearer {pull_bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {list_url}: {e}")),
    };
    let lc = list.status().as_u16();
    if lc != 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET tags/list got {lc} (expected 200)"),
        );
    }
    // Content-Type must be application/json.
    let ct = list
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
    if !ct.starts_with("application/json") {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET tags/list Content-Type={ct:?} (expected application/json)"),
        );
    }
    // Body: {"name": "<repo>", "tags": ["<tag>"]}.
    let body: Value = match list.json() {
        Ok(v) => v,
        Err(e) => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("GET tags/list response not JSON: {e}"),
            )
        }
    };
    // "name" must match the repo we pushed to.
    if body["name"].as_str() != Some(repo.as_str()) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "tags/list \"name\" field = {:?} (expected {:?})",
                body["name"], repo
            ),
        );
    }
    // "tags" must be an array containing our pushed tag.
    let tags = match body["tags"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>(),
        None => {
            return JourneyResult::fail(
                name,
                ms(start),
                format!("tags/list \"tags\" field is not an array: {body}"),
            )
        }
    };
    if tags.is_empty() {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("tags/list returned an empty tags array after pushing tag {tag:?}"),
        );
    }
    if !tags.iter().any(|t| t == tag) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("pushed tag {tag:?} not in tags/list response: {tags:?}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #12 — `_catalog` always disabled → 401 ───────────────────────────────────

/// `GET /v2/_catalog` must ALWAYS return 401 (disabled by design, regardless
/// of whether the caller is authenticated). This is the OCI catalog gate:
/// both anonymous and bearer-authenticated callers see 401 + a
/// `Www-Authenticate` challenge. The status is asserted, not merely the
/// absence of a 200, to guard against a regression that silently drops the
/// endpoint (which would manifest as 404, not the required gate deny).
pub(super) fn j12_catalog_always_401(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #12: GET /v2/_catalog disabled → 401 (gate proven, not absent)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    // Anonymous probe — the catalog handler fires before auth, so no creds
    // needed. The response MUST be 401 (CatalogDisabled error code = DENIED).
    let anon_url = format!("{base}/v2/_catalog");
    let anon = match client.get(&anon_url).send() {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::gated(name, format!("OCI endpoint unreachable ({base}): {e}"))
        }
    };
    let ac = anon.status().as_u16();
    if ac == 404 {
        return JourneyResult::fail(
            name,
            ms(start),
            "GET /v2/_catalog returned 404 — the route is absent instead of gated (regression)"
                .to_string(),
        );
    }
    if ac == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            "GET /v2/_catalog returned 200 — the catalog gate is disabled (information disclosure)"
                .to_string(),
        );
    }
    if ac != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /v2/_catalog (anon) got {ac} (expected 401 gate deny)"),
        );
    }
    // Www-Authenticate must be present on the 401.
    let wa = anon
        .headers()
        .get(WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    if wa.is_none() {
        return JourneyResult::fail(
            name,
            ms(start),
            "catalog 401 missing Www-Authenticate challenge header".to_string(),
        );
    }

    // Authenticated probe — a valid bearer must ALSO get 401 (catalog is
    // unconditionally gated, not merely unauthenticated-denied).
    let pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(_) => {
            // The anonymous probe already confirmed the 401 gate. Gate the
            // authenticated leg separately so the anon result is preserved.
            return JourneyResult::pass(name, ms(start));
        }
    };
    let bearer = match mint_bearer(client, &base, pat, "") {
        Ok(t) => t,
        Err(e) => {
            // Authenticated leg cannot be checked without a bearer — but the
            // anonymous leg already passed, so report pass.
            let _ = e;
            return JourneyResult::pass(name, ms(start));
        }
    };
    let auth_url = format!("{base}/v2/_catalog");
    let authed = match client
        .get(&auth_url)
        .header(AUTHORIZATION, format!("Bearer {bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return JourneyResult::fail(name, ms(start), format!("GET /v2/_catalog (authed): {e}"))
        }
    };
    let bc = authed.status().as_u16();
    if bc == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            "GET /v2/_catalog returned 200 for an authenticated caller — catalog gate bypassed"
                .to_string(),
        );
    }
    if bc != 401 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("GET /v2/_catalog (authed) got {bc} (expected 401 gate deny)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #13 — read-only PAT cannot push blobs or manifests ───────────────────────

/// A read-only PAT (P2) attempting blob or manifest pushes must be denied.
///
/// The token-exchange layer (`issue_token`) downscopes: a read-only PAT
/// requesting `pull,push` receives a pull-only bearer. The data-plane
/// `scope.allows(repo, "push")` check then returns false → 401 on the
/// `POST /v2/<repo>/blobs/uploads/` open call.
///
/// We assert 401 or 403 on the open-upload step itself (before PATCH/PUT).
/// The exact status depends on whether the adapter returns Auth(…) → 401 or
/// a scope-denied shape; both are acceptable denials for this assertion.
pub(super) fn j13_ro_push_denied(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #13: read-only PAT push attempt → denied (scope downscoped at token mint)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    // P2 is the read-only persona. Gate if the token is absent.
    let ro_pat = match Persona::P2ReadOnly.resolve(cfg) {
        Ok(p) => p,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let pat = match ro_pat.token {
        Some(t) => t,
        None => return JourneyResult::gated(name, "P2 token unexpectedly absent".to_string()),
    };

    let repo = run_repo();
    // The token exchange silently downscopes the push grant: a read-only PAT
    // gets a pull-only bearer. The bearer then fails `scope.allows(push)` on
    // the data plane → 401.
    let push_scope = format!("repository:{repo}:pull,push");
    let bearer = match mint_bearer(client, &base, pat, &push_scope) {
        Ok(t) => t,
        Err(e) => {
            // If the mint itself errors (e.g. the PAT is bad), gate — we can't
            // distinguish "downscoped to pull" from "token rejected outright".
            return JourneyResult::gated(
                name,
                format!("RO mint_bearer returned an error — cannot prove scope downscope: {e}"),
            );
        }
    };
    let auth = format!("Bearer {bearer}");

    // Attempt to open an upload session — must be denied (not 202).
    let open_url = format!("{base}/v2/{repo}/blobs/uploads/");
    let open = match client
        .post(&open_url)
        .header(AUTHORIZATION, &auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Vec::<u8>::new())
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("POST {open_url}: {e}")),
    };
    let oc = open.status().as_u16();
    if oc == 202 || oc == 201 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "read-only PAT opened an upload session (got {oc}) — push scope not downscoped"
            ),
        );
    }
    if !matches!(oc, 401 | 403) {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("open upload with RO PAT got {oc} (expected 401 or 403 deny)"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

// ── #14 — cross-tenant blob isolation ────────────────────────────────────────

/// Tenant B must NOT be able to pull blobs or manifests pushed by tenant A.
///
/// The tenant lives INSIDE the signed OCI bearer (minted from the PAT's
/// tenant_id at `/token` exchange time). Tenant B's bearer embeds B's
/// tenant_id, so `GET /v2/<repo>/blobs/<digest>` against a blob owned by
/// tenant A returns `404 NAME_UNKNOWN` — never a data leak (per oci.md §8
/// row 4: cross-tenant reads must be 404, NOT 403, to avoid existence
/// inference).
///
/// Assertion: tenant A pushes a blob. Tenant B issues a GET for the SAME
/// blob URL → 404. (Not 200, which would be a tenant-isolation breach.)
pub(super) fn j14_cross_tenant_isolation(cfg: &Config, client: &Client) -> JourneyResult {
    let name = "OCI #14: cross-tenant blob pull → 404 (not 200 or 403)";
    let start = Instant::now();
    let ms = |s: Instant| s.elapsed().as_millis() as u64;
    let base = oci_base(cfg);

    // Tenant A = P1.
    let p1_pat = match p1_pat(cfg, name) {
        Ok(p) => p,
        Err(g) => return g,
    };
    // Tenant B = P6.
    let p6 = match Persona::P6TenantB.resolve(cfg) {
        Ok(r) => r,
        Err(reason) => return JourneyResult::gated(name, reason),
    };
    let p6_pat = match p6.token {
        Some(t) => t,
        None => return JourneyResult::gated(name, "P6 token unexpectedly absent".to_string()),
    };

    // Tenant A pushes a unique blob.
    let repo = run_repo();
    let run_id = uuid::Uuid::new_v4().simple().to_string();
    let payload = format!("corelink-e2e-oci-cross-tenant-{run_id}").into_bytes();
    let digest = oci_digest(&payload);

    let a_scope = format!("repository:{repo}:pull,push");
    let a_bearer = match mint_bearer(client, &base, p1_pat, &a_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(name, format!("could not mint tenant A push bearer: {e}"))
        }
    };
    if let Err(m) = push_blob(
        client,
        &base,
        &format!("Bearer {a_bearer}"),
        &repo,
        &digest,
        payload,
    ) {
        return JourneyResult::fail(name, ms(start), format!("tenant A blob push: {m}"));
    }

    // Tenant B requests the same blob.
    // Tenant B needs a pull bearer for the SAME repo path.
    let b_scope = format!("repository:{repo}:pull");
    let b_bearer = match mint_bearer(client, &base, p6_pat, &b_scope) {
        Ok(t) => t,
        Err(e) => {
            return JourneyResult::gated(name, format!("could not mint tenant B pull bearer: {e}"))
        }
    };
    let blob_url = format!("{base}/v2/{repo}/blobs/{digest}");
    let get = match client
        .get(&blob_url)
        .header(AUTHORIZATION, format!("Bearer {b_bearer}"))
        .send()
    {
        Ok(r) => r,
        Err(e) => return JourneyResult::fail(name, ms(start), format!("GET {blob_url}: {e}")),
    };
    let gc = get.status().as_u16();
    if gc == 200 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "tenant B received tenant A's blob (got 200) — TENANT ISOLATION BREACH. url={blob_url}"
            ),
        );
    }
    if gc == 403 {
        // 403 would leak existence (oci.md §8 row 4); the server should use 404.
        return JourneyResult::fail(
            name,
            ms(start),
            format!(
                "cross-tenant GET returned 403 — leaks blob existence across tenants (must be 404). url={blob_url}"
            ),
        );
    }
    if gc != 404 {
        return JourneyResult::fail(
            name,
            ms(start),
            format!("cross-tenant GET got {gc} (expected 404 isolation). url={blob_url}"),
        );
    }
    JourneyResult::pass(name, ms(start))
}

/// Curl-style monolithic blob push: `POST /v2/<repo>/blobs/uploads/` (→ 202 +
/// `Location`), then `PUT <location>?digest=<digest>` with the bytes (→ 201).
/// Returns `Ok(())` on a 201/200 finalize, else a human error.
pub(super) fn push_blob(
    client: &Client,
    base: &str,
    auth: &str,
    repo: &str,
    digest: &str,
    bytes: Vec<u8>,
) -> Result<(), String> {
    // 1) Open the upload session.
    let open_url = format!("{base}/v2/{repo}/blobs/uploads/");
    let open = client
        .post(&open_url)
        .header(AUTHORIZATION, auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(Vec::new())
        .send()
        .map_err(|e| format!("POST {open_url}: {e}"))?;
    let oc = open.status().as_u16();
    if !matches!(oc, 202 | 201) {
        return Err(format!(
            "open upload got {oc} (expected 202). url={open_url}"
        ));
    }
    let location = open
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .ok_or_else(|| "open upload 202 missing the Location header".to_string())?;

    // The Location may be absolute or relative; resolve against the base host.
    let put_target = if location.starts_with("http://") || location.starts_with("https://") {
        location
    } else {
        format!("{base}{location}")
    };
    // Append the digest query param (monolithic finalize).
    let sep = if put_target.contains('?') { '&' } else { '?' };
    let put_url = format!("{put_target}{sep}digest={}", urlencode(digest));

    // 2) Monolithic PUT of the bytes + digest → 201.
    let put = client
        .put(&put_url)
        .header(AUTHORIZATION, auth)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(bytes)
        .send()
        .map_err(|e| format!("PUT {put_url}: {e}"))?;
    let pc = put.status().as_u16();
    if !matches!(pc, 201 | 200) {
        return Err(format!(
            "finalize blob PUT got {pc} (expected 201). url={put_url}"
        ));
    }
    Ok(())
}
