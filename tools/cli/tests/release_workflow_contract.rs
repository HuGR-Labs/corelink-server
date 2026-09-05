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

fn load_script(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts")
        .join(name);
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("{name} script must be readable"))
}

fn assert_release_contract(workflow: &str) {
    for required in [
        "- \"cli-v*\"",
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
        "STAGING_ASSET_ID=\"$(python3 - \"${RELEASE_API}\"",
        "gh api --method DELETE \"repos/HuGR-Labs/corelink-cli/releases/assets/${STAGING_ASSET_ID}\"",
        "STAGING_ASSET_COUNT=\"$(gh api \"repos/HuGR-Labs/corelink-cli/releases/tags/${TAG}\"",
        "release-slsa3:",
        "release-slsa3:\n    needs: [final-manifest, release-readiness]\n    if: needs.release-readiness.outputs.publish == 'true'\n    # Called workflows cannot elevate the caller's token permissions. Grant\n    # OIDC only to this provenance call; all other release jobs retain the\n    # workflow-level contents:read default.\n    permissions:\n      contents: read\n      id-token: write\n    uses: ./.github/workflows/release-slsa3.yml",
        "publish-release:",
        "Verify complete authenticated inventory before publication",
        "gh release download \"${TAG}\" --repo HuGR-Labs/corelink-cli --dir \"${PUBLISHED}\" --clobber",
        "scripts/verify_cli_release_inventory.py",
        "APPLE_NOTARIZATION_API_KEY",
        "uses: ./.github/workflows/sign-linux.yml",
        "uses: ./.github/workflows/sign-windows.yml",
        "uses: ./.github/workflows/notarize-macos.yml",
        "tag: ${{ github.ref_name }}",
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
            workflow.contains("cp \"./assets/extracted/corelink.exe\" \"./assets/corelink-windows-x86_64.exe\""),
            "a raw Windows release executable must be replaced with signed bytes"
        );
        assert!(
            workflow.contains("EXPECTED_OSSLSIGNCODE_VERSION: \"2.13\"")
                && !workflow.contains("apt-get install"),
            "Windows signing must use the pinned runner-image toolchain, not a mutable apt install"
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

fn assert_slsa_contract(workflow: &str) {
    for required in [
        "workflow_call:",
        "manifest_sha256:",
        "SOURCE_SHA: ${{ inputs.source_sha }}",
        "SUBJECT_LIST: ${{ needs.build-artifacts.outputs.subject-list }}",
        "RELEASE_TAG: ${{ inputs.release_tag }}",
        "release-manifest.json",
        "--pattern checksums.txt",
        "Bind OIDC provenance to the immutable release tag and source",
        "test \"${GITHUB_REF}\" = \"refs/tags/${TAG}\"",
        "test \"${GITHUB_SHA}\" = \"${SOURCE_SHA}\"",
        "re.escape(os.environ[\"TAG\"])",
        "@refs/tags/${ESCAPED_TAG}$",
        "contents: read # No source-repository write permission is needed for provenance.",
        "Upload provenance to the release",
        "test \"$(gh --version | awk 'NR == 1 {print $3}')\" = \"2.79.0\"",
        "--tlog-upload=true",
        "scripts/verify_cli_rekor_bundle.py",
    ] {
        assert!(
            workflow.contains(required),
            "release-slsa3 missing immutable-inventory/OIDC invariant: {required}"
        );
    }
}

fn assert_publication_inventory_contract(workflow: &str) {
    for required in [
        "Verify complete authenticated inventory before publication",
        "immediately before the irreversible draft=false",
        "gh api \"repos/HuGR-Labs/corelink-cli/releases/tags/${TAG}\" > \"${API_JSON}\"",
        "FINAL_ASSETS=\"${RUNNER_TEMP}/corelink-publish-assets-final\"",
        "gh release download \"${TAG}\" --repo HuGR-Labs/corelink-cli \\",
        "--dir \"${PUBLISHED}\" --clobber",
        "scripts/verify_cli_release_inventory.py",
        "cosign verify-blob",
        "--certificate-oidc-issuer \"https://token.actions.githubusercontent.com\"",
        "gh release edit \"${TAG}\" --repo HuGR-Labs/corelink-cli --draft=false",
    ] {
        assert!(
            workflow.contains(required),
            "publication inventory control missing: {required}"
        );
    }
}

fn assert_inventory_helper_contract(script: &str) {
    for required in [
        "staging-manifest.json",
        "set(api_names) != expected",
        "actual_names != expected",
        "duplicate name",
        "subject_digests != artifact_digests",
        "except (OSError, ValueError, json.JSONDecodeError)",
    ] {
        assert!(
            script.contains(required),
            "release inventory helper control missing: {required}"
        );
    }
}

fn assert_rekor_helper_contract(script: &str) {
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

#[test]
fn release_workflow_preserves_the_installer_and_signer_contract_and_rejects_mutations() {
    let workflow = release_workflow();
    assert_release_contract(&workflow);
    assert_publication_inventory_contract(&workflow);

    let stale_home = workflow.replace("HuGR-Labs/corelink-cli", "HumanGuardrail/corelink-cli");
    assert!(
        std::panic::catch_unwind(|| assert_release_contract(&stale_home)).is_err(),
        "the stale 404 release home must fail the structural control"
    );
    let no_readback = workflow.replace(
        "Read back published artifacts and verify release-root digests",
        "readback removed",
    );
    let public_before_signing = workflow.replace(
        "--notes-file /tmp/release-notes.md --draft",
        "--notes-file /tmp/release-notes.md",
    );
    assert!(
        std::panic::catch_unwind(|| assert_release_contract(&public_before_signing)).is_err(),
        "a release must remain draft until the chained signing/SLSA gates succeed"
    );
    let no_final_slsa = workflow.replace("release-slsa3:", "slsa removed:");
    assert!(
        std::panic::catch_unwind(|| assert_release_contract(&no_final_slsa)).is_err(),
        "SLSA must remain a terminal chained stage before publication"
    );
    let missing_caller_oidc = workflow.replace(
        "      id-token: write\n    uses: ./.github/workflows/release-slsa3.yml",
        "      OIDC permission removed\n    uses: ./.github/workflows/release-slsa3.yml",
    );
    assert!(
        std::panic::catch_unwind(|| assert_release_contract(&missing_caller_oidc)).is_err(),
        "the reusable SLSA caller must explicitly grant its OIDC permission"
    );
    let orphan_staging_asset = workflow.replace(
        "gh api --method DELETE \"repos/HuGR-Labs/corelink-cli/releases/assets/${STAGING_ASSET_ID}\"",
        "staging asset cleanup removed",
    );
    assert!(
        std::panic::catch_unwind(|| assert_release_contract(&orphan_staging_asset)).is_err(),
        "a staging manifest left on the public release must fail the structural control"
    );
    let late_reintroduced = workflow.replace(
        "Verify complete authenticated inventory before publication",
        "publication inventory check removed",
    );
    assert!(
        std::panic::catch_unwind(|| assert_release_contract(&late_reintroduced)).is_err(),
        "late staging reintroduction and unmatched assets must be rejected immediately before publication"
    );
    let api_failure_ignored = workflow.replace(
        "gh api \"repos/HuGR-Labs/corelink-cli/releases/tags/${TAG}\" > \"${API_JSON}\"",
        "gh api release inventory || true",
    );
    assert!(
        std::panic::catch_unwind(|| assert_publication_inventory_contract(&api_failure_ignored))
            .is_err(),
        "an authenticated release API failure must not be ignored"
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
        workflow.contains("sign-windows:\n    needs: [sign-linux, release]"),
        "Windows signing must wait for Linux through a release-root needs edge"
    );
    assert!(
        !windows.contains("workflow_dispatch:"),
        "privileged signer dispatch must remain disabled"
    );
    assert!(
        workflow.contains("notarize-macos:\n    needs: [sign-windows, release]"),
        "macOS notarization must wait for Windows through a release-root needs edge"
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
    assert_slsa_contract(&slsa);
    for forbidden in [
        "printf '%s\\n' \"${{ needs.build-artifacts.outputs.subject-list }}\"",
        "\"release\":\"${{ github.event.release.tag_name || inputs.release_tag }}\"",
    ] {
        assert!(
            !slsa.contains(forbidden),
            "release-slsa3 must not shell-expand dynamic release data: {forbidden}"
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
    let workflow_run_signer = load_workflow("notarize-macos.yml").replace(
        "workflow_call:",
        "workflow_run:\n    workflows: [\"sign-windows\"]",
    );
    assert!(
        std::panic::catch_unwind(|| assert_downstream_signer_contract(
            "notarize-macos.yml",
            &workflow_run_signer
        ))
        .is_err(),
        "a downstream signer must not regress from a reusable workflow to workflow_run"
    );

    let public_unsigned_windows = load_workflow("sign-windows.yml").replace(
        "cp \"./assets/extracted/corelink.exe\" \"./assets/corelink-windows-x86_64.exe\"",
        "raw Windows asset copy removed",
    );
    assert!(
        std::panic::catch_unwind(|| assert_downstream_signer_contract(
            "sign-windows.yml",
            &public_unsigned_windows
        ))
        .is_err(),
        "a public raw Windows executable must be replaced by signed bytes"
    );
    let bypassed_gatekeeper = load_workflow("notarize-macos.yml").replace(
        "spctl --assess --type execute --verbose=4 \"${BINARY}\"",
        "spctl --assess --type execute --verbose=4 \"${BINARY}\" || true",
    );
    assert!(
        std::panic::catch_unwind(|| assert_downstream_signer_contract(
            "notarize-macos.yml",
            &bypassed_gatekeeper
        ))
        .is_err(),
        "Gatekeeper assessment must not be bypassed"
    );
    let unsigned_linux_raw = load_workflow("sign-linux.yml").replace(
        "for target in \"${ASSET}\" \"${RAW_ASSET}\"; do",
        "for target in \"${ASSET}\"; do",
    );
    assert!(
        std::panic::catch_unwind(|| assert_downstream_signer_contract(
            "sign-linux.yml",
            &unsigned_linux_raw
        ))
        .is_err(),
        "a public raw Linux executable must retain its detached-signature gate"
    );
    let argv_notary_password = load_workflow("notarize-macos.yml").replace(
        "xcrun notarytool submit ./submission.zip \\",
        "xcrun notarytool submit ./submission.zip --password \"${APPLE_NOTARIZATION_PASSWORD}\" \\",
    );
    assert!(
        std::panic::catch_unwind(|| assert_downstream_signer_contract(
            "notarize-macos.yml",
            &argv_notary_password
        ))
        .is_err(),
        "notary credentials must never regress onto argv"
    );
    let unbound_slsa_oidc = slsa.replace(
        "test \"${GITHUB_REF}\" = \"refs/tags/${TAG}\"",
        "tag-bound OIDC check removed",
    );
    assert!(
        std::panic::catch_unwind(|| assert_slsa_contract(&unbound_slsa_oidc)).is_err(),
        "SLSA OIDC must remain bound to the exact tag"
    );
    let unchecked_inventory = slsa.replace("--pattern checksums.txt", "inventory checksum fetch removed");
    assert!(
        std::panic::catch_unwind(|| assert_slsa_contract(&unchecked_inventory)).is_err(),
        "SLSA must attest the exact manifest inventory, including checksums"
    );
    let writable_slsa_source = slsa.replace(
        "contents: read # No source-repository write permission is needed for provenance.",
        "contents: write # privilege regression",
    );
    assert!(
        std::panic::catch_unwind(|| assert_slsa_contract(&writable_slsa_source)).is_err(),
        "SLSA provenance must not regain source-repository contents write"
    );
    let unpinned_slsa_uploader = slsa.replace(
        "test \"$(gh --version | awk 'NR == 1 {print $3}')\" = \"2.79.0\"",
        "gh version guard removed",
    );
    assert!(
        std::panic::catch_unwind(|| assert_slsa_contract(&unpinned_slsa_uploader)).is_err(),
        "the provenance-uploading job must verify the exact gh version"
    );

    let disabled_rekor = slsa.replace("--tlog-upload=true", "COSIGN_TLOG_UPLOAD_DISABLED=1");
    assert!(
        std::panic::catch_unwind(|| assert_slsa_contract(&disabled_rekor)).is_err(),
        "a tlog-disable mutation must fail the production SLSA contract"
    );
    let inventory_helper = load_script("verify_cli_release_inventory.py");
    assert_inventory_helper_contract(&inventory_helper);
    let unmatched_asset = inventory_helper.replace("if set(api_names) != expected:", "if False:");
    assert!(
        std::panic::catch_unwind(|| assert_inventory_helper_contract(&unmatched_asset)).is_err(),
        "an unmatched public release asset must fail the closed-world helper contract"
    );
    let orphaned_staging =
        inventory_helper.replace("if \"staging-manifest.json\" in api_names:", "if False:");
    assert!(
        std::panic::catch_unwind(|| assert_inventory_helper_contract(&orphaned_staging)).is_err(),
        "a late staging-manifest reintroduction must fail the helper contract"
    );
    let rekor_helper = load_script("verify_cli_rekor_bundle.py");
    assert_rekor_helper_contract(&rekor_helper);
    let missing_inclusion = rekor_helper.replace("if not entries:", "if False:");
    assert!(
        std::panic::catch_unwind(|| assert_rekor_helper_contract(&missing_inclusion)).is_err(),
        "missing inclusion mutation must fail the production verifier contract"
    );
    let digest_unbound = rekor_helper.replace("digest.lower() == expected", "digest.lower() == True");
    assert_ne!(
        digest_unbound, rekor_helper,
        "the digest-binding mutation must change the production verifier"
    );
    assert!(
        std::panic::catch_unwind(|| assert_rekor_helper_contract(&digest_unbound)).is_err(),
        "unbound Rekor entry mutation must fail the production verifier contract"
    );
}
