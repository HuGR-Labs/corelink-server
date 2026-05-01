//! Example — demonstrate the PII redaction surface (hash newtypes +
//! `redact_pat!` macro).
//!
//! Run with `cargo run --example redaction_macros -p corelink-audit`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    reason = "example program — pretty-printing to stdout is the point"
)]

use corelink_audit::{redact_pat, EmailHash, PatIdHash, PrincipalIdHash};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Raw PII inputs (NEVER persist these directly).
    let raw_email = "alice@example.com";
    let raw_principal = "user_2NkX8a3Bq";
    let raw_pat_id = "pat_01938af0123456789abcdef";
    let raw_pat_plaintext = "corelink_pat_live_xxx.secret_yyy.sig_zzz";

    // Derive pseudonymous 16-hex-char prefixes — the only PII surface
    // an audit envelope ever sees.
    let email_hash = EmailHash::derive(raw_email)?;
    let principal_hash = PrincipalIdHash::derive(raw_principal)?;
    let pat_hash = PatIdHash::derive(raw_pat_id)?;

    println!("Raw email          : {raw_email}");
    println!("Email hash         : {email_hash}");
    println!();
    println!("Raw principal id   : {raw_principal}");
    println!("Principal hash     : {principal_hash}");
    println!();
    println!("Raw PAT id         : {raw_pat_id}");
    println!("PAT id hash        : {pat_hash}");
    println!();
    println!("Raw PAT plaintext  : {raw_pat_plaintext}");
    println!("redact_pat! output : {}", redact_pat!(raw_pat_plaintext));

    // Verify same input → same hash (deterministic pseudonym).
    assert_eq!(EmailHash::derive(raw_email)?, email_hash);
    Ok(())
}
