//! Nonproduction exercise for the protected B-054 key custody environment.
//!
//! This ignored test is run only by the main-branch protected workflow. Its
//! inputs are synthetic GitHub environment secrets; the code exercises only
//! in-memory audit-chain primitives and writes a public, redacted receipt.

use std::env;

use corelink_audit_chain::{
    key_matches_commitment, link_for_epoch, link_key_commitment, split_verifying_prefix_for_epoch,
    ChainEpoch, EpochChainState, LinkKeyring, SealedArchiveLine, KEYED_ALGORITHM_ID,
    SEALED_LINE_SCHEMA_V2,
};
use ed25519_dalek::{Signer, SigningKey, Verifier};
use serde_json::json;
use zeroize::Zeroizing;

const HISTORIC_EPOCH: u64 = 1;
const ACTIVE_EPOCH: u64 = 2;
const HISTORIC_LINK_KEY_ID: u64 = 101;
const ACTIVE_LINK_KEY_ID: u64 = 102;
const RECEIPT_SCHEMA: &str = "corelink.b054.custody-rotation-receipt.v1";

fn protected_hex_secret(name: &str) -> Zeroizing<String> {
    let value =
        env::var(name).unwrap_or_else(|_| panic!("required protected value {name} is absent"));
    let value = Zeroizing::new(value);
    assert!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "protected value {name} must be 32 bytes of lower-case hexadecimal"
    );
    value
}

fn protected_seed(name: &str) -> Zeroizing<[u8; 32]> {
    let encoded = protected_hex_secret(name);
    let mut bytes = Zeroizing::new([0; 32]);
    hex::decode_to_slice(encoded.as_bytes(), bytes.as_mut())
        .unwrap_or_else(|_| panic!("protected value {name} is malformed"));
    bytes
}

/// The hosted workflow supplies `B054_RECEIPT_PATH` in the runner temp area.
#[test]
#[ignore = "requires the reviewed, protected nonproduction B-054 environment"]
fn rotation_retains_history_and_early_revocation_fails_closed() {
    let historic_seed = protected_seed("B054_E1_SIGNING_SEED_HEX");
    let active_seed = protected_seed("B054_E2_SIGNING_SEED_HEX");
    let historic_link_hex = protected_hex_secret("B054_E1_LINK_KEY_HEX");
    let active_link_hex = protected_hex_secret("B054_E2_LINK_KEY_HEX");

    let historic_signer = SigningKey::from_bytes(&historic_seed);
    let active_signer = SigningKey::from_bytes(&active_seed);
    assert_ne!(
        historic_signer.verifying_key().to_bytes(),
        active_signer.verifying_key().to_bytes(),
        "forward rotation requires a distinct signing key"
    );
    let historic_message = b"b054 synthetic historic head";
    let historic_signature = historic_signer.sign(historic_message);
    assert!(historic_signer
        .verifying_key()
        .verify(historic_message, &historic_signature)
        .is_ok());
    assert!(active_signer
        .verifying_key()
        .verify(historic_message, &historic_signature)
        .is_err());

    // Keep both epoch-specific values in memory during the successor check.
    // LinkKeyring owns zeroizing LinkKey values and exposes commitments only.
    let keyring_json = Zeroizing::new(format!(
        "{{\"{HISTORIC_LINK_KEY_ID}\":\"{}\",\"{ACTIVE_LINK_KEY_ID}\":\"{}\"}}",
        historic_link_hex.as_str(),
        active_link_hex.as_str()
    ));
    let keyring = LinkKeyring::parse_json(&keyring_json)
        .unwrap_or_else(|_| panic!("protected versioned key material is malformed"));
    let historic_link_key = keyring
        .get(HISTORIC_LINK_KEY_ID)
        .expect("historic E1 material stays available through retention");
    let active_link_key = keyring
        .get(ACTIVE_LINK_KEY_ID)
        .expect("active E2 material is available");

    let historic_commitment = link_key_commitment(historic_link_key);
    let active_commitment = link_key_commitment(active_link_key);
    assert_ne!(historic_commitment, active_commitment);
    assert!(key_matches_commitment(
        historic_link_key,
        &historic_commitment
    ));
    assert!(key_matches_commitment(active_link_key, &active_commitment));

    let mut legacy = EpochChainState::new(ChainEpoch::legacy()).expect("legacy epoch is valid");
    legacy
        .append(0, legacy.head(), br#"{"case":"b054-e0"}"#, None)
        .expect("legacy synthetic event appends");

    let historic_epoch = ChainEpoch::keyed_successor(
        HISTORIC_EPOCH,
        HISTORIC_LINK_KEY_ID,
        legacy.next_sequence(),
        *legacy.head(),
        0,
    )
    .expect("E1 is the next epoch after E0");
    let mut historic = legacy
        .transition(historic_epoch.clone())
        .expect("E0 to E1 is forward-only");
    let historic_prev = *historic.head();
    let historic_event = br#"{"case":"b054-e1-historic"}"#;
    let historic_hash = historic
        .append(
            historic.next_sequence(),
            historic.head(),
            historic_event,
            Some(historic_link_key),
        )
        .expect("historic keyed event appends");

    let active_epoch = ChainEpoch::keyed_successor(
        ACTIVE_EPOCH,
        ACTIVE_LINK_KEY_ID,
        historic.next_sequence(),
        *historic.head(),
        HISTORIC_EPOCH,
    )
    .expect("E2 advances with a new epoch and key id");
    historic
        .epoch()
        .validate_successor(&active_epoch, historic.next_sequence(), historic.head())
        .expect("E2 starts exactly at the E1 head");
    let _active = historic
        .transition(active_epoch)
        .expect("E1 to E2 transition is forward-only");

    // Recompute E1 under the retained key after E2 is active.
    assert_eq!(
        historic_hash,
        link_for_epoch(
            &historic_epoch,
            &historic_prev,
            historic_event,
            Some(historic_link_key)
        )
        .expect("historic E1 links remain verifiable after rotation")
    );
    assert_eq!(
        historic_commitment,
        link_key_commitment(historic_link_key),
        "rotation preserves the historic commitment"
    );

    let historic_line = SealedArchiveLine {
        schema: SEALED_LINE_SCHEMA_V2.to_owned(),
        algorithm_id: KEYED_ALGORITHM_ID,
        epoch_id: HISTORIC_EPOCH,
        link_key_id: Some(HISTORIC_LINK_KEY_ID),
        tenant_id: "synthetic-redacted".to_owned(),
        region: "enam".to_owned(),
        sequence_number: 1,
        prev_hash: historic_prev.to_hex(),
        chain_hash: historic_hash.to_hex(),
        enqueued_at_ms: 0,
        row_id: "b054-synthetic-row".to_owned(),
        canonical_jcs: String::from_utf8(historic_event.to_vec()).expect("fixture is UTF-8"),
    };
    let (verified, failure) = split_verifying_prefix_for_epoch(
        std::slice::from_ref(&historic_line),
        &historic_epoch,
        Some(historic_link_key),
    );
    assert_eq!(verified.len(), 1);
    assert!(failure.is_none());

    // Simulate an early revocation in an ephemeral keyring. No protected
    // secret is changed; missing history produces INDETERMINATE, never fallback.
    let active_only_json = Zeroizing::new(format!(
        "{{\"{ACTIVE_LINK_KEY_ID}\":\"{}\"}}",
        active_link_hex.as_str()
    ));
    let active_only = LinkKeyring::parse_json(&active_only_json)
        .unwrap_or_else(|_| panic!("active E2 key material is malformed"));
    let missing_historic_key = active_only.get(HISTORIC_LINK_KEY_ID);
    let (verified_without_history, revocation_result) = split_verifying_prefix_for_epoch(
        std::slice::from_ref(&historic_line),
        &historic_epoch,
        missing_historic_key,
    );
    assert!(verified_without_history.is_empty());
    assert_eq!(
        revocation_result
            .as_ref()
            .map(|failure| failure.reason_code())
            .as_deref(),
        Some("missing_link_key:epoch=1")
    );
    assert!(link_for_epoch(&historic_epoch, &historic_prev, historic_event, None).is_err());

    let malformed_history = LinkKeyring::parse_json("{\"101\":\"malformed\"}");
    assert!(malformed_history.is_err());

    let receipt = json!({
        "schema": RECEIPT_SCHEMA,
        "mode": "nonproduction",
        "run_id": env::var("GITHUB_RUN_ID").unwrap_or_else(|_| "local-unset".to_owned()),
        "commit": env::var("GITHUB_SHA").unwrap_or_else(|_| "local-unset".to_owned()),
        "epochs": [
            {"epoch_id": HISTORIC_EPOCH, "link_key_id": HISTORIC_LINK_KEY_ID, "state": "historic_retained"},
            {"epoch_id": ACTIVE_EPOCH, "link_key_id": ACTIVE_LINK_KEY_ID, "state": "active"}
        ],
        "link_key_commitments": {
            HISTORIC_LINK_KEY_ID.to_string(): historic_commitment.to_hex(),
            ACTIVE_LINK_KEY_ID.to_string(): active_commitment.to_hex()
        },
        "checks": {
            "signing_key_rotation_distinct": true,
            "historic_head_verifies_before_rotation": true,
            "historic_head_rejected_by_active_signer": true,
            "historic_link_verifies_after_rotation": true,
            "historic_commitment_unchanged": true,
            "missing_historic_key": "INDETERMINATE",
            "malformed_historic_keyring": "INDETERMINATE",
            "simulated_early_revocation": "INDETERMINATE",
            "historic_key_revocation": "not_performed_before_retention_expiry"
        },
        "storage": {"d1_touched": false, "r2_touched": false, "worker_touched": false}
    });
    let receipt_text = serde_json::to_string_pretty(&receipt).expect("receipt serializes");
    for secret in [
        historic_seed.as_slice(),
        active_seed.as_slice(),
        historic_link_hex.as_bytes(),
        active_link_hex.as_bytes(),
    ] {
        assert!(!receipt_text
            .as_bytes()
            .windows(secret.len())
            .any(|part| part == secret));
    }
    let receipt_path = env::var("B054_RECEIPT_PATH")
        .expect("workflow supplies a runner-temporary receipt location");
    std::fs::write(receipt_path, receipt_text).expect("redacted receipt writes to runner temp");

    println!("B-054 rotation verified: historic epoch retained, successor active");
    println!("B-054 missing or malformed historic material: INDETERMINATE");
    println!("B-054 D1, R2, and Worker untouched; receipt contains public commitments only");
}
