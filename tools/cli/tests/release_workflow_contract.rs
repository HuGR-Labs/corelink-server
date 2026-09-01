//! Structural regression tests for the public CLI release root.
//!
//! These tests deliberately inspect the workflow text.  The critical contract
//! spans GitHub Actions, the installer, and three independent signing lanes;
//! a Rust-only unit test cannot observe it.  Keep the mutation controls here so
//! a superficially valid workflow cannot silently repoint, skip tool validation,
//! or stop publishing the files downstream signers request.

use std::path::PathBuf;

fn release_workflow() -> String {
    load_workflow("release-cli.yml")
}

fn load_workflow(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".github/workflows")
        .join(name);
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("{name} workflow must be readable"))
}

fn assert_release_contract(workflow: &str) {
    for required in [
        "- \"cli-v*\"",
        "HuGR-Labs/corelink-cli",
        "EXPECTED_ZIG_VERSION=0.16.0",
        "EXPECTED_CARGO_ZIGBUILD_VERSION=0.19.8",
        "CARGO_ZIGBUILD_CACHE_DIR=${ZIGBUILD_CACHE}",
        "corelink-package-${{ matrix.target.name }}",
        "zip -q -X \"$ARCHIVE_PATH\" corelink.exe",
        "ARCHIVE_MEMBERS=\"$(unzip -Z1 \"$ARCHIVE_PATH\")\"",
        "Windows signer archive must contain exactly corelink.exe",
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
        assert!(
            workflow.contains(required),
            "missing release invariant: {required}"
        );
    }
    assert!(
        !workflow.contains("--repo HumanGuardrail/corelink-cli"),
        "the cross-repository release target must not regress to the 404 home"
    );
    assert!(
        !workflow.contains("ditto -c -k --sequesterRsrc --keepParent"),
        "the Windows archive must not nest corelink.exe below a ditto parent directory"
    );
}

fn assert_downstream_signer_contract(name: &str, workflow: &str) {
    for required in [
        "CORELINK_CLI_RELEASE_TOKEN",
        "RELEASE_REPOSITORY: HuGR-Labs/corelink-cli",
        "--repo \"${RELEASE_REPOSITORY}\"",
    ] {
        assert!(
            workflow.contains(required),
            "{name} is missing cross-repository signer invariant: {required}"
        );
    }
    assert!(
        !workflow.contains("GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}"),
        "{name} must not use the source-repository GITHUB_TOKEN for a CLI release asset"
    );
}

fn assert_checksum_refresh_contract(name: &str, workflow: &str) {
    for required in [
        "--pattern 'corelink-*.sha256'",
        "LC_ALL=C sort corelink-*.sha256 > checksums.txt",
        "shasum -a 256 \"${ASSET}\" > \"${ASSET}.sha256\"",
        "./assets/checksums.txt",
        "Read back signer-updated release checksums",
        "shasum -a 256 -c checksums.txt",
    ] {
        assert!(
            workflow.contains(required),
            "{name} is missing signed-asset checksum refresh invariant: {required}"
        );
    }
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

    for name in ["sign-linux.yml", "sign-windows.yml", "notarize-macos.yml"] {
        let signer = load_workflow(name);
        assert_downstream_signer_contract(name, &signer);
        assert_checksum_refresh_contract(name, &signer);
    }

    let windows = load_workflow("sign-windows.yml");
    assert!(
        windows.contains("workflows: [\"sign-linux\"]"),
        "Windows signing must wait for Linux checksum publication"
    );
    assert!(
        windows.contains("cli-v0.1.0"),
        "signer dispatch documentation must preserve cli-v tag semantics"
    );
    let macos = load_workflow("notarize-macos.yml");
    assert!(
        macos.contains("workflows: [\"sign-windows\"]"),
        "macOS notarization must wait for Windows checksum publication"
    );
    let linux = load_workflow("sign-linux.yml");
    assert!(
        linux.contains("Reject a failed release root")
            && linux.contains("refusing to make a signer skip look successful"),
        "Linux signing must not proceed after a failed release root"
    );

    let slsa = load_workflow("release-slsa3.yml");
    for required in [
        "CORELINK_CLI_RELEASE_TOKEN",
        "RELEASE_REPOSITORY: HuGR-Labs/corelink-cli",
        "--repo \"${RELEASE_REPOSITORY}\"",
    ] {
        assert!(
            slsa.contains(required),
            "release-slsa3 missing release-root invariant: {required}"
        );
    }

    let stale_signer_home = load_workflow("sign-windows.yml").replace(
        "RELEASE_REPOSITORY: HuGR-Labs/corelink-cli",
        "RELEASE_REPOSITORY: HumanGuardrail/corelink-cli",
    );
    assert!(
        std::panic::catch_unwind(|| assert_downstream_signer_contract(
            "sign-windows.yml",
            &stale_signer_home
        ))
        .is_err(),
        "a signer targeting the stale release home must fail the structural control"
    );
    let unrefreshed_checksum = load_workflow("notarize-macos.yml").replace(
        "LC_ALL=C sort corelink-*.sha256 > checksums.txt",
        "checksum refresh removed",
    );
    assert!(
        std::panic::catch_unwind(|| assert_checksum_refresh_contract(
            "notarize-macos.yml",
            &unrefreshed_checksum
        ))
        .is_err(),
        "a signed archive without aggregate checksum refresh must fail the structural control"
    );
    let nested_windows_archive = workflow.replace(
        "zip -q -X \"$ARCHIVE_PATH\" corelink.exe",
        "ditto -c -k --sequesterRsrc --keepParent corelink.exe archive.zip",
    );
    assert!(
        std::panic::catch_unwind(|| assert_release_contract(&nested_windows_archive)).is_err(),
        "a nested ditto Windows archive must fail the structural control"
    );
}
