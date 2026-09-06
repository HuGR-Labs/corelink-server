#[tokio::test]
async fn token_backend_fault_is_opaque_to_unauth_caller() {
    // A24 (UNAUTH-reachable, highest priority): the OCI `/token` leg runs
    // the PAT-verify backend. When that backend faults, the public response
    // must NOT echo the raw CF D1 API error (status / body / SQL). An
    // UNauthenticated caller (it presents only a syntactically-valid PAT in
    // Basic, never a verified credential) must learn nothing about the
    // internal store. The OCI error envelope SHAPE is preserved; only the
    // `message` content is scrubbed to an opaque, ref-tagged string.
    let key = test_key();
    // A real (HMAC-valid) PAT so verification proceeds PAST the cheap
    // fast-reject and actually hits the (faulting) D1 lookup.
    let tenant_uuid = Uuid::from_u128(0xA24);
    let (plaintext, _pat) = mint(
        PatEnv::Pat,
        PatTenantId(tenant_uuid),
        PrincipalId(Uuid::from_u128(0xBEEF)),
        PatScopes::from_u64(SCOPE_CACHE_RW),
        None,
        &key,
        1,
    )
    .unwrap();
    let pt = plaintext.into_string();
    let verifier = Arc::new(PatVerifier::new(Arc::new(BackendErrLookup), key));
    let cas = Arc::new(StubCas::default());
    let app = router(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(OciKvFake::default()),
        verifier,
        SecretWrap::new(OCI_KEY.to_owned()),
        None,
        None,
        None, // cap resolver: tests use StubCas (no byte-accounting); cap is inert
        None, // suspend resolver: off (dedicated G4b suspend tests below)
    );

    let resp = app
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:alpine:pull",
            Some(&basic(&pt)),
            None,
        ))
        .await
        .unwrap();
    // The exchange fails (backend fault) — but the wire body must be clean.
    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8_lossy(&body_bytes);

    // Envelope SHAPE preserved (clients depend on it): { "errors": [...] }.
    let json: serde_json::Value =
        serde_json::from_slice(&body_bytes).expect("OCI error envelope must remain valid JSON");
    assert!(
        json.get("errors").and_then(|e| e.as_array()).is_some(),
        "OCI error envelope shape must be preserved"
    );

    // NO internal detail crosses the wire (the heart of A24).
    for needle in [
        "D1",
        "HTTP 500",
        "no such table",
        "SELECT",
        "FROM pat",
        "token_id",
        "errors\":[{\"code\":7500", // raw CF error object
        "backend",
    ] {
        assert!(
            !body.contains(needle),
            "A24: scrubbed /token body leaked internal detail {needle:?}; got: {body}"
        );
    }
    // It IS the opaque, code-keyed message + a correlation ref.
    assert!(
        body.contains("authentication failed") && body.contains("ref:"),
        "A24: expected opaque ref-tagged message, got: {body}"
    );
    // The status is the auth-failure shape (401), unchanged.
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[test]
fn oci_bearer_tenant_resolves_only_a_verified_bearer() {
    // rt-nuclear #2/#8/#9: the $-ceiling cost-attribution tenant is recovered
    // by VERIFYING the HMAC bearer (not a request header the Worker strips,
    // and not an unverified token claim that would let one tenant bill
    // another).
    use corelink_adapter_host::oci::auth::{mint, OciScope};
    let key = SecretWrap::new("0123456789abcdef0123456789abcdef".to_owned());
    let tenant =
        TenantId::from_uuid(Uuid::parse_str("00000000-0000-4000-8000-0000000abcde").expect("uuid"));
    let scope = OciScope::new("t/img", vec!["push".to_owned(), "pull".to_owned()]);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let token = mint(&key, &tenant, &scope, Some(0), now, 300).expect("mint");

    let mut h = axum::http::HeaderMap::new();
    h.insert(
        axum::http::header::AUTHORIZATION,
        format!("Bearer {token}").parse().expect("hv"),
    );
    // valid bearer → its tenant
    assert_eq!(
        oci_bearer_tenant(&key, &h).as_deref(),
        Some(tenant.to_canonical_text().as_str()),
        "a verified bearer must resolve to its tenant",
    );
    // forged bearer (wrong realm key) → None (cannot bill a victim)
    let attacker_key = SecretWrap::new("fedcba9876543210fedcba9876543210".to_owned());
    assert!(
        oci_bearer_tenant(&attacker_key, &h).is_none(),
        "a bearer that fails HMAC verify must NOT resolve a tenant",
    );
    // absent bearer → None (the data plane 401s the write; left uncharged)
    assert!(
        oci_bearer_tenant(&key, &axum::http::HeaderMap::new()).is_none(),
        "no Authorization header ⇒ no tenant",
    );
}

// ── rt-nuclear #8: OCI write metered against the monthly request-count cap ──

/// In-memory [`RequestCountStore`](crate::request_count::RequestCountStore)
/// for the OCI gate test; pre-seedable to drive the over-cap (429) path.
#[derive(Debug, Default)]
struct GateCounter(Mutex<HashMap<(String, String), i64>>);
#[async_trait]
impl crate::request_count::RequestCountStore for GateCounter {
    async fn increment(
        &self,
        tenant_id: &str,
        year_month: &str,
        _now_ms: i64,
    ) -> Result<i64, String> {
        let mut m = self.0.lock().unwrap();
        let c = m
            .entry((tenant_id.to_owned(), year_month.to_owned()))
            .or_insert(0);
        *c += 1;
        Ok(*c)
    }
}

/// Fixed-tier resolver for the OCI gate test.
#[derive(Debug)]
struct GateTier(&'static str);
#[async_trait]
impl crate::request_count::TierResolver for GateTier {
    async fn tier(&self, _tenant_id: &str) -> Result<String, String> {
        Ok(self.0.to_owned())
    }
}

#[tokio::test]
async fn oci_write_over_request_count_cap_is_429_keyed_on_bearer_tenant() {
    // rt-nuclear #8 (request-count half): an OCI WRITE method is metered
    // against the tenant's monthly request-count cap (`monthly_request_counts`),
    // keyed on the SAME verified-HMAC-bearer tenant the $-ceiling gate uses
    // (#318). The Worker forwards OCI RAW and never counts these, so the
    // container must. Pre-seed the counter to the free cap so the write is
    // the (cap+1)-th request → the gate rejects it 429 BEFORE the adapter.
    let key = test_key();
    let tenant_uuid = Uuid::from_u128(0xD00D);
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        PatTenantId(tenant_uuid),
        PrincipalId(Uuid::from_u128(0x5151)),
        PatScopes::from_u64(SCOPE_CACHE_RW),
        None,
        &key,
        1,
    )
    .unwrap();
    let pt = plaintext.into_string();
    let lookup = OneTokenLookup {
        token_id: pat.token_id.as_str().to_owned(),
        row: PatRow {
            tenant_id: pat.tenant_id.0.to_string(),
            pat_hash: pat.hash.as_str().to_owned(),
            scope: SCOPE_RW.to_owned(),
            find_only: false,
            runner_job: false,
        },
    };
    let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

    // Pre-seed the request-count store at the free cap, keyed on the bearer
    // tenant's canonical UUID text + the CURRENT UTC month (the gate derives
    // the same bucket from the system clock).
    let store = Arc::new(GateCounter::default());
    let now_ms = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    let tenant_text = TenantId::from_uuid(tenant_uuid).to_canonical_text();
    let year_month = crate::request_count::year_month_utc(now_ms);
    store.0.lock().unwrap().insert(
        (tenant_text, year_month),
        crate::request_count::CAP_FREE, // next op is the (cap+1)-th
    );

    let rc_gate = crate::request_count::RequestCountGate::new(
        store,
        Arc::new(GateTier("free")),
        Arc::new(crate::wall_clock::SystemWallClock::new()),
    );

    let cas = Arc::new(StubCas::default());
    let app = router(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(OciKvFake::default()),
        verifier,
        SecretWrap::new(OCI_KEY.to_owned()),
        None,          // no $-ceiling gate in this test
        Some(rc_gate), // request-count gate under test
        None,          // cap resolver inert (StubCas)
        None,          // suspend resolver: off in this test
    );

    // Exchange the PAT for a push,pull bearer.
    let resp = app
        .clone()
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:alpine:push,pull",
            Some(&basic(&pt)),
            Some(SCOPE_RW),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let bearer = json["token"].as_str().expect("token field").to_owned();

    // A WRITE (manifest PUT) with the valid bearer → over the cap → 429.
    // The gate runs as the outer layer, so it rejects BEFORE the adapter.
    let put = HttpRequest::builder()
        .method(Method::PUT)
        .uri("/v2/alpine/manifests/latest")
        .header("authorization", format!("Bearer {bearer}"))
        .header("content-type", "application/vnd.oci.image.manifest.v1+json")
        .body(Body::from(r#"{"schemaVersion":2}"#))
        .unwrap();
    let resp = app.clone().oneshot(put).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "an OCI write over the monthly request-count cap must be 429"
    );
    assert!(
        resp.headers().contains_key(axum::http::header::RETRY_AFTER),
        "a 429 must carry Retry-After (next-month-start)"
    );

    // A READ (GET) over the cap is ALSO 429 now (rt-nuclear r34 #1/#11):
    // `docker pull` is billable work, so it is metered on the request-count
    // axis exactly like a write. (Pre-seed left the counter AT the cap; the
    // earlier write was rejected 402-style/429 before incrementing, so the
    // GET is still the (cap+1)-th countable op → over the cap → 429.)
    let get = req(
        Method::GET,
        "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
        Some(&format!("Bearer {bearer}")),
        None,
    );
    let resp = app.oneshot(get).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "an OCI read over the monthly request-count cap must be 429 (reads are now metered)"
    );
}

#[tokio::test]
async fn oci_read_increments_request_count_and_write_still_counts() {
    // rt-nuclear r34 #1/#11 regression: an OCI READ (GET/HEAD) with a valid
    // bearer increments the monthly request-count gate (closing the
    // `docker pull` → unmetered exploit), and a WRITE still charges the
    // request-count axis, both keyed on the verified-HMAC-bearer tenant. We
    // observe the in-memory store directly to prove each increment. (The
    // write-only $-ceiling axis is covered by
    // `oci_write_over_request_count_cap_is_429_keyed_on_bearer_tenant` and the
    // $-ceiling 402 tests; this edit leaves the write $-path untouched.)
    let key = test_key();
    let tenant_uuid = Uuid::from_u128(0xBEEF);
    let (plaintext, pat) = mint(
        PatEnv::Pat,
        PatTenantId(tenant_uuid),
        PrincipalId(Uuid::from_u128(0x7171)),
        PatScopes::from_u64(SCOPE_CACHE_RW),
        None,
        &key,
        1,
    )
    .unwrap();
    let pt = plaintext.into_string();
    let lookup = OneTokenLookup {
        token_id: pat.token_id.as_str().to_owned(),
        row: PatRow {
            tenant_id: pat.tenant_id.0.to_string(),
            pat_hash: pat.hash.as_str().to_owned(),
            scope: SCOPE_RW.to_owned(),
            find_only: false,
            runner_job: false,
        },
    };
    let verifier = Arc::new(PatVerifier::new(Arc::new(lookup), key));

    // Fresh store (counter starts at 0, well under the cap).
    let store = Arc::new(GateCounter::default());
    let now_ms = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    let tenant_text = TenantId::from_uuid(tenant_uuid).to_canonical_text();
    let year_month = crate::request_count::year_month_utc(now_ms);
    let bucket = (tenant_text.clone(), year_month.clone());

    let rc_gate = crate::request_count::RequestCountGate::new(
        Arc::clone(&store) as Arc<dyn crate::request_count::RequestCountStore>,
        Arc::new(GateTier("free")),
        Arc::new(crate::wall_clock::SystemWallClock::new()),
    );

    let cas = Arc::new(StubCas::default());
    let app = router(
        Arc::clone(&cas) as Arc<dyn CasReadHandler>,
        cas as Arc<dyn CasWriteHandler>,
        Arc::new(FakeMap::default()),
        Arc::new(OciKvFake::default()),
        verifier,
        SecretWrap::new(OCI_KEY.to_owned()),
        None,          // no $-ceiling gate in this test (write $-path covered elsewhere)
        Some(rc_gate), // request-count gate under test (now charged on reads too)
        None,          // cap resolver inert (StubCas)
        None,          // suspend resolver: off in this test
    );

    // Exchange the PAT for a push,pull bearer.
    let resp = app
        .clone()
        .oneshot(req(
            Method::GET,
            "/token?scope=repository:alpine:push,pull",
            Some(&basic(&pt)),
            Some(SCOPE_RW),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let bearer = json["token"].as_str().expect("token field").to_owned();

    let count =
        |b: &(String, String)| -> i64 { store.0.lock().unwrap().get(b).copied().unwrap_or(0) };
    assert_eq!(count(&bucket), 0, "no countable op yet");

    // A READ (GET blob) with the valid bearer → reaches the adapter (404 for
    // the absent blob) AND increments the request-count gate.
    let get = req(
        Method::GET,
        "/v2/alpine/blobs/sha256:0000000000000000000000000000000000000000000000000000000000000000",
        Some(&format!("Bearer {bearer}")),
        None,
    );
    let resp = app.clone().oneshot(get).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "the read is authorized + reaches the adapter (absent blob ⇒ 404)"
    );
    assert_eq!(
        count(&bucket),
        1,
        "an OCI read with a valid bearer MUST increment the request-count gate"
    );

    // A WRITE (manifest PUT) → charges the request-count axis again (now 2),
    // confirming writes are still metered after the read-metering change.
    let put = HttpRequest::builder()
        .method(Method::PUT)
        .uri("/v2/alpine/manifests/latest")
        .header("authorization", format!("Bearer {bearer}"))
        .header("content-type", "application/vnd.oci.image.manifest.v1+json")
        .body(Body::from(r#"{"schemaVersion":2}"#))
        .unwrap();
    let resp = app.oneshot(put).await.unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "under the cap, the write is not 429'd"
    );
    assert_eq!(
        count(&bucket),
        2,
        "an OCI write MUST still increment the request-count gate"
    );
}

// This fragment is included from the OCI test module but owns its path
// resolution; keep the pre-existing G4b fixture under the source-local
// tests directory explicit.
#[path = "tests/oci_g4b_tests.rs"]
mod g4b_tests;

// External, sibling guard for the path-loaded G4b fixture. Keep this outside
// `g4b_tests`: removing/renaming the `#[path]` module must become a compile/test
// failure instead of silently dropping five suspend regressions.
#[cfg(test)]
mod b126_m2_g4b_reanchor {
    #[test]
    fn path_loaded_g4b_fixture_is_load_bearing() {
        assert_eq!(
            super::g4b_tests::B126_M2_OCI_G4B_WIRING_SENTINEL,
            "oci-g4b-tests-wired-v1"
        );
    }
}

#[allow(dead_code)]
const B126_M2_TEST_1_3_REANCHOR: () = ();
