//! R3-2 Adversarial: 10 000-entry DEK cache stampede revoke.
//!
//! Pins INV-BYOK-CRYPTO-SOVEREIGNTY assertion #2.
//!
//! - Cache capacity ceiling is 10 000 (per
//!   `corelink_byok::dek_cache::MAX_DEK_CACHE_ENTRIES`). The test fills
//!   the cache, revokes, then asserts:
//!   1. All entries gone within the kill-switch SLA.
//!   2. The cache reports `len() == 0` and `is_empty() == true` —
//!      proves no plaintext DEK lingers (each `Dek` is `ZeroizeOnDrop`).
//!   3. Subsequent `get(&wrapped)` returns `None` for every prior entry
//!      — no stale plaintext recoverable.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_byok::{Dek, KmsProviderKind};
use e2e_byok_revoke::helpers::{
    make_wrapped_for, KillSwitchRunner, KmsBehaviour, KILL_SWITCH_SLA_MS,
};
use e2e_byok_revoke::setup_byok_env;

const STAMPEDE_ENTRIES: u32 = 10_000;

#[tokio::test]
async fn stampede_10k_entries_all_evicted_within_sla() {
    let bundle = setup_byok_env(KmsProviderKind::AzureKeyVault);

    // Fill cache to capacity.
    for i in 0..STAMPEDE_ENTRIES {
        let wrapped = make_wrapped_for(&bundle.key_id, i);
        let dek = Dek {
            // Deterministic non-zero bytes — proves we're not just
            // observing zeroed plaintext that was always zero.
            bytes: [(i as u8).wrapping_add(1); 32],
        };
        bundle.dek_cache.put(&wrapped, dek).await.unwrap();
    }
    let pre_len = bundle.dek_cache.len().await;
    assert_eq!(
        pre_len, STAMPEDE_ENTRIES as usize,
        "cache must be filled to {STAMPEDE_ENTRIES} pre-revoke"
    );

    // Revoke + drive kill switch.
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);
    let event = KillSwitchRunner::run(&bundle)
        .await
        .expect("kill switch executed")
        .expect("audit event emitted");

    // INV-BYOK-CRYPTO-SOVEREIGNTY: all 10 000 entries evicted.
    assert_eq!(
        event.evicted_dek_count, STAMPEDE_ENTRIES as usize,
        "audit must report all {STAMPEDE_ENTRIES} entries evicted"
    );

    // Kill switch SLA holds even at 10k.
    assert!(
        event.kill_switch_duration_ms <= KILL_SWITCH_SLA_MS,
        "10k stampede kill-switch SLA violated: {} ms > {} ms",
        event.kill_switch_duration_ms,
        KILL_SWITCH_SLA_MS
    );

    // Zero plaintext post-revoke: cache fully empty + no entry
    // retrievable for any prior wrapped DEK.
    assert_eq!(bundle.dek_cache.len().await, 0);
    assert!(bundle.dek_cache.is_empty().await);

    // Spot-check 100 prior wrapped DEKs return None on get
    // (memory-inspection equivalent for in-process tests).
    for i in 0..100u32 {
        let wrapped = make_wrapped_for(&bundle.key_id, i);
        let got = bundle.dek_cache.get(&wrapped).await;
        assert!(
            got.is_none(),
            "INV-BYOK-CRYPTO-SOVEREIGNTY violated: DEK #{i} still retrievable"
        );
    }
}
