//! Targeted regression tests that close mutation-testing surface
//! coverage gaps identified by `cargo mutants -p corelink-byok` on
//! 2026-05-14.
//!
//! These tests are NOT redundant with existing unit / property tests:
//! each test is the minimum case required to kill a specific mutation
//! that the existing suite missed (or would miss with a more
//! aggressive AST visitor).
//!
//! See `specs/_audits/2026-05-14-mutation-baseline.md` for the full
//! mutant-by-mutant classification.

#![forbid(unsafe_code)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]

use corelink_byok::{
    dek_cache::DekCache,
    types::{Dek, KmsKeyId, KmsProviderKind, WrappedDek},
};

fn make_wrapped(arn: &str) -> WrappedDek {
    WrappedDek {
        provider: KmsProviderKind::AwsKms,
        key_id: KmsKeyId {
            provider: KmsProviderKind::AwsKms,
            key_arn_or_id: arn.to_string(),
            region: "us-east-1".to_string(),
        },
        ciphertext: vec![0xAB; 64],
        encryption_context: None,
    }
}

// =====================================================================
// types.rs surface — KmsProviderKind::as_str + KmsKeyId::as_str/Display
// (kills mutations that return "" or "xyzzy" or Default::default()).
// =====================================================================

#[test]
fn kms_provider_kind_as_str_canonical_strings_stable() {
    // Each variant has a canonical string and they MUST be distinct.
    assert_eq!(KmsProviderKind::AwsKms.as_str(), "aws");
    assert_eq!(KmsProviderKind::GcpKms.as_str(), "gcp");
    assert_eq!(KmsProviderKind::AzureKeyVault.as_str(), "azure");
    assert_eq!(KmsProviderKind::HashicorpVault.as_str(), "vault");
    // Surface stability: no two variants share a string.
    let all = [
        KmsProviderKind::AwsKms.as_str(),
        KmsProviderKind::GcpKms.as_str(),
        KmsProviderKind::AzureKeyVault.as_str(),
        KmsProviderKind::HashicorpVault.as_str(),
    ];
    let mut sorted: Vec<&str> = all.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 4, "as_str must produce 4 distinct labels");
    // None of them are empty or the canary "xyzzy".
    for s in all {
        assert!(!s.is_empty(), "provider kind as_str returned empty string");
        assert_ne!(s, "xyzzy", "provider kind as_str returned canary");
    }
}

#[test]
fn kms_key_id_as_str_returns_arn_field_verbatim() {
    let arn = "arn:aws:kms:us-east-1:000000000000:key/mut-test";
    let k = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: arn.to_string(),
        region: "us-east-1".to_string(),
    };
    assert_eq!(k.as_str(), arn);
    assert!(!k.as_str().is_empty());
    assert_ne!(k.as_str(), "xyzzy");
}

#[test]
fn kms_key_id_display_writes_arn() {
    let arn = "vault://kv/data/cmk-42";
    let k = KmsKeyId {
        provider: KmsProviderKind::HashicorpVault,
        key_arn_or_id: arn.to_string(),
        region: "eu-central-1".to_string(),
    };
    let rendered = format!("{}", k);
    assert_eq!(rendered, arn);
    assert!(!rendered.is_empty());
}

#[test]
fn dek_debug_format_redacts_bytes_but_is_non_empty() {
    // Custom Debug must not be Default::default() (empty string).
    let dek = Dek { bytes: [0xCD; 32] };
    let rendered = format!("{:?}", dek);
    assert!(rendered.contains("Dek"), "Debug must contain type name");
    assert!(
        rendered.contains("REDACTED"),
        "Debug must redact key bytes; got {}",
        rendered
    );
    assert!(!rendered.contains("cd"), "Debug must NOT leak key bytes");
    assert!(!rendered.is_empty());
}

#[test]
fn dek_generate_produces_non_zero_random_bytes() {
    // Kills mutation: Dek::generate -> Ok(Default::default()) (all zeros).
    let dek = Dek::generate().expect("DEK generation");
    assert_ne!(
        dek.bytes,
        [0u8; 32],
        "Dek::generate must use getrandom, not Default::default()"
    );
    // Two independent generations must differ (CSPRNG, not Default).
    let dek2 = Dek::generate().expect("DEK generation 2");
    assert_ne!(
        dek.bytes, dek2.bytes,
        "two Dek::generate calls must produce different bytes"
    );
}

// =====================================================================
// dek_cache.rs — TTL expiry, len, is_empty, evict counts, put arithmetic
// =====================================================================

#[tokio::test]
async fn dek_cache_ttl_zero_expires_immediately() {
    // Kills mutations on match-guard `entry.expires_at > Instant::now()`:
    //   - replaced with `true` (always cache-hit) → this test sees a None
    //     and would pass; the mutant returns Some(dek) → assertion fires.
    //   - replaced with `false` (always cache-miss) → also catches the
    //     put-then-get assertion below.
    //   - replaced operator `==` / `<` → still fails because TTL=0 means
    //     `expires_at` == now()-ish so `>` is false → None; mutant `==`
    //     may produce different behaviour.
    let cache = DekCache::new(0).expect("TTL=0 is permitted");
    let wrapped = make_wrapped("arn:aws:kms:us-east-1:000:key/ttl0");
    let dek = Dek { bytes: [7u8; 32] };
    cache.put(&wrapped, dek).await.expect("put");
    // Sleep 1ms to ensure `Instant::now()` has advanced past the
    // (Instant::now() + 0s) recorded at put-time.
    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    let fetched = cache.get(&wrapped).await;
    assert!(
        fetched.is_none(),
        "TTL=0 + 1ms wait must produce cache-miss; got {:?}",
        fetched.is_some()
    );
}

#[tokio::test]
async fn dek_cache_put_then_get_within_ttl_returns_dek() {
    // Kills mutation: DekCache::get -> None (always-miss). The TTL is
    // 300s which is far longer than the test runtime so the cache MUST
    // hit. Also kills DekCache::put -> Ok(()) which would skip storage
    // (this test asserts the value comes back).
    let cache = DekCache::new(300).expect("TTL=300 ok");
    let wrapped = make_wrapped("arn:aws:kms:us-east-1:000:key/within-ttl");
    let bytes = [0x42u8; 32];
    cache.put(&wrapped, Dek { bytes }).await.expect("put");
    let fetched = cache.get(&wrapped).await.expect("must hit cache");
    assert_eq!(fetched.bytes, bytes);
}

#[tokio::test]
async fn dek_cache_put_arithmetic_uses_addition_not_subtraction() {
    // Kills mutations on `Instant::now() + self.ttl`:
    //   - replaced with `*` or `-`. With TTL=300s, `-` yields a past
    //     timestamp → match guard fails → cache-miss; this test asserts
    //     a cache HIT for a freshly-put entry with a 300s TTL, so the
    //     `-` mutant produces None and the test fails.
    let cache = DekCache::new(300).expect("ttl ok");
    let wrapped = make_wrapped("arn:aws:kms:us-east-1:000:key/arith");
    cache
        .put(&wrapped, Dek { bytes: [9u8; 32] })
        .await
        .expect("put");
    let fetched = cache.get(&wrapped).await;
    assert!(
        fetched.is_some(),
        "300s TTL freshly put must hit; arithmetic mutation regression"
    );
}

#[tokio::test]
async fn dek_cache_len_returns_count_for_known_entry_counts() {
    // Kills mutations: DekCache::len -> 0 / 1 (constant).
    let cache = DekCache::new(300).expect("ttl ok");
    assert_eq!(cache.len().await, 0);
    cache
        .put(&make_wrapped("arn:k1"), Dek { bytes: [1u8; 32] })
        .await
        .expect("put1");
    assert_eq!(cache.len().await, 1);
    cache
        .put(&make_wrapped("arn:k2"), Dek { bytes: [2u8; 32] })
        .await
        .expect("put2");
    cache
        .put(&make_wrapped("arn:k3"), Dek { bytes: [3u8; 32] })
        .await
        .expect("put3");
    // 3 distinct ARNs → 3 entries (kills `len -> 0` and `len -> 1`).
    assert_eq!(cache.len().await, 3);
}

#[tokio::test]
async fn dek_cache_is_empty_tracks_true_and_false() {
    // Kills mutations: DekCache::is_empty -> true / false (constant).
    let cache = DekCache::new(300).expect("ttl ok");
    assert!(cache.is_empty().await, "freshly constructed cache empty");
    cache
        .put(&make_wrapped("arn:e1"), Dek { bytes: [1u8; 32] })
        .await
        .expect("put");
    assert!(
        !cache.is_empty().await,
        "post-put cache must report non-empty"
    );
}

#[tokio::test]
async fn dek_cache_evict_returns_exact_match_count_two_entries() {
    // Kills mutations:
    //   - evict_all_for_key -> Ok(0) / Ok(1) (constant). Two matching
    //     entries here ⇒ Ok(2) is the only correct answer.
    //   - filter `k.key_arn == key_id.key_arn_or_id` replaced with `!=`.
    //     Three entries: 2 matching ARN A, 1 with ARN B. `!=` would
    //     return Ok(1) instead of Ok(2); test asserts == 2.
    let cache = DekCache::new(300).expect("ttl ok");
    let arn_a = "arn:aws:kms:us-east-1:000:key/A";
    let arn_b = "arn:aws:kms:us-east-1:000:key/B";
    // Two entries against arn_a — same key but different ciphertexts
    // so they live as distinct cache keys.
    let mut w_a1 = make_wrapped(arn_a);
    w_a1.ciphertext = vec![0x10; 32];
    let mut w_a2 = make_wrapped(arn_a);
    w_a2.ciphertext = vec![0x20; 32];
    let w_b = make_wrapped(arn_b);
    cache
        .put(&w_a1, Dek { bytes: [1u8; 32] })
        .await
        .expect("put a1");
    cache
        .put(&w_a2, Dek { bytes: [2u8; 32] })
        .await
        .expect("put a2");
    cache
        .put(&w_b, Dek { bytes: [3u8; 32] })
        .await
        .expect("put b");
    assert_eq!(cache.len().await, 3);
    let key_id_a = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: arn_a.to_string(),
        region: "us-east-1".to_string(),
    };
    let evicted = cache
        .evict_all_for_key(&key_id_a)
        .await
        .expect("evict");
    assert_eq!(evicted, 2, "exactly the two arn_a entries must be evicted");
    assert_eq!(cache.len().await, 1, "arn_b entry must remain");
    // The remaining entry must still be reachable.
    assert!(cache.get(&w_b).await.is_some(), "arn_b not evicted");
}
