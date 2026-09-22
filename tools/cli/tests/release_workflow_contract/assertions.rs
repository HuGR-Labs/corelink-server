//! Structural regression tests for the public CLI release root.
//!
//! These tests deliberately inspect the workflow text.  The critical contract
//! spans GitHub Actions, the installer, and three independent signing lanes;
//! a Rust-only unit test cannot observe it.  Keep the mutation controls here so
//! a superficially valid workflow cannot silently repoint, skip tool validation,
//! or stop publishing the files downstream signers request.

use std::path::PathBuf;

pub(super) fn release_workflow() -> Result<String, String> {
    load_workflow("release-cli.yml")
}

pub(super) fn load_workflow(name: &str) -> Result<String, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".github/workflows")
        .join(name);
    std::fs::read_to_string(path).map_err(|_| format!("{name} workflow must be readable"))
}

pub(super) fn load_script(name: &str) -> Result<String, String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts")
        .join(name);
    std::fs::read_to_string(path).map_err(|_| format!("{name} script must be readable"))
}

pub(super) fn assert_release_contract(workflow: &str) {
    for required in [
        "description: \"Existing cli-vMAJOR.MINOR.PATCH tag to build and publish\"",
        "HuGR-Labs/corelink-cli",
        "EXPECTED_ZIG_VERSION=0.16.0",
        "EXPECTED_CARGO_ZIGBUILD_VERSION=0.19.8",
        "CARGO_ZIGBUILD_CACHE_DIR=${ZIGBUILD_CACHE}",
        "corelink-package-${TARGET_NAME}",
        "Validate matrix target values before shell use",
        "zip -q -X \"$ARCHIVE_PATH\" corelink.exe",
        "ARCHIVE_MEMBERS=\"$(unzip -Z1 \"$ARCHIVE_PATH\")\"",
        "Windows signer archive must contain exactly corelink.exe",
        "signer_name: corelink-linux-arm64",
        "signer_name: corelink-darwin-arm64",
        "${TARGET_SIGNER_NAME}.tar.gz",
        "${TARGET_SIGNER_NAME}.zip",
        "(cd dist && shasum -a 256 -c \"$(basename \"$CHECKSUM\")\")",
        "Read back published artifacts and verify release-root digests",
        "--pattern 'corelink-*'",
        "(cd \"$PUBLISHED\" && shasum -a 256 -c checksums.txt)",
        "CORELINK_CLI_RELEASE_TOKEN is required only to publish",
        "--notes-file /tmp/release-notes.md --draft",
        "Bind staged artifacts to the immutable source manifest",
        "staging-manifest.json",
        "final-manifest:",
        "RELEASE_API=\"${RUNNER_TEMP}/corelink-final-manifest-release.json\"",
        "case \"${STAGING_ASSET_COUNT}\" in",
        "re-derive the complete",
        "--pattern 'corelink-*' --pattern checksums.txt --pattern release-manifest.json",
        "cli_release_manifest.py verify --directory final-assets",
        "STAGING_ASSET_ID=\"$(python3 - \"${RELEASE_API}\"",
        "gh api --method DELETE \"repos/HuGR-Labs/corelink-cli/releases/assets/${STAGING_ASSET_ID}\"",
        "STAGING_ASSET_COUNT=\"$(gh api \"repos/HuGR-Labs/corelink-cli/releases/tags/${TAG}\"",
        "release-slsa3:",
        "release-slsa3:\n    needs: [final-manifest, release-readiness, release]\n    # Called workflows cannot elevate the caller's token permissions. Grant\n    # OIDC only to this provenance call; all other release jobs retain the\n    # workflow-level contents:read default.\n    permissions:\n      attestations: write\n      contents: read\n      id-token: write\n    uses: ./.github/workflows/release-slsa3.yml\n    with:\n      release_tag: ${{ inputs.release_tag }}\n      source_sha: ${{ needs.release.outputs.source_sha }}\n      manifest_sha256: ${{ needs.final-manifest.outputs.sha256 }}",
        "publish-release:\n    name: publish verified signed release\n    needs: [release-slsa3, release-readiness, final-manifest, release]",
        "Verify complete authenticated inventory before publication",
        "gh release download \"${TAG}\" --repo HuGR-Labs/corelink-cli --dir \"${PUBLISHED}\" --clobber",
        "scripts/verify_cli_release_inventory.py",
        "APPLE_NOTARIZATION_API_KEY",
        "uses: ./.github/workflows/sign-linux.yml",
        "uses: ./.github/workflows/sign-windows.yml",
        "uses: ./.github/workflows/notarize-macos.yml",
        "tag: ${{ inputs.release_tag }}",
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
        !workflow.contains("Swatinem/rust-cache@"),
        "a release publisher must not consume a mutable build cache"
    );
    assert!(
        !workflow.contains("ditto -c -k --sequesterRsrc --keepParent"),
        "the Windows archive must not nest corelink.exe below a ditto parent directory"
    );
}

pub(super) fn assert_downstream_signer_contract(name: &str, workflow: &str) {
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
    for required in [
        "workflow_call:",
        "INPUT_TAG: ${{ inputs.tag }}",
        "source_sha:",
        "staging_manifest_sha256:",
        "INPUT_SOURCE_SHA: ${{ inputs.source_sha }}",
        "STAGING_MANIFEST_SHA256: ${{ inputs.staging_manifest_sha256 }}",
        "shasum -a 256 ./assets/staging-manifest.json",
        "git rev-list -n 1 \"${INPUT_TAG}\"",
        "staging-manifest.json",
        "scripts/cli_release_manifest.py verify",
        "RAW_ASSET:",
        "case \"${INPUT_TAG}\" in cli-v*) ;; *)",
        "git show-ref --verify --quiet \"refs/tags/${INPUT_TAG}\"",
        "fetch-tags: true",
        "persist-credentials: false",
    ] {
        assert!(
            workflow.contains(required),
            "{name} is missing reusable-signer tag invariant: {required}"
        );
    }
    assert!(
        !workflow.contains("workflow_run:"),
        "{name} must not use workflow_run for a privileged release signer"
    );
    assert!(
        !workflow.contains("workflow_dispatch:"),
        "{name} must not expose a privileged manual signing entrypoint"
    );
    assert!(
        !workflow.contains("--password \"${APPLE_NOTARIZATION_PASSWORD}\""),
        "{name} must not expose an Apple notarization password on argv"
    );
    if name == "sign-windows.yml" {
        assert!(
            workflow.contains(
                "Copy-Item ./assets/extracted/corelink.exe ./assets/corelink-windows-x86_64.exe -Force"
            ),
            "a raw Windows release executable must be replaced with signed bytes"
        );
        assert!(
            workflow.contains("runs-on: windows-2022")
                && workflow.contains("signtool.exe")
                && workflow.contains("Import-PfxCertificate")
                && workflow.contains("/sha1 $cert.Thumbprint"),
            "Windows signing must use the hosted signtool certificate store without a password argv"
        );
    }
    if name == "sign-linux.yml" {
        assert!(
            workflow.contains("for target in \"${ASSET}\" \"${RAW_ASSET}\"; do"),
            "the raw Linux executable and archive must both receive detached signatures"
        );
    }
    if name == "notarize-macos.yml" {
        assert!(
            workflow.contains("spctl --assess --type execute --verbose=4 \"${BINARY}\""),
            "macOS Gatekeeper assessment is mandatory"
        );
        assert!(
            !workflow.contains("spctl --assess --type execute --verbose=4 \"${BINARY}\" || true"),
            "macOS Gatekeeper assessment must not be bypassed"
        );
        assert!(
            workflow.contains("cp \"${BINARY}\" \"./assets/${RAW_ASSET}\""),
            "the raw macOS release executable must be replaced after stapling"
        );
    }
    assert!(
        !workflow.contains("${GITHUB_ENV}"),
        "{name} must not propagate signer state through GITHUB_ENV"
    );
    assert!(
        !workflow.contains("workflow_run:"),
        "{name} must not use workflow_run for a privileged release signer"
    );
}

pub(super) fn assert_checksum_refresh_contract(name: &str, workflow: &str) {
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

pub(super) fn assert_slsa_contract(workflow: &str) {
    for required in [
        "workflow_call:",
        "manifest_sha256:",
        "SOURCE_SHA: ${{ inputs.source_sha }}",
        "release-manifest.json",
        "--pattern checksums.txt",
        "test \"${GITHUB_REF}\" = \"refs/tags/${TAG}\"",
        "test \"${GITHUB_SHA}\" = \"${SOURCE_SHA}\"",
        "attestations: write",
        "id-token: write",
        "actions/attest-build-provenance@4d101475d8b20a2381f78447822ac1eab6504dd8",
        "subject-checksums: provenance-subjects.sha256",
        "provenance.intoto.jsonl.bundle",
        "gh attestation verify",
        "--signer-workflow",
        "--source-ref",
        "--source-digest",
        "--cert-oidc-issuer \"https://token.actions.githubusercontent.com\"",
    ] {
        assert!(
            workflow.contains(required),
            "release-slsa3 missing immutable-inventory/OIDC invariant: {required}"
        );
    }
}

pub(super) fn assert_publication_inventory_contract(workflow: &str) {
    for required in [
        "Verify complete authenticated inventory before publication",
        "immediately before the irreversible draft=false",
        "gh api \"repos/HuGR-Labs/corelink-cli/releases/tags/${TAG}\" > \"${API_JSON}\"",
        "FINAL_ASSETS=\"${RUNNER_TEMP}/corelink-publish-assets-final\"",
        "gh release download \"${TAG}\" --repo HuGR-Labs/corelink-cli \\",
        "--dir \"${PUBLISHED}\" --clobber",
        "scripts/verify_cli_release_inventory.py",
        "gh attestation verify",
        "--signer-workflow",
        "--cert-oidc-issuer \"https://token.actions.githubusercontent.com\"",
        "gh release edit \"${TAG}\" --repo HuGR-Labs/corelink-cli --draft=false",
    ] {
        assert!(
            workflow.contains(required),
            "publication inventory control missing: {required}"
        );
    }
}

pub(super) fn assert_retry_manifest_contract(workflow: &str) {
    let retry = workflow
        .split("            0)\n")
        .nth(1)
        .and_then(|rest| rest.split("            1)\n").next());
    assert!(
        retry.is_some(),
        "final-manifest retry branch must be present"
    );
    if let Some(retry) = retry {
        for required in [
            "--pattern 'corelink-*' --pattern checksums.txt --pattern release-manifest.json",
            "manifest.get(\"staging_manifest_sha256\") != sys.argv[2]",
            "manifest.get(\"tag\") != sys.argv[3]",
            "manifest.get(\"source_sha\") != sys.argv[4]",
            "cli_release_manifest.py verify --directory final-assets",
        ] {
            assert!(
                retry.contains(required),
                "retry branch missing inventory proof: {required}"
            );
        }
    }
}

pub(super) fn assert_inventory_helper_contract(script: &str) {
    for required in [
        "staging-manifest.json",
        "METADATA",
        "provenance.intoto.jsonl.bundle",
        "signed_statement",
        "dsseEnvelope",
        "subject_digests",
        "verify_checksums(directory, artifacts)",
        "CHECKSUM_LINE",
        "release inventory is not closed-world",
        "except (OSError, ValueError, json.JSONDecodeError)",
    ] {
        assert!(
            script.contains(required),
            "release inventory helper control missing: {required}"
        );
    }
}

pub(super) fn assert_rekor_helper_contract(script: &str) {
    for required in [
        "tlogEntries",
        "if not entries:",
        "inclusionProof",
        "canonicalizedBody",
        "expected = hashlib.sha256(payload_path.read_bytes()).hexdigest()",
        "_inclusion_root",
        "inclusionProof.rootHash",
        "integrated_time is None",
        "digest.lower() == expected",
    ] {
        assert!(
            script.contains(required),
            "Rekor verifier control missing: {required}"
        );
    }
}
