//! Structural regression tests for the public CLI release root.
//!
//! These tests deliberately inspect the workflow text.  The critical contract
//! spans GitHub Actions, the installer, and three independent signing lanes;
//! a Rust-only unit test cannot observe it.  Keep the mutation controls here so
//! a superficially valid workflow cannot silently repoint, skip tool validation,
//! or stop publishing the files downstream signers request.

use std::path::PathBuf;

fn release_workflow() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".github/workflows/release-cli.yml");
    std::fs::read_to_string(path).expect("release-cli workflow must be readable")
}

fn assert_release_contract(workflow: &str) {
    for required in [
        "- \"cli-v*\"",
        "HuGR-Labs/corelink-cli",
        "EXPECTED_ZIG_VERSION=0.16.0",
        "EXPECTED_CARGO_ZIGBUILD_VERSION=0.19.8",
        "CARGO_ZIGBUILD_CACHE_DIR=${ZIGBUILD_CACHE}",
        "corelink-package-${{ matrix.target.name }}",
        "signer_name: corelink-linux-arm64",
        "signer_name: corelink-darwin-arm64",
        "${{ matrix.target.signer_name }}.tar.gz",
        "${{ matrix.target.signer_name }}.zip",
        "(cd dist && shasum -a 256 -c \"$(basename \"$CHECKSUM\")\")",
        "Read back published artifacts and verify release-root digests",
        "--pattern 'corelink-*'",
        "(cd \"$PUBLISHED\" && shasum -a 256 -c checksums.txt)",
        "CORELINK_CLI_RELEASE_TOKEN is required only to publish",
    ] {
        assert!(workflow.contains(required), "missing release invariant: {required}");
    }
    assert!(
        !workflow.contains("--repo HumanGuardrail/corelink-cli"),
        "the cross-repository release target must not regress to the 404 home"
    );
}

#[test]
fn release_workflow_preserves_the_installer_and_signer_contract_and_rejects_mutations() {
    let workflow = release_workflow();
    assert_release_contract(&workflow);

    let stale_home = workflow.replace("HuGR-Labs/corelink-cli", "HumanGuardrail/corelink-cli");
    assert!(
        std::panic::catch_unwind(|| assert_release_contract(&stale_home)).is_err(),
        "the stale 404 release home must fail the structural control"
    );
    let no_readback = workflow.replace(
        "Read back published artifacts and verify release-root digests",
        "readback removed",
    );
    assert!(
        std::panic::catch_unwind(|| assert_release_contract(&no_readback)).is_err(),
        "removing published-artifact digest verification must fail the control"
    );
}
