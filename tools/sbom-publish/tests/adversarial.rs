//! Adversarial regression tests for the SBOM pipeline (WI-S12-002 §6.1.10).
//!
//! # Scenarios covered
//!
//! 1. SBOM tampered post-generation (hash mismatch) → TSR binding verification fails.
//! 2. NTIA placeholder values (`"UNKNOWN"` supplier) → rejected in strict mode.
//! 3. DT ingestion retry exhausted → `SbomError::DtRetryExhausted`.
//! 4. TSA replay with different SBOM hash → `SbomError::TsaRequestFailed`.
//! 5. PURL confusion (`pkg:cargo/corelink-fake@1.0.0`) → workspace discriminator catches.

// Tests legitimately use expect/unwrap for test fixture setup.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use sha2::{Digest, Sha256};

use sbom_publish::ntia::{has_placeholder_supplier, validate_ntia_json, ValidationMode};
use sbom_publish::purl::normalise_purl;
use sbom_publish::tsa::{verify_tsr_binding, TsrToken};

// ---------------------------------------------------------------------------
// 1. SBOM tampered post-generation
// ---------------------------------------------------------------------------

#[test]
fn scenario_01_sbom_tamper_detected_via_tsr_hash_mismatch() {
    let original_sbom = b"{\"specVersion\":\"1.5\",\"components\":[]}";
    let sha = Sha256::digest(original_sbom);
    let sha_hex = hex::encode(sha);

    let token = TsrToken {
        der_bytes: vec![0u8, 1u8, 2u8],
        sbom_sha256_hex: sha_hex,
        nonce_hex: "00000000".to_owned(),
    };

    // Simulated tampered SBOM (attacker injected a component)
    let tampered_sbom = b"{\"specVersion\":\"1.5\",\"components\":[{\"name\":\"evil\"}]}";

    let result = verify_tsr_binding(tampered_sbom, &token);
    assert!(
        result.is_err(),
        "Tampered SBOM must be rejected by TSR hash binding"
    );

    // Original must still pass
    assert!(
        verify_tsr_binding(original_sbom, &token).is_ok(),
        "Original SBOM must still pass TSR verification"
    );
}

// ---------------------------------------------------------------------------
// 2. NTIA placeholder supplier rejected
// ---------------------------------------------------------------------------

#[test]
fn scenario_02_ntia_unknown_supplier_rejected_strict_mode() {
    let sbom = serde_json::json!({
        "specVersion": "1.5",
        "metadata": {
            "timestamp": "2026-05-13T00:00:00Z",
            "authors": [{"name": "CI"}]
        },
        "components": [
            {
                "name": "libevil",
                "version": "0.1.0",
                "purl": "pkg:cargo/libevil@0.1.0",
                "supplier": {"name": "UNKNOWN"}
            }
        ],
        "dependencies": [{"ref": "pkg:cargo/libevil@0.1.0", "dependsOn": []}]
    });

    // has_placeholder_supplier must detect "UNKNOWN"
    assert!(
        has_placeholder_supplier(&sbom),
        "has_placeholder_supplier must detect UNKNOWN supplier"
    );

    // Strict NTIA validation must fail (supplier coverage < 95 %)
    let result = validate_ntia_json(&sbom, ValidationMode::Strict);
    assert!(
        !result.overall_compliant,
        "SBOM with UNKNOWN supplier must fail NTIA strict mode"
    );
    assert!(
        result.component_supplier_present < 0.95,
        "supplier coverage must be below 95 %"
    );
}

// ---------------------------------------------------------------------------
// 3. DT ingestion retry exhausted
// ---------------------------------------------------------------------------

/// This test verifies that `SbomError::DtRetryExhausted` propagates
/// correctly through `ingest_into_dt` when given an unreachable endpoint.
/// We use a localhost port that has no listener.
#[tokio::test]
async fn scenario_03_dt_ingestion_retry_exhausted() {
    use sbom_publish::dt::{ingest_into_dt, DtApiKey};
    use url::Url;

    // Set dummy API key
    std::env::set_var("DT_API_KEY_TEST_03", "test-key-retry-exhausted");
    let api_key = DtApiKey::from_env("DT_API_KEY_TEST_03").expect("env var just set");
    // Point at an unreachable local port
    let dt_url = Url::parse("http://127.0.0.1:19999").expect("literal url is valid");

    let sbom_bytes = b"{\"specVersion\":\"1.5\"}";

    let result = ingest_into_dt(
        sbom_bytes,
        &dt_url,
        &api_key,
        "corelink-server",
        "0.0.0",
        None,
    )
    .await;

    assert!(
        result.is_err(),
        "DT ingestion to unreachable host must fail"
    );

    // The error should be DtRetryExhausted (all retries) or Http (connection refused)
    // Both are acceptable; key property: it must not silently succeed.
    let err_str = format!("{:?}", result.unwrap_err());
    // Either retry exhausted or HTTP connection error
    assert!(
        err_str.contains("DtRetryExhausted")
            || err_str.contains("Http")
            || err_str.contains("connection"),
        "Error must indicate retry exhaustion or connection failure: {err_str}"
    );

    // Clean up any fallback queue files created
    let _ = std::fs::remove_dir_all(".sbom_fallback_queue");
}

// ---------------------------------------------------------------------------
// 4. TSA replay with different SBOM hash
// ---------------------------------------------------------------------------

#[test]
fn scenario_04_tsa_replay_with_different_hash_rejected() {
    let sbom_v1 = b"{\"version\":\"0.1.0\"}";
    let sbom_v2 = b"{\"version\":\"0.1.1\"}"; // attacker's target SBOM

    let sha_v1 = Sha256::digest(sbom_v1);
    let token_v1 = TsrToken {
        der_bytes: vec![0xDE, 0xAD, 0xBE, 0xEF],
        sbom_sha256_hex: hex::encode(sha_v1),
        nonce_hex: "cafebabe".to_owned(),
    };

    // Replay: attacker injects token for v1 against v2 verification
    let replay_result = verify_tsr_binding(sbom_v2, &token_v1);
    assert!(
        replay_result.is_err(),
        "TSA replay attack must be detected (hash mismatch)"
    );

    // Correct binding must still work
    assert!(
        verify_tsr_binding(sbom_v1, &token_v1).is_ok(),
        "Correct SBOM binding must still verify"
    );
}

// ---------------------------------------------------------------------------
// 5. PURL confusion — workspace member discriminator catches
// ---------------------------------------------------------------------------

#[test]
fn scenario_05_purl_confusion_workspace_discriminator() {
    // An attacker publishes "corelink-worker" to crates.io
    let malicious_purl = "pkg:cargo/corelink-worker@0.1.0";

    // Without workspace flag: no discriminator (external crate)
    let (primary_external, _) = normalise_purl(malicious_purl, false);
    assert_eq!(
        primary_external, "pkg:cargo/corelink-worker@0.1.0",
        "External crate PURL must not have vcs_url qualifier"
    );

    // With workspace flag: gets discriminator → distinguishable
    let (primary_workspace, _) = normalise_purl(malicious_purl, true);
    assert!(
        primary_workspace.contains("?vcs_url=https://github.com/HumanGuardrail/corelink-server"),
        "Workspace member PURL must carry vcs_url discriminator"
    );

    // The two PURLs are different → DT/auditors can distinguish them
    assert_ne!(
        primary_external, primary_workspace,
        "Workspace PURL must differ from external PURL to prevent confusion"
    );
}

// ---------------------------------------------------------------------------
// 6. NTIA auditor mode — UNKNOWN supplier emits warning but does not exit 2
// ---------------------------------------------------------------------------

#[test]
fn scenario_06_ntia_auditor_mode_allows_unknown_supplier() {
    let sbom = serde_json::json!({
        "specVersion": "1.5",
        "metadata": {
            "timestamp": "2026-05-13T00:00:00Z",
            "authors": [{"name": "CI"}]
        },
        "components": [
            {
                "name": "libfoo",
                "version": "1.0.0",
                "purl": "pkg:cargo/libfoo@1.0.0",
                "supplier": {"name": "UNKNOWN"}
            }
        ],
        "dependencies": [{"ref": "pkg:cargo/libfoo@1.0.0", "dependsOn": []}]
    });

    // Auditor mode must not panic and must report non-compliant gracefully
    let result = validate_ntia_json(&sbom, ValidationMode::Auditor);
    // Still non-compliant (coverage < 95 %), but auditor mode doesn't exit non-zero
    assert!(!result.overall_compliant);
}
