//! `corelink-pat` — mint + verify roundtrip example.
//!
//! Demonstrates the canonical mint API surface and a successful
//! end-to-end verify. Run with:
//!
//! ```text
//! cargo run --example mint_and_verify -p corelink-pat
//! ```

#![allow(
    clippy::print_stdout,
    reason = "examples emit human-readable narration to stdout"
)]

use corelink_pat::{
    mint, verify_with_hash, PatEnv, PatScopes, PatSigningKey, PrincipalId, TenantId,
    SCOPE_CACHE_RW,
};
use std::time::Duration;
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // In production the signing key comes from the per-region secret
    // rotation pipeline (key_management.md §3.2.1; 24h overlap).
    let signing_key = PatSigningKey::from_bytes(vec![0x42u8; 32])?;
    let signing_key_id = 1;

    let tenant = TenantId(Uuid::now_v7());
    let principal = PrincipalId(Uuid::now_v7());
    let scopes = PatScopes::from_u64(SCOPE_CACHE_RW);

    let (plaintext, pat) = mint(
        PatEnv::Pat,
        tenant,
        principal,
        scopes,
        Some(Duration::from_secs(86_400 * 90)), // 90d default per auth_model.md §1.2
        &signing_key,
        signing_key_id,
    )?;

    println!("Minted PAT id={} env={}", pat.id, pat.env);
    println!("Persistable hash (PHC): {}", pat.hash.as_str());
    println!("Token id (DB lookup key): {}", pat.token_id);
    println!("Scopes: {:?}", pat.scopes);

    // The plaintext lives only on the response path. Production code
    // moves this directly into the API response body. Here we
    // immediately consume it for the demo verify.
    let pt_string = plaintext.into_string();
    println!("Plaintext length = {} chars", pt_string.len());

    let verified = verify_with_hash(&pt_string, &pat.token_id, &pat.hash, &signing_key)?;
    println!("Verified env={:?} token_id={}", verified.env, verified.token_id);

    // Print the full plaintext at the end so the user can paste it
    // into their CLI config — production would do this exactly once
    // through the API response.
    println!("\nPAT plaintext (one-time display):\n  {pt_string}");

    Ok(())
}
