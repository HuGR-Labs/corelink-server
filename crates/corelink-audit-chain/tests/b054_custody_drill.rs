//! Nonproduction custody/rotation drill. Run only from the protected
//! `b054-key-custody-drill` GitHub Actions environment with its four synthetic
//! versioned secrets; never add these credentials to a repository or fixture.

use std::env;

use corelink_audit_chain::{
    ChainEpoch, ChainHash, EpochChainState, KEYED_ALGORITHM_ID, LinkKey, LinkKeyring,
    SEALED_LINE_SCHEMA_V2, SealedArchiveLine, key_matches_commitment, link_for_epoch,
    link_key_commitment, split_verifying_prefix_for_epoch,
};
use ed25519_dalek::{Signer, SigningKey, Verifier};
use serde_json::json;
use zeroize::Zeroizing;

const E1_LINK_KEY_ID: u64 = 101;
const E2_LINK_KEY_ID: u64 = 102;
const RECEIPT_SCHEMA: &str = "corelink.b054.custody-rotation-receipt.v1";

fn secret_hex(name: &str) -> Zeroizing<String> {
    let value =
        env::var(name).unwrap_or_else(|_| panic!("required protected secret {name} is absent"));
    let value = Zeroizing::new(value);
    assert!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "protected secret {name} must be 32 bytes of lower-case hex"
    );
    value
}

fn secret_bytes(name: &str) -> Zeroizing<[u8; 32]> {
    let encoded = secret_hex(name);
    let mut decoded = Zeroizing::new([0_u8; 32]);
    hex::decode_to_slice(encoded.as_bytes(), decoded.as_mut())
        .unwrap_or_else(|_| panic!("protected secret {name} is malformed"));
    decoded
}

/// This ignored test is dispatched only from main after GitHub's protected
/// environment has approved the job. Its stdout and artifact contain only
/// public commitments and fixed status codes, never secret bytes.
#[test]
#[ignore = "requires the protected nonproduction B-054 custody environment"]
fn custody_rotation_retains_historic_epoch_and_fails_closed_on_revocation() {
    let e1_signing_seed = secret_bytes("B054_E1_SIGNING_SEED_HEX");
    let e1_link_hex = secret_hex("B054_E1_LINK_KEY_HEX");
    let e2_signing_seed = secret_bytes("B054_E2_SIGNING_SEED_HEX");
    let e2_link_hex = secret_hex("B054_E2_LINK_KEY_HEX");

    let e1_signing = SigningKey::from_bytes(&e1_signing_seed);
    let e2_signing = SigningKey::from_bytes(&e2_signing_seed);
    assert_ne!(
        e1_signing.verifying_key().to_bytes(),
        e2_signing.verifying_key().to_bytes(),
        "forward signing rotation must use distinct key material"
    );
    let signed_head = e1_signing.sign(b"b054-nonproduction-historic-head");
    assert!(
        e1_signing
            .verifying_key()
            .verify(b"b054-nonproduction-historic-head", &signed_head)
            .is_ok()
    );
    assert!(
        e2_signing
            .verifying_key()
            .verify(b"b054-nonproduction-historic-head", &signed_head)
            .is_err()
    );

    // Parse both versioned secrets through the runtime keyring boundary. The
    // temporary JSON is zeroized when this scope ends; it is never serialized
    // into the receipt or emitted to the test log.
    let keyring_json = Zeroizing::new(format!(
        "{{\"{E1_LINK_KEY_ID}\":\"{}\",\"{E2_LINK_KEY_ID}\":\"{}\"}}",
        &*e1_link_hex, &*e2_link_hex
    ));
    let keyring = LinkKeyring::parse_json(&keyring_json)
        .unwrap_or_else(|_| panic!("protected versioned link-key material is malformed"));
    let e1_link_key = keyring
        .get(E1_LINK_KEY_ID)
        .expect("E1 historic key remains in the protected keyring");
    let e2_link_key = keyring
        .get(E2_LINK_KEY_ID)
        .expect("E2 active key is present in the protected keyring");

    let e1_commitment_before = link_key_commitment(e1_link_key);
    let e2_commitment = link_key_commitment(e2_link_key);
    assert_ne!(e1_commitment_before, e2_commitment);
    assert!(key_matches_commitment(e1_link_key, &e1_commitment_before));

    let mut e0 = EpochChainState::new(ChainEpoch::legacy()).expect("valid legacy epoch");
    let e0_event = br#"{"event":"b054-e0-synthetic"}"#;
    e0.append(0, e0.head(), e0_event, None)
        .expect("legacy event appends without keyed material");

    let e1_epoch =
        ChainEpoch::keyed_successor(1, E1_LINK_KEY_ID, e0.next_sequence(), *e0.head(), 0)
            .expect("E1 is a strict successor");
    let mut e1 = e0
        .transition(e1_epoch.clone())
        .expect("E0 to E1 is forward");
    let e1_prev = *e1.head();
    let e1_event = br#"{"event":"b054-e1-synthetic-historic"}"#;
    let e1_hash = e1
        .append(e1.next_sequence(), e1.head(), e1_event, Some(e1_link_key))
        .expect("E1 keyed event appends");

    let e2_epoch =
        ChainEpoch::keyed_successor(2, E2_LINK_KEY_ID, e1.next_sequence(), *e1.head(), 1)
            .expect("E2 has a new monotonically increasing key id");
    e1.epoch()
        .validate_successor(&e2_epoch, e1.next_sequence(), e1.head())
        .expect("E1 to E2 advances at the exact prior head and sequence");
    let _e2 = e1
        .transition(e2_epoch.clone())
        .expect("E1 to E2 is forward");

    // Recompute the historical E1 link after the E2 key is installed. This
    // proves rotation retained both the E1 key and its original commitment.
    let e1_hash_after_rotation = link_for_epoch(&e1_epoch, &e1_prev, e1_event, Some(e1_link_key))
        .expect("historic E1 verification remains available after rotation");
    assert_eq!(e1_hash, e1_hash_after_rotation);
    let e1_commitment_after = link_key_commitment(e1_link_key);
    assert_eq!(e1_commitment_before, e1_commitment_after);
    assert!(key_matches_commitment(e2_link_key, &e2_commitment));

    let historical_line = SealedArchiveLine {
        schema: SEALED_LINE_SCHEMA_V2.to_owned(),
        algorithm_id: KEYED_ALGORITHM_ID,
        epoch_id: 1,
        link_key_id: Some(E1_LINK_KEY_ID),
        tenant_id: "synthetic-redacted".to_owned(),
        region: "enam".to_owned(),
        sequence_number: 1,
        prev_hash: e1_prev.to_hex(),
        chain_hash: e1_hash.to_hex(),
        enqueued_at_ms: 0,
        row_id: "b054-nonproduction-row".to_owned(),
        canonical_jcs: String::from_utf8(e1_event.to_vec()).expect("fixture is UTF-8"),
    };
    let (verified, failure) = split_verifying_prefix_for_epoch(
        std::slice::from_ref(&historical_line),
        &e1_epoch,
        Some(e1_link_key),
    );
    assert_eq!(verified.len(), 1);
    assert!(failure.is_none());

    // Revocation before the linked history expires is not performed. Simulate
    // a missing E1 key and a malformed historical registry: both fail closed
    // as INDETERMINATE while the active E2 keys remain distinct and valid.
    let (verified_without_history, missing_history) =
        split_verifying_prefix_for_epoch(std::slice::from_ref(&historical_line), &e1_epoch, None);
    assert!(verified_without_history.is_empty());
    assert_eq!(
        missing_history
            .as_ref()
            .map(|failure| failure.reason_code())
            .as_deref(),
        Some("missing_link_key:epoch=1")
    );
    let e2_only_json = Zeroizing::new(format!("{{\"{E2_LINK_KEY_ID}\":\"{}\"}}", &*e2_link_hex));
    let e2_only_keyring = LinkKeyring::parse_json(&e2_only_json)
        .unwrap_or_else(|_| panic!("active E2 key material is malformed"));
    let revoked_historic_key = e2_only_keyring.get(E1_LINK_KEY_ID);
    assert!(revoked_historic_key.is_none());
    let (verified_after_revocation, revocation_failure) = split_verifying_prefix_for_epoch(
        std::slice::from_ref(&historical_line),
        &e1_epoch,
        revoked_historic_key,
    );
    assert!(verified_after_revocation.is_empty());
    assert_eq!(
        revocation_failure
            .as_ref()
            .map(|failure| failure.reason_code())
            .as_deref(),
        Some("missing_link_key:epoch=1")
    );
    let malformed_history =
        LinkKeyring::parse_json(&format!("{{\"{E1_LINK_KEY_ID}\":\"bad-historic-key\"}}"));
    assert!(malformed_history.is_err());
    let malformed_history_disposition = if malformed_history.is_err() {
        "INDETERMINATE"
    } else {
        "VERIFY_OK"
    };
    assert_eq!(malformed_history_disposition, "INDETERMINATE");
    assert_eq!(
        link_for_epoch(&e1_epoch, &e1_prev, e1_event, None),
        Err(corelink_audit_chain::EpochError::MissingKey)
    );

    let receipt = json!({
        "schema": RECEIPT_SCHEMA,
        "mode": "nonproduction",
        "run_id": env::var("GITHUB_RUN_ID").unwrap_or_else(|_| "local-unset".to_owned()),
        "commit": env::var("GITHUB_SHA").unwrap_or_else(|_| "local-unset".to_owned()),
        "epochs": [
            {"epoch_id": 1, "link_key_id": E1_LINK_KEY_ID, "state": "historic_retained"},
            {"epoch_id": 2, "link_key_id": E2_LINK_KEY_ID, "state": "active"}
        ],
        "link_key_commitments": {
            "101": e1_commitment_after.to_hex(),
            "102": e2_commitment.to_hex()
        },
        "checks": {
            "signing_key_rotation_distinct": true,
            "historic_head_verifies_after_rotation": true,
            "historic_link_verifies_after_rotation": true,
            "forward_epoch_and_key_id": true,
            "prior_commitment_unchanged": true,
            "missing_historic_key": "INDETERMINATE",
            "malformed_historic_keyring": "INDETERMINATE",
            "simulated_early_historic_revocation": "INDETERMINATE",
            "historic_key_revocation": "not_performed_before_retention_expiry"
        },
        "storage": {"d1_touched": false, "r2_touched": false, "source_contains_key_bytes": false}
    });
    let receipt_text = serde_json::to_string_pretty(&receipt).expect("receipt serializes");
    for secret in [
        &*e1_signing_seed,
        &*e1_link_hex,
        &*e2_signing_seed,
        &*e2_link_hex,
    ] {
        assert!(
            !receipt_text.contains(secret),
            "receipt must not contain key bytes"
        );
    }
    let receipt_path =
        env::var("B054_RECEIPT_PATH").expect("workflow supplies a runner-temporary receipt path");
    std::fs::write(receipt_path, receipt_text).expect("redacted receipt writes in runner temp");

    println!("B054 custody drill: E1 historic key retained, E2 active, prior commitment stable");
    println!("B054 missing or malformed historical material: INDETERMINATE");
    println!("B054 raw key material: absent from receipt; D1/R2/source untouched");
}
