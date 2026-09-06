    /// The CONCURRENT batch path, production shape: when the durable-audit
    /// D1 write fails (reachable-but-wrong credentials), `exists_batch`
    /// returns `AuditFailed` — NO probe result reaches the caller — and the
    /// joined window is attributed ONCE to `ostore` with `oaudit` ABSENT
    /// (proving neither `append_batch_async` nor the per-probe helper
    /// opened a scope of its own). Mirrors
    /// `r2_cas_list_concurrent_path_fails_closed_on_bad_audit_creds`.
    ///
    /// `#[ignore]` for the same reason as that test: needs outbound
    /// reachability to `api.cloudflare.com` (a 401 still proves the join
    /// ran). Run manually with:
    ///
    /// ```bash
    /// cargo test -p corelink-server r2_cas_exists_batch_fails_closed_on_bad_audit_creds -- --ignored
    /// ```
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires outbound network reachability to api.cloudflare.com"]
    async fn r2_cas_exists_batch_fails_closed_on_bad_audit_creds() {
        let stub_env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(),
            d1_database_id: "test".to_owned(),
        };
        let client = R2S3Client::new(&stub_env, "test-bucket")
            .await
            .expect("stub client");
        let audit_concrete = cas_audit_sink_from_d1_concrete(D1HttpClient::new(&stub_env))
            .expect("D1HttpClient constructs over the stub env");
        let audit: Arc<dyn AuditSink> = audit_concrete.clone();
        let sli = Arc::new(InMemorySliObserver::new());
        let handler = std::sync::Arc::new(
            R2CasHandler::new(client, "iad", None, audit, sli).with_async_audit(audit_concrete),
        );

        let app = axum::Router::new()
            .route(
                "/x",
                axum::routing::get(move || {
                    let handler = std::sync::Arc::clone(&handler);
                    async move {
                        let reqs: Vec<CasReadRequest> = (0..4u8)
                            .map(|i| {
                                CasReadRequest::new(
                                    "tenant-x",
                                    format!("{i:064x}"),
                                    "caller@tenant-x",
                                    "tenant-x",
                                    1,
                                )
                            })
                            .collect();
                        let result = CasReadHandler::exists_batch(&*handler, &reqs)
                            .expect("batch capability is wired");
                        assert!(
                            matches!(result, Err(CasHandlerError::AuditFailed(_))),
                            "a rejected D1 credential must surface as AuditFailed, \
                             never as probe results: {result:?}"
                        );
                        axum::http::StatusCode::OK
                    }
                }),
            )
            .layer(axum::middleware::from_fn(
                crate::origin_timing::origin_timing_layer,
            ));

        let resp = tower::ServiceExt::oneshot(
            app,
            axum::http::Request::builder()
                .uri("/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        let header = resp
            .headers()
            .get("server-timing")
            .expect("origin_timing_layer must stamp Server-Timing")
            .to_str()
            .unwrap()
            .to_owned();
        let parsed = parse_server_timing(&header);
        assert!(
            parsed.contains_key("ostore"),
            "the joined window must be attributed to ostore. Header: {header}"
        );
        assert!(
            !parsed.contains_key("oaudit"),
            "oaudit must be ABSENT on the concurrent batch path — a second \
             scope over the same window would double-count it. Header: {header}"
        );
    }

    /// A non-zero fake 32-byte TDK for tests that must exercise the
    /// PRODUCTION always-HMAC prefix path (`Some(tdk)` arm).
    fn fake_tdk() -> Zeroizing<[u8; 32]> {
        Zeroizing::new([0x5au8; 32])
    }

    /// Build an `R2CasHandler` with a real (fake) TDK over a stub S3
    /// client — exercises the production `Some(tdk)` key-derivation arm.
    async fn make_test_handler_with_tdk(region: &str) -> R2CasHandler {
        let stub_env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(),
            d1_database_id: "test".to_owned(),
        };
        let client = R2S3Client::new(&stub_env, "test-bucket")
            .await
            .expect("stub client");
        let audit = Arc::new(InMemoryAuditSink::new());
        let sli = Arc::new(InMemorySliObserver::new());
        R2CasHandler::new(client, region, Some(fake_tdk()), audit, sli)
    }

    /// Build an `R2AcHandler` over a stub S3 client (no TDK → test
    /// raw-pad prefix; only the divergent-body/region logic is
    /// exercised here, any S3 I/O fails against the stub).
    async fn make_test_ac_handler(region: &str) -> R2AcHandler {
        let stub_env = StorageEnv {
            r2_endpoint: "https://localhost:1".to_owned(),
            r2_access_key_id: "test".to_owned(),
            r2_secret_access_key: "test".to_owned(),
            cloudflare_account_id: "test".to_owned(),
            cf_api_token: "test".to_owned(),
            d1_database_id: "test".to_owned(),
        };
        let client = R2S3Client::new(&stub_env, "test-bucket")
            .await
            .expect("stub client");
        let audit = Arc::new(corelink_handler_ac::InMemoryAuditSink::new());
        let sli = Arc::new(corelink_handler_ac::InMemorySliObserver::new());
        R2AcHandler::new(client, region, None, audit, sli)
    }

    // ---------------------------------------------------------------
    // F1/F2 — production path ALWAYS HMACs the full tenant id
    // ---------------------------------------------------------------

    /// With a TDK configured (the production posture), a canonical UUID
    /// tenant resolves to the secret-keyed `derive_prefix` HMAC — NOT
    /// the public raw-padded prefix of the tenant string. This is the
    /// regression pin for F1/F2: the predictable public prefix must
    /// never appear on the TDK path for a UUID tenant.
    #[tokio::test]
    async fn r2_cas_tdk_path_uses_hmac_prefix_not_raw_tenant() {
        let handler = make_test_handler_with_tdk("iad").await;
        // A canonical UUIDv7-shaped tenant id.
        let tenant = "0190abcd-1234-75ab-8def-0123456789ab";
        let key = handler
            .r2_key(tenant, &"d".repeat(64), DigestAlgo::Blake3)
            .unwrap();
        let parts: Vec<&str> = key.split('/').collect();
        assert_eq!(parts.len(), 3, "key: {key}");
        let prefix = parts[1];
        assert_eq!(prefix.len(), 16, "prefix must be 16 chars: {key}");
        // The HMAC prefix must NOT be the predictable public prefix of
        // the tenant string (the F1 raw-padded fallback).
        let raw_public = &tenant[..16];
        assert_ne!(
            prefix, raw_public,
            "production prefix leaked the public tenant-id prefix (F1/F2)"
        );
        // And it must equal the canonical secret-keyed derivation.
        let expected = derive_prefix(
            &TenantDerivationKey::from_bytes(fake_tdk()),
            Uuid::try_parse(tenant).unwrap(),
        )
        .to_string();
        assert_eq!(prefix, expected, "prefix must be derive_prefix(tdk, uuid)");
    }

    // ---------------------------------------------------------------
    // F2 — production prefix derivation FAILS CLOSED for a non-derivable
    // tenant (never an empty `<region>//<digest>` SHARED keyspace).
    // ---------------------------------------------------------------

    /// The production-strict derivation (`derive_tenant_prefix_strict`,
    /// the authority the live `tenant_prefix` path delegates to) MUST
    /// refuse a non-UUID tenant and a missing TDK — there is NO public/
    /// empty fallback. This is the regression pin for finding #2/#8: a
    /// non-derivable tenant on the prod path errors out instead of
    /// keying under `<region>//<digest>` (a SHARED, cross-tenant
    /// keyspace).
    #[test]
    fn prod_strict_prefix_fails_closed_for_non_derivable_tenant() {
        let tdk = TenantDerivationKey::from_bytes(fake_tdk());

        // Non-UUID tenant under a present TDK → Err (no raw-padded
        // fallback on the prod path).
        let err = derive_tenant_prefix_strict(Some(&tdk), "tenant-abc")
            .expect_err("non-UUID tenant must fail closed on the prod path");
        assert!(
            err.contains("INV-TENANT-ISOLATION"),
            "error must cite the isolation invariant: {err}"
        );

        // Missing TDK → Err (the production builders fail closed before
        // this, but the derivation itself must not produce a prefix).
        assert!(
            derive_tenant_prefix_strict(None, "0190abcd-1234-75ab-8def-0123456789ab").is_err(),
            "absent TDK must fail closed"
        );

        // Sanity: a canonical UUID under a present TDK IS derivable and
        // is exactly the secret-keyed prefix (never empty).
        let ok = derive_tenant_prefix_strict(Some(&tdk), "0190abcd-1234-75ab-8def-0123456789ab")
            .expect("a canonical UUID tenant must derive a prefix");
        assert_eq!(ok.len(), 16, "derived prefix must be 16 chars, got {ok:?}");
        assert!(!ok.is_empty(), "derived prefix must never be empty");
    }

    #[test]
    fn public_namespace_resolves_a_stable_isolated_derived_prefix() {
        // The brew-502 root cause: `_public` (the shared cross-tenant dedup
        // namespace) is not a UUID, so the prod path failed CLOSED on it. It MUST
        // instead resolve a stable, secret-keyed prefix — public content is shared
        // by design, but every caller must dedup the same blob to the same key.
        let tdk = TenantDerivationKey::from_bytes(fake_tdk());
        let p = tenant_prefix(Some(&tdk), crate::adapter_cache::PUBLIC_NAMESPACE)
            .expect("_public must resolve a prefix (not fail closed)");
        assert_eq!(p.len(), 16, "prefix must be 16 chars: {p:?}");
        // Secret-keyed reserved-sentinel HMAC (NOT a predictable raw prefix).
        assert_eq!(p, derive_prefix(&tdk, PUBLIC_NAMESPACE_UUID).to_string());
        // Deterministic across calls (so cross-tenant dedup actually dedups).
        assert_eq!(
            p,
            tenant_prefix(Some(&tdk), crate::adapter_cache::PUBLIC_NAMESPACE).unwrap()
        );
        // Reserved: it never collides with a real (UUID) tenant's prefix.
        let real = tenant_prefix(Some(&tdk), "0190abcd-1234-75ab-8def-0123456789ab").unwrap();
        assert_ne!(
            p, real,
            "_public must not collide with a real tenant prefix"
        );
    }

    // ---------------------------------------------------------------
    // F7 — CAS storage is residency-aware: keyed by the handler's
    // region, never a process-global. A regional handler MUST prefix
    // its keys with that region.
    // ---------------------------------------------------------------

    /// Each regional CAS handler keys objects under its OWN region — an
    /// `lhr` handler must never write into the `iad` key space. This is
    /// the invariant that makes per-env `R2_CAS_REGION` (the frozen
    /// contract) load-bearing rather than cosmetic. If a regional env
    /// fails to thread its region through, this fails.
    #[tokio::test]
    async fn r2_cas_keys_are_residency_scoped_per_region() {
        for region in ["iad", "lhr", "sam", "nrt", "syd"] {
            let handler = make_test_handler_with_tdk(region).await;
            let key = handler
                .r2_key(
                    "0190abcd-1234-75ab-8def-0123456789ab",
                    &"a".repeat(64),
                    DigestAlgo::Blake3,
                )
                .unwrap();
            assert!(
                key.starts_with(&format!("{region}/")),
                "CAS key for region {region} must be region-scoped (residency): {key}"
            );
        }
        // A non-iad region must NOT collapse to the iad default.
        let lhr = make_test_handler_with_tdk("lhr").await;
        let key = lhr
            .r2_key(
                "0190abcd-1234-75ab-8def-0123456789ab",
                &"a".repeat(64),
                DigestAlgo::Blake3,
            )
            .unwrap();
        assert!(
            !key.starts_with("iad/"),
            "EU (lhr) CAS write fell back to the US (iad) key space: {key}"
        );
    }

    // ---------------------------------------------------------------
    // F5 — AC update divergent-body invariant (no silent overwrite)
    // ---------------------------------------------------------------

    #[test]
    fn ac_storage_key_rejects_noncanonical_digest() {
        assert!(is_canonical_ac_digest(&"a".repeat(64)));
        assert!(!is_canonical_ac_digest(&"A".repeat(64)));
        assert!(!is_canonical_ac_digest(&"a".repeat(63)));
        assert!(!is_canonical_ac_digest("../poison"));
    }

    /// On an AMBIGUOUS pre-PUT GET (the stub endpoint is unreachable, so
    /// GET errors), `R2AcHandler::update` MUST fail closed with
    /// `Internal` and NEVER fall through to a blind PUT that could
    /// overwrite a proven AC result. This pins the F5 fail-closed branch
    /// (a proven result is never overwritten on unknown prior state).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn r2_ac_update_fails_closed_on_ambiguous_get() {
        use corelink_handler_ac::{AcHandlerError, AcUpdateHandler, AcUpdateRequest};
        let handler = make_test_ac_handler("iad").await;
        let req = AcUpdateRequest::new("t1", "d".repeat(64), b"payload".to_vec(), "p@t1", "t1", 1);
        let err = handler
            .update(req)
            .expect_err("ambiguous GET against stub must fail closed");
        assert!(
            matches!(err, AcHandlerError::Internal(_)),
            "update must fail closed (Internal) on an ambiguous pre-PUT GET, \
             never blind-overwrite — got {err:?}"
        );
    }

    // ---------------------------------------------------------------
    // BYOK Wave 3a — handler-level encrypt/decrypt hooks + Option-gating
    // + fail-closed (the data-plane integration, minus the R2 I/O which
    // is unchanged plumbing). These exercise `R2CasHandler`'s own
    // `byok_encrypt_for_write` / `byok_decrypt_for_read` /
    // `resolve_byok_ctx`; the convergent crypto + caches themselves are
    // covered in `storage::byok_cas::tests`.
    // ---------------------------------------------------------------

    use crate::customer_d1::{
        ByokConfigError, ByokCryptoMode, ByokMode, ByokState, TenantByokConfig,
    };
    use crate::storage::byok_cas::{
        ByokConfigCache, ByokConfigSource, ByokEnvelopeRow, ByokEnvelopeStore, ByokSecretSource,
        ModeBEncryptor, TcsResolver, WrappedTcsRow,
    };
    use corelink_byok::{
        BYOKError, Dek, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, WrappedDek,
    };
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex as StdMutex;

    /// Hermetic in-memory `byok_envelope` store for the Mode-B handler tests.
    /// `fail_delete` injects a reclaim (`delete_envelope`) failure for the Wave-4a
    /// safe-fail test.
    #[derive(Debug, Default)]
    struct MemEnvStore {
        inner: StdMutex<StdHashMap<(String, String), ByokEnvelopeRow>>,
        fail_delete: bool,
    }
    impl MemEnvStore {
        fn failing_delete() -> Self {
            Self {
                inner: StdMutex::default(),
                fail_delete: true,
            }
        }
        fn len(&self) -> usize {
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len()
        }
        fn contains(&self, tenant: &str, blob_key: &str) -> bool {
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains_key(&(tenant.to_owned(), blob_key.to_owned()))
        }
        fn wrapped_dek(&self, tenant: &str, blob_key: &str) -> Option<Vec<u8>> {
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&(tenant.to_owned(), blob_key.to_owned()))
                .map(|r| r.wrapped_dek.clone())
        }
    }
    #[async_trait::async_trait]
    impl ByokEnvelopeStore for MemEnvStore {
        async fn get_envelope(
            &self,
            tenant: &str,
            blob_key: &str,
        ) -> Result<Option<ByokEnvelopeRow>, String> {
            Ok(self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&(tenant.to_owned(), blob_key.to_owned()))
                .cloned())
        }
        async fn put_envelope_if_absent(
            &self,
            tenant: &str,
            blob_key: &str,
            row: &ByokEnvelopeRow,
            _created_at_ms: i64,
        ) -> Result<(), String> {
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry((tenant.to_owned(), blob_key.to_owned()))
                .or_insert_with(|| row.clone());
            Ok(())
        }
        async fn delete_envelope(&self, tenant: &str, blob_key: &str) -> Result<(), String> {
            if self.fail_delete {
                return Err("mem env store: injected delete failure".to_owned());
            }
            self.inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&(tenant.to_owned(), blob_key.to_owned()));
            Ok(())
        }

        async fn record_reconciliation_intent(
            &self,
            _tenant: &str,
            _digest: &str,
            _physical_r2_key: &str,
            _reason: &str,
            _created_at_ms: u64,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    const BYOK_TENANT: &str = "byok-tenant-x";

    fn byok_cfg(crypto_mode: ByokCryptoMode, state: ByokState) -> TenantByokConfig {
        TenantByokConfig {
            tenant_id: BYOK_TENANT.to_owned(),
            mode: ByokMode::Byok,
            crypto_mode,
            cmk_provider: Some("aws".to_owned()),
            cmk_key_id: Some("arn:cmk".to_owned()),
            cmk_region: Some("iad".to_owned()),
            state,
        }
    }

    #[derive(Debug)]
    struct CfgSrc(Option<TenantByokConfig>);
    #[async_trait::async_trait]
    impl ByokConfigSource for CfgSrc {
        async fn get_byok_config(
            &self,
            _t: &str,
        ) -> Result<Option<TenantByokConfig>, ByokConfigError> {
            Ok(self.0.clone())
        }
    }

    #[derive(Debug)]
    struct SecSrc;
    #[async_trait::async_trait]
    impl ByokSecretSource for SecSrc {
        async fn get_wrapped_tcs(&self, _t: &str) -> Result<Option<WrappedTcsRow>, String> {
            Ok(Some(WrappedTcsRow {
                tcs_wrapped: vec![5u8; 32],
                cmk_key_id: Some("arn:cmk".to_owned()),
                tcs_version: 1,
            }))
        }
    }

    #[derive(Debug)]
    struct Kms {
        fail: bool,
    }
    #[async_trait::async_trait]
    impl KmsProvider for Kms {
        fn provider_kind(&self) -> KmsProviderKind {
            KmsProviderKind::AwsKms
        }
        fn region(&self) -> &str {
            "iad"
        }
        fn fips_level(&self) -> corelink_byok::FipsLevel {
            corelink_byok::FipsLevel::Fips140_3_L1
        }
        async fn wrap_dek(
            &self,
            dek: &Dek,
            key_id: &KmsKeyId,
            ec: Option<&serde_json::Value>,
        ) -> Result<WrappedDek, BYOKError> {
            Ok(WrappedDek {
                provider: KmsProviderKind::AwsKms,
                key_id: key_id.clone(),
                ciphertext: dek.bytes.to_vec(),
                encryption_context: ec.cloned(),
            })
        }
        async fn unwrap_dek(&self, w: &WrappedDek) -> Result<Dek, BYOKError> {
            if self.fail {
                return Err(BYOKError::Provider("kms down".to_owned()));
            }
            let mut b = [0u8; 32];
            b.copy_from_slice(&w.ciphertext);
            Ok(Dek { bytes: b })
        }
        async fn check_access(&self, _k: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
            Ok(KmsAccessStatus::Ok)
        }
    }

    /// Wire a handler with BYOK collaborators (config + Tcs resolver).
    async fn handler_with_byok(cfg: Option<TenantByokConfig>, kms_fail: bool) -> R2CasHandler {
        let base = make_test_handler_with_tdk("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: kms_fail }), 300).unwrap(),
        );
        base.with_byok(cache, resolver)
    }

    /// A CAS handler with BOTH the convergent collaborators AND the Mode-B
    /// (random-DEK) encryptor wired over a shared in-memory envelope store.
    async fn handler_with_byok_random(cfg: Option<TenantByokConfig>) -> R2CasHandler {
        let base = make_test_handler_with_tdk("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: false }), 300).unwrap(),
        );
        let kms: Arc<dyn KmsProvider> = Arc::new(Kms { fail: false });
        let store: Arc<dyn ByokEnvelopeStore> = Arc::new(MemEnvStore::default());
        let mode_b = Arc::new(ModeBEncryptor::new(kms, store, 300).unwrap());
        base.with_byok(cache, resolver).with_byok_random(mode_b)
    }

    /// Like [`handler_with_byok_random`] but threads a caller-supplied
    /// [`MemEnvStore`] so a test can assert on the persisted envelope rows
    /// (Wave 4a reclaim). The store is shared (the handler holds an `Arc` clone).
    async fn handler_with_byok_random_store(
        cfg: Option<TenantByokConfig>,
        store: Arc<MemEnvStore>,
    ) -> R2CasHandler {
        let base = make_test_handler_with_tdk("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: false }), 300).unwrap(),
        );
        let kms: Arc<dyn KmsProvider> = Arc::new(Kms { fail: false });
        let store_dyn: Arc<dyn ByokEnvelopeStore> = store;
        let mode_b = Arc::new(ModeBEncryptor::new(kms, store_dyn, 300).unwrap());
        base.with_byok(cache, resolver).with_byok_random(mode_b)
    }

    fn write_req(tenant: &str, bytes: Vec<u8>) -> CasWriteRequest {
        let claimed = Digest::compute(&bytes).to_hex();
        CasWriteRequest::new(tenant, claimed, bytes, "p", tenant, 1)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_inactive_handler_is_plaintext_passthrough() {
        // No `with_byok` at all → the existing plaintext path, byte-identical.
        let h = make_test_handler_with_tdk("iad").await;
        let req = write_req(BYOK_TENANT, b"hello".to_vec());
        assert!(
            h.byok_encrypt_for_write(&req).await.unwrap().is_none(),
            "no BYOK collaborators ⇒ store plaintext (None)"
        );
        let rreq = CasReadRequest::new(BYOK_TENANT, &req.claimed_hash, "p", BYOK_TENANT, 1);
        let out = h
            .byok_decrypt_for_read(&rreq, b"hello".to_vec())
            .await
            .unwrap();
        assert_eq!(out, b"hello", "read must return the stored bytes unchanged");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_inactive_config_is_plaintext_passthrough() {
        // Collaborators present but state=inactive → still the plaintext path.
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Inactive)),
            false,
        )
        .await;
        let req = write_req(BYOK_TENANT, b"data".to_vec());
        assert!(h.byok_encrypt_for_write(&req).await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_active_encrypts_then_round_trips() {
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let plaintext = b"top secret artifact".to_vec();
        let req = write_req(BYOK_TENANT, plaintext.clone());

        let stored = h
            .byok_encrypt_for_write(&req)
            .await
            .unwrap()
            .expect("active tenant must encrypt");
        assert_ne!(stored, plaintext, "stored bytes must be ciphertext");

        // Convergent dedup: a second encrypt of the same content is identical.
        let stored2 = h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        assert_eq!(stored, stored2, "convergent ⇒ idempotent stored bytes");

        // Read back: decrypt → plaintext; the content-hash re-verify (in `read`)
        // then runs on this plaintext.
        let rreq = CasReadRequest::new(BYOK_TENANT, &req.claimed_hash, "p", BYOK_TENANT, 1);
        let out = h.byok_decrypt_for_read(&rreq, stored).await.unwrap();
        assert_eq!(out, plaintext, "decrypt must recover the plaintext");
        assert!(verify_content_hash(DigestAlgo::Blake3, &req.claimed_hash, &out).is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_active_write_fails_closed_when_kms_down() {
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            true, // KMS unwrap fails
        )
        .await;
        let req = write_req(BYOK_TENANT, b"secret".to_vec());
        assert!(
            matches!(
                h.byok_encrypt_for_write(&req).await,
                Err(CasHandlerError::Internal(_))
            ),
            "active tenant + failing KMS must fail closed on write (never plaintext)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_active_read_fails_closed_when_kms_down() {
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            true,
        )
        .await;
        let rreq = CasReadRequest::new(BYOK_TENANT, "a".repeat(64), "p", BYOK_TENANT, 1);
        // Even given some stored bytes, a failed unwrap must NOT return them raw.
        assert!(
            matches!(
                h.byok_decrypt_for_read(&rreq, b"CLB1raw".to_vec()).await,
                Err(CasHandlerError::Internal(_))
            ),
            "active tenant + failing KMS must fail closed on read (raw bytes never served)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_unwired_fails_closed() {
        // Mode B active but the random-mode encryptor is NOT wired → fail closed,
        // never plaintext (frozen policy: `byok_mode_b` is `None` here).
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            false,
        )
        .await;
        let req = write_req(BYOK_TENANT, b"x".to_vec());
        assert!(matches!(
            h.byok_encrypt_for_write(&req).await,
            Err(CasHandlerError::Internal(_))
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_wired_round_trips_via_envelope() {
        // Mode B (random) wired: encrypt → CLB2 ciphertext (+ a byok_envelope row),
        // read back → plaintext.
        let h = handler_with_byok_random(Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)))
            .await;
        let plaintext = b"mode-b artifact bytes".to_vec();
        let req = write_req(BYOK_TENANT, plaintext.clone());
        let stored = h
            .byok_encrypt_for_write(&req)
            .await
            .unwrap()
            .expect("active Mode-B tenant must encrypt");
        assert_ne!(stored, plaintext, "Mode-B stored bytes must be ciphertext");
        let rreq = CasReadRequest::new(BYOK_TENANT, &req.claimed_hash, "p", BYOK_TENANT, 1);
        let out = h.byok_decrypt_for_read(&rreq, stored).await.unwrap();
        assert_eq!(out, plaintext, "Mode-B decrypt must recover the plaintext");
        assert!(verify_content_hash(DigestAlgo::Blake3, &req.claimed_hash, &out).is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_re_put_is_idempotent_no_orphan() {
        // Re-encrypt of the same blob reuses the persisted envelope ⇒ byte-identical
        // ciphertext (no orphan; audit C2), and the original still decrypts.
        let h = handler_with_byok_random(Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)))
            .await;
        let req = write_req(BYOK_TENANT, b"idempotent mode-b".to_vec());
        let a = h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        let b = h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        assert_eq!(a, b, "Mode-B re-PUT reuses the persisted DEK (no orphan)");
        let rreq = CasReadRequest::new(BYOK_TENANT, &req.claimed_hash, "p", BYOK_TENANT, 1);
        assert_eq!(
            h.byok_decrypt_for_read(&rreq, a).await.unwrap(),
            req.bytes,
            "the original Mode-B ciphertext still decrypts"
        );
    }

    // ── BYOK Wave 4a — Mode-B `byok_envelope` reclaim on delete ───────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_delete_reclaims_envelope_row_cas_surface() {
        // A Mode-B blob delete removes the matching CAS-surface envelope row.
        let store = Arc::new(MemEnvStore::default());
        let h = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = write_req(BYOK_TENANT, b"delete-me mode-b".to_vec());
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        let key = format!("cas:{}", req.claimed_hash);
        assert!(
            store.contains(BYOK_TENANT, &key),
            "write minted the cas: envelope row"
        );
        assert!(
            !store.contains(BYOK_TENANT, &format!("ac:{}", req.claimed_hash)),
            "no ac: row"
        );
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.claimed_hash, DigestAlgo::Blake3)
            .await
            .unwrap();
        assert!(
            !store.contains(BYOK_TENANT, &key),
            "delete reclaimed the envelope row"
        );
        assert_eq!(store.len(), 0, "no orphan row lingers");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_a_delete_touches_no_envelope_row() {
        // Mode A (convergent) writes NO envelope row; reclaim is a no-op.
        let store = Arc::new(MemEnvStore::default());
        let h = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = write_req(BYOK_TENANT, b"convergent payload".to_vec());
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        assert_eq!(store.len(), 0, "Mode A writes no envelope row");
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.claimed_hash, DigestAlgo::Blake3)
            .await
            .unwrap();
        assert_eq!(store.len(), 0, "Mode-A delete touches no envelope row");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_non_byok_and_public_delete_touch_no_envelope_row() {
        // Non-BYOK (no collaborators) → reclaim is a plaintext no-op.
        let h = make_test_handler_with_tdk("iad").await;
        h.byok_reclaim_for_delete(BYOK_TENANT, &"a".repeat(64), DigestAlgo::Blake3)
            .await
            .unwrap();
        // `_public` under an active Mode-B config → still plaintext (no row).
        let store = Arc::new(MemEnvStore::default());
        let hp = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        hp.byok_reclaim_for_delete(
            crate::adapter_cache::PUBLIC_NAMESPACE,
            &"b".repeat(64),
            DigestAlgo::Blake3,
        )
        .await
        .unwrap();
        assert_eq!(
            store.len(),
            0,
            "_public never writes/reclaims an envelope row"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_re_put_after_delete_mints_fresh_envelope() {
        // After a delete-reclaim, a re-PUT of the SAME blob mints a FRESH envelope
        // row (no stale reuse of the deleted DEK) — confirms the reclaim happened.
        let store = Arc::new(MemEnvStore::default());
        let h = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = write_req(BYOK_TENANT, b"re-put mode-b".to_vec());
        let key = format!("cas:{}", req.claimed_hash);
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        let dek_before = store.wrapped_dek(BYOK_TENANT, &key).unwrap();
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.claimed_hash, DigestAlgo::Blake3)
            .await
            .unwrap();
        assert_eq!(store.len(), 0, "row gone after reclaim");
        // Re-PUT: a brand-new envelope row (fresh random DEK), not the deleted one.
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        assert!(
            store.contains(BYOK_TENANT, &key),
            "re-PUT minted a fresh envelope row"
        );
        let dek_after = store.wrapped_dek(BYOK_TENANT, &key).unwrap();
        assert_ne!(
            dek_before, dek_after,
            "re-PUT after delete uses a FRESH DEK, not the reclaimed one"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_reclaim_failure_is_safe_fail() {
        // A failed envelope-delete is the SAFE-fail direction: the production
        // delete WARNS and continues (the R2 object is already gone, never rolled
        // back). The reclaim hook surfaces the Err that production swallows.
        let store = Arc::new(MemEnvStore::failing_delete());
        let h = handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = write_req(BYOK_TENANT, b"reclaim-fails".to_vec());
        h.byok_encrypt_for_write(&req).await.unwrap().unwrap();
        let res = h
            .byok_reclaim_for_delete(BYOK_TENANT, &req.claimed_hash, DigestAlgo::Blake3)
            .await;
        assert!(
            res.is_err(),
            "reclaim failure surfaces as Err (production warns, never fails the delete)"
        );
        // The row lingers (the failed delete left it) — an orphan that wraps the
        // already-deleted ciphertext (the safe direction); the blob delete itself
        // is unaffected (R2 delete is not rolled back).
        assert!(store.contains(BYOK_TENANT, &format!("cas:{}", req.claimed_hash)));
    }
