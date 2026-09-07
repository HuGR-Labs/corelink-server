use super::*;

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
            );
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
            return JourneyResult::fail(name, ms(start), format!("PUT manifest {put_url}: {e}"));
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
            );
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
            );
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
            );
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
