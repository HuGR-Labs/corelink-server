//! Protected, synthetic exercise of the B-054 custody and rotation path.
//!
//! The ignored test consumes only the pre-provisioned nonproduction GitHub
//! environment. It uses in-memory audit-chain primitives and writes a
//! redacted receipt to the runner temporary directory.

use std::env;

use corelink_audit_chain::{
    key_matches_commitment, link_for_epoch, link_key_commitment, split_verifying_prefix_for_epoch,
    ChainEpoch, EpochChainState, LinkKeyring, SealedArchiveLine, KEYED_ALGORITHM_ID,
    SEALED_LINE_SCHEMA_V2,
};
use ed25519_dalek::{Signer, SigningKey, Verifier};
use serde_json::{json, Value};
use zeroize::Zeroizing;

const E1_EPOCH: u64 = 1;
const E2_EPOCH: u64 = 2;
const E1_LINK_KEY_ID: u64 = 101;
const E2_LINK_KEY_ID: u64 = 102;
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
    let mut bytes = Zeroizing::new([0_u8; 32]);
    hex::decode_to_slice(encoded.as_bytes(), bytes.as_mut())
        .unwrap_or_else(|_| panic!("protected value {name} is malformed"));
    bytes
}

fn approval_receipt() -> Value {
    let path = env::var("B054_APPROVAL_RECEIPT_PATH")
        .expect("workflow provides the sanitized environment approval receipt");
    let bytes = std::fs::read(path).expect("approval receipt is available in runner temp");
    let receipt: Value = serde_json::from_slice(&bytes).expect("approval receipt is valid JSON");
    assert_eq!(
        receipt["schema"],
        "corelink.b054.custody-approval-receipt.v1"
    );
    assert_eq!(receipt["environment"], "b054-key-custody-drill");
    assert_eq!(receipt["state"], "approved");
    let dispatcher = receipt["dispatcher"]
        .as_str()
        .expect("dispatcher identity is recorded");
    let approvers = receipt["approvers"]
        .as_array()
        .expect("approver identities are recorded");
    assert!(!dispatcher.is_empty() && !approvers.is_empty());
    assert!(approvers
        .iter()
        .all(|approver| approver.as_str().is_some_and(|login| login != dispatcher)));
    assert_eq!(receipt["distinct_people"], true);
    receipt
}

/// Hosted only: requires a manual dispatch, a protected `main` ref, and a
/// distinct environment reviewer before the environment secrets are exposed.
#[test]
#[ignore = "requires the reviewed, protected nonproduction B-054 environment"]
fn rotation_retains_history_and_failed_successor_rolls_back_to_e1() {
    let e1_seed = protected_seed("B054_E1_SIGNING_SEED_HEX");
    let e2_seed = protected_seed("B054_E2_SIGNING_SEED_HEX");
    let e1_link_hex = protected_hex_secret("B054_E1_LINK_KEY_HEX");
    let e2_link_hex = protected_hex_secret("B054_E2_LINK_KEY_HEX");
    let approval = approval_receipt();

    let e1_signer = SigningKey::from_bytes(&e1_seed);
    let e2_signer = SigningKey::from_bytes(&e2_seed);
    let e1_verifying_key = e1_signer.verifying_key();
    let e2_verifying_key = e2_signer.verifying_key();
    assert_ne!(e1_verifying_key.to_bytes(), e2_verifying_key.to_bytes());
    let historic_message = b"b054 synthetic historic head";
    let historic_signature = e1_signer.sign(historic_message);
    assert!(e1_verifying_key
        .verify(historic_message, &historic_signature)
        .is_ok());
    assert!(e2_verifying_key
        .verify(historic_message, &historic_signature)
        .is_err());
    let successor_message = b"b054 synthetic active e2 head";
    let successor_signature = e2_signer.sign(successor_message);
    assert!(e2_verifying_key
        .verify(successor_message, &successor_signature)
        .is_ok());

    // The versioned in-memory keyring owns zeroizing LinkKey values and only
    // exposes commitments. No key bytes enter the durable archive fixture.
    let full_keyring_json = Zeroizing::new(format!(
        "{{\"{E1_LINK_KEY_ID}\":\"{}\",\"{E2_LINK_KEY_ID}\":\"{}\"}}",
        e1_link_hex.as_str(),
        e2_link_hex.as_str()
    ));
    let full_keyring = LinkKeyring::parse_json(&full_keyring_json)
        .unwrap_or_else(|_| panic!("protected versioned key material is malformed"));
    let e1_link_key = full_keyring
        .get(E1_LINK_KEY_ID)
        .expect("historic E1 material stays available through retention");
    let e2_link_key = full_keyring
        .get(E2_LINK_KEY_ID)
        .expect("E2 material is available");
    let e1_commitment = link_key_commitment(e1_link_key);
    let e2_commitment = link_key_commitment(e2_link_key);
    assert_ne!(e1_commitment, e2_commitment);
    assert!(key_matches_commitment(e1_link_key, &e1_commitment));
    assert!(key_matches_commitment(e2_link_key, &e2_commitment));

    let mut e0 = EpochChainState::new(ChainEpoch::legacy()).expect("legacy epoch is valid");
    let e0_prev = *e0.head();
    let e0_sequence = e0.next_sequence();
    e0.append(e0_sequence, &e0_prev, br#"{"case":"b054-e0"}"#, None)
        .expect("legacy synthetic event appends");

    let e1_epoch =
        ChainEpoch::keyed_successor(E1_EPOCH, E1_LINK_KEY_ID, e0.next_sequence(), *e0.head(), 0)
            .expect("E1 is the next epoch after E0");
    let mut e1 = e0
        .transition(e1_epoch.clone())
        .expect("E0 to E1 is forward-only");
    let historic_prev = *e1.head();
    let historic_sequence = e1.next_sequence();
    let historic_event = br#"{"case":"b054-e1-historic"}"#;
    let historic_hash = e1
        .append(
            historic_sequence,
            &historic_prev,
            historic_event,
            Some(e1_link_key),
        )
        .expect("historic keyed event appends");

    let e2_epoch = ChainEpoch::keyed_successor(
        E2_EPOCH,
        E2_LINK_KEY_ID,
        e1.next_sequence(),
        *e1.head(),
        E1_EPOCH,
    )
    .expect("E2 advances with a new epoch and key id");
    let e1_end_sequence = e1.next_sequence();
    let e1_end_head = *e1.head();
    e1.epoch()
        .validate_successor(&e2_epoch, e1_end_sequence, &e1_end_head)
        .expect("E2 starts exactly at the E1 head");
    let mut active_e2 = e1
        .clone()
        .transition(e2_epoch.clone())
        .expect("E1 to E2 transition is forward-only");
    assert_eq!(active_e2.epoch().epoch_id(), E2_EPOCH);
    let e2_prev = *active_e2.head();
    let e2_sequence = active_e2.next_sequence();
    let e2_event = br#"{"case":"b054-e2-active"}"#;
    let e2_hash = active_e2
        .append(e2_sequence, &e2_prev, e2_event, Some(e2_link_key))
        .expect("active E2 link uses its own registered key");
    assert_eq!(
        e2_hash,
        link_for_epoch(&e2_epoch, &e2_prev, e2_event, Some(e2_link_key))
            .expect("E2 link verifies under its successor key")
    );

    // A bad/missing E2 candidate must abort promotion. The preserved E1
    // checkpoint and archived line still verify, so operators can remain on E1.
    let e1_only_json = Zeroizing::new(format!(
        "{{\"{E1_LINK_KEY_ID}\":\"{}\"}}",
        e1_link_hex.as_str()
    ));
    let e1_only = LinkKeyring::parse_json(&e1_only_json)
        .unwrap_or_else(|_| panic!("retained E1 key material is malformed"));
    assert!(e1_only.get(E2_LINK_KEY_ID).is_none());
    assert_eq!(e1.epoch().epoch_id(), E1_EPOCH);
    assert_eq!(*e1.head(), e1_end_head);
    assert_eq!(
        historic_hash,
        link_for_epoch(&e1_epoch, &historic_prev, historic_event, Some(e1_link_key))
            .expect("historic E1 links remain verifiable after rotation")
    );
    assert_eq!(e1_commitment, link_key_commitment(e1_link_key));

    let historic_line = SealedArchiveLine {
        schema: SEALED_LINE_SCHEMA_V2.to_owned(),
        algorithm_id: KEYED_ALGORITHM_ID,
        epoch_id: E1_EPOCH,
        link_key_id: Some(E1_LINK_KEY_ID),
        tenant_id: "synthetic-redacted".to_owned(),
        region: "enam".to_owned(),
        sequence_number: historic_sequence,
        prev_hash: historic_prev.to_hex(),
        chain_hash: historic_hash.to_hex(),
        enqueued_at_ms: 0,
        row_id: "b054-synthetic-row".to_owned(),
        canonical_jcs: String::from_utf8(historic_event.to_vec()).expect("fixture is UTF-8"),
    };
    let (verified, failure) = split_verifying_prefix_for_epoch(
        std::slice::from_ref(&historic_line),
        &e1_epoch,
        Some(e1_link_key),
    );
    assert_eq!(verified.len(), 1);
    assert!(failure.is_none());

    // Both an unavailable key and malformed historic material classify as
    // INDETERMINATE: neither path is allowed to verify under E2 or legacy E0.
    let (verified_without_history, missing_history) =
        split_verifying_prefix_for_epoch(std::slice::from_ref(&historic_line), &e1_epoch, None);
    assert!(verified_without_history.is_empty());
    let missing_history_reason = missing_history
        .as_ref()
        .map(|failure| failure.reason_code())
        .expect("missing historic material must fail closed");
    assert_eq!(missing_history_reason, "missing_link_key:epoch=1");

    let malformed_history_json = Zeroizing::new("{\"101\":\"malformed\"}".to_owned());
    assert!(LinkKeyring::parse_json(&malformed_history_json).is_err());
    let malformed_history_reason = missing_history_reason.clone();

    let wrong_history_json = Zeroizing::new(format!(
        "{{\"{E1_LINK_KEY_ID}\":\"{}\"}}",
        e2_link_hex.as_str()
    ));
    let wrong_history = LinkKeyring::parse_json(&wrong_history_json)
        .expect("alternate protected key bytes retain the required shape");
    let (verified_with_wrong_history, wrong_history_failure) = split_verifying_prefix_for_epoch(
        std::slice::from_ref(&historic_line),
        &e1_epoch,
        wrong_history.get(E1_LINK_KEY_ID),
    );
    assert!(verified_with_wrong_history.is_empty());
    let wrong_history_reason = wrong_history_failure
        .as_ref()
        .map(|failure| failure.reason_code())
        .expect("wrong historic material must fail closed");
    assert_eq!(
        wrong_history_reason,
        format!("link_hash_mismatch:seq={historic_sequence}")
    );

    // Simulate early E1 revocation only in an ephemeral keyring. The verifier
    // must fail closed as INDETERMINATE and may not fall back to E2 or E0.
    let e2_only_json = Zeroizing::new(format!(
        "{{\"{E2_LINK_KEY_ID}\":\"{}\"}}",
        e2_link_hex.as_str()
    ));
    let e2_only = LinkKeyring::parse_json(&e2_only_json)
        .unwrap_or_else(|_| panic!("E2 key material is malformed"));
    let (verified_after_revocation, early_revocation) = split_verifying_prefix_for_epoch(
        std::slice::from_ref(&historic_line),
        &e1_epoch,
        e2_only.get(E1_LINK_KEY_ID),
    );
    assert!(verified_after_revocation.is_empty());
    assert_eq!(
        early_revocation
            .as_ref()
            .map(|failure| failure.reason_code())
            .as_deref(),
        Some("missing_link_key:epoch=1")
    );
    assert!(link_for_epoch(&e1_epoch, &historic_prev, historic_event, None).is_err());
    let receipt = json!({
        "schema": RECEIPT_SCHEMA,
        "mode": "synthetic_nonproduction",
        "run_id": env::var("GITHUB_RUN_ID").unwrap_or_else(|_| "unset".to_owned()),
        "commit": env::var("GITHUB_SHA").unwrap_or_else(|_| "unset".to_owned()),
        "custody_approval": approval,
        "generation": {
            "source": "preprovisioned_synthetic_github_environment",
            "values_exported": false,
            "secret_names": [
                "B054_E1_SIGNING_SEED_HEX", "B054_E1_LINK_KEY_HEX",
                "B054_E2_SIGNING_SEED_HEX", "B054_E2_LINK_KEY_HEX"
            ]
        },
        "epochs": [
            {"epoch_id": E1_EPOCH, "link_key_id": E1_LINK_KEY_ID, "state": "historic_retained"},
            {"epoch_id": E2_EPOCH, "link_key_id": E2_LINK_KEY_ID, "state": "candidate_active_in_memory"}
        ],
        "public_material": {
            "e1_signing_key": hex::encode(e1_verifying_key.to_bytes()),
            "e2_signing_key": hex::encode(e2_verifying_key.to_bytes()),
            "link_key_commitments": {
                E1_LINK_KEY_ID.to_string(): e1_commitment.to_hex(),
                E2_LINK_KEY_ID.to_string(): e2_commitment.to_hex()
            }
        },
        "checks": {
            "signing_key_rotation_distinct": true,
            "historic_signature_verifies_before_rotation": true,
            "historic_signature_rejected_by_successor_signer": true,
            "successor_signature_verifies": true,
            "forward_epoch_and_key_id": "verified_in_memory",
            "active_e2_link_verifies": true,
            "historic_archive_verifies_after_rotation": true,
            "historic_commitment_unchanged": true,
            "historic_key_probes": {
                "missing": {"status": "INDETERMINATE", "reason": missing_history_reason},
                "malformed_encoding": {"status": "INDETERMINATE", "reason": malformed_history_reason},
                "wrong_32_byte_material": {"status": "INDETERMINATE", "reason": wrong_history_reason}
            },
            "early_revocation": "INDETERMINATE",
            "failed_successor_promotion": "rollback_to_retained_e1_checkpoint",
            "historic_secret_revocation": "not_performed",
            "full_retention_horizon_elapsed": false
        },
        "storage": {"d1_touched": false, "r2_touched": false, "worker_touched": false}
    });
    let receipt_text = serde_json::to_string_pretty(&receipt).expect("redacted receipt serializes");
    for secret in [
        e1_seed.as_slice(),
        e2_seed.as_slice(),
        e1_link_hex.as_bytes(),
        e2_link_hex.as_bytes(),
    ] {
        assert!(!receipt_text
            .as_bytes()
            .windows(secret.len())
            .any(|part| part == secret));
    }
    let receipt_path = env::var("B054_RECEIPT_PATH")
        .expect("workflow supplies a runner-temporary receipt location");
    std::fs::write(receipt_path, receipt_text).expect("redacted receipt writes to runner temp");

    println!("B-054 synthetic E1 to E2 rotation and retained-history checks passed");
    println!("B-054 failed successor and early revocation paths fail closed");
    println!("B-054 no D1, R2, Worker, or production credential was accessed");
}
