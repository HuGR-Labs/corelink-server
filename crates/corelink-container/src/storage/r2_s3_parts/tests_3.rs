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
