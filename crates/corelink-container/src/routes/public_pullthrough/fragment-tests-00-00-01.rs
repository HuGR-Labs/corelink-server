            0,
            "an index must not fetch blobs"
        );
        assert!(r.cas.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn tenant_a_resolution_is_not_visible_to_tenant_b() {
        let r = rig();
        let ta = tenant(10);
        let tb = tenant(11);
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        r.manifest_fetcher.insert(
            repo,
            "latest",
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let _ = r
            .resolver
            .resolve_on_miss(&ta, repo, "latest")
            .await
            .unwrap();
        // Tenant A's manifest is in A's KV, NOT B's.
        let kv = r.kv.0.lock().unwrap();
        assert!(kv.contains_key(&(ta.to_canonical_text(), manifest_key(repo, "latest"))));
        assert!(!kv.contains_key(&(tb.to_canonical_text(), manifest_key(repo, "latest"))));
    }

    #[tokio::test]
    async fn oversize_manifest_fails_open() {
        let r = rig();
        let t = tenant(5);
        let repo = "library/alpine";
        // A manifest body just over the cap → Ok(None), nothing stored.
        let big = vec![b'x'; MAX_PULLTHROUGH_MANIFEST_BYTES + 1];
        r.manifest_fetcher.insert(
            repo,
            "huge",
            FetchedManifest {
                bytes: big,
                content_type: None,
                docker_content_digest: None,
            },
        );
        let out = r.resolver.resolve_on_miss(&t, repo, "huge").await.unwrap();
        assert!(out.is_none(), "over-size manifest must fail open");
        assert!(r.kv.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn oversize_blob_fails_open() {
        let r = rig();
        let t = tenant(6);
        let repo = "library/alpine";
        let big_layer = vec![b'y'; MAX_PULLTHROUGH_BLOB_BYTES + 1];
        let config_bytes = b"cfg".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&big_layer);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        r.manifest_fetcher.insert(
            repo,
            "latest",
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, big_layer);

        let out = r
            .resolver
            .resolve_on_miss(&t, repo, "latest")
            .await
            .unwrap();
        assert!(
            out.is_none(),
            "an over-size layer blob must fail the resolve open"
        );
        // The manifest was NOT persisted (blobs are persisted first; a failure
        // aborts before the manifest write).
        assert!(
            r.kv.0.lock().unwrap().is_empty(),
            "no manifest stored when a blob is over-size"
        );
    }

    #[tokio::test]
    async fn concurrent_identical_misses_fetch_upstream_once() {
        let r = Arc::new(rig());
        let t = tenant(7);
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        r.manifest_fetcher.insert(
            repo,
            "latest",
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let mut set = tokio::task::JoinSet::new();
        for _ in 0..24 {
            let rr = Arc::clone(&r);
            set.spawn(async move { rr.resolver.resolve_on_miss(&t, repo, "latest").await });
        }
        while let Some(res) = set.join_next().await {
            assert!(
                res.unwrap().unwrap().is_some(),
                "every coalesced miss resolves"
            );
        }
        assert_eq!(
            r.manifest_fetcher.call_count(),
            1,
            "concurrent identical misses must coalesce into ONE upstream fetch"
        );
    }

    // ── M2 `_public` cross-tenant tests ──────────────────────────────────────

    /// True iff `(PUBLIC_NAMESPACE, url_hash)` has a map row (the digest was
    /// promoted into `_public`).
    fn in_public(rig: &Rig, url_hash: &str) -> bool {
        rig.map
            .0
            .lock()
            .unwrap()
            .contains_key(&(PUBLIC_NAMESPACE.to_owned(), url_hash.to_owned()))
    }

    /// Count of distinct `_public` map rows (promoted manifests + blobs).
    fn public_row_count(rig: &Rig) -> usize {
        rig.map
            .0
            .lock()
            .unwrap()
            .keys()
            .filter(|(ns, _)| ns == PUBLIC_NAMESPACE)
            .count()
    }

    /// Allowlisted IMAGE digest, dedup ON: the config + layer + the manifest are
    /// promoted to `_public`, served — and a SECOND tenant that never pushed it,
    /// resolving the SAME digest, is served from `_public` with ZERO extra
    /// upstream fetch. This is the cross-tenant proof.
    #[tokio::test]
    async fn allowlisted_image_promotes_closure_and_serves_cross_tenant() {
        let repo = "library/alpine";
        let config_bytes = b"cfg-object".to_vec();
        let layer_bytes = b"layer-bytes".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        let r = rig_full(generous_limiter(), allowlist_of(&[&manifest_digest]), true);
        r.manifest_fetcher.insert(
            repo,
            &manifest_digest,
            FetchedManifest {
                bytes: manifest.clone(),
                content_type: Some("application/vnd.oci.image.manifest.v1+json".to_owned()),
                docker_content_digest: Some(manifest_digest.clone()),
            },
        );
        r.blob_fetcher
            .insert(repo, &config_digest, config_bytes.clone());
        r.blob_fetcher
            .insert(repo, &layer_digest, layer_bytes.clone());

        // Tenant A (by-digest FROM) → promote closure to `_public`, serve it.
        let ta = tenant(100);
        let out = r
            .resolver
            .resolve_on_miss(&ta, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("allowlisted image resolves from the promote");
        assert_eq!(out.digest, manifest_digest);
        assert_eq!(out.bytes.as_ref(), manifest.as_slice());
        assert_eq!(
            r.manifest_fetcher.call_count(),
            1,
            "one manifest fetch for A"
        );

        // Full closure lives in `_public`: manifest + config + layer.
        assert!(in_public(&r, &manifest_digest), "manifest in `_public`");
        assert!(in_public(&r, &config_digest), "config in `_public`");
        assert!(in_public(&r, &layer_digest), "layer in `_public`");
        // Nothing landed in tenant A's OWN namespace (cross-tenant, not per-tenant).
        assert!(
            r.kv.0.lock().unwrap().is_empty(),
            "an allowlisted `_public` promote does NOT write per-tenant KV"
        );
        assert!(
            !r.cas
                .0
                .lock()
                .unwrap()
                .keys()
                .any(|(ns, _)| *ns == ta.to_canonical_text()),
            "no per-tenant CAS write on the `_public` promote"
        );

        // Tenant B (never pushed alpine) resolves the SAME digest → served from
        // `_public`, ZERO extra upstream fetch (the cross-tenant proof).
        let tb = tenant(101);
        let out_b = r
            .resolver
            .resolve_on_miss(&tb, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("tenant B served from `_public`");
        assert_eq!(out_b.bytes.as_ref(), manifest.as_slice());
        assert_eq!(
            r.manifest_fetcher.call_count(),
            1,
            "tenant B must be served from `_public` with NO extra upstream fetch"
        );
    }
