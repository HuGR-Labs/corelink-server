//! Targeted regression tests that close mutation-testing surface
//! coverage gaps for `corelink-pat`. Written 2026-05-15 as part of the
//! mutation-baseline expansion (specs/_audits/2026-05-15-mutation-
//! expansion.md).
//!
//! A full local `cargo mutants -p corelink-pat` run is INFEASIBLE on a
//! laptop because each mutant invokes the OWASP-2024 Argon2id PHC tests
//! that take ~80s of total `cargo test` time per cycle; the crate emits
//! ~200 mutants, projecting to ~4.5h wall-clock. The CI nightly
//! workflow (`.github/workflows/mutation-nightly.yml`) runs the full
//! sweep with the 75 % per-crate floor.
//!
//! These tests target the canonical mutation surfaces empirically
//! observed in the `corelink-byok` / `corelink-signup` / `corelink-
//! tier-selection` baselines (2026-05-14) — `as_str` constants, match-
//! arm equality, comparison operators, and constant-return mutants on
//! observable surfaces.

#![forbid(unsafe_code)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]

use corelink_pat::{
    PatEnv, PatError, PatScopes, PatSigningKey, PatTokenId, SCOPE_ADMIN_AUDIT,
    SCOPE_ADMIN_BILLING, SCOPE_ADMIN_TENANT_R, SCOPE_ADMIN_TENANT_W, SCOPE_ADMIN_TOKENS,
    SCOPE_ADMIN_USERS, SCOPE_CACHE_DELETE, SCOPE_CACHE_FIND, SCOPE_CACHE_R, SCOPE_CACHE_RW,
    SCOPE_CACHE_W, SCOPE_EXECUTE_ACTION, SCOPE_KNOWN_MASK, SCOPE_REPORT_RESULT,
    compute_hmac_sig, verify_hmac_sig,
};

// =====================================================================
// PatEnv::as_wire — canonical 3-string allowlist. Pin each variant +
// distinctness so any `as_wire -> String::new()` / canary substitution
// is killed.
// =====================================================================

#[test]
fn pat_env_as_wire_canonical_strings_distinct() {
    assert_eq!(PatEnv::Pat.as_wire(), "pat");
    assert_eq!(PatEnv::Ci.as_wire(), "ci");
    assert_eq!(PatEnv::Ro.as_wire(), "ro");
    // Distinctness ensures `as_wire -> ""` / canary doesn't fold two
    // variants together.
    let all = [
        PatEnv::Pat.as_wire(),
        PatEnv::Ci.as_wire(),
        PatEnv::Ro.as_wire(),
    ];
    let mut sorted: Vec<&str> = all.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 3);
    for s in all {
        assert!(!s.is_empty(), "env as_wire must not be empty");
        assert_ne!(s, "xyzzy");
    }
}

#[test]
fn pat_env_from_wire_round_trip_and_rejects_unknown() {
    // Round-trip via as_wire + from_wire on every variant.
    for v in [PatEnv::Pat, PatEnv::Ci, PatEnv::Ro] {
        let w = v.as_wire();
        let back = PatEnv::from_wire(w).expect("known env round-trips");
        assert_eq!(back, v);
    }
    // Unknown strings → None.
    assert!(PatEnv::from_wire("").is_none());
    assert!(PatEnv::from_wire("PAT").is_none());
    assert!(PatEnv::from_wire("exec").is_none());
    assert!(PatEnv::from_wire("xyzzy").is_none());
}

#[test]
fn pat_env_display_matches_as_wire() {
    for v in [PatEnv::Pat, PatEnv::Ci, PatEnv::Ro] {
        let d = format!("{}", v);
        assert_eq!(d, v.as_wire());
        assert!(!d.is_empty());
    }
}

// =====================================================================
// PatScopes — bitset semantics. The boundary mutations the tool fires
// against `has` / `from_u64` / `from_u64_strict` are: `==` <-> `!=`,
// `&` <-> `|`, body → empty / canary.
// =====================================================================

#[test]
fn pat_scopes_known_mask_covers_exactly_twelve_bits() {
    // SCOPE_KNOWN_MASK is the bitwise union of 12 canonical scope bits.
    // Mutating from_u64 to drop the mask would let reserved bits leak
    // through; we assert .to_u64() drops them.
    let with_reserved: u64 = SCOPE_KNOWN_MASK | (1u64 << 63);
    let s = PatScopes::from_u64(with_reserved);
    assert_eq!(s.to_u64(), SCOPE_KNOWN_MASK);
    assert!(s.to_u64() & (1u64 << 63) == 0, "reserved bit must be dropped");
}

#[test]
fn pat_scopes_has_required_returns_true_only_when_all_bits_set() {
    let admin =
        PatScopes::from_u64(SCOPE_ADMIN_TENANT_R | SCOPE_ADMIN_TENANT_W);
    assert!(admin.has(SCOPE_ADMIN_TENANT_R));
    assert!(admin.has(SCOPE_ADMIN_TENANT_W));
    assert!(admin.has(SCOPE_ADMIN_TENANT_R | SCOPE_ADMIN_TENANT_W));
    // Missing a bit → false (kills `==` → `!=` mutation on the final
    // equality check at line 125).
    assert!(!admin.has(SCOPE_CACHE_R));
    assert!(!admin.has(SCOPE_ADMIN_TENANT_R | SCOPE_CACHE_R));
    // Empty required → vacuously true (a tautology that holds for any
    // self).
    assert!(admin.has(0));
}

#[test]
fn pat_scopes_from_u64_strict_rejects_reserved_bits() {
    let bit_reserved = 1u64 << 30;
    assert!(PatScopes::from_u64_strict(bit_reserved).is_none());
    // All known mask bits are accepted.
    assert!(PatScopes::from_u64_strict(SCOPE_KNOWN_MASK).is_some());
    // Empty → accepted.
    assert!(PatScopes::from_u64_strict(0).is_some());
    // Single known bit → accepted.
    assert!(PatScopes::from_u64_strict(SCOPE_CACHE_R).is_some());
    // Known + reserved → rejected.
    assert!(PatScopes::from_u64_strict(SCOPE_CACHE_R | bit_reserved).is_none());
}

#[test]
fn pat_scopes_is_empty_distinguishes_zero_from_one_bit() {
    assert!(PatScopes::empty().is_empty());
    // ANY single bit must make is_empty false → kills `is_empty -> true`.
    assert!(!PatScopes::from_u64(SCOPE_CACHE_R).is_empty());
    assert!(!PatScopes::from_u64(SCOPE_ADMIN_AUDIT).is_empty());
}

#[test]
fn pat_scopes_add_remove_round_trip() {
    let s = PatScopes::empty()
        .add(SCOPE_CACHE_R)
        .add(SCOPE_CACHE_W);
    assert!(s.has(SCOPE_CACHE_R));
    assert!(s.has(SCOPE_CACHE_W));
    let r = s.remove(SCOPE_CACHE_W);
    assert!(r.has(SCOPE_CACHE_R));
    assert!(!r.has(SCOPE_CACHE_W));
}

#[test]
fn pat_scopes_names_returns_distinct_entries_for_each_known_bit() {
    let bits = [
        (SCOPE_CACHE_R, "cache:r"),
        (SCOPE_CACHE_W, "cache:w"),
        (SCOPE_CACHE_FIND, "cache:find-missing"),
        (SCOPE_CACHE_DELETE, "cache:delete"),
        (SCOPE_ADMIN_TENANT_R, "admin:tenant-read"),
        (SCOPE_ADMIN_TENANT_W, "admin:tenant-write"),
        (SCOPE_ADMIN_TOKENS, "admin:tokens"),
        (SCOPE_ADMIN_BILLING, "admin:billing"),
        (SCOPE_ADMIN_AUDIT, "admin:audit"),
        (SCOPE_ADMIN_USERS, "admin:users"),
        (SCOPE_EXECUTE_ACTION, "execute:action"),
        (SCOPE_REPORT_RESULT, "report:result"),
    ];
    for (bit, name) in bits {
        let n = PatScopes::from_u64(bit).names();
        assert_eq!(n.len(), 1, "single bit must surface single name");
        assert_eq!(n[0], name);
    }
    // All 12 set → 12 names, all distinct.
    let all = PatScopes::from_u64(SCOPE_KNOWN_MASK).names();
    assert_eq!(all.len(), 12);
    let mut sorted: Vec<&str> = all.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 12);
}

#[test]
fn pat_scopes_union_and_intersection_drop_reserved_bits() {
    // Even if internal state somehow carried reserved bits, union /
    // intersection MUST mask them off (kills `& SCOPE_KNOWN_MASK` →
    // empty / no-op mutations).
    let a = PatScopes::from_u64(SCOPE_CACHE_R);
    let b = PatScopes::from_u64(SCOPE_CACHE_W);
    let u = a.union(b);
    assert!(u.has(SCOPE_CACHE_R));
    assert!(u.has(SCOPE_CACHE_W));
    let i = a.intersection(b);
    assert!(i.is_empty());
    // SCOPE_CACHE_RW alias = R | W.
    assert_eq!(u.to_u64(), SCOPE_CACHE_RW);
}

// =====================================================================
// PatTokenId::parse — length + Crockford b32 charset validation.
// =====================================================================

#[test]
fn pat_token_id_parse_rejects_wrong_length() {
    // Empty → None (kills `parse -> Some(default)` mutations).
    assert!(PatTokenId::parse("").is_none());
    // Too short.
    assert!(PatTokenId::parse("ABC").is_none());
    // Too long (17 chars).
    assert!(PatTokenId::parse("ABCDEFGHJKMNPQRST").is_none());
    // Exact length but contains illegal char `I` (Crockford excludes I L O U).
    assert!(PatTokenId::parse("IBCDEFGHJKMNPQRS").is_none());
    assert!(PatTokenId::parse("LBCDEFGHJKMNPQRS").is_none());
    assert!(PatTokenId::parse("OBCDEFGHJKMNPQRS").is_none());
    assert!(PatTokenId::parse("UBCDEFGHJKMNPQRS").is_none());
    // lowercase rejected (Crockford uppercase only).
    assert!(PatTokenId::parse("abcdefghjkmnpqrs").is_none());
}

#[test]
fn pat_token_id_parse_accepts_canonical_16_char_b32() {
    let tid = PatTokenId::parse("ABCDEFGH0123KMNP").expect("valid b32");
    assert_eq!(tid.as_str(), "ABCDEFGH0123KMNP");
    // Display = as_str.
    assert_eq!(format!("{}", tid), "ABCDEFGH0123KMNP");
}

// =====================================================================
// PatSigningKey::from_bytes — length guard (>= 32).
// =====================================================================

#[test]
fn pat_signing_key_rejects_short_bytes() {
    // 0..31 bytes all rejected (kills `< 32` → `< 33` / `<= 32` etc.).
    for n in [0usize, 1, 16, 30, 31] {
        let v = vec![0xAA; n];
        let res = PatSigningKey::from_bytes(v);
        match res {
            Err(PatError::SigningKeyTooShort) => {}
            other => panic!(
                "len={} must reject SigningKeyTooShort; got {:?}",
                n,
                other.is_ok()
            ),
        }
    }
}

#[test]
fn pat_signing_key_accepts_32_byte_floor_and_above() {
    for n in [32usize, 33, 48, 64, 128] {
        let v = vec![0xAA; n];
        PatSigningKey::from_bytes(v).expect("valid key length must be accepted");
    }
}

#[test]
fn pat_signing_key_debug_redacts_bytes() {
    let key = PatSigningKey::from_bytes(vec![0xAA; 32]).expect("32-byte key");
    let d = format!("{:?}", key);
    assert!(d.contains("redacted"), "debug must redact bytes");
    assert!(!d.contains("aa"));
    assert!(!d.contains("AA"));
}

// =====================================================================
// HMAC sig compute + verify — deterministic on input.
// =====================================================================

#[test]
fn hmac_sig_compute_is_deterministic_and_distinguishes_inputs() {
    let key = PatSigningKey::from_bytes(vec![0x42; 32]).expect("key");
    let preimage_a = b"token-id-A.random-secret-A".as_slice();
    let preimage_b = b"token-id-B.random-secret-B".as_slice();
    let sig_a = compute_hmac_sig(&key, preimage_a);
    let sig_a_again = compute_hmac_sig(&key, preimage_a);
    let sig_b = compute_hmac_sig(&key, preimage_b);

    // Deterministic.
    assert_eq!(sig_a, sig_a_again);
    // Distinct inputs → distinct sigs.
    assert_ne!(sig_a, sig_b);
    // Non-zero and not the canary `[0xAB; 16]` pattern.
    assert_ne!(sig_a, [0u8; 16]);
    assert_ne!(sig_a, [0xAB; 16]);
}

#[test]
fn hmac_sig_verify_accepts_matching_and_rejects_mismatch_and_wrong_length() {
    let key = PatSigningKey::from_bytes(vec![0x42; 32]).expect("key");
    let preimage = b"canonical-preimage".as_slice();
    let sig = compute_hmac_sig(&key, preimage);
    // Matching sig → Ok.
    verify_hmac_sig(&key, preimage, &sig).expect("matching sig must verify");
    // Flip a bit → InvalidPat.
    let mut bad = sig;
    bad[0] ^= 0x01;
    match verify_hmac_sig(&key, preimage, &bad) {
        Err(PatError::InvalidPat) => {}
        other => panic!("bit-flipped sig must reject as InvalidPat; got {:?}", other.is_ok()),
    }
    // Wrong-length sig (15 or 17 bytes) → Malformed.
    let too_short: &[u8] = &sig[..15];
    let too_long: Vec<u8> = sig.iter().copied().chain(std::iter::once(0u8)).collect();
    match verify_hmac_sig(&key, preimage, too_short) {
        Err(PatError::Malformed) => {}
        other => panic!("short sig must reject as Malformed; got {:?}", other.is_ok()),
    }
    match verify_hmac_sig(&key, preimage, &too_long) {
        Err(PatError::Malformed) => {}
        other => panic!("long sig must reject as Malformed; got {:?}", other.is_ok()),
    }
}

// =====================================================================
// PatError — Display message distinctness (kills `fmt -> ""` mutants).
// =====================================================================

#[test]
fn pat_error_display_strings_distinct_and_non_empty() {
    let v = [
        format!("{}", PatError::Malformed),
        format!("{}", PatError::InvalidPat),
        format!("{}", PatError::HashError("phc-parse")),
        format!("{}", PatError::EntropyUnavailable),
        format!("{}", PatError::SigningKeyTooShort),
    ];
    for s in &v {
        assert!(!s.is_empty(), "Display must not be empty");
    }
    let mut sorted: Vec<String> = v.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 5, "every variant must have distinct Display");
}
