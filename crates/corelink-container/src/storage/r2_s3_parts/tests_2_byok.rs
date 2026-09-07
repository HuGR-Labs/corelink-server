
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
