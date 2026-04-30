//! `corelink-pat` — scope bitset usage example.
//!
//! Shows construction, bitwise operations, and the canonical
//! permission check pattern used by the middleware (WI-S03-003).

#![allow(
    clippy::print_stdout,
    reason = "examples emit human-readable narration to stdout"
)]

use corelink_pat::{
    PatScopes, SCOPE_ADMIN_AUDIT, SCOPE_ADMIN_TOKENS, SCOPE_CACHE_FIND, SCOPE_CACHE_R,
    SCOPE_CACHE_RW, SCOPE_CACHE_W,
};

fn main() {
    // CI runner gets read+write + find-missing.
    let ci_scopes = PatScopes::from_u64(SCOPE_CACHE_RW | SCOPE_CACHE_FIND);
    println!("CI scope set: {:?}", ci_scopes);
    assert!(ci_scopes.has(SCOPE_CACHE_R));
    assert!(ci_scopes.has(SCOPE_CACHE_W));
    assert!(!ci_scopes.has(SCOPE_ADMIN_TOKENS));

    // Read-only token: only cache:r and find-missing.
    let ro_scopes = PatScopes::from_u64(SCOPE_CACHE_R | SCOPE_CACHE_FIND);
    assert!(!ro_scopes.has(SCOPE_CACHE_W));
    println!("RO names: {:?}", ro_scopes.names());

    // Bitwise operators.
    let combined = ci_scopes | PatScopes::from_u64(SCOPE_ADMIN_AUDIT);
    println!("Combined names: {:?}", combined.names());
    let intersection = ci_scopes & PatScopes::from_u64(SCOPE_CACHE_W);
    println!("Intersection names: {:?}", intersection.names());

    // Required scope check pattern (the middleware does this on
    // every request after the verify pipeline).
    let required = SCOPE_CACHE_W;
    if ci_scopes.has(required) {
        println!("ALLOWED: CI token has cache:w");
    } else {
        println!("DENIED: CI token lacks required scope");
    }
}
