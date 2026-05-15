//! Property tests for `corelink-survey`.
//!
//! Two canonical invariants:
//!
//! 1. Token unforgeability — for every `(payload, key, attacker_key)`
//!    triple with `attacker_key != key`, the recorder rejects an
//!    attacker-forged token with `SurveyError::InvalidSignature`.
//!
//! 2. Replay rejection — for every `(token, response)` accepted by the
//!    recorder, a second `record` call with the same encoded token + the
//!    same response + a later `now_ms` (still within TTL) is rejected
//!    with `SurveyError::ReplayRejected`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "proptest harness — assertions panic by design"
)]

use corelink_survey::{
    InMemoryFake, RecipientHash, SigningKey, SurveyError, SurveyId, SurveyKind, SurveyLinkSigner,
    SurveyResponse, SurveyResponseRecorder, TenantId,
};
use proptest::prelude::*;
use uuid::Uuid;

fn signing_key_strategy() -> impl Strategy<Value = SigningKey> {
    prop::array::uniform32(any::<u8>()).prop_map(SigningKey::from_bytes)
}

fn nps_score_strategy() -> impl Strategy<Value = u8> {
    0u8..=10u8
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// INV: a token signed with key A cannot be verified with key B
    /// (unless A == B). The recorder rejects with InvalidSignature.
    #[test]
    fn forged_token_with_wrong_key_is_rejected(
        key_a in signing_key_strategy(),
        key_b in signing_key_strategy(),
        score in nps_score_strategy(),
        ttl in 1u64..1_000_000u64,
    ) {
        prop_assume!(key_a.as_bytes() != key_b.as_bytes());
        let signer = InMemoryFake::new(key_a.clone());
        let verifier = InMemoryFake::new(key_b);
        let tenant = TenantId::from_uuid(Uuid::nil());
        let recipient = RecipientHash::derive_with_salt("a@b.com", b"salt-v1");
        let token = signer
            .sign_invite(
                tenant,
                recipient,
                SurveyId::new("nps-w1"),
                SurveyKind::Nps,
                0,
                ttl,
            )
            .unwrap();
        let err = verifier
            .record(
                token.as_str(),
                SurveyResponse::Nps { score },
                ttl / 2,
                [0; 32],
                [0; 32],
            )
            .unwrap_err();
        prop_assert!(
            matches!(err, SurveyError::InvalidSignature),
            "expected InvalidSignature, got {:?}",
            err
        );
    }

    /// INV: a single bit flipped anywhere in the encoded token causes
    /// recorder rejection (sig or malformed).
    #[test]
    fn single_bit_flip_rejected(
        key in signing_key_strategy(),
        score in nps_score_strategy(),
        flip_byte_pos in 0usize..512usize,
        flip_bit in 0u8..8u8,
    ) {
        let f = InMemoryFake::new(key);
        let tenant = TenantId::from_uuid(Uuid::nil());
        let recipient = RecipientHash::derive_with_salt("a@b.com", b"salt-v1");
        let tok = f
            .sign_invite(
                tenant,
                recipient,
                SurveyId::new("nps-w1"),
                SurveyKind::Nps,
                0,
                1_000_000,
            )
            .unwrap();
        let bytes = tok.as_str().as_bytes();
        let pos = flip_byte_pos % bytes.len();
        let mut tampered = bytes.to_vec();
        tampered[pos] ^= 1u8 << flip_bit;
        let tampered_str = match std::str::from_utf8(&tampered) {
            Ok(s) => s.to_owned(),
            Err(_) => return Ok(()), // non-UTF-8 path — recorder rejects at decode
        };
        if tampered_str == tok.as_str() {
            // Flipping a bit of a non-ASCII byte may collapse to the
            // same string after str conversion in pathological cases.
            return Ok(());
        }
        let result = f.record(
            &tampered_str,
            SurveyResponse::Nps { score },
            1,
            [0; 32],
            [0; 32],
        );
        prop_assert!(result.is_err(), "tampered token must not record");
        let err = result.unwrap_err();
        prop_assert!(
            matches!(
                err,
                SurveyError::InvalidSignature
                    | SurveyError::MalformedToken
                    | SurveyError::MalformedRecipientHash
            ),
            "unexpected error variant: {:?}",
            err
        );
    }

    /// INV: a successful record is followed by a replay-rejected
    /// record against the same token.
    #[test]
    fn replay_is_rejected(
        key in signing_key_strategy(),
        score in nps_score_strategy(),
        ttl in 100u64..1_000_000u64,
    ) {
        let f = InMemoryFake::new(key);
        let tenant = TenantId::from_uuid(Uuid::nil());
        let recipient = RecipientHash::derive_with_salt("a@b.com", b"salt-v1");
        let tok = f
            .sign_invite(
                tenant,
                recipient,
                SurveyId::new("nps-w1"),
                SurveyKind::Nps,
                0,
                ttl,
            )
            .unwrap();
        f.record(
            tok.as_str(),
            SurveyResponse::Nps { score },
            ttl / 4,
            [0; 32],
            [0; 32],
        )
        .unwrap();
        let err = f
            .record(
                tok.as_str(),
                SurveyResponse::Nps { score },
                ttl / 2,
                [0; 32],
                [0; 32],
            )
            .unwrap_err();
        prop_assert!(
            matches!(err, SurveyError::ReplayRejected),
            "expected ReplayRejected, got {:?}",
            err
        );
        prop_assert_eq!(f.len(), 1);
    }
}
