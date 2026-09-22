    #[tokio::test]
    async fn r2_capped_body_stall_after_headers_hits_idle_deadline() {
        struct StalledAfterHeaders {
            emitted: bool,
        }

        impl R2BodyChunkStream for StalledAfterHeaders {
            fn next_chunk<'a>(&'a mut self) -> R2BodyChunkFuture<'a> {
                Box::pin(async move {
                    if !self.emitted {
                        self.emitted = true;
                        Some(Ok(bytes::Bytes::from_static(b"headers-arrived")))
                    } else {
                        std::future::pending::<Option<Result<bytes::Bytes, String>>>().await
                    }
                })
            }
        }

        let err = R2S3Client::collect_capped_body_with_deadlines(
            "stalled-after-headers",
            1024,
            StalledAfterHeaders { emitted: false },
            std::time::Duration::from_millis(10),
            std::time::Duration::from_millis(50),
        )
        .await
        .expect_err("a body that stalls after headers must fail closed");
        assert_eq!(err, "R2 body idle timeout for key stalled-after-headers");
    }

    #[tokio::test]
    async fn r2_capped_body_total_deadline_does_not_reset_between_chunks() {
        struct SlowChunks;

        impl R2BodyChunkStream for SlowChunks {
            fn next_chunk<'a>(&'a mut self) -> R2BodyChunkFuture<'a> {
                Box::pin(async {
                    // Each chunk arrives below the 20 ms idle deadline. The
                    // stream never goes idle, so only a non-resetting total
                    // deadline can terminate this body.
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    Some(Ok(bytes::Bytes::from_static(b"slow-chunk")))
                })
            }
        }

        let err = R2S3Client::collect_capped_body_with_deadlines(
            "slow-total",
            1024 * 1024,
            SlowChunks,
            std::time::Duration::from_millis(20),
            std::time::Duration::from_millis(35),
        )
        .await
        .expect_err("continuous below-idle chunks must hit the total deadline");
        assert_eq!(err, "R2 body total timeout for key slow-total");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_mode_b_read_fails_closed_when_kms_down() {
        // A wired Mode-B handler whose KMS unwrap fails must NOT serve raw bytes.
        let base = make_test_handler_with_tdk("iad").await;
        let cfg = Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active));
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: false }), 300).unwrap(),
        );
        let kms: Arc<dyn KmsProvider> = Arc::new(Kms { fail: true });
        let store: Arc<dyn ByokEnvelopeStore> = Arc::new(MemEnvStore::default());
        let mode_b = Arc::new(ModeBEncryptor::new(kms, store, 300).unwrap());
        let h = base.with_byok(cache, resolver).with_byok_random(mode_b);
        let rreq = CasReadRequest::new(BYOK_TENANT, "a".repeat(64), "p", BYOK_TENANT, 1);
        let mut blob = b"CLB2".to_vec();
        blob.extend_from_slice(b"ciphertext");
        assert!(
            matches!(
                h.byok_decrypt_for_read(&rreq, blob).await,
                Err(CasHandlerError::Internal(_))
            ),
            "Mode-B read with KMS down must fail closed"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn byok_partial_state_fails_closed() {
        // Partial/backfill dual-read is deferred to Wave 4 — fail closed.
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Partial)),
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
    async fn byok_public_namespace_never_encrypts() {
        // `_public` is shared deterministic content — it must stay plaintext even
        // with an active config, so cross-tenant dedup is preserved (plan §3).
        let h = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let req = write_req(
            crate::adapter_cache::PUBLIC_NAMESPACE,
            b"public bottle".to_vec(),
        );
        assert!(
            h.byok_encrypt_for_write(&req).await.unwrap().is_none(),
            "_public must never be encrypted"
        );
    }

    // ---------------------------------------------------------------
    // BYOK Wave 3b — AC handler encrypt/decrypt hooks (surface="ac"),
    // Option-gating, fail-closed, + surface separation from CAS (H1).
    // Exercise `R2AcHandler::byok_encrypt_for_update` /
    // `byok_decrypt_for_lookup` / `resolve_byok_ctx`; the crypto itself
    // is covered in `storage::byok_cas::tests`.
    // ---------------------------------------------------------------

    /// Wire an `R2AcHandler` with BYOK collaborators (mirrors `handler_with_byok`).
    async fn ac_handler_with_byok(cfg: Option<TenantByokConfig>, kms_fail: bool) -> R2AcHandler {
        let base = make_test_ac_handler("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: kms_fail }), 300).unwrap(),
        );
        base.with_byok(cache, resolver)
    }

    /// Wire an `R2AcHandler` with the Mode-B encryptor over a shared envelope
    /// store (mirrors `handler_with_byok_random_store`).
    async fn ac_handler_with_byok_random_store(
        cfg: Option<TenantByokConfig>,
        store: Arc<MemEnvStore>,
    ) -> R2AcHandler {
        let base = make_test_ac_handler("iad").await;
        let cache = Arc::new(ByokConfigCache::new(Arc::new(CfgSrc(cfg)), 60));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: false }), 300).unwrap(),
        );
        let kms: Arc<dyn KmsProvider> = Arc::new(Kms { fail: false });
        let store_dyn: Arc<dyn ByokEnvelopeStore> = store;
        let mode_b = Arc::new(ModeBEncryptor::new(kms, store_dyn, 300).unwrap());
        base.with_byok(cache, resolver).with_byok_random(mode_b)
    }

    fn ac_update_req(tenant: &str, payload: Vec<u8>) -> corelink_handler_ac::AcUpdateRequest {
        corelink_handler_ac::AcUpdateRequest::new(tenant, "a".repeat(64), payload, "p", tenant, 1)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_mode_b_delete_reclaims_envelope_row_ac_surface() {
        // Wave 4a: an AC Mode-B delete reclaims the `ac:<digest>` envelope row —
        // surface-correct (a CAS row for the same digest would be `cas:<digest>`).
        let store = Arc::new(MemEnvStore::default());
        let h = ac_handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Random, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = ac_update_req(BYOK_TENANT, b"ac mode-b payload".to_vec());
        h.byok_encrypt_for_update(&req).await.unwrap().unwrap();
        let ac_key = format!("ac:{}", req.action_digest);
        assert!(
            store.contains(BYOK_TENANT, &ac_key),
            "write minted the ac: envelope row"
        );
        assert!(
            !store.contains(BYOK_TENANT, &format!("cas:{}", req.action_digest)),
            "no cas: row"
        );
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.action_digest)
            .await
            .unwrap();
        assert!(
            !store.contains(BYOK_TENANT, &ac_key),
            "AC delete reclaimed the ac: row"
        );
        assert_eq!(store.len(), 0, "no orphan AC envelope row lingers");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_mode_a_delete_touches_no_envelope_row() {
        // Mode A on the AC surface writes no envelope row; reclaim is a no-op.
        let store = Arc::new(MemEnvStore::default());
        let h = ac_handler_with_byok_random_store(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            Arc::clone(&store),
        )
        .await;
        let req = ac_update_req(BYOK_TENANT, b"ac convergent".to_vec());
        h.byok_encrypt_for_update(&req).await.unwrap().unwrap();
        assert_eq!(store.len(), 0, "Mode A writes no AC envelope row");
        h.byok_reclaim_for_delete(BYOK_TENANT, &req.action_digest)
            .await
            .unwrap();
        assert_eq!(store.len(), 0, "Mode-A AC delete touches no envelope row");
    }

    #[derive(Debug)]
    struct TransitionCfgSrc {
        result: std::sync::Mutex<Result<Option<TenantByokConfig>, ByokConfigError>>,
    }

    #[async_trait::async_trait]
    impl ByokConfigSource for TransitionCfgSrc {
        async fn get_byok_config(
            &self,
            _tenant: &str,
        ) -> Result<Option<TenantByokConfig>, ByokConfigError> {
            self.result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn router_factory_attachments_share_cache_across_storage_and_accounting() {
        // Exercise the four helpers called by router assembly with controlled
        // R2/KMS seams. A second cache in any attachment breaks pointer identity
        // and would observe this transition independently.
        let source = Arc::new(TransitionCfgSrc {
            result: std::sync::Mutex::new(Ok(Some(byok_cfg(
                ByokCryptoMode::Convergent,
                ByokState::Inactive,
            )))),
        });
        let cache = Arc::new(ByokConfigCache::new(source.clone(), 0));
        let resolver = Arc::new(
            TcsResolver::new(Arc::new(SecSrc), Arc::new(Kms { fail: false }), 300).unwrap(),
        );
        let mode_b = Arc::new(
            ModeBEncryptor::new(
                Arc::new(Kms { fail: false }) as Arc<dyn KmsProvider>,
                Arc::new(MemEnvStore::default()) as Arc<dyn ByokEnvelopeStore>,
                300,
            )
            .unwrap(),
        );
        let byok = crate::storage::byok_cas::DataPlaneByok::new(cache.clone(), resolver, mode_b);
        let cas = Arc::new(crate::routes::cas::attach_byok_to_r2_handler(
            make_test_handler_with_tdk("iad").await,
            Some(&byok),
        ));
        let ac = Arc::new(crate::routes::ac::attach_byok_to_r2_handler(
            make_test_ac_handler("iad").await,
            Some(&byok),
        ));
        let byte_store = Arc::new(crate::byte_accounting::testing::InMemoryByteStore::new());
        let accountant = Arc::new(crate::byte_accounting::ByteAccountant::new(
            byte_store as Arc<dyn crate::byte_accounting::ByteStore>,
            "iad".to_owned(),
        ));
        let cas_accounting = crate::routes::build::attach_byok_to_cas_accounting(
            crate::byte_accounting::AccountingCasHandler::new(
                cas.clone() as Arc<dyn CasWriteHandler>,
                cas.clone() as Arc<dyn CasDeleteHandler>,
                accountant.clone(),
            ),
            Some(&byok),
        );
        let ac_accounting = crate::routes::build::attach_byok_to_ac_accounting(
            crate::byte_accounting::AccountingAcHandler::new(
                ac.clone() as Arc<dyn corelink_handler_ac::AcUpdateHandler>,
                ac.clone() as Arc<dyn corelink_handler_ac::AcDeleteHandler>,
                accountant,
            ),
            Some(&byok),
        );
        for attached in [
            cas.byok_config_cache_for_test().expect("CAS cache"),
            ac.byok_config_cache_for_test().expect("AC cache"),
            cas_accounting
                .byok_config_cache_for_test()
                .expect("CAS accounting cache"),
            ac_accounting
                .byok_config_cache_for_test()
                .expect("AC accounting cache"),
        ] {
            assert!(Arc::ptr_eq(&cache, attached));
        }

        let cas_request = write_req(BYOK_TENANT, b"factory transition".to_vec());
        assert!(cas.byok_encrypt_for_write(&cas_request).await.unwrap().is_none());
        assert_eq!(
            crate::byte_accounting::byok_committed_len_for_test(
                cas_accounting.byok_config_cache_for_test(),
                BYOK_TENANT,
                cas_request.bytes.len() as i64,
            )
            .unwrap(),
            cas_request.bytes.len() as i64,
        );

        *source
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Ok(Some(byok_cfg(
            ByokCryptoMode::Convergent,
            ByokState::Active,
        )));
        assert!(cas.byok_encrypt_for_write(&cas_request).await.unwrap().is_some());
        assert_eq!(
            crate::byte_accounting::byok_committed_len_for_test(
                ac_accounting.byok_config_cache_for_test(),
                BYOK_TENANT,
                cas_request.bytes.len() as i64,
            )
            .unwrap(),
            cas_request.bytes.len() as i64 + BYOK_CLB1_OVERHEAD as i64,
        );

        *source
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Err(ByokConfigError::Transport(
            "injected control-plane failure".to_owned(),
        ));
        assert!(cas.byok_encrypt_for_write(&cas_request).await.is_err());
        assert!(crate::byte_accounting::byok_committed_len_for_test(
            cas_accounting.byok_config_cache_for_test(),
            BYOK_TENANT,
            cas_request.bytes.len() as i64,
        )
        .is_err());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_inactive_handler_is_plaintext_passthrough() {
        // No `with_byok` ⇒ AC plaintext path, byte-identical to today.
        let h = make_test_ac_handler("iad").await;
        let req = ac_update_req(BYOK_TENANT, b"ac-result".to_vec());
        assert!(
            h.byok_encrypt_for_update(&req).await.unwrap().is_none(),
            "no BYOK collaborators ⇒ store the AC payload plaintext (None)"
        );
        let out = h
            .byok_decrypt_for_lookup(BYOK_TENANT, &req.action_digest, b"ac-result".to_vec())
            .await
            .unwrap();
        assert_eq!(
            out, b"ac-result",
            "lookup must return the stored bytes unchanged"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_active_encrypts_then_round_trips() {
        let h = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let payload = b"proven action result metadata".to_vec();
        let req = ac_update_req(BYOK_TENANT, payload.clone());
        let stored = h
            .byok_encrypt_for_update(&req)
            .await
            .unwrap()
            .expect("active tenant must encrypt the AC payload");
        assert_ne!(stored, payload, "AC stored bytes must be ciphertext");
        // Convergent ⇒ identical payload yields byte-identical stored bytes — this
        // is what keeps the divergent-body GET-and-compare idempotent for an
        // active tenant (ciphertext-vs-ciphertext).
        let stored2 = h.byok_encrypt_for_update(&req).await.unwrap().unwrap();
        assert_eq!(stored, stored2, "convergent ⇒ idempotent AC stored bytes");
        let out = h
            .byok_decrypt_for_lookup(BYOK_TENANT, &req.action_digest, stored)
            .await
            .unwrap();
        assert_eq!(out, payload, "decrypt must recover the AC plaintext");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_active_write_fails_closed_when_kms_down() {
        let h = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            true,
        )
        .await;
        let req = ac_update_req(BYOK_TENANT, b"secret".to_vec());
        assert!(
            matches!(
                h.byok_encrypt_for_update(&req).await,
                Err(corelink_handler_ac::AcHandlerError::Internal(_))
            ),
            "active tenant + failing KMS must fail closed on AC write (never plaintext)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_active_read_fails_closed_when_kms_down() {
        let h = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            true,
        )
        .await;
        assert!(
            matches!(
                h.byok_decrypt_for_lookup(BYOK_TENANT, &"a".repeat(64), b"CLB1raw".to_vec())
                    .await,
                Err(corelink_handler_ac::AcHandlerError::Internal(_))
            ),
            "active tenant + failing KMS must fail closed on AC read (raw bytes never served)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_mode_b_and_partial_fail_closed() {
        for (mode, state) in [
            (ByokCryptoMode::Random, ByokState::Active),
            (ByokCryptoMode::Convergent, ByokState::Partial),
        ] {
            let h = ac_handler_with_byok(Some(byok_cfg(mode, state)), false).await;
            let req = ac_update_req(BYOK_TENANT, b"x".to_vec());
            assert!(
                matches!(
                    h.byok_encrypt_for_update(&req).await,
                    Err(corelink_handler_ac::AcHandlerError::Internal(_))
                ),
                "Mode B / partial AC write must fail closed, never plaintext ({mode:?},{state:?})"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_byok_public_namespace_never_encrypts() {
        let h = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let req = ac_update_req(crate::adapter_cache::PUBLIC_NAMESPACE, b"shared".to_vec());
        assert!(
            h.byok_encrypt_for_update(&req).await.unwrap().is_none(),
            "_public AC entries must never be encrypted (dedup)"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn ac_blob_does_not_decrypt_under_cas_and_vice_versa() {
        // Frozen policy H1 at the HANDLER level: an AC-encrypted blob must NOT be
        // decryptable by the CAS read hook, and a CAS blob must NOT be decryptable
        // by the AC lookup hook — even for the SAME tenant + digest + tcs (same
        // SecSrc/Kms). Domain separation by `surface` makes an AC↔CAS blob swap
        // fail closed end-to-end.
        let digest = "a".repeat(64);
        let payload = b"cross-surface".to_vec();
        let ac = ac_handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;
        let cas = handler_with_byok(
            Some(byok_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            false,
        )
        .await;

        let ac_req = corelink_handler_ac::AcUpdateRequest::new(
            BYOK_TENANT,
            digest.clone(),
            payload.clone(),
            "p",
            BYOK_TENANT,
            1,
        );
        let ac_blob = ac.byok_encrypt_for_update(&ac_req).await.unwrap().unwrap();
        let cas_rreq = CasReadRequest::new(BYOK_TENANT, &digest, "p", BYOK_TENANT, 1);
        assert!(
            cas.byok_decrypt_for_read(&cas_rreq, ac_blob).await.is_err(),
            "an AC blob must NOT decrypt under the CAS surface"
        );

        let cas_wreq =
            CasWriteRequest::new(BYOK_TENANT, digest.clone(), payload, "p", BYOK_TENANT, 1);
        let cas_blob = cas
            .byok_encrypt_for_write(&cas_wreq)
            .await
            .unwrap()
            .unwrap();
        assert!(
            ac.byok_decrypt_for_lookup(BYOK_TENANT, &digest, cas_blob)
                .await
                .is_err(),
            "a CAS blob must NOT decrypt under the AC surface"
        );
    }

    // ---------------------------------------------------------------
    // WP-C — AC conditional PUT: prior-state classification
    // ---------------------------------------------------------------

    #[test]
    fn classify_prior_absent_allows_put() {
        assert_eq!(
            classify_prior(None, b"payload"),
            PriorState::Absent,
            "no prior object → the caller may create it"
        );
    }

    #[test]
    fn classify_prior_identical_is_idempotent_no_op() {
        let view = vec![7u8; 32];
        assert_eq!(
            classify_prior(Some(&view), &view),
            PriorState::Identical,
            "byte-identical re-PUT → no-op (durable=false)"
        );
    }

    #[test]
    fn classify_prior_divergent_refuses() {
        let view = vec![1u8; 16];
        let other = vec![2u8; 16];
        assert_eq!(
            classify_prior(Some(&other), &view),
            PriorState::Divergent,
            "divergent stored bytes → DivergentBody (409), never overwrite"
        );
    }

    #[test]
    fn classify_prior_empty_vs_empty_is_identical_not_divergent() {
        // Edge: an empty stored body and empty would-be-stored bytes are
        // EQUAL — the immutability contract compares bytes, not presence.
        assert_eq!(classify_prior(Some(&[]), &[]), PriorState::Identical);
    }

    #[test]
    fn lost_race_resolution_mirrors_pre_put_compare() {
        // The Ok(false) arm of put_if_absent re-GETs and applies the SAME
        // classifier: identical → no-op, divergent → DivergentBody. Pin the
        // mapping so a future edit cannot make the loser overwrite or
        // double-report.
        let winner_bytes = vec![9u8; 64];
        let loser_view = vec![1u8; 64];

        // Loser wrote the SAME bytes → idempotent success (durable=false).
        assert_eq!(
            classify_prior(Some(&winner_bytes), &winner_bytes),
            PriorState::Identical
        );

        // Loser's body DIVERGES from what won → refused, winner preserved.
        assert_eq!(
            classify_prior(Some(&winner_bytes), &loser_view),
            PriorState::Divergent
        );
    }
