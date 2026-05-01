//! Example — compute the content + chain hashes for a synthetic
//! 3-event chain. Mirrors the formal property in the chain crate
//! rustdoc.
//!
//! Run with `cargo run --example chain_hash_compute -p corelink-audit`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    reason = "example program — pretty-printing to stdout is the point"
)]

use corelink_audit::{
    compute_content_hash, link_chain_hash, AuthEvent, AuthEventData, AuthEventType, ChainHash,
    PrincipalIdHash, RegionTag, RequestId, RetentionHint, TenantId, TokenKind,
};
use uuid::Uuid;

fn make_event(req: &str, principal: &str) -> Result<AuthEvent, Box<dyn std::error::Error>> {
    Ok(AuthEvent::new(
        AuthEventType::TokenValidated,
        "corelink://wnam/auth/middleware",
        TenantId::from_uuid(Uuid::nil()),
        PrincipalIdHash::derive(principal)?,
        RegionTag::Wnam,
        RequestId::new(req),
        RetentionHint::Team90d,
        1_700_000_000_000,
        AuthEventData::TokenValidated {
            token_kind: TokenKind::Pat,
            scope_bitset: 1,
        },
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let e1 = make_event("req_001", "user_alice")?;
    let e2 = make_event("req_002", "user_bob")?;
    let e3 = make_event("req_003", "user_carol")?;

    let c1 = compute_content_hash(&e1)?;
    let c2 = compute_content_hash(&e2)?;
    let c3 = compute_content_hash(&e3)?;

    let h0 = ChainHash::genesis();
    let h1 = link_chain_hash(&h0, &c1);
    let h2 = link_chain_hash(&h1, &c2);
    let h3 = link_chain_hash(&h2, &c3);

    println!("Genesis chain hash : {h0}");
    println!();
    println!("Event 1 content   : {c1}");
    println!("Event 1 chain hash : {h1}");
    println!();
    println!("Event 2 content   : {c2}");
    println!("Event 2 chain hash : {h2}");
    println!();
    println!("Event 3 content   : {c3}");
    println!("Event 3 chain hash : {h3}");
    println!();
    println!("Re-running event 1 link is byte-equal:");
    let h1_again = link_chain_hash(&h0, &c1);
    assert_eq!(h1, h1_again);
    println!("  link_chain_hash(genesis, c1) = {h1_again}");
    Ok(())
}
