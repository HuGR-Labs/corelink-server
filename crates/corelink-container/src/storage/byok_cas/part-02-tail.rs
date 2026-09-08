    // ── Mode B (random, max-isolation — Wave 3c) ──────────────────────────────

    #[derive(Debug, Default)]
    struct MemEnvelopeStore {
        inner: Mutex<HashMap<(String, String), ByokEnvelopeRow>>,
    }
    #[async_trait]
    impl ByokEnvelopeStore for MemEnvelopeStore {
        async fn get_envelope(
            &self,
            tenant: &str,
            blob_key: &str,
        ) -> Result<Option<ByokEnvelopeRow>, String> {
            Ok(lock(&self.inner)
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
            // INSERT … ON CONFLICT DO NOTHING semantics: only the first writer wins.
            lock(&self.inner)
                .entry((tenant.to_owned(), blob_key.to_owned()))
                .or_insert_with(|| row.clone());
            Ok(())
        }
        async fn delete_envelope(&self, tenant: &str, blob_key: &str) -> Result<(), String> {
            lock(&self.inner).remove(&(tenant.to_owned(), blob_key.to_owned()));
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

    fn mode_b_ctx() -> CryptoContext {
        cas_crypto_context_for(
            TENANT,
            &"d".repeat(64),
            DigestAlgo::Blake3,
            "arn:cmk",
            CryptoMode::Random,
        )
    }

    fn mode_b_enc(store: Arc<MemEnvelopeStore>, kms_fail: bool) -> ModeBEncryptor {
        let kms: Arc<dyn KmsProvider> = if kms_fail {
            Arc::new(MockKms::failing())
        } else {
            Arc::new(MockKms::ok())
        };
        ModeBEncryptor::new(kms, store, 300).unwrap()
    }

    #[tokio::test]
    async fn mode_b_round_trip() {
        let store = Arc::new(MemEnvelopeStore::default());
        let me = mode_b_enc(store, false);
        let ctx = mode_b_ctx();
        let pt = b"mode b secret payload".to_vec();
        let stored = me.encrypt(&pt, &ctx).await.unwrap();
        assert_eq!(
            &stored[..4],
            MODE_B_MAGIC,
            "Mode-B blob carries the CLB2 magic"
        );
        assert_ne!(stored[4..].to_vec(), pt, "stored bytes must be ciphertext");
        assert_eq!(me.decrypt(&stored, &ctx).await.unwrap(), pt, "round-trip");
    }

    #[tokio::test]
    async fn mode_b_independent_writes_do_not_converge() {
        // No convergence: two INDEPENDENT Mode-B encryptors each mint their own
        // random DEK, so the SAME content+digest yields DIFFERENT ciphertext —
        // and each still decrypts under its own envelope.
        let ctx = mode_b_ctx();
        let pt = b"identical bytes".to_vec();
        let s1 = Arc::new(MemEnvelopeStore::default());
        let s2 = Arc::new(MemEnvelopeStore::default());
        let e1 = mode_b_enc(Arc::clone(&s1), false);
        let e2 = mode_b_enc(Arc::clone(&s2), false);
        let c1 = e1.encrypt(&pt, &ctx).await.unwrap();
        let c2 = e2.encrypt(&pt, &ctx).await.unwrap();
        assert_ne!(c1, c2, "random DEK ⇒ no convergence (vs Mode A)");
        assert_eq!(e1.decrypt(&c1, &ctx).await.unwrap(), pt);
        assert_eq!(e2.decrypt(&c2, &ctx).await.unwrap(), pt);
    }

    #[tokio::test]
    async fn mode_b_re_put_reuses_envelope_no_orphan() {
        // Re-PUT of the same blob_hash reuses the persisted (DEK, nonce) — the
        // ciphertext is byte-identical (idempotent) and the original still
        // decrypts (the envelope was NOT rotated to a fresh DEK; audit C2).
        let store = Arc::new(MemEnvelopeStore::default());
        let me = mode_b_enc(Arc::clone(&store), false);
        let ctx = mode_b_ctx();
        let pt = b"idempotent payload".to_vec();
        let first = me.encrypt(&pt, &ctx).await.unwrap();
        let second = me.encrypt(&pt, &ctx).await.unwrap();
        assert_eq!(first, second, "re-PUT reuses the persisted DEK (no orphan)");
        assert_eq!(
            me.decrypt(&first, &ctx).await.unwrap(),
            pt,
            "original still decrypts"
        );
        assert_eq!(
            lock(&store.inner).len(),
            1,
            "exactly one envelope row per blob"
        );
    }

    #[tokio::test]
    async fn mode_b_reclaim_deletes_the_envelope_row() {
        // Wave 4a: reclaim removes the matching surface-qualified envelope row.
        let store = Arc::new(MemEnvelopeStore::default());
        let me = mode_b_enc(Arc::clone(&store), false);
        let ctx = mode_b_ctx();
        me.encrypt(b"reclaim me", &ctx).await.unwrap();
        assert_eq!(lock(&store.inner).len(), 1, "write minted one envelope row");
        let expected_key = envelope_blob_key(&ctx.surface, &ctx.plaintext_digest);
        assert!(
            lock(&store.inner).contains_key(&(TENANT.to_owned(), expected_key)),
            "row stored under the surface-qualified key"
        );
        me.reclaim(&ctx).await.unwrap();
        assert_eq!(
            lock(&store.inner).len(),
            0,
            "reclaim deletes the envelope row"
        );
    }

    #[tokio::test]
    async fn mode_b_reclaim_then_re_put_mints_a_fresh_envelope() {
        // After reclaim, a re-PUT of the SAME blob mints a FRESH envelope (the
        // deleted DEK is not reused) — proves the reclaim actually happened.
        let store = Arc::new(MemEnvelopeStore::default());
        let me = mode_b_enc(Arc::clone(&store), false);
        let ctx = mode_b_ctx();
        let key = (
            TENANT.to_owned(),
            envelope_blob_key(&ctx.surface, &ctx.plaintext_digest),
        );
        me.encrypt(b"fresh-dek", &ctx).await.unwrap();
        let dek_before = lock(&store.inner).get(&key).unwrap().wrapped_dek.clone();
        let nonce_before = lock(&store.inner).get(&key).unwrap().nonce;
        me.reclaim(&ctx).await.unwrap();
        assert!(
            lock(&store.inner).get(&key).is_none(),
            "row gone after reclaim"
        );
        // Re-PUT mints a brand-new random DEK + nonce (no stale reuse).
        me.encrypt(b"fresh-dek", &ctx).await.unwrap();
        let row_after = lock(&store.inner).get(&key).unwrap().clone();
        assert!(
            row_after.wrapped_dek != dek_before || row_after.nonce != nonce_before,
            "re-PUT after reclaim mints a FRESH envelope, not the deleted one"
        );
    }

    #[tokio::test]
    async fn mode_b_reclaim_is_idempotent_on_absent_row() {
        // Deleting an absent row is a no-op success (idempotent reclaim).
        let store = Arc::new(MemEnvelopeStore::default());
        let me = mode_b_enc(store, false);
        let ctx = mode_b_ctx();
        me.reclaim(&ctx).await.unwrap();
        me.reclaim(&ctx).await.unwrap();
    }

    #[tokio::test]
    async fn mode_b_write_fails_closed_when_kms_down() {
        let store = Arc::new(MemEnvelopeStore::default());
        let me = mode_b_enc(store, true);
        let ctx = mode_b_ctx();
        assert!(
            me.encrypt(b"x", &ctx).await.is_err(),
            "kms unwrap down ⇒ Mode-B write fails closed (never plaintext)"
        );
    }

    #[tokio::test]
    async fn mode_b_read_fails_closed_without_envelope_row() {
        let store = Arc::new(MemEnvelopeStore::default());
        let me = mode_b_enc(store, false);
        let ctx = mode_b_ctx();
        // Valid CLB2 magic but no envelope row → fail closed (never serve raw).
        let mut orphan = MODE_B_MAGIC.to_vec();
        orphan.extend_from_slice(b"ciphertext-without-a-row");
        assert!(me.decrypt(&orphan, &ctx).await.is_err());
        // A non-magic (legacy plaintext) object → fail closed.
        assert!(me.decrypt(b"raw plaintext", &ctx).await.is_err());
    }

    #[test]
    fn envelope_blob_key_is_surface_qualified() {
        assert_eq!(envelope_blob_key("cas", "abc"), "cas:abc");
        assert_ne!(
            envelope_blob_key("cas", "abc"),
            envelope_blob_key("ac", "abc"),
            "CAS and AC must not collide on one envelope row"
        );
    }

    #[test]
    fn provider_kind_str_round_trips() {
        for k in [
            KmsProviderKind::AwsKms,
            KmsProviderKind::GcpKms,
            KmsProviderKind::AzureKeyVault,
            KmsProviderKind::HashicorpVault,
        ] {
            assert_eq!(parse_provider_kind(provider_kind_str(k)).unwrap(), k);
        }
        assert!(parse_provider_kind("nope").is_err());
    }
