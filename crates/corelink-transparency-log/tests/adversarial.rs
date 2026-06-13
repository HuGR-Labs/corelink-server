//! Adversarial tests: the fail-OPEN witness policy and response-parser
//! robustness (ADR-0066 — a Rekor outage must NEVER reach a caller's write
//! path; a malformed response must NEVER panic).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "adversarial tests may use unwrap/expect/panic"
)]

use corelink_transparency_log::{
    InMemoryRekor, RekorWitnessRecord, SignedEntry, TransparencyLogError, WitnessOutcome,
    witness_or_degrade,
};

fn entry() -> SignedEntry {
    SignedEntry::new(b"payload".to_vec(), "ed25519", vec![1, 2, 3], vec![4, 5, 6])
}

#[tokio::test]
async fn rekor_outage_never_errors_the_caller() {
    let rekor = InMemoryRekor::new();
    rekor
        .arm_fault(TransparencyLogError::Transport("connection refused".into()))
        .unwrap();
    // No panic, no Err — the only observable is a Degraded outcome.
    let out = witness_or_degrade(&rekor, &entry()).await;
    assert!(matches!(out, WitnessOutcome::Degraded(_)));
    assert!(out.record().is_none());
}

#[tokio::test]
async fn deterministic_rejection_is_decoupled_from_durability() {
    let rekor = InMemoryRekor::new();
    rekor
        .arm_fault(TransparencyLogError::Rejected("unsupported algorithm".into()))
        .unwrap();
    let out = witness_or_degrade(&rekor, &entry()).await;
    // Degraded (not witnessed) — but crucially never an Err that the write
    // path would have to handle.
    assert!(!out.is_witnessed());
}

#[test]
fn malformed_responses_never_panic() {
    let cases = [
        "",
        "{}",
        "[]",
        "null",
        "not json at all",
        r#"{"u":{}}"#,
        r#"{"u":{"logIndex":"not-a-number"}}"#,
        r#"{"u":{"logIndex":1}}"#,
        r#"{"u":{"logIndex":1,"logID":"x"}}"#,
        r#"{"u":{"logIndex":1,"logID":"x","verification":{}}}"#,
        r#"{"u":{"logIndex":1,"logID":"x","verification":{"inclusionProof":{}}}}"#,
        r#"{"u":{"logIndex":1,"logID":"x","verification":{"inclusionProof":{"logIndex":1,"treeSize":2,"rootHash":"a","hashes":"not-array","checkpoint":"c"}}}}"#,
    ];
    for body in cases {
        let result = RekorWitnessRecord::from_rekor_response_json(body, 0);
        // Every malformed case is a clean Err, never a panic.
        assert!(result.is_err(), "expected parse error for: {body}");
    }
}

#[test]
fn transient_classification_only_for_transport() {
    assert!(TransparencyLogError::Transport("x".into()).is_transient());
    assert!(!TransparencyLogError::Rejected("x".into()).is_transient());
    assert!(!TransparencyLogError::ResponseParse("x".into()).is_transient());
    assert!(!TransparencyLogError::Serialization("x".into()).is_transient());
}
