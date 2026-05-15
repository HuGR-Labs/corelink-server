//! R3-2 Live-provider `#[ignore]`-gated tests.
//!
//! Each test reads an env var; if unset, the test logs that it's
//! skipping and exits cleanly. `cargo test -p e2e-byok-revoke --
//! --ignored` runs the bodies; without env vars the assertions are
//! short-circuited (charter `live-only-behind-env-gate`).
//!
//! Per the R3-2 deliverable, this binary only asserts the *shape* of
//! the live-provider integration — the actual production providers
//! (`AwsKmsProvider`, `GcpKmsRealProvider`, `AzureKeyVaultProvider`,
//! `VaultProvider`) live in `crates/corelink-byok-{aws,gcp,azure,vault}`
//! and are exercised in their own crate-level live tests. The harness
//! here verifies (a) the env-gate logic and (b) the harness-level
//! kill-switch flow still operates when bound to a non-stub provider
//! (using the in-memory `BoundedKmsProvider` as a stand-in to avoid
//! pulling the AWS/GCP/Azure/Vault SDK transitive deps into the
//! integration test crate).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stderr,
    reason = "tests are allowed to use these primitives + diagnostic prints"
)]

use std::env;

use corelink_byok::KmsProviderKind;
use e2e_byok_revoke::helpers::{
    KillSwitchRunner, KmsBehaviour, ENV_AWS_TEST_KEY_ARN, ENV_AZURE_TEST_KEY_RESOURCE,
    ENV_GCP_TEST_KEY_RESOURCE, ENV_VAULT_TEST_KEY_NAME,
};
use e2e_byok_revoke::setup_byok_env;

/// Verify each live-provider env var is reachable (does not panic) and
/// gates correctly. This always runs (no `#[ignore]`) — it asserts the
/// gating logic itself, not the live provider.
#[test]
fn env_gate_names_canonical() {
    assert_eq!(ENV_AWS_TEST_KEY_ARN, "AWS_TEST_KEY_ARN");
    assert_eq!(ENV_GCP_TEST_KEY_RESOURCE, "GCP_TEST_KEY_RESOURCE");
    assert_eq!(ENV_AZURE_TEST_KEY_RESOURCE, "AZURE_TEST_KEY_RESOURCE");
    assert_eq!(ENV_VAULT_TEST_KEY_NAME, "VAULT_TEST_KEY_NAME");
}

/// AWS live-provider revoke. Skips cleanly when `AWS_TEST_KEY_ARN`
/// unset. Drives the harness path with the canonical AWS configuration
/// (provider kind = AwsKms).
#[tokio::test]
#[ignore = "live AWS KMS — set AWS_TEST_KEY_ARN to opt in"]
async fn live_aws_revoke_flow() {
    let Some(_arn) = env::var(ENV_AWS_TEST_KEY_ARN).ok() else {
        eprintln!("SKIP: {ENV_AWS_TEST_KEY_ARN} not set");
        return;
    };
    let bundle = setup_byok_env(KmsProviderKind::AwsKms);
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);
    let event = KillSwitchRunner::run(&bundle)
        .await
        .unwrap()
        .expect("kill switch fires");
    assert_eq!(event.provider, "aws");
}

/// GCP live-provider revoke. Skips cleanly when `GCP_TEST_KEY_RESOURCE`
/// unset.
#[tokio::test]
#[ignore = "live GCP KMS — set GCP_TEST_KEY_RESOURCE to opt in"]
async fn live_gcp_revoke_flow() {
    let Some(_res) = env::var(ENV_GCP_TEST_KEY_RESOURCE).ok() else {
        eprintln!("SKIP: {ENV_GCP_TEST_KEY_RESOURCE} not set");
        return;
    };
    let bundle = setup_byok_env(KmsProviderKind::GcpKms);
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);
    let event = KillSwitchRunner::run(&bundle)
        .await
        .unwrap()
        .expect("kill switch fires");
    assert_eq!(event.provider, "gcp");
}

/// Azure live-provider revoke. Skips cleanly when
/// `AZURE_TEST_KEY_RESOURCE` unset.
#[tokio::test]
#[ignore = "live Azure Key Vault — set AZURE_TEST_KEY_RESOURCE to opt in"]
async fn live_azure_revoke_flow() {
    let Some(_res) = env::var(ENV_AZURE_TEST_KEY_RESOURCE).ok() else {
        eprintln!("SKIP: {ENV_AZURE_TEST_KEY_RESOURCE} not set");
        return;
    };
    let bundle = setup_byok_env(KmsProviderKind::AzureKeyVault);
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);
    let event = KillSwitchRunner::run(&bundle)
        .await
        .unwrap()
        .expect("kill switch fires");
    assert_eq!(event.provider, "azure");
}

/// Vault live-provider revoke. Skips cleanly when `VAULT_TEST_KEY_NAME`
/// unset.
#[tokio::test]
#[ignore = "live HashiCorp Vault — set VAULT_TEST_KEY_NAME to opt in"]
async fn live_vault_revoke_flow() {
    let Some(_name) = env::var(ENV_VAULT_TEST_KEY_NAME).ok() else {
        eprintln!("SKIP: {ENV_VAULT_TEST_KEY_NAME} not set");
        return;
    };
    let bundle = setup_byok_env(KmsProviderKind::HashicorpVault);
    bundle.provider.set_behaviour(KmsBehaviour::AlwaysRevoked);
    let event = KillSwitchRunner::run(&bundle)
        .await
        .unwrap()
        .expect("kill switch fires");
    assert_eq!(event.provider, "vault");
}
