//! Example — emit a single `auth.token.validated` event into the
//! in-memory emitter, then dump its canonical bytes + content hash.
//!
//! Run with `cargo run --example emit_token_validated -p corelink-audit`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::print_stdout,
    reason = "example program — pretty-printing to stdout is the point"
)]

use corelink_audit::{
    compute_content_hash, AuthEvent, AuthEventData, AuthEventType, Emitter, InMemoryEmitter,
    PrincipalIdHash, RegionTag, RequestId, RetentionHint, TenantId, TokenKind,
};
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tenant = TenantId::from_uuid(Uuid::from_u128(0x0193_8af0_aaaa_bbbb_cccc_dddd_eeee_ffff));
    let principal = PrincipalIdHash::derive("user_2NkX8a3Bq")?;

    let event = AuthEvent::new(
        AuthEventType::TokenValidated,
        "corelink://wnam/auth/middleware",
        tenant,
        principal,
        RegionTag::Wnam,
        RequestId::new("req_abc123"),
        RetentionHint::Team90d,
        1_700_000_000_000,
        AuthEventData::TokenValidated {
            token_kind: TokenKind::Pat,
            scope_bitset: 0b0000_0001,
        },
    );

    let canonical = serde_jcs::to_vec(&event)?;
    let canonical_str = String::from_utf8(canonical.clone())?;
    let hash = compute_content_hash(&event)?;

    println!("Canonical JCS bytes ({} bytes):", canonical.len());
    println!("{canonical_str}");
    println!();
    println!("content_hash: {hash}");

    let emitter = InMemoryEmitter::new();
    emitter.emit(event)?;
    println!();
    println!("Emitter captured {} event(s).", emitter.len());
    Ok(())
}
