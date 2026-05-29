//! `emit_e2e_seed` — one-off harness that mints a real PAT via the canonical
//! `corelink_pat::mint::mint()` function and prints:
//!   1. the customer-side PAT plaintext (for `CORELINK_E2E_TOKEN`);
//!   2. the SQL needed to seed the corresponding `tenant` + `pat` rows in a
//!      D1 database so the Worker's PAT lookup + the DO's Argon2id verify
//!      both pass.
//!
//! It is a `#[test]` so we get the workspace's release/debug + sccache plumbing
//! for free. It is `#[ignore]` so a regular `cargo test` run never executes it
//! (no side-effects); invoke explicitly:
//!
//! ```sh
//! CORELINK_PAT_SIGNING_KEY_HEX=<64-hex-chars> \
//!   cargo test --test emit_e2e_seed emit_e2e_pat_seed -- --ignored --nocapture
//! ```
//!
//! The output is consumable by `wrangler d1 execute CONFIG_DB --env prod --remote
//! --file=<emitted.sql>`. The PAT plaintext is printed to stdout so the caller
//! can capture it without it touching the filesystem.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    reason = "test harness — local-only side-effect script"
)]

use std::time::Duration;
use uuid::Uuid;

use corelink_pat::{
    mint::mint,
    scopes::{PatScopes, SCOPE_CACHE_R, SCOPE_CACHE_W},
    PatEnv, PatSigningKey, PrincipalId, TenantId,
};

#[test]
#[ignore = "side-effect harness — invoke with --ignored --nocapture; needs CORELINK_PAT_SIGNING_KEY_HEX"]
fn emit_e2e_pat_seed() {
    // 1. Resolve the signing key. Must MATCH the PAT_SIGNING_KEY secret bound
    //    on the prod Worker, otherwise the Worker's HMAC fast-fail will reject
    //    the minted token.
    let hex = std::env::var("CORELINK_PAT_SIGNING_KEY_HEX")
        .expect("CORELINK_PAT_SIGNING_KEY_HEX env var required (64 hex chars = 32 bytes)");
    let key_bytes = hex_decode(&hex).expect("CORELINK_PAT_SIGNING_KEY_HEX must be valid hex");
    assert_eq!(
        key_bytes.len(),
        32,
        "signing key must decode to 32 bytes; got {}",
        key_bytes.len()
    );
    let signing_key = PatSigningKey::from_bytes(key_bytes).expect("signing key build");

    // 2. Mint a fresh PAT with cache R+W scope (covers the e2e CAS journeys).
    //    Tenant + principal are fresh UUIDs — we seed both via the SQL emitted
    //    below.
    let tenant_id = TenantId(Uuid::now_v7());
    let principal_id = PrincipalId(Uuid::now_v7());
    let scopes = PatScopes::from_u64(SCOPE_CACHE_R | SCOPE_CACHE_W);
    let ttl = Some(Duration::from_secs(7 * 24 * 60 * 60)); // 7d
    let (plaintext, pat) =
        mint(PatEnv::Pat, tenant_id, principal_id, scopes, ttl, &signing_key, 1).expect("mint");

    // 3. Emit the customer-facing token + the seed SQL. The plaintext is
    //    capturable by the harness caller; the SQL is structured so the FK
    //    (pat.tenant_id → tenant.tenant_id) is satisfied.
    println!("---PAT_PLAINTEXT---");
    println!("{}", plaintext.into_string());
    println!("---END_PAT_PLAINTEXT---");

    let now_ms: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as i64;
    let expires_ms: i64 = pat
        .expires_at
        .expect("ttl set")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis() as i64;
    // `shown_once_consumed=1`: this PAT is being delivered out-of-band (not via
    // the orchestrator reveal endpoint), so we pre-consume the reveal slot to
    // prevent later reveal attempts.
    let shown_once_token = Uuid::now_v7();

    println!("---SEED_SQL---");
    println!(
        "INSERT INTO tenant (tenant_id, primary_region, created_at_ms, updated_at_ms) \
         VALUES ('{tenant}', 'enam', {now}, {now});",
        tenant = pat.tenant_id.0,
        now = now_ms
    );
    println!(
        "INSERT INTO pat (pat_id, tenant_id, pat_hash, scope, expires_ms, \
         shown_once_token, shown_once_consumed, created_ms, token_id) VALUES \
         ('{pat_id}', '{tenant}', '{hash}', 'admin', {expires}, '{shown}', 1, {now}, '{token_id}');",
        pat_id = pat.id.0,
        tenant = pat.tenant_id.0,
        hash = pat.hash.as_str(),
        expires = expires_ms,
        shown = shown_once_token,
        now = now_ms,
        token_id = pat.token_id.as_str(),
    );
    println!("---END_SEED_SQL---");
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Some(out)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
