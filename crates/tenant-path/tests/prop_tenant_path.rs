//! WI-S01-001 acceptance + property tests for `corelink-tenant-path`.
//!
//! Coverage map (cross-referenced with WI-S01-001 §8):
//!
//! - AC-1 (determinism): [`determinism_1000_iter`].
//! - AC-2 (injectivity 10k pairs): [`prop_injectivity_distinct_tenant_ids`].
//! - AC-3 (cross-TDK separation): [`prop_cross_tdk_separation`] + [`cross_tdk_unit`].
//! - AC-4 (HMAC16 b64 url-safe encoding): [`prop_encoding_alphabet`] +
//!   [`encoding_format_unit`] + [`encoding_no_padding_or_url_reserved`].
//! - AC-5 (no-panic on adversarial input): exercised here on diverse inputs;
//!   sustained 1h fuzz target lives in `fuzz/fuzz_targets/derive_prefix.rs`.
//! - AC-6 (perf p99 < 100μs): criterion bench in `benches/derive.rs`.
//! - AC-7 (secret hygiene Debug REDACTED): [`tdk_debug_redacted`].

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::missing_docs_in_private_items,
    clippy::print_stderr,
    clippy::print_stdout,
    missing_docs,
    reason = "test code; panic on assertion failure is the contract"
)]

use std::collections::HashSet;

use corelink_tenant_path::{derive_prefix, TenantDerivationKey, TenantPrefix, TENANT_PREFIX_LEN};
use proptest::prelude::*;
use uuid::Uuid;
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const FIXTURE_TDK_BYTES: [u8; 32] = *b"fixture-tdk-32-bytes-constant!ok";
const FIXTURE_TDK_ALT_BYTES: [u8; 32] = *b"alt-tdk-32-bytes-constant-test!!";
const FIXTURE_TENANT_ID: &str = "01938af0-abcd-7123-8456-000000000001";
const FIXTURE_TENANT_ID_ALT: &str = "01938af0-abcd-7123-8456-000000000002";

fn fixture_tdk() -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new(FIXTURE_TDK_BYTES))
}

fn fixture_tdk_alt() -> TenantDerivationKey {
    TenantDerivationKey::from_bytes(Zeroizing::new(FIXTURE_TDK_ALT_BYTES))
}

fn fixture_uuid() -> Uuid {
    Uuid::parse_str(FIXTURE_TENANT_ID).expect("static UUID literal parses")
}

fn fixture_uuid_alt() -> Uuid {
    Uuid::parse_str(FIXTURE_TENANT_ID_ALT).expect("static UUID literal parses")
}

// URL-safe base64 alphabet RFC 4648 §5.
fn is_url_safe_b64_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

// ---------------------------------------------------------------------------
// 12 unit cases (WI §6.1) + AC-1 / AC-3 / AC-4 / AC-7 fixed-vector unit checks
// ---------------------------------------------------------------------------

/// AC-1: determinism over 1000 iterations.
#[test]
fn determinism_1000_iter() {
    let tdk = fixture_tdk();
    let tid = fixture_uuid();
    let baseline = derive_prefix(&tdk, tid);
    for _ in 0..1000 {
        let again = derive_prefix(&tdk, tid);
        assert_eq!(baseline, again, "derive_prefix must be deterministic");
    }
}

/// Determinism is invariant across cloned TDKs.
#[test]
fn determinism_with_cloned_tdk() {
    let tdk = fixture_tdk();
    let tdk_clone = tdk.clone();
    let tid = fixture_uuid();
    assert_eq!(derive_prefix(&tdk, tid), derive_prefix(&tdk_clone, tid));
}

/// AC-3 (unit): distinct TDKs yield distinct prefixes for the same UUID.
#[test]
fn cross_tdk_unit() {
    let p_a = derive_prefix(&fixture_tdk(), fixture_uuid());
    let p_b = derive_prefix(&fixture_tdk_alt(), fixture_uuid());
    assert_ne!(p_a, p_b, "different TDKs must produce different prefixes");
}

/// AC-4 (format): exactly 16 ASCII chars, all from URL-safe base64 alphabet.
#[test]
fn encoding_format_unit() {
    let prefix = derive_prefix(&fixture_tdk(), fixture_uuid());
    assert_eq!(prefix.as_str().len(), TENANT_PREFIX_LEN);
    assert_eq!(prefix.as_str().len(), TENANT_PREFIX_LEN);
    for &b in prefix.as_str().as_bytes() {
        assert!(
            is_url_safe_b64_char(b),
            "byte 0x{b:02x} ({}) is not in URL-safe base64 alphabet",
            b as char
        );
    }
}

/// AC-4: no padding (`=`) or URL-reserved chars (`/ ? # %`) ever appear.
#[test]
fn encoding_no_padding_or_url_reserved() {
    let prefix = derive_prefix(&fixture_tdk(), fixture_uuid());
    for &b in prefix.as_str().as_bytes() {
        assert_ne!(b, b'=', "padding char '=' is forbidden in URL-safe no-pad");
        assert_ne!(b, b'/', "'/' is not in URL-safe alphabet");
        assert_ne!(b, b'?', "'?' is not in URL-safe alphabet");
        assert_ne!(b, b'#', "'#' is not in URL-safe alphabet");
        assert_ne!(b, b'%', "'%' is not in URL-safe alphabet");
    }
}

/// `Display` and `as_str` agree.
#[test]
fn display_matches_as_str() {
    let prefix = derive_prefix(&fixture_tdk(), fixture_uuid());
    assert_eq!(format!("{prefix}"), prefix.as_str());
}

/// AC-7: `Debug` impl redacts TDK bytes.
#[test]
fn tdk_debug_redacted() {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0xAB; 32]));
    let dbg = format!("{tdk:?}");
    assert_eq!(dbg, "TenantDerivationKey(REDACTED)");
    assert!(!dbg.contains("ab"), "Debug output must not leak key bytes");
    assert!(!dbg.contains("AB"));
}

/// `Debug` impl of `TenantPrefix` shows the prefix value (not redacted —
/// prefix is non-secret and visible in R2 keys / logs by design).
#[test]
fn prefix_debug_shows_value() {
    let prefix = derive_prefix(&fixture_tdk(), fixture_uuid());
    let dbg = format!("{prefix:?}");
    assert!(dbg.starts_with("TenantPrefix("));
    assert!(dbg.contains(prefix.as_str()));
}

/// Single-byte change in TDK propagates to a different prefix (avalanche).
#[test]
fn tdk_avalanche_single_byte_flip() {
    let mut bytes = FIXTURE_TDK_BYTES;
    let tdk_a = TenantDerivationKey::from_bytes(Zeroizing::new(bytes));
    bytes[0] ^= 0x01;
    let tdk_b = TenantDerivationKey::from_bytes(Zeroizing::new(bytes));
    let p_a = derive_prefix(&tdk_a, fixture_uuid());
    let p_b = derive_prefix(&tdk_b, fixture_uuid());
    assert_ne!(p_a, p_b, "single-bit TDK flip must flip the derived prefix");
}

/// Single-bit change in tenant_id propagates (avalanche on the message side).
#[test]
fn tenant_id_avalanche_single_byte_flip() {
    let tdk = fixture_tdk();
    let mut bytes = *fixture_uuid().as_bytes();
    let tid_a = Uuid::from_bytes(bytes);
    bytes[15] ^= 0x01;
    let tid_b = Uuid::from_bytes(bytes);
    assert_ne!(derive_prefix(&tdk, tid_a), derive_prefix(&tdk, tid_b));
}

/// Adversarial input AC-5 (unit smoke): UUID nil + max are accepted (no panic).
#[test]
fn boundary_uuids_accepted() {
    let tdk = fixture_tdk();
    let _nil = derive_prefix(&tdk, Uuid::nil());
    let _max = derive_prefix(&tdk, Uuid::max());
}

/// All-zero TDK is structurally accepted (no panic). Real deployments use
/// HSM-backed random TDKs, but the type system does not exclude zero keys.
#[test]
fn zero_tdk_accepted() {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
    let _ = derive_prefix(&tdk, fixture_uuid());
}

/// All-`0xFF` TDK is structurally accepted.
#[test]
fn all_ones_tdk_accepted() {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0xFF; 32]));
    let _ = derive_prefix(&tdk, fixture_uuid());
}

/// Two different UUIDs under the same TDK collide with vanishing probability;
/// the canonical fixture pair must not collide.
#[test]
fn fixture_pair_no_collision() {
    let tdk = fixture_tdk();
    let p1 = derive_prefix(&tdk, fixture_uuid());
    let p2 = derive_prefix(&tdk, fixture_uuid_alt());
    assert_ne!(p1, p2);
}

/// `TenantPrefix` is `Copy`; aliasing across calls preserves equality.
#[test]
fn prefix_is_copy() {
    let prefix = derive_prefix(&fixture_tdk(), fixture_uuid());
    let alias: TenantPrefix = prefix;
    assert_eq!(prefix, alias);
}

// ---------------------------------------------------------------------------
// Property tests (AC-2, AC-3, AC-4, AC-5)
// ---------------------------------------------------------------------------

proptest! {
    // AC-2: distinct UUIDs ⇒ distinct prefixes (10k iter).
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        max_shrink_iters: 64,
        ..ProptestConfig::default()
    })]

    /// AC-2: with the same TDK, distinct UUIDs almost always yield distinct
    /// prefixes. We assert *strict* injectivity here: a property test failure
    /// would almost certainly indicate an algorithm bug, not a real collision
    /// (collisions on 96 bits of HMAC entropy are vanishingly rare).
    #[test]
    fn prop_injectivity_distinct_tenant_ids(
        tid_a in any::<u128>(),
        tid_b in any::<u128>(),
    ) {
        prop_assume!(tid_a != tid_b);
        let tdk = fixture_tdk();
        let p_a = derive_prefix(&tdk, Uuid::from_u128(tid_a));
        let p_b = derive_prefix(&tdk, Uuid::from_u128(tid_b));
        prop_assert_ne!(p_a, p_b);
    }

    /// AC-3: with a single tenant_id, distinct TDKs almost always yield
    /// distinct prefixes.
    #[test]
    fn prop_cross_tdk_separation(
        tdk_a in any::<[u8; 32]>(),
        tdk_b in any::<[u8; 32]>(),
        tid in any::<u128>(),
    ) {
        prop_assume!(tdk_a != tdk_b);
        let p_a = derive_prefix(&TenantDerivationKey::from_bytes(Zeroizing::new(tdk_a)), Uuid::from_u128(tid));
        let p_b = derive_prefix(&TenantDerivationKey::from_bytes(Zeroizing::new(tdk_b)), Uuid::from_u128(tid));
        prop_assert_ne!(p_a, p_b);
    }

    /// AC-4: every byte of the prefix is from the URL-safe base64 alphabet,
    /// for any TDK and any UUID.
    #[test]
    fn prop_encoding_alphabet(
        tdk_bytes in any::<[u8; 32]>(),
        tid in any::<u128>(),
    ) {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(tdk_bytes));
        let prefix = derive_prefix(&tdk, Uuid::from_u128(tid));
        prop_assert_eq!(prefix.as_str().len(), TENANT_PREFIX_LEN);
        for &b in prefix.as_str().as_bytes() {
            prop_assert!(
                is_url_safe_b64_char(b),
                "byte 0x{:02x} is not in URL-safe base64 alphabet",
                b
            );
        }
    }

    /// AC-5 (sample): adversarial inputs do not panic (proptest will shrink
    /// any panic to a minimal counterexample).
    #[test]
    fn prop_no_panic(
        tdk_bytes in any::<[u8; 32]>(),
        tid_bytes in any::<[u8; 16]>(),
    ) {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(tdk_bytes));
        let _ = derive_prefix(&tdk, Uuid::from_bytes(tid_bytes));
    }
}

// ---------------------------------------------------------------------------
// Performance regression gate (AC-6)
//
// `cargo bench` (criterion) is the nightly perf observatory (committed
// baseline ≈ 3.2 μs/call on release builds at the time this file was
// written); this test is the per-PR regression gate. AC-6 specifies the
// budget in CF Workers-like — i.e. release-mode / optimized — terms, so
// the test is `#[cfg_attr(debug_assertions, ignore)]` and only runs under
// `cargo test --release`. CI invokes that exact command (see
// `.github/workflows/tenant-path.yml`).
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "release-only perf test; run via `cargo test --release` (AC-6)"
)]
fn perf_regression_10k_under_1s() {
    use std::time::Instant;

    let tdk = fixture_tdk();
    let tid = fixture_uuid();

    // Warmup so the first iteration's branch-predictor / cache miss doesn't
    // dominate the steady-state measurement.
    for _ in 0..1000 {
        std::hint::black_box(derive_prefix(&tdk, tid));
    }

    let iters = 10_000u32;
    let start = Instant::now();
    for _ in 0..iters {
        std::hint::black_box(derive_prefix(&tdk, tid));
    }
    let elapsed = start.elapsed();

    let per_call_ns = elapsed.as_nanos() / u128::from(iters);
    eprintln!("perf_regression_10k_under_1s: {iters} calls in {elapsed:?} ({per_call_ns} ns/call)");

    // Hard ceiling: 1s for 10k iterations = 100 μs per call mean. The AC-6
    // p99 budget is 100 μs and mean is always ≤ p99, so a mean above 100 μs
    // is an unambiguous failure signal. Healthy steady-state on commodity
    // x86-64 release builds is ~3 μs/call (3% of budget).
    assert!(
        elapsed.as_secs() < 1,
        "derive_prefix mean exceeded 100 μs / call (regression vs AC-6); {iters} iters took {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// Canonical regression vectors
//
// These are cross-implementation reference values computed independently with
// Python's `hmac.new(tdk, uuid_bytes, hashlib.sha256).digest()` followed by
// `base64.urlsafe_b64encode(...).rstrip(b"=")[:16]`. Any future refactor that
// changes the algorithm — even subtly (e.g. byte order of UUID, padding rules,
// truncation point) — will fail these immediately.
//
// Generation script lives in `crates/tenant-path/README.md`.
// ---------------------------------------------------------------------------

#[test]
fn canonical_vectors() {
    let cases: &[(&[u8; 32], &str, &str)] = &[
        (
            &[0u8; 32],
            "00000000-0000-0000-0000-000000000000",
            "hTx0A5N9i2I5VpsY",
        ),
        (
            b"fixture-tdk-32-bytes-constant!ok",
            "01938af0-abcd-7123-8456-000000000001",
            "oLIKxsQkUHvAJkis",
        ),
        (
            b"alt-tdk-32-bytes-constant-test!!",
            "01938af0-abcd-7123-8456-000000000001",
            "mnF8aPEhObrs5NmD",
        ),
        (
            b"fixture-tdk-32-bytes-constant!ok",
            "01938af0-abcd-7123-8456-000000000002",
            "WQ86lbkTPyvMg8sO",
        ),
        (
            &[0xFFu8; 32],
            "ffffffff-ffff-ffff-ffff-ffffffffffff",
            "heorm-jgI3oyE89_",
        ),
    ];
    for (tdk_bytes, uuid_str, expected) in cases {
        let tdk = TenantDerivationKey::from_bytes(Zeroizing::new(**tdk_bytes));
        let tid = Uuid::parse_str(uuid_str).expect("static UUID literal parses");
        let prefix = derive_prefix(&tdk, tid);
        assert_eq!(
            prefix.as_str(),
            *expected,
            "canonical vector drift for tdk={tdk_bytes:?} uuid={uuid_str}"
        );
    }
}

// ---------------------------------------------------------------------------
// Bulk injectivity sample (cheaper than the 10k proptest case for fast CI)
// ---------------------------------------------------------------------------

/// Concrete bulk injectivity check: 1024 distinct UUIDs ⇒ 1024 distinct
/// prefixes. Acts as a sanity baseline for the property test above.
#[test]
fn bulk_injectivity_1024() {
    let tdk = fixture_tdk();
    let mut seen: HashSet<TenantPrefix> = HashSet::with_capacity(1024);
    for i in 0..1024u128 {
        let prefix = derive_prefix(&tdk, Uuid::from_u128(i));
        assert!(seen.insert(prefix), "collision at i={i}");
    }
}
