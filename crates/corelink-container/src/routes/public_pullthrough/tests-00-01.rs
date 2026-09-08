    /// Allowlisted INDEX digest, dedup ON: the per-arch child (skip
    /// `unknown/unknown`) + its blobs + the index are all promoted to `_public`;
    /// a by-digest GET of the child — which is NOT individually allowlisted —
    /// serves from `_public` (the existence read).
    #[tokio::test]
    async fn allowlisted_index_promotes_children_and_child_serves_from_public() {
        let repo = "library/debian";
        let config_bytes = b"debian-cfg".to_vec();
        let layer_bytes = b"debian-layer".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let child_manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let child_digest = sha256_wire(&child_manifest);
        // Index with one real linux/amd64 child + a `windows/amd64` child + an
        // `unknown/unknown` attestation entry. ONLY the linux child is promoted;
        // the windows + attestation entries MUST be skipped (never fetched — their
        // digests are intentionally NOT registered with the fetcher, so any fetch
        // of them would error and abort the promote).
        let index = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": child_digest,
                    "size": 1,
                    "platform": { "os": "linux", "architecture": "amd64" }
                },
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": sha256_wire(b"windows-image-manifest"),
                    "size": 1,
                    "platform": { "os": "windows", "architecture": "amd64" }
                },
                {
                    "mediaType": "application/vnd.oci.image.manifest.v1+json",
                    "digest": sha256_wire(b"attestation-blob"),
                    "size": 1,
                    "platform": { "os": "unknown", "architecture": "unknown" }
                }
            ]
        }))
        .unwrap();
        let index_digest = sha256_wire(&index);

        let r = rig_full(generous_limiter(), allowlist_of(&[&index_digest]), true);
        r.manifest_fetcher.insert(
            repo,
            &index_digest,
            FetchedManifest {
                bytes: index.clone(),
                content_type: Some("application/vnd.oci.image.index.v1+json".to_owned()),
                docker_content_digest: Some(index_digest.clone()),
            },
        );
        r.manifest_fetcher.insert(
            repo,
            &child_digest,
            FetchedManifest {
                bytes: child_manifest.clone(),
                content_type: Some("application/vnd.oci.image.manifest.v1+json".to_owned()),
                docker_content_digest: Some(child_digest.clone()),
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let ta = tenant(110);
        let out = r
            .resolver
            .resolve_on_miss(&ta, repo, &index_digest)
            .await
            .unwrap()
            .expect("index resolves via promote");
        assert_eq!(out.digest, index_digest);
        // index + the ONE linux child fetched (2); the windows + attestation
        // entries are skipped (never fetched).
        assert_eq!(
            r.manifest_fetcher.call_count(),
            2,
            "only the index + its one LINUX child are fetched (windows + attestation skipped)"
        );
        // Closure fully in `_public`: index, child manifest, config, layer.
        assert!(in_public(&r, &index_digest));
        assert!(in_public(&r, &child_digest));
        assert!(in_public(&r, &config_digest));
        assert!(in_public(&r, &layer_digest));

        // A by-digest GET of the child (NOT individually allowlisted) serves from
        // `_public` via the existence read — for a tenant that never pushed it.
        let tb = tenant(111);
        let child_out = r
            .resolver
            .resolve_on_miss(&tb, repo, &child_digest)
            .await
            .unwrap()
            .expect("child manifest served from `_public` by existence");
        assert_eq!(child_out.bytes.as_ref(), child_manifest.as_slice());
        assert_eq!(
            r.manifest_fetcher.call_count(),
            2,
            "the child existence read must NOT trigger any upstream fetch"
        );
    }

    /// UN-allowlisted digest, dedup ON: nothing enters `_public`; the M1
    /// per-tenant path is taken.
    #[tokio::test]
    async fn unallowlisted_digest_dedup_on_stays_per_tenant() {
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        // dedup ON but the allowlist does NOT contain this digest.
        let r = rig_full(
            generous_limiter(),
            allowlist_of(&[&sha256_wire(b"some-other-image")]),
            true,
        );
        r.manifest_fetcher.insert(
            repo,
            &manifest_digest,
            FetchedManifest {
                bytes: manifest.clone(),
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let t = tenant(120);
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("un-allowlisted digest resolves per-tenant (M1)");
        assert_eq!(out.digest, manifest_digest);
        assert_eq!(public_row_count(&r), 0, "nothing in `_public`");
        // Per-tenant path: manifest in the tenant's OWN KV.
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, &manifest_digest))));
    }

    /// TAG reference, dedup ON: never touches `_public` (per-tenant only) even
    /// when a homograph digest is allowlisted — a tag is mutable.
    #[tokio::test]
    async fn tag_reference_dedup_on_never_public() {
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        // The manifest's digest IS allowlisted, but the request is by TAG.
        let r = rig_full(generous_limiter(), allowlist_of(&[&manifest_digest]), true);
        r.manifest_fetcher.insert(
            repo,
            "3.20",
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: Some(manifest_digest.clone()),
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let t = tenant(130);
        let _ = r
            .resolver
            .resolve_on_miss(&t, repo, "3.20")
            .await
            .unwrap()
            .expect("tag resolves per-tenant");
        assert_eq!(public_row_count(&r), 0, "a TAG never writes `_public`");
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, "3.20"))));
    }

    /// dedup OFF, even an allowlisted digest: no `_public` reads/writes — the
    /// resolver is byte-identical to M1 (this is the explicit dedup-OFF proof;
    /// the whole M1 suite runs on `rig()` which is dedup OFF too).
    #[tokio::test]
    async fn dedup_off_allowlisted_digest_no_public() {
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        // Allowlisted, but dedup is OFF → per-tenant.
        let r = rig_full(generous_limiter(), allowlist_of(&[&manifest_digest]), false);
        r.manifest_fetcher.insert(
            repo,
            &manifest_digest,
            FetchedManifest {
                bytes: manifest,
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let t = tenant(140);
        let _ = r
            .resolver
            .resolve_on_miss(&t, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("resolves per-tenant with dedup OFF");
        assert_eq!(public_row_count(&r), 0, "dedup OFF ⇒ no `_public` writes");
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, &manifest_digest))));
    }

    /// Over the `_public` growth ceiling: no promote (fail-open to per-tenant).
    #[tokio::test]
    async fn over_ceiling_no_public_promote_fails_open() {
        let repo = "library/alpine";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);

        let mut r = rig_full(generous_limiter(), allowlist_of(&[&manifest_digest]), true);
        // Force the ceiling to zero: `0 >= 0` ⇒ every promote is refused.
        r.resolver.public_ceiling_bytes = 0;
        r.manifest_fetcher.insert(
            repo,
            &manifest_digest,
            FetchedManifest {
                bytes: manifest.clone(),
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let t = tenant(150);
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, &manifest_digest)
            .await
            .unwrap()
            .expect("over-ceiling still resolves per-tenant (fail-open)");
        assert_eq!(out.digest, manifest_digest);
        assert_eq!(
            public_row_count(&r),
            0,
            "over-ceiling ⇒ no `_public` promote"
        );
        // Fell through to the per-tenant M1 path.
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, &manifest_digest))));
    }

    /// A broken child fetch mid-closure ⇒ nothing partial served, and the index
    /// ROOT is NOT left in `_public` (so a later existence read cannot serve an
    /// incomplete closure as complete). The resolve fails open to per-tenant.
    #[tokio::test]
    async fn broken_child_mid_closure_leaves_no_public_root() {
        let repo = "library/debian";
        let config_bytes = b"cfg".to_vec();
        let layer_bytes = b"lyr".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let child_manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let child_digest = sha256_wire(&child_manifest);
        let index = index_bytes(&child_digest);
        let index_digest = sha256_wire(&index);

        let r = rig_full(generous_limiter(), allowlist_of(&[&index_digest]), true);
        // The index is fetchable, but the CHILD manifest is NOT (upstream fetch
        // fails) → the closure promote aborts.
        r.manifest_fetcher.insert(
            repo,
            &index_digest,
            FetchedManifest {
                bytes: index,
                content_type: Some("application/vnd.oci.image.index.v1+json".to_owned()),
                docker_content_digest: Some(index_digest.clone()),
            },
        );
        // (child_digest intentionally NOT inserted → fetch_manifest errs.)
        // Also make the index itself resolvable per-tenant on the fail-open path.

        let t = tenant(160);
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, &index_digest)
            .await
            .unwrap();
        // Fell open to per-tenant (the index IS fetchable there → resolves), but
        // CRUCIALLY the index root is NOT in `_public`.
        assert!(
            !in_public(&r, &index_digest),
            "a mid-closure failure must NOT leave the index root in `_public`"
        );
        // Whatever the per-tenant outcome, no `_public` root exists for a later
        // existence read to serve as a complete closure.
        assert!(
            out.is_some(),
            "index still resolves per-tenant on the fail-open path"
        );
        // The index was stored per-tenant, never as a `_public` root.
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, &index_digest))));
    }
