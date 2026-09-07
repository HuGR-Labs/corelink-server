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
pub(in crate::journeys::oci) fn j10_chunked_patch_upload(cfg: &Config, client: &Client) -> JourneyResult {
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
            );
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
            );
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
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PATCH chunk1 {session_url}: {e}"),
            );
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
            return JourneyResult::fail(
                name,
                ms(start),
                format!("PATCH chunk2 {session_url}: {e}"),
            );
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
            return JourneyResult::fail(name, ms(start), format!("PUT finalize {put_url}: {e}"));
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
            );
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
            );
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
