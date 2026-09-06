
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use corelink_handler_cas::{
        CasHandlerError, CasReadHandler, CasReadRequest, CasReadResponse, CasWriteHandler,
        CasWriteRequest, CasWriteResponse,
    };

    use crate::adapter_cache::{UrlMapStore, PUBLIC_NAMESPACE};
    use crate::routes::public_mirror::FetchedManifest;

    use super::*;

    const SERVICE_PRINCIPAL: &str = "oci-pullthrough-test";

    /// Canonical `sha256:<hex>` of `bytes`.
    fn sha256_wire(bytes: &[u8]) -> String {
        OciDigest::compute(OciDigestAlgo::Sha256, bytes)
            .expect("sha256 compute")
            .to_wire()
    }

    // ── Fakes ────────────────────────────────────────────────────────────────

    /// Fake manifest fetcher: returns canned `FetchedManifest`s keyed by
    /// `(repo, reference)`, counting every call.
    #[derive(Debug, Default)]
    struct FakeManifestFetcher {
        by_ref: Mutex<HashMap<(String, String), FetchedManifest>>,
        calls: AtomicUsize,
    }
    impl FakeManifestFetcher {
        fn insert(&self, repo: &str, reference: &str, m: FetchedManifest) {
            self.by_ref
                .lock()
                .unwrap()
                .insert((repo.to_owned(), reference.to_owned()), m);
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }
    #[async_trait]
    impl UpstreamManifestFetcher for FakeManifestFetcher {
        async fn fetch_manifest(
            &self,
            repository: &str,
            reference: &str,
        ) -> Result<FetchedManifest, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.by_ref
                .lock()
                .unwrap()
                .get(&(repository.to_owned(), reference.to_owned()))
                .cloned()
                .ok_or_else(|| "not found".to_owned())
        }
    }

    /// Fake blob fetcher: returns canned bytes keyed by `(repo, digest)`,
    /// counting every call.
    #[derive(Debug, Default)]
    struct FakeBlobFetcher {
        by_digest: Mutex<HashMap<(String, String), Vec<u8>>>,
        calls: AtomicUsize,
    }
    impl FakeBlobFetcher {
        fn insert(&self, repo: &str, digest: &str, bytes: Vec<u8>) {
            self.by_digest
                .lock()
                .unwrap()
                .insert((repo.to_owned(), digest.to_owned()), bytes);
        }
        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }
    #[async_trait]
    impl UpstreamBlobFetcher for FakeBlobFetcher {
        async fn fetch_blob(&self, repository: &str, digest: &str) -> Result<Vec<u8>, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.by_digest
                .lock()
                .unwrap()
                .get(&(repository.to_owned(), digest.to_owned()))
                .cloned()
                .ok_or_else(|| "blob not found".to_owned())
        }
    }

    /// In-memory `ManifestKvStore` keyed by `(tenant-text, key)`.
    #[derive(Debug, Default)]
    struct FakeKv(Mutex<HashMap<(String, String), Bytes>>);
    #[async_trait]
    impl ManifestKvStore for FakeKv {
        async fn get(&self, tenant: &TenantId, key: &str) -> PortResult<Option<Bytes>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(tenant.to_canonical_text(), key.to_owned()))
                .cloned())
        }
        async fn put(&self, tenant: &TenantId, key: &str, value: Bytes) -> PortResult<()> {
            self.0
                .lock()
                .unwrap()
                .insert((tenant.to_canonical_text(), key.to_owned()), value);
            Ok(())
        }
        async fn list_prefix(&self, tenant: &TenantId, prefix: &str) -> PortResult<Vec<String>> {
            let t = tenant.to_canonical_text();
            Ok(self
                .0
                .lock()
                .unwrap()
                .keys()
                .filter(|(kt, ks)| *kt == t && ks.starts_with(prefix))
                .map(|(_, ks)| ks.clone())
                .collect())
        }
    }

    /// In-memory url→content-hash map recording `(namespace, url_hash)`.
    #[derive(Debug, Default)]
    struct FakeMap(Mutex<HashMap<(String, String), String>>);
    #[async_trait]
    impl UrlMapStore for FakeMap {
        async fn get(&self, ns: &str, url_hash: &str) -> Result<Option<String>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(ns.to_owned(), url_hash.to_owned()))
                .cloned())
        }
        async fn put(
            &self,
            ns: &str,
            url_hash: &str,
            content_hash: &str,
            _len: u64,
        ) -> Result<(), String> {
            self.0.lock().unwrap().insert(
                (ns.to_owned(), url_hash.to_owned()),
                content_hash.to_owned(),
            );
            Ok(())
        }
    }

    /// Non-verifying in-memory CAS keyed by `(namespace, claimed_hash)`.
    #[derive(Debug, Default)]
    struct RecordingCas(Mutex<HashMap<(String, String), Vec<u8>>>);
    impl CasReadHandler for RecordingCas {
        fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
            match self
                .0
                .lock()
                .unwrap()
                .get(&(req.tenant.clone(), req.hash.clone()))
            {
                Some(b) => Ok(CasReadResponse::new(b.clone(), req.hash)),
                None => Err(CasHandlerError::Internal("absent".into())),
            }
        }
    }
    impl CasWriteHandler for RecordingCas {
        fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
            self.0
                .lock()
                .unwrap()
                .insert((req.tenant, req.claimed_hash.clone()), req.bytes);
            Ok(CasWriteResponse::new(req.claimed_hash, true))
        }
    }

    /// Deterministic non-crypto content hasher for the moat (orthogonal to the
    /// OCI sha256 verify).
    fn fake_hash(bytes: &[u8]) -> String {
        format!("h{:08x}", bytes.iter().map(|b| u32::from(*b)).sum::<u32>())
    }

    fn moat(cas: Arc<RecordingCas>, map: Arc<FakeMap>) -> Arc<MoatCache> {
        Arc::new(MoatCache::new(
            Arc::clone(&cas) as Arc<dyn CasReadHandler>,
            cas as Arc<dyn CasWriteHandler>,
            map,
            fake_hash,
            SERVICE_PRINCIPAL,
        ))
    }

    /// A generous limiter (burst 1000) so rate-limiting never interferes with a
    /// functional test (the conservative production limiter is exercised by the
    /// flag/const wiring, not these behavioral tests).
    fn generous_limiter(
    ) -> InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics> {
        let config = RateLimitConfig::with_overrides(
            1000,
            1000,
            corelink_ratelimit::DEFAULT_RETRY_AFTER_FLOOR_SECS,
            corelink_ratelimit::RETRY_AFTER_HARD_CEILING_SECS,
            corelink_ratelimit::RETRY_AFTER_CANCELED_TENANT_SECS,
        )
        .unwrap();
        InMemoryTokenBucketRateLimiter::new(
            Arc::new(NoOpRateLimitAuditSink::new()),
            Arc::new(NoOpRateLimitMetrics::new()),
            config,
        )
    }

    struct Rig {
        resolver: UpstreamManifestResolver,
        manifest_fetcher: Arc<FakeManifestFetcher>,
        blob_fetcher: Arc<FakeBlobFetcher>,
        kv: Arc<FakeKv>,
        cas: Arc<RecordingCas>,
        map: Arc<FakeMap>,
    }

    /// Default rig: dedup OFF, empty (deny-all) allowlist → the M1 per-tenant-only
    /// behavior. Every M1 test uses this, so they double as the "dedup OFF ⇒
    /// byte-identical to M1, no `_public` writes/reads" proof.
    fn rig() -> Rig {
        rig_full(generous_limiter(), PublicBaseAllowlist::default(), false)
    }

    /// Build an allowlist from a set of already-canonical `sha256:` digests.
    fn allowlist_of(digests: &[&str]) -> PublicBaseAllowlist {
        let manifest = digests.iter().map(|d| format!("{d}\n")).collect::<String>();
        PublicBaseAllowlist::parse(&manifest).expect("hermetic allowlist parses")
    }

    fn rig_full(
        limiter: InMemoryTokenBucketRateLimiter<NoOpRateLimitAuditSink, NoOpRateLimitMetrics>,
        allowlist: PublicBaseAllowlist,
        dedup: bool,
    ) -> Rig {
        let manifest_fetcher = Arc::new(FakeManifestFetcher::default());
        let blob_fetcher = Arc::new(FakeBlobFetcher::default());
        let kv = Arc::new(FakeKv::default());
        let cas = Arc::new(RecordingCas::default());
        let map = Arc::new(FakeMap::default());
        let resolver = UpstreamManifestResolver::with_parts(
            Arc::clone(&manifest_fetcher) as Arc<dyn UpstreamManifestFetcher>,
            Arc::clone(&blob_fetcher) as Arc<dyn UpstreamBlobFetcher>,
            Arc::clone(&kv) as Arc<dyn ManifestKvStore>,
            moat(Arc::clone(&cas), Arc::clone(&map)),
            None, // cap resolver: None → moat.put gets None; RecordingCas ignores it.
            limiter,
            allowlist,
            dedup,
        );
        Rig {
            resolver,
            manifest_fetcher,
            blob_fetcher,
            kv,
            cas,
            map,
        }
    }

    fn tenant(n: u128) -> TenantId {
        TenantId::from_uuid(uuid::Uuid::from_u128(n))
    }

    /// Build a minimal image manifest JSON with the given config + layer blob
    /// digests, returning the serialized bytes.
    fn image_manifest_bytes(config_digest: &str, layer_digests: &[&str]) -> Vec<u8> {
        let layers: Vec<serde_json::Value> = layer_digests
            .iter()
            .map(|d| {
                serde_json::json!({
                    "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                    "digest": d,
                    "size": 1
                })
            })
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "config": {
                "mediaType": "application/vnd.oci.image.config.v1+json",
                "digest": config_digest,
                "size": 1
            },
            "layers": layers
        }))
        .unwrap()
    }

    fn index_bytes(child_digest: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": 2,
            "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": [{
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "digest": child_digest,
                "size": 1,
                "platform": { "os": "linux", "architecture": "amd64" }
            }]
        }))
        .unwrap()
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn image_manifest_miss_caches_manifest_and_blobs() {
        let r = rig();
        let t = tenant(1);
        let repo = "library/alpine";
        let config_bytes = b"config-object-bytes".to_vec();
        let layer_bytes = b"layer-tar-gzip-bytes".to_vec();
        let config_digest = sha256_wire(&config_bytes);
        let layer_digest = sha256_wire(&layer_bytes);
        let manifest = image_manifest_bytes(&config_digest, &[&layer_digest]);
        let manifest_digest = sha256_wire(&manifest);
        r.manifest_fetcher.insert(
            repo,
            "3.20",
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

        let out = r
            .resolver
            .resolve_on_miss(&t, repo, "3.20")
            .await
            .expect("no internal error")
            .expect("resolves the image manifest");
        assert_eq!(out.digest, manifest_digest);
        assert_eq!(out.bytes.as_ref(), manifest.as_slice());

        // Manifest stored in per-tenant KV under BOTH the tag + the digest slot.
        let kv = r.kv.0.lock().unwrap();
        assert!(kv.contains_key(&(t.to_canonical_text(), manifest_key(repo, "3.20"))));
        assert!(kv.contains_key(&(t.to_canonical_text(), manifest_key(repo, &manifest_digest))));
        drop(kv);
        // Config + layer blobs stored in the PER-TENANT moat namespace.
        let cas = r.cas.0.lock().unwrap();
        assert_eq!(
            cas.get(&(t.to_canonical_text(), fake_hash(&config_bytes))),
            Some(&config_bytes)
        );
        assert_eq!(
            cas.get(&(t.to_canonical_text(), fake_hash(&layer_bytes))),
            Some(&layer_bytes)
        );
        // Nothing landed in `_public`.
        assert!(!cas.keys().any(|(ns, _)| ns == PUBLIC_NAMESPACE));
    }

    #[tokio::test]
    async fn second_get_hits_kv_no_second_upstream_fetch() {
        let r = rig();
        let t = tenant(2);
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
                bytes: manifest.clone(),
                content_type: None,
                docker_content_digest: None,
            },
        );
        r.blob_fetcher.insert(repo, &config_digest, config_bytes);
        r.blob_fetcher.insert(repo, &layer_digest, layer_bytes);

        let _ = r
            .resolver
            .resolve_on_miss(&t, repo, "latest")
            .await
            .unwrap();
        assert_eq!(r.manifest_fetcher.call_count(), 1);
        // A 2nd resolve (a peer/racing miss) coalesces via the KV re-check — no
        // 2nd upstream fetch.
        let again = r
            .resolver
            .resolve_on_miss(&t, repo, "latest")
            .await
            .unwrap()
            .expect("served from KV");
        assert_eq!(again.bytes.as_ref(), manifest.as_slice());
        assert_eq!(
            r.manifest_fetcher.call_count(),
            1,
            "a warm KV re-check must not re-fetch upstream"
        );
    }

    #[tokio::test]
    async fn digest_reference_mismatch_stores_nothing() {
        let r = rig();
        let t = tenant(3);
        let repo = "library/alpine";
        // The reference is a digest of SOME bytes, but the fetcher returns
        // DIFFERENT bytes → verify fails → Ok(None), nothing stored.
        let honest = b"the-real-manifest".to_vec();
        let ref_digest = sha256_wire(&honest);
        r.manifest_fetcher.insert(
            repo,
            &ref_digest,
            FetchedManifest {
                bytes: b"tampered-manifest".to_vec(),
                content_type: None,
                docker_content_digest: None,
            },
        );
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, &ref_digest)
            .await
            .unwrap();
        assert!(out.is_none(), "a digest-lie must not resolve");
        assert!(
            r.kv.0.lock().unwrap().is_empty(),
            "nothing stored on mismatch"
        );
        assert!(r.cas.0.lock().unwrap().is_empty());
        assert_eq!(
            r.blob_fetcher.call_count(),
            0,
            "no blob fetch on a bad manifest"
        );
    }

    #[tokio::test]
    async fn index_miss_stores_index_without_blob_fetches() {
        let r = rig();
        let t = tenant(4);
        let repo = "library/debian";
        let child = sha256_wire(b"per-arch-manifest");
        let index = index_bytes(&child);
        let index_digest = sha256_wire(&index);
        r.manifest_fetcher.insert(
            repo,
            "12",
            FetchedManifest {
                bytes: index.clone(),
                content_type: Some("application/vnd.oci.image.index.v1+json".to_owned()),
                docker_content_digest: Some(index_digest.clone()),
            },
        );
        let out = r
            .resolver
            .resolve_on_miss(&t, repo, "12")
            .await
            .unwrap()
            .expect("index resolves");
        assert_eq!(out.digest, index_digest);
        assert_eq!(out.content_type, "application/vnd.oci.image.index.v1+json");
        // Index stored; NO blob fetches, NO recursion into the per-arch child.
        assert!(r
            .kv
            .0
            .lock()
            .unwrap()
            .contains_key(&(t.to_canonical_text(), manifest_key(repo, "12"))));
        assert_eq!(
            r.blob_fetcher.call_count(),
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
