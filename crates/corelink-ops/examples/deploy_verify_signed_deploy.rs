//! Example: verify a fully signed deploy (happy path).
// Examples are CLI programs; printing to stdout/stderr is intentional.
#![allow(clippy::print_stdout, clippy::print_stderr)]
//!
//! Demonstrates the complete flow:
//! 1. Webhook HMAC authentication.
//! 2. Cosign signature + Rekor inclusion + Fulcio chain verification.
//! 3. Audit event emit (success).
//! 4. Cloudflare API propagation with pinned digest.
//!
//! In production this uses the `sigstore` Rust crate for cryptographic
//! verification and the Cloudflare REST API for propagation.  This example
//! uses the in-memory verifier to demonstrate the trait contract.

use std::sync::Arc;

use corelink_ops::deploy::{
    audit::InMemoryDeployAuditSink,
    types::{CfDeployWebhook, CosignIdentityPattern, DeployTarget, GitHubActor, OciImageRef},
    verifier::InMemoryDeployVerifier,
    DeployVerifier,
};

fn main() {
    // Set up structured logging (optional in tests; useful for demo output).
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let sink = Arc::new(InMemoryDeployAuditSink::new());
    let verifier = InMemoryDeployVerifier::new_signed_from(Arc::clone(&sink));

    // Build webhook payload from GitHub Actions release event
    let webhook = CfDeployWebhook::new(
        "v0.1.0",
        "abc123def456abc123def456abc123def456abc1",
        "refs/tags/v0.1.0",
        DeployTarget::new(
            "corelink-worker",
            // 32-hex-char Cloudflare zone ID
            "00000000000000000000000000000001",
            "api.corelink.humangr.com/*",
        ),
        GitHubActor::new(
            "github-actions[bot]",
            "humangr-labs/corelink-server/.github/workflows/release-slsa3.yml@refs/tags/v0.1.0",
        ),
    );

    // OCI image reference (tag; digest resolved during verify)
    let image_ref = OciImageRef::from_tag("ghcr.io/humangr-labs/corelink-worker:v0.1.0");

    // Expected identity: canonical CoreLink release pipeline pattern
    let identity = CosignIdentityPattern::corelink_release();

    println!("Verifying deploy for release_tag=v0.1.0 ...");

    match verifier.verify_and_propagate(&webhook, &image_ref, &identity) {
        Ok(propagated) => {
            println!("Deploy PROPAGATED:");
            println!("  release_tag:              {}", propagated.release_tag);
            println!(
                "  cosign_signature_verified: {}",
                propagated.cosign_signature_verified
            );
            println!(
                "  rekor_log_index:           {}",
                propagated.rekor_log_index
            );
            println!(
                "  fulcio_cert_san:           {}",
                propagated.fulcio_cert_san
            );
            println!(
                "  cf_deployment_id:          {}",
                propagated.cf_deployment_id
            );
        }
        Err(e) => {
            eprintln!("Deploy BLOCKED: {e}");
            std::process::exit(1);
        }
    }

    println!("\nAudit events emitted: {}", sink.len());
    for event in sink.events() {
        println!(
            "  [{}] type={} outcome={:?} tag={}",
            event.event_id, event.event_type, event.outcome, event.release_tag
        );
    }
}
