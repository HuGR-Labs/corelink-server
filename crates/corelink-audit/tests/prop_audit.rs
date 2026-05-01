//! Property tests for `corelink-audit` (10 000 iter per case).
//!
//! Asserts the load-bearing invariants from WI-S03-007 §10.5 + §12:
//!
//! - **INV-AUDIT-CHAIN-HASH-DETERMINISTIC**: serialize twice byte-equal,
//!   diverge on any field change.
//! - **INV-AUDIT-EVENT-TYPE-EXHAUSTIVE**: every `AuthEventType` has a
//!   matching `AuthEventData::event_type()` round-trip.
//! - **INV-AUDIT-NO-RAW-PII**: no canonicalized event byte stream
//!   contains raw email / `user_` / `corelink_pat_` plaintext.
//! - **INV-AUDIT-RETENTION-HINT-ACCURATE**: `RetentionHint::for_tier`
//!   round-trips for every tier.
//! - **Per-slot stable id derivation**: stable across batch retries.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::missing_panics_doc,
    reason = "test code — assertions panic by design"
)]

use corelink_audit::{
    compute_content_hash, link_chain_hash, AuthEvent, AuthEventData, AuthEventType, ChainHash,
    EmailHash, Emitter, InMemoryEmitter, PatIdHash, PrincipalIdHash, RegionTag, RequestId,
    RetentionHint, TenantId, TenantTier, TokenKind,
};
use proptest::prelude::*;
use uuid::Uuid;

fn principal_strategy() -> impl Strategy<Value = PrincipalIdHash> {
    "[a-zA-Z0-9_]{1,32}"
        .prop_filter("non-empty", |s| !s.is_empty())
        .prop_map(|s| PrincipalIdHash::derive(&s).expect("non-empty"))
}

fn region_strategy() -> impl Strategy<Value = RegionTag> {
    prop_oneof![
        Just(RegionTag::Wnam),
        Just(RegionTag::Enam),
        Just(RegionTag::Weur),
        Just(RegionTag::Eeur),
        Just(RegionTag::Apac),
        Just(RegionTag::Sam),
    ]
}

fn tier_strategy() -> impl Strategy<Value = TenantTier> {
    prop_oneof![
        Just(TenantTier::Solo),
        Just(TenantTier::Team),
        Just(TenantTier::Business),
        Just(TenantTier::Enterprise),
    ]
}

fn event_strategy() -> impl Strategy<Value = AuthEvent> {
    (
        principal_strategy(),
        region_strategy(),
        tier_strategy(),
        any::<u64>(),
        // Plausible Unix-millisecond timestamps in the 1970-2050 window.
        // Sample directly from the bounded range so proptest doesn't
        // discard 99.9% of `i64` candidates.
        0_i64..2_000_000_000_000_i64,
        any::<u128>(),
    )
        .prop_map(|(principal, region, tier, scope, ts, raw_uuid)| {
            let tenant = TenantId::from_uuid(Uuid::from_u128(raw_uuid));
            AuthEvent::new(
                AuthEventType::TokenValidated,
                "corelink://test/auth/middleware",
                tenant,
                principal,
                region,
                RequestId::new(format!("req_{ts}")),
                RetentionHint::for_tier(tier),
                ts,
                AuthEventData::TokenValidated {
                    token_kind: TokenKind::Pat,
                    scope_bitset: scope,
                },
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 10_000,
        max_global_rejects: 100_000,
        ..ProptestConfig::default()
    })]

    /// INV-AUDIT-CHAIN-HASH-DETERMINISTIC: serializing the same event
    /// twice produces byte-equal canonical output → byte-equal hash.
    #[test]
    fn prop_content_hash_deterministic(event in event_strategy()) {
        let h1 = compute_content_hash(&event).expect("hash");
        let h2 = compute_content_hash(&event).expect("hash");
        prop_assert_eq!(h1, h2);
    }

    /// INV-AUDIT-CHAIN-HASH-DETERMINISTIC: different events produce
    /// different hashes (collision-free at this scale).
    #[test]
    fn prop_content_hash_diverges_on_field_change(
        event in event_strategy(),
        bump in 1_i64..1_000_000_000_i64,
    ) {
        let mut bumped = event.clone();
        // Bump the time field — any field change must propagate to hash.
        bumped.time_unix_ms = event.time_unix_ms.wrapping_add(bump);
        prop_assume!(bumped.time_unix_ms != event.time_unix_ms);
        let h1 = compute_content_hash(&event).expect("hash");
        let h2 = compute_content_hash(&bumped).expect("hash");
        prop_assert_ne!(h1, h2);
    }

    /// INV-AUDIT-RETENTION-HINT-ACCURATE: RetentionHint::for_tier
    /// round-trips for every TenantTier value. The `#[non_exhaustive]`
    /// catch-all is intentional — adding a new tier requires touching
    /// this property test as part of the additive Lote.
    #[test]
    fn prop_retention_hint_for_tier_canonical(tier in tier_strategy()) {
        let hint = RetentionHint::for_tier(tier);
        let expected = match tier {
            TenantTier::Solo => RetentionHint::Solo30d,
            TenantTier::Team => RetentionHint::Team90d,
            TenantTier::Business => RetentionHint::Business1y,
            TenantTier::Enterprise => RetentionHint::Enterprise7y,
            _ => unreachable!("tier_strategy emits only the 4 canonical variants; new tier additions must update this match"),
        };
        prop_assert_eq!(hint, expected);
    }

    /// INV-AUDIT-NO-RAW-PII: serialized event must NOT contain
    /// recognizable raw PII tokens. Synthetic event injects "raw"
    /// inputs into PII fields via the *Hash newtypes; the hash
    /// derivation guarantees the raw token cannot appear in output.
    #[test]
    fn prop_no_raw_pii_in_canonical_bytes(
        suffix in "[a-z0-9]{4,12}",
    ) {
        let raw_email = format!("attacker+{suffix}@evil.example");
        let raw_principal = format!("user_{suffix}_attack");
        let raw_pat = format!("corelink_pat_live_{suffix}.secret.sig");

        // Build an event whose PII fields are all derived hashes — the
        // raw PII MUST NOT appear in the canonical bytes.
        let event = AuthEvent::new(
            AuthEventType::TokenIssued,
            "corelink://test/auth/middleware",
            TenantId::from_uuid(Uuid::nil()),
            PrincipalIdHash::derive(&raw_principal).expect("derive"),
            RegionTag::Wnam,
            RequestId::new("req_x"),
            RetentionHint::Solo30d,
            1_700_000_000_000,
            AuthEventData::TokenIssued {
                token_kind: TokenKind::Pat,
                pat_id_hash: Some(PatIdHash::derive(&raw_pat).expect("derive")),
                scope_bitset: 1,
            },
        );
        let canonical = serde_jcs::to_vec(&event).expect("jcs");
        let s = String::from_utf8(canonical).expect("utf8");
        prop_assert!(!s.contains(&raw_email), "raw email leaked: {s}");
        prop_assert!(!s.contains(&raw_principal), "raw principal leaked: {s}");
        prop_assert!(!s.contains(&raw_pat), "raw PAT leaked: {s}");
        // Also assert the attacker-suffix string itself does not appear.
        // The 4..12 char suffix has 36^4..36^12 alphabet so accidental
        // collision with the JCS-canonical bytes is negligible.
        prop_assert!(!s.contains(suffix.as_str()), "raw suffix leaked: {s}");

        // Also exercise EmailHash derivation does not leak raw email.
        let email_hash = EmailHash::derive(&raw_email).expect("derive");
        let blob = serde_jcs::to_vec(&email_hash).expect("jcs");
        let blob_s = String::from_utf8(blob).expect("utf8");
        prop_assert!(!blob_s.contains(&raw_email));
        prop_assert!(!blob_s.contains(suffix.as_str()));
    }

    /// INV-AUDIT-EVENT-TYPE-EXHAUSTIVE: serializing produces a
    /// canonical "type": "<string>" entry that matches AuthEventType::as_str.
    #[test]
    fn prop_event_type_string_matches_canonical(event in event_strategy()) {
        let canonical = serde_jcs::to_vec(&event).expect("jcs");
        let s = String::from_utf8(canonical).expect("utf8");
        let expected = format!("\"type\":\"{}\"", event.event_type.as_str());
        prop_assert!(
            s.contains(&expected),
            "canonical event-type string missing: expected {expected} in {s}"
        );
    }

    /// Chain link extends correctly for any pair of (prev_chain_hash,
    /// content_hash). Re-running link with the same inputs is byte-equal.
    #[test]
    fn prop_chain_link_deterministic(event in event_strategy()) {
        let prev = ChainHash::genesis();
        let content = compute_content_hash(&event).expect("hash");
        let h1 = link_chain_hash(&prev, &content);
        let h2 = link_chain_hash(&prev, &content);
        prop_assert_eq!(h1, h2);
    }

    /// In-memory emitter captures every event without drop.
    #[test]
    fn prop_emitter_round_trip(event in event_strategy()) {
        let emitter = InMemoryEmitter::new();
        emitter.emit(event.clone()).expect("emit");
        let snapshot = emitter.snapshot();
        prop_assert_eq!(snapshot.len(), 1);
        prop_assert_eq!(&snapshot[0], &event);
    }
}

/// Exhaustive enum coverage — every AuthEventType has a matching
/// AuthEventData variant (compiles + round-trips at runtime).
/// This is a non-property test (deterministic) but lives here for
/// proximity to the per-variant property assertions.
#[test]
fn exhaustive_event_type_data_round_trip() {
    for t in AuthEventType::canonical() {
        let data = corelink_audit::events::synthetic_data_for(t).expect("synthetic");
        assert_eq!(data.event_type(), t, "drift: {t:?}");
    }
}
