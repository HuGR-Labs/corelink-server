#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use super::*;
    use crate::customer_d1::ByokMode;
    use corelink_byok::{Dek, KmsAccessStatus, KmsKeyId, KmsProviderKind};

    const TENANT: &str = "tenant-byok-a";

    fn active_cfg(crypto_mode: ByokCryptoMode, state: ByokState) -> TenantByokConfig {
        TenantByokConfig {
            tenant_id: TENANT.to_owned(),
            mode: ByokMode::Byok,
            crypto_mode,
            cmk_provider: Some("aws".to_owned()),
            cmk_key_id: Some("arn:aws:kms:iad:1:key/cmk".to_owned()),
            cmk_region: Some("iad".to_owned()),
            state,
        }
    }

    // ── Mock config source ──────────────────────────────────────────────────

    #[derive(Debug)]
    struct MockConfigSource {
        cfg: Option<TenantByokConfig>,
        calls: Mutex<usize>,
        fail: bool,
    }
    impl MockConfigSource {
        fn ok(cfg: Option<TenantByokConfig>) -> Self {
            Self {
                cfg,
                calls: Mutex::new(0),
                fail: false,
            }
        }
        fn failing() -> Self {
            Self {
                cfg: None,
                calls: Mutex::new(0),
                fail: true,
            }
        }
        fn call_count(&self) -> usize {
            *lock(&self.calls)
        }
    }
    #[async_trait]
    impl ByokConfigSource for MockConfigSource {
        async fn get_byok_config(
            &self,
            _tenant: &str,
        ) -> Result<Option<TenantByokConfig>, ByokConfigError> {
            *lock(&self.calls) += 1;
            if self.fail {
                return Err(ByokConfigError::Transport("mock down".to_owned()));
            }
            Ok(self.cfg.clone())
        }
    }

    // ── Mock secret source + KMS provider ───────────────────────────────────

    #[derive(Debug)]
    struct MockSecretSource {
        row: Option<WrappedTcsRow>,
    }
    #[async_trait]
    impl ByokSecretSource for MockSecretSource {
        async fn get_wrapped_tcs(&self, _tenant: &str) -> Result<Option<WrappedTcsRow>, String> {
            Ok(self.row.clone())
        }
    }

    /// A mock KMS that "unwraps" by treating the wrapped ciphertext as the raw
    /// 32-byte secret (good enough to exercise the resolution + round-trip).
    #[derive(Debug)]
    struct MockKms {
        fail: bool,
        calls: Mutex<usize>,
    }
    impl MockKms {
        fn ok() -> Self {
            Self {
                fail: false,
                calls: Mutex::new(0),
            }
        }
        fn failing() -> Self {
            Self {
                fail: true,
                calls: Mutex::new(0),
            }
        }
        fn unwrap_count(&self) -> usize {
            *lock(&self.calls)
        }
    }
    #[async_trait]
    impl KmsProvider for MockKms {
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
            encryption_context: Option<&Value>,
        ) -> Result<WrappedDek, BYOKError> {
            Ok(WrappedDek {
                provider: KmsProviderKind::AwsKms,
                key_id: key_id.clone(),
                ciphertext: dek.bytes.to_vec(),
                encryption_context: encryption_context.cloned(),
            })
        }
        async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
            *lock(&self.calls) += 1;
            if self.fail {
                return Err(BYOKError::Provider("mock kms down".to_owned()));
            }
            if wrapped.ciphertext.len() != 32 {
                return Err(BYOKError::DekLengthInvalid {
                    got: wrapped.ciphertext.len(),
                });
            }
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&wrapped.ciphertext);
            Ok(Dek { bytes })
        }
        async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
            Ok(KmsAccessStatus::Ok)
        }
    }

    fn wrapped_tcs() -> WrappedTcsRow {
        WrappedTcsRow {
            tcs_wrapped: vec![7u8; 32],
            cmk_key_id: Some("arn:aws:kms:iad:1:key/cmk".to_owned()),
            tcs_version: 1,
        }
    }

    // ── ByokConfigCache ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn config_cache_hit_does_one_d1_read() {
        let src = Arc::new(MockConfigSource::ok(Some(active_cfg(
            ByokCryptoMode::Convergent,
            ByokState::Active,
        ))));
        let cache = ByokConfigCache::new(src.clone(), 60);
        let a = cache.get(TENANT).await.unwrap();
        let b = cache.get(TENANT).await.unwrap();
        assert!(a.is_some());
        assert_eq!(a, b);
        assert_eq!(src.call_count(), 1, "second get must be an in-memory HIT");
    }

    #[tokio::test]
    async fn config_cache_caches_the_none_answer() {
        // Non-BYOK tenants must not re-hit D1.
        let src = Arc::new(MockConfigSource::ok(None));
        let cache = ByokConfigCache::new(src.clone(), 60);
        assert!(cache.get(TENANT).await.unwrap().is_none());
        assert!(cache.get(TENANT).await.unwrap().is_none());
        assert_eq!(
            src.call_count(),
            1,
            "the not-configured answer must be cached"
        );
    }

    #[tokio::test]
    async fn config_cache_propagates_source_error_fail_closed() {
        let src = Arc::new(MockConfigSource::failing());
        let cache = ByokConfigCache::new(src, 60);
        assert!(matches!(
            cache.get(TENANT).await,
            Err(ByokConfigError::Transport(_))
        ));
    }

    #[tokio::test]
    async fn config_cache_zero_ttl_refetches() {
        let src = Arc::new(MockConfigSource::ok(None));
        let cache = ByokConfigCache::new(src.clone(), 0);
        cache.get(TENANT).await.unwrap();
        cache.get(TENANT).await.unwrap();
        assert_eq!(src.call_count(), 2, "a 0s TTL entry is always expired");
    }

    // ── TcsResolver ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn tcs_resolver_unwraps_then_caches() {
        let kms = Arc::new(MockKms::ok());
        let resolver = TcsResolver::new(
            Arc::new(MockSecretSource {
                row: Some(wrapped_tcs()),
            }),
            kms.clone(),
            300,
        )
        .unwrap();
        let cfg = active_cfg(ByokCryptoMode::Convergent, ByokState::Active);
        let t1 = resolver.resolve(&cfg).await.unwrap();
        let t2 = resolver.resolve(&cfg).await.unwrap();
        assert_eq!(t1.bytes, [7u8; 32]);
        assert_eq!(t1.bytes, t2.bytes);
        assert_eq!(
            kms.unwrap_count(),
            1,
            "the unwrapped Tcs must be cached (≤300s)"
        );
    }

    #[tokio::test]
    async fn tcs_resolver_fail_closed_when_secret_missing() {
        let resolver = TcsResolver::new(
            Arc::new(MockSecretSource { row: None }),
            Arc::new(MockKms::ok()),
            300,
        )
        .unwrap();
        let cfg = active_cfg(ByokCryptoMode::Convergent, ByokState::Active);
        assert!(resolver.resolve(&cfg).await.is_err());
    }

    #[tokio::test]
    async fn tcs_resolver_fail_closed_when_kms_down() {
        let resolver = TcsResolver::new(
            Arc::new(MockSecretSource {
                row: Some(wrapped_tcs()),
            }),
            Arc::new(MockKms::failing()),
            300,
        )
        .unwrap();
        let cfg = active_cfg(ByokCryptoMode::Convergent, ByokState::Active);
        assert!(resolver.resolve(&cfg).await.is_err());
    }

    #[test]
    fn tcs_cache_rejects_ttl_over_300() {
        assert!(TcsCache::new(301).is_err());
        assert!(TcsCache::new(300).is_ok());
    }

    // ── engagement_for ────────────────────────────────────────────────────────

    #[test]
    fn engagement_truth_table() {
        // Wave 3c: an active tenant now engages BOTH modes (Random no longer
        // fail-closed); the caller dispatches on the carried mode.
        assert_eq!(
            engagement_for(&active_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            ByokEngagement::Encrypt(ByokCryptoMode::Convergent)
        );
        assert_eq!(
            engagement_for(&active_cfg(ByokCryptoMode::Random, ByokState::Active)),
            ByokEngagement::Encrypt(ByokCryptoMode::Random)
        );
        // Partial/backfill dual-read stays deferred (Wave 4) → fail-closed.
        assert!(matches!(
            engagement_for(&active_cfg(ByokCryptoMode::Convergent, ByokState::Partial)),
            ByokEngagement::FailClosed(_)
        ));
        assert!(matches!(
            engagement_for(&active_cfg(ByokCryptoMode::Random, ByokState::Partial)),
            ByokEngagement::FailClosed(_)
        ));
        for s in [ByokState::Inactive, ByokState::Pending, ByokState::Shredded] {
            assert_eq!(
                engagement_for(&active_cfg(ByokCryptoMode::Convergent, s)),
                ByokEngagement::Plaintext
            );
        }
    }

    // ── §4 key-hardening (audit H-4) ──────────────────────────────────────────

    #[test]
    fn harden_digest_is_not_the_raw_digest_and_is_deterministic() {
        let tcs = Tcs::from_bytes([4u8; 32]);
        let digest = "a".repeat(64);
        let h1 = harden_digest(&tcs, &digest);
        let h2 = harden_digest(&tcs, &digest);
        assert_ne!(h1, digest, "hardened key must not reveal the raw digest");
        assert_eq!(
            h1, h2,
            "deterministic per (TCS, digest) ⇒ intra-tenant dedup hits"
        );
        // HMAC-SHA256 ⇒ 32 bytes ⇒ 64 hex chars (same shape as a digest slot).
        assert_eq!(h1.len(), 64);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn harden_digest_differs_per_tcs_and_per_digest() {
        let digest = "a".repeat(64);
        let other = "b".repeat(64);
        let a = harden_digest(&Tcs::from_bytes([1u8; 32]), &digest);
        let b = harden_digest(&Tcs::from_bytes([2u8; 32]), &digest);
        assert_ne!(
            a, b,
            "different TCS ⇒ different hardened key (no cross-tenant correlation)"
        );
        let c = harden_digest(&Tcs::from_bytes([1u8; 32]), &other);
        assert_ne!(a, c, "different digest ⇒ different hardened key");
    }

    #[test]
    fn convergent_ctx_keeps_the_real_digest_after_hardening() {
        // The storage key uses the hardened digest, but the AAD-bound context
        // digest stays the REAL digest so the post-decrypt integrity re-verify
        // (on the plaintext) still checks the true content hash.
        let tcs = Tcs::from_bytes([7u8; 32]);
        let digest = "c".repeat(64);
        let hardened = harden_digest(&tcs, &digest);
        let ctx = cas_crypto_context(TENANT, &digest, DigestAlgo::Blake3, "arn:cmk");
        assert_eq!(
            ctx.plaintext_digest, digest,
            "ctx must bind the REAL digest"
        );
        assert_ne!(
            ctx.plaintext_digest, hardened,
            "ctx digest is NOT the storage key"
        );
        // Round-trip still works with the real-digest context.
        let pt = b"payload".to_vec();
        let stored = encrypt_cas_blob(&pt, &tcs, &ctx).unwrap();
        assert_eq!(decrypt_cas_blob(&stored, &tcs, &ctx).unwrap(), pt);
    }

    // ── blob round-trip + convergent dedup + tamper ──────────────────────────

    fn ctx() -> CryptoContext {
        cas_crypto_context(TENANT, &"a".repeat(64), DigestAlgo::Blake3, "arn:cmk")
    }

    fn ac_ctx() -> CryptoContext {
        ac_crypto_context(TENANT, &"a".repeat(64), "arn:cmk")
    }

    #[test]
    fn blob_roundtrip_and_ciphertext_differs_from_plaintext() {
        let tcs = Tcs::from_bytes([3u8; 32]);
        let pt = b"sensitive build artifact bytes".to_vec();
        let stored = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
        assert_ne!(stored, pt, "stored bytes must be ciphertext, not plaintext");
        assert_eq!(&stored[..4], BLOB_MAGIC);
        let out = decrypt_cas_blob(&stored, &tcs, &ctx()).unwrap();
        assert_eq!(out, pt, "round-trip must recover the plaintext");
    }

    #[test]
    fn convergent_dedup_same_content_same_ciphertext() {
        let tcs = Tcs::from_bytes([9u8; 32]);
        let pt = b"identical content".to_vec();
        let a = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
        let b = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
        assert_eq!(
            a, b,
            "convergent ⇒ identical stored bytes (dedup-idempotent)"
        );
    }

    #[test]
    fn decrypt_rejects_non_magic_bytes_fail_closed() {
        // A plaintext object served to an active tenant must NOT be returned raw.
        let tcs = Tcs::from_bytes([1u8; 32]);
        assert!(decrypt_cas_blob(b"raw plaintext, no magic", &tcs, &ctx()).is_err());
        assert!(decrypt_cas_blob(b"", &tcs, &ctx()).is_err());
    }

    #[test]
    fn decrypt_wrong_tcs_fails_closed() {
        let stored = encrypt_cas_blob(b"x", &Tcs::from_bytes([1u8; 32]), &ctx()).unwrap();
        assert!(decrypt_cas_blob(&stored, &Tcs::from_bytes([2u8; 32]), &ctx()).is_err());
    }

    #[test]
    fn decode_blob_accepts_array_and_base64() {
        assert_eq!(decode_blob(&json!([1, 2, 3])).unwrap(), vec![1u8, 2, 3]);
        let b64 = base64::engine::general_purpose::STANDARD.encode([4u8, 5, 6]);
        assert_eq!(decode_blob(&json!(b64)).unwrap(), vec![4u8, 5, 6]);
        assert!(decode_blob(&json!(true)).is_err());
        assert!(decode_blob(&json!([1, 999])).is_err());
    }

    #[test]
    fn namespace_is_algo_separated() {
        assert_ne!(
            namespace_for(DigestAlgo::Blake3),
            namespace_for(DigestAlgo::Sha256)
        );
    }

    // ── AC surface (Wave 3b) ──────────────────────────────────────────────────

    #[test]
    fn ac_blob_roundtrips_under_ac_context() {
        // The AC payload encrypts + decrypts cleanly under an `"ac"`-surface ctx.
        let tcs = Tcs::from_bytes([5u8; 32]);
        let pt = b"action-result-metadata payload".to_vec();
        let stored = encrypt_cas_blob(&pt, &tcs, &ac_ctx()).unwrap();
        assert_ne!(stored, pt, "AC stored bytes must be ciphertext");
        assert_eq!(&stored[..4], BLOB_MAGIC);
        assert_eq!(decrypt_cas_blob(&stored, &tcs, &ac_ctx()).unwrap(), pt);
    }

    #[test]
    fn ac_and_cas_surfaces_are_domain_separated() {
        // Frozen policy H1: an AC blob must NOT decrypt under a CAS context, and
        // a CAS blob must NOT decrypt under an AC context — `surface` is bound
        // into the derived key + AEAD AAD, so swapping surfaces fails closed even
        // for the identical (tenant, digest, tcs).
        let tcs = Tcs::from_bytes([6u8; 32]);
        let pt = b"swap-me".to_vec();
        let ac_blob = encrypt_cas_blob(&pt, &tcs, &ac_ctx()).unwrap();
        let cas_blob = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
        // Different surface ⇒ different ciphertext for identical input.
        assert_ne!(ac_blob, cas_blob, "surface must perturb the derivation");
        // Cross-surface decrypt must fail (never returns plaintext).
        assert!(
            decrypt_cas_blob(&ac_blob, &tcs, &ctx()).is_err(),
            "AC blob must not decrypt as CAS"
        );
        assert!(
            decrypt_cas_blob(&cas_blob, &tcs, &ac_ctx()).is_err(),
            "CAS blob must not decrypt as AC"
        );
    }

    #[test]
    fn clb1_overhead_matches_the_wire_format() {
        // Single-source check: the stored blob is exactly plaintext + 32 (4 magic
        // + 12 nonce + 16 GCM tag), so the accounting const can never drift.
        assert_eq!(BYOK_CLB1_OVERHEAD, 32);
        let tcs = Tcs::from_bytes([8u8; 32]);
        for pt_len in [0usize, 1, 17, 4096] {
            let pt = vec![b'z'; pt_len];
            let stored = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
            assert_eq!(
                stored.len() as u64,
                pt_len as u64 + BYOK_CLB1_OVERHEAD,
                "stored len must be plaintext + CLB1 overhead for pt_len={pt_len}"
            );
        }
    }

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
}
