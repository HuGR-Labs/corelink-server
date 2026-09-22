//! Structural regression tests for the public CLI release root.
//!
//! These tests deliberately inspect the workflow text.  The critical contract
//! spans GitHub Actions, the installer, and three independent signing lanes;
//! a Rust-only unit test cannot observe it.  Keep the mutation controls here so
//! a superficially valid workflow cannot silently repoint, skip tool validation,
//! or stop publishing the files downstream signers request.

#[path = "release_workflow_contract/assertions.rs"]
mod assertions;
#[path = "release_workflow_contract/cases.rs"]
mod cases;

use assertions::*;

fn assert_managed_cli_provenance_contract(caller: &str, generator: &str) {
    assert!(
        generator.contains("actions/attest-build-provenance@"),
        "CLI provenance must use GitHub's managed provenance action"
    );
    assert!(
        generator
            .lines()
            .any(|line| line.trim().starts_with("uses:")
                && line.contains("actions/attest-build-provenance@")
                && line.rsplit_once('@').is_some_and(
                    |(_, sha)| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
                )),
        "GitHub's provenance action must be pinned to a full commit SHA"
    );
    for required in [
        "provenance.intoto.jsonl",
        "provenance.intoto.jsonl.bundle",
        "https://token.actions.githubusercontent.com",
        "--cert-oidc-issuer",
        "--signer-workflow",
        "--source-ref",
        "--source-digest",
        "subject-checksums",
        "subject-list",
        "release-manifest.json",
    ] {
        assert!(
            generator.contains(required),
            "CLI provenance contract missing {required}"
        );
    }
    assert!(
        caller.contains("provenance.intoto.jsonl")
            && caller.contains("provenance.intoto.jsonl.bundle"),
        "release caller must publish both standard SLSA provenance assets"
    );
    assert!(
        caller.contains("scripts/verify_cli_release_inventory.py")
            && caller.contains("--provenance"),
        "release publication must verify provenance subjects against the exact uploaded inventory"
    );
}

#[test]
fn cli_provenance_uses_managed_generator_and_exact_release_identity() -> Result<(), String> {
    let caller = release_workflow()?;
    let generator = load_workflow("release-slsa3.yml")?;
    assert_managed_cli_provenance_contract(&caller, &generator);

    for (mutant, label) in [
        (generator.replace("actions/attest-build-provenance@", "actions/attest-build-provenance@v4"), "unpinned generator"),
        (generator.replace("--signer-workflow", "--owner"), "wrong caller identity verifier"),
        (generator.replace("release-manifest.json", "wrong-subject.json"), "tampered subject"),
        (generator.replace("actions/attest-build-provenance@", "local-generator@"), "unmanaged builder"),
        (generator.replace("https://token.actions.githubusercontent.com", "https://wrong-issuer.example"), "wrong Fulcio issuer"),
    ] {
        assert!(
            std::panic::catch_unwind(|| assert_managed_cli_provenance_contract(&caller, &mutant)).is_err(),
            "mutation control accepted {label}"
        );
    }
    Ok(())
}
