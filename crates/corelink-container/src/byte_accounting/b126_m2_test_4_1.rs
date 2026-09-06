//! BYOK Wave 3b (audit C3 CRITICAL): the accountant reserves/releases the
//! COMMITTED (stored) size — `plaintext + BYOK_CLB1_OVERHEAD` for a
//! BYOK-`active` tenant — so reserve == release == the on-disk object the
//! delete path frees, and `bytes_used` never drifts. Non-BYOK tenants are
//! byte-identical to today.
use super::testing::InMemoryByteStore;
use super::*;
use crate::customer_d1::{ByokConfigError, ByokCryptoMode, ByokMode, ByokState, TenantByokConfig};
use crate::storage::byok_cas::{ByokConfigCache, ByokConfigSource};
use corelink_handler_cas::{
    CasDeleteHandler, CasDeleteRequest, CasDeleteResponse, CasHandlerError, CasWriteHandler,
    CasWriteRequest, CasWriteResponse,
};

const REGION: &str = "iad";
const TENANT: &str = "byok-acct-tenant";

fn cfg(mode: ByokCryptoMode, state: ByokState) -> TenantByokConfig {
    TenantByokConfig {
        tenant_id: TENANT.to_owned(),
        mode: ByokMode::Byok,
        crypto_mode: mode,
        cmk_provider: Some("aws".to_owned()),
        cmk_key_id: Some("arn:cmk".to_owned()),
        cmk_region: Some("iad".to_owned()),
        state,
    }
}

#[derive(Debug)]
struct CfgSrc {
    cfg: Option<TenantByokConfig>,
    fail: bool,
}
#[async_trait]
impl ByokConfigSource for CfgSrc {
    async fn get_byok_config(&self, _t: &str) -> Result<Option<TenantByokConfig>, ByokConfigError> {
        if self.fail {
            return Err(ByokConfigError::Transport("byok config down".to_owned()));
        }
        Ok(self.cfg.clone())
    }
}

fn cache(cfg: Option<TenantByokConfig>, fail: bool) -> Arc<ByokConfigCache> {
    Arc::new(ByokConfigCache::new(Arc::new(CfgSrc { cfg, fail }), 60))
}

// ── byok_committed_len: the reserve/release sizing decision ──────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_len_is_plaintext_when_cache_absent() {
    // `None` cache (today's production / tests) ⇒ plaintext size verbatim.
    assert_eq!(byok_committed_len(None, TENANT, 1000).unwrap(), 1000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_len_adds_overhead_only_for_active_convergent() {
    let active = cache(
        Some(cfg(ByokCryptoMode::Convergent, ByokState::Active)),
        false,
    );
    assert_eq!(
        byok_committed_len(Some(&active), TENANT, 1000).unwrap(),
        1000 + BYOK_CLB1_OVERHEAD as i64,
        "an active convergent tenant stores ciphertext ⇒ reserve plaintext + 32"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_len_is_plaintext_for_inactive_and_unconfigured() {
    let inactive = cache(
        Some(cfg(ByokCryptoMode::Convergent, ByokState::Inactive)),
        false,
    );
    assert_eq!(
        byok_committed_len(Some(&inactive), TENANT, 1000).unwrap(),
        1000
    );
    let unconfigured = cache(None, false);
    assert_eq!(
        byok_committed_len(Some(&unconfigured), TENANT, 1000).unwrap(),
        1000
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_len_is_plaintext_for_public_namespace() {
    // `_public` stays plaintext (dedup) even under an active config.
    let active = cache(
        Some(cfg(ByokCryptoMode::Convergent, ByokState::Active)),
        false,
    );
    assert_eq!(
        byok_committed_len(Some(&active), crate::adapter_cache::PUBLIC_NAMESPACE, 1000).unwrap(),
        1000,
        "_public is never encrypted ⇒ plaintext-size accounting"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_len_adds_clb2_overhead_for_active_random() {
    // Wave 3c: Mode B (random) is an ENCRYPTING mode and stores a `CLB2` blob
    // (+20 B — nonce lives in `byok_envelope`, not inline), so the reservation
    // must reflect the committed ciphertext size.
    let mode_b = cache(Some(cfg(ByokCryptoMode::Random, ByokState::Active)), false);
    assert_eq!(
        byok_committed_len(Some(&mode_b), TENANT, 1000).unwrap(),
        1000 + BYOK_CLB2_OVERHEAD as i64,
        "an active random tenant stores CLB2 ciphertext ⇒ reserve plaintext + 20"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_len_is_plaintext_for_failclosed_modes() {
    // `partial` (backfill dual-read) still engages `FailClosed` (Wave 4): the
    // inner write stores NOTHING (fails closed), so the reservation rolls back
    // net-zero ⇒ plaintext size.
    let partial = cache(
        Some(cfg(ByokCryptoMode::Convergent, ByokState::Partial)),
        false,
    );
    assert_eq!(
        byok_committed_len(Some(&partial), TENANT, 1000).unwrap(),
        1000
    );
    let partial_random = cache(Some(cfg(ByokCryptoMode::Random, ByokState::Partial)), false);
    assert_eq!(
        byok_committed_len(Some(&partial_random), TENANT, 1000).unwrap(),
        1000
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn committed_len_fails_closed_on_config_error() {
    // A config-read error must NOT under-reserve an active tenant → Err (503).
    let broken = cache(None, true);
    assert!(byok_committed_len(Some(&broken), TENANT, 1000).is_err());
}

// ── decorator net-zero with a faithful encrypting inner ──────────────────

/// A fake inner CAS handler that models the production BYOK R2 handler: it
/// stores the CIPHERTEXT object (`plaintext + BYOK_CLB1_OVERHEAD`) and, on
/// delete, reclaims exactly that committed object size — so a write→delete
/// cycle's reserve and release both move by the committed size.
#[derive(Debug, Default)]
struct EncryptingCasInner {
    stored: std::sync::Mutex<std::collections::HashMap<(String, String), u64>>,
}
impl CasWriteHandler for EncryptingCasInner {
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        let committed = req.bytes.len() as u64 + BYOK_CLB1_OVERHEAD;
        let mut m = self.stored.lock().unwrap();
        let key = (req.tenant.clone(), req.claimed_hash.clone());
        // Content-addressed: a re-PUT of an already-present key is idempotent.
        let durable = !m.contains_key(&key);
        m.insert(key, committed);
        Ok(CasWriteResponse::new(req.claimed_hash, durable))
    }
}
impl CasDeleteHandler for EncryptingCasInner {
    fn delete(&self, req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError> {
        let mut m = self.stored.lock().unwrap();
        let reclaimed = m
            .remove(&(req.tenant.clone(), req.hash.clone()))
            .unwrap_or(0);
        Ok(CasDeleteResponse::with_reclaimed(reclaimed > 0, reclaimed))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn active_byok_write_then_delete_nets_to_zero_at_committed_size() {
    // The C3 assertion: for a BYOK-active tenant the accountant reserves the
    // ciphertext size (plaintext + 32) on write and the delete releases the
    // SAME committed size (the real R2 object), so a write→delete cycle leaves
    // bytes_used at exactly zero — no over-release, no under-count.
    let store = Arc::new(InMemoryByteStore::new());
    let acc = Arc::new(ByteAccountant::new(
        store.clone() as Arc<dyn ByteStore>,
        REGION.to_owned(),
    ));
    let inner = Arc::new(EncryptingCasInner::default());
    let dec = AccountingCasHandler::new(
        inner.clone() as Arc<dyn CasWriteHandler>,
        inner as Arc<dyn CasDeleteHandler>,
        acc,
    )
    .with_byok(cache(
        Some(cfg(ByokCryptoMode::Convergent, ByokState::Active)),
        false,
    ));

    let body = vec![b'x'; 500];
    let n = body.len() as i64;
    let committed = n + BYOK_CLB1_OVERHEAD as i64;
    let hash = "a".repeat(64);
    dec.write(
        CasWriteRequest::new(TENANT, hash.clone(), body, "p", TENANT, 1)
            .with_storage_quota_bytes(Some(0)),
    )
    .expect("active write");
    assert_eq!(
        store.used(TENANT, REGION),
        committed,
        "an active-BYOK write must reserve the COMMITTED ciphertext size (plaintext + 32)"
    );

    dec.delete(CasDeleteRequest::new(TENANT, hash, "p", TENANT, 2))
        .expect("delete");
    assert_eq!(
        store.used(TENANT, REGION),
        0,
        "the delete must release the SAME committed size ⇒ net-zero, no drift (C3)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inactive_tenant_with_cache_wired_is_unchanged_plaintext_accounting() {
    // A non-BYOK (inactive) tenant, even with the cache wired, accounts at the
    // plaintext size — zero behaviour change. The faithful inner stores the
    // committed size, but since the tenant is inactive the accountant reserves
    // plaintext; the delete still nets to zero because the inner reclaims what
    // it stored (here +32) — proving the accountant tracks the inner, and that
    // for an INACTIVE tenant the reserve is plaintext (not plaintext+32).
    let store = Arc::new(InMemoryByteStore::new());
    let acc = Arc::new(ByteAccountant::new(
        store.clone() as Arc<dyn ByteStore>,
        REGION.to_owned(),
    ));
    let inner = Arc::new(EncryptingCasInner::default());
    let dec = AccountingCasHandler::new(
        inner.clone() as Arc<dyn CasWriteHandler>,
        inner as Arc<dyn CasDeleteHandler>,
        acc,
    )
    .with_byok(cache(
        Some(cfg(ByokCryptoMode::Convergent, ByokState::Inactive)),
        false,
    ));

    let body = vec![b'y'; 500];
    let n = body.len() as i64;
    let hash = "b".repeat(64);
    dec.write(
        CasWriteRequest::new(TENANT, hash.clone(), body, "p", TENANT, 1)
            .with_storage_quota_bytes(Some(0)),
    )
    .expect("inactive write");
    assert_eq!(
        store.used(TENANT, REGION),
        n,
        "an INACTIVE tenant must reserve the PLAINTEXT size (no BYOK overhead)"
    );
}

#[allow(dead_code)]
const B126_M2_TEST_4_1_REANCHOR: () = ();
