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
        generator.contains(
            "slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml@"
        ),
        "CLI provenance must delegate to the managed SLSA L3 generator"
    );
    assert!(
        generator
            .lines()
            .any(|line| line.trim().starts_with("uses:")
                && line.contains("generator_generic_slsa3.yml@")
                && line.rsplit_once('@').is_some_and(
                    |(_, sha)| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
                )),
        "managed SLSA L3 generator must be pinned to a full commit SHA"
    );
    for required in [
        "provenance.intoto.jsonl",
        "provenance.intoto.jsonl.bundle",
        "https://token.actions.githubusercontent.com",
        "--certificate-oidc-issuer",
        "--certificate-identity-regexp",
        "HuGR-Labs/corelink-server/.github/workflows/release-cli.yml@refs/tags/cli-v",
        "base64-subjects",
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
fn cli_provenance_uses_managed_l3_generator_and_exact_release_identity() -> Result<(), String> {
    let caller = release_workflow()?;
    let generator = load_workflow("release-slsa3.yml")?;
    assert_managed_cli_provenance_contract(&caller, &generator);

    for (mutant, label) in [
        (generator.replace("generator_generic_slsa3.yml@", "generator_generic_slsa3.yml@v2"), "unpinned generator"),
        (generator.replace("HuGR-Labs/corelink-server/.github/workflows/release-cli.yml@refs/tags/cli-v", "attacker/repo/.github/workflows/release-cli.yml@refs/tags/cli-v"), "wrong caller identity"),
        (generator.replace("release-manifest.json", "wrong-subject.json"), "tampered subject"),
        (generator.replace("slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml@", "local-generator@"), "unmanaged builder"),
        (generator.replace("--certificate-identity-regexp", "--certificate-identity"), "wrong caller identity verifier"),
        (generator.replace("https://token.actions.githubusercontent.com", "https://wrong-issuer.example"), "wrong Fulcio issuer"),
    ] {
        assert!(
            std::panic::catch_unwind(|| assert_managed_cli_provenance_contract(&caller, &mutant)).is_err(),
            "mutation control accepted {label}"
        );
    }
    Ok(())
}
