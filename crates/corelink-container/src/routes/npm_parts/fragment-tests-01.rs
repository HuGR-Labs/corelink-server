    #[tokio::test]
    async fn metadata_cache_hit_round_trip_with_tenant_stripped() {
        // Mint a real PAT; seed the metadata KV (PUBLIC namespace, unscoped
        // package) so a GET /<pkg> is a cache HIT (no upstream). Proves
        // end-to-end: nest_service mount + tenant-strip + scope + Option-B
        // resolve + KV get + JSON passthrough.
        let key = test_key();
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(Uuid::from_u128(0xBEEF)),
            PrincipalId(Uuid::from_u128(0xF00D)),
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

        // Seed the metadata KV under PUBLIC (unscoped `lodash`). The bytes must
        // be VALID JSON: the cache-hit path re-validates the cached payload
        // (`serve_metadata` → `validate_metadata_json`) and fail-CLOSEs (502) on
        // malformed JSON.
        let meta_json = serde_json::to_vec(&serde_json::json!({
            "name": "lodash",
            "versions": {},
        }))
        .unwrap();
        let kv_backend = Arc::new(FakeKv::default());
        kv_backend.0.lock().unwrap().insert(
            (PUBLIC_NAMESPACE.to_owned(), kv_key_for_pkg("lodash")),
            (meta_json.clone(), now_ms()),
        );

        let cas = Arc::new(StubCas::default());
        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            Arc::new(FakeMap::default()),
            npm_kv(kv_backend),
            verifier,
            None,
        );

        // path-tenant \"ignored\" ≠ the PAT tenant → proves the path tenant is
        // stripped + untrusted (unscoped metadata uses the shared PUBLIC ns).
        let resp = app
            .oneshot(get("/npm/ignored/lodash", Some(&pt), Some(SCOPE_RW)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), meta_json.as_slice());
    }
    #[tokio::test]
    async fn tarball_cache_hit_round_trip_through_moat() {
        // Seed BOTH the metadata KV (needed for dist.shasum lookup) and the
        // 2-level moat (tarball bytes) so a GET /<pkg>/-/<file> is a full
        // cache HIT served from the moat with NO upstream. Proves the
        // CasStore→MoatCache wiring end-to-end.
        let key = test_key();
        let tenant_uuid = Uuid::from_u128(0xCAFE);
        let (plaintext, pat) = mint(
            PatEnv::Pat,
            PatTenantId(tenant_uuid),
            PrincipalId(Uuid::from_u128(0xD00D)),
            PatScopes::from_u64(SCOPE_CACHE_RW),
            None,
            &key,
            1,
        )
        .unwrap();
        let pt = plaintext.into_string();
        // The tarball-byte namespace is the PAT's tenant (the security-fix
        // isolation), NOT PUBLIC. `NpmMoatStore` derives it from the typed
        // `TenantId`, which is the parsed UUID text of `pat.tenant_id`.
        let tenant_ns = TenantId::from_uuid(tenant_uuid).to_string();
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

        let tarball_bytes = b"\x1f\x8bfake-tarball".to_vec();
        // The adapter verifies SHA1(bytes) == dist.shasum before serving even
        // on a CAS hit? No — on a CAS hit it serves directly. But it STILL
        // re-parses metadata to derive the version + dist.shasum, so the KV
        // must contain a matching version entry. Compute the real sha1.
        let shasum = corelink_adapter_host::npm::tarball::sha1_hex(&tarball_bytes);
        let meta_json = serde_json::to_vec(&serde_json::json!({
            "name": "lodash",
            "versions": {
                "4.17.21": { "dist": { "shasum": shasum } }
            }
        }))
        .unwrap();

        let kv_backend = Arc::new(FakeKv::default());
        kv_backend.0.lock().unwrap().insert(
            (PUBLIC_NAMESPACE.to_owned(), kv_key_for_pkg("lodash")),
            (meta_json, now_ms()),
        );

        // The tarball CAS key is SHA256(tarball-URL); the adapter builds the
        // URL as `<registry>/<pkg>/-/<file>`.
        let tarball_url = format!("{}/lodash/-/lodash-4.17.21.tgz", DEFAULT_UPSTREAM_REGISTRY);
        let digest = tarball_url_digest(&tarball_url).unwrap();
        let url_hash = digest.to_hex();
        let content_hash = canonical_hash_hex(&tarball_bytes);

        // Seed both moat levels under the PER-TENANT namespace — tarball bytes
        // are tenant-isolated by the security fix (NOT PUBLIC). This also
        // proves the moat get is keyed by the PAT tenant, not the path tenant.
        let cas = Arc::new(StubCas::default());
        cas.0.lock().unwrap().insert(
            (tenant_ns.clone(), content_hash.clone()),
            tarball_bytes.clone(),
        );
        let map = Arc::new(FakeMap::default());
        map.0
            .lock()
            .unwrap()
            .insert((tenant_ns.clone(), url_hash), content_hash);

        let app = router(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            npm_kv(kv_backend),
            verifier,
            None,
        );

        let resp = app
            .oneshot(get(
                "/npm/ignored/lodash/-/lodash-4.17.21.tgz",
                Some(&pt),
                Some(SCOPE_RW),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), tarball_bytes.as_slice());
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
