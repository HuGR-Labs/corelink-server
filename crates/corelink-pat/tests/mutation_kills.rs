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
    parse_env, PatEnv, PatError, PatScopes, PatSigningKey, PatTokenId, SCOPE_ADMIN_AUDIT,
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

// =====================================================================
// Wave-24 additions (2026-05-16 empirical sweep): targeted kills for
// mutants that survived the wave-15 defensive baseline. See
// `specs/_audits/2026-05-16-debt-008-wave24-pat-clerk-sweep.md` for
// the per-mutant analysis.
// =====================================================================

/// Kill `format.rs:241:37 replace + with *` in `parse_env`. The guard
/// `bytes.len() < PAT_PREFIX_LEN + 3` becomes `bytes.len() < 9 * 3 =
/// 27`, which would reject canonical-shape inputs of total length
/// [12..27). Calling `parse_env` on the 13-byte string
/// `"corelink_pat_"` MUST return `Ok(PatEnv::Pat)` because the env
/// segment + trailing `_` is fully present — the `*` mutant would
/// short-circuit to `Err(Malformed)`.
#[test]
fn parse_env_accepts_13_byte_pat_prefix_with_trailing_separator() {
    let input = "corelink_pat_"; // len 13
    assert_eq!(input.len(), 13);
    let parsed = parse_env(input).expect("parse_env must accept canonical 13-byte pat prefix");
    assert_eq!(parsed, PatEnv::Pat);
}

/// Companion: same guard against the `+` → `-` flavor (cap drops to
/// `9 - 3 = 6`). A 7-byte input that starts with the literal `core...`
/// MUST still produce `Err(Malformed)` because the prefix scanner +
/// downstream length check both reject. Pinning the explicit Err
/// keeps this surface a 1:1 regression catcher if the early-length
/// guard is mutated to a different arithmetic.
#[test]
fn parse_env_rejects_7_byte_truncated_input() {
    let input = "corelin"; // len 7
    assert_eq!(input.len(), 7);
    assert!(matches!(parse_env(input), Err(PatError::Malformed)));
}

/// Kill `scopes.rs:110:20 & → |` and `& → ^` in `PatScopes::single`.
/// The unmutated body is `Self(scope & SCOPE_KNOWN_MASK)` — single bit
/// in, single bit out. Mutants:
/// - `scope | SCOPE_KNOWN_MASK` → all 12 bits set (popcount 12).
/// - `scope ^ SCOPE_KNOWN_MASK` → 11 bits set (popcount 11) when
///   input is one of the 12 canonical bits.
#[test]
fn pat_scopes_single_returns_exactly_the_input_canonical_bit() {
    let r = PatScopes::single(SCOPE_CACHE_R);
    assert_eq!(r.to_u64(), SCOPE_CACHE_R);
    assert_eq!(r.to_u64().count_ones(), 1);
    let w = PatScopes::single(SCOPE_CACHE_W);
    assert_eq!(w.to_u64(), SCOPE_CACHE_W);
    assert_eq!(w.to_u64().count_ones(), 1);
    // Distinctness: a constant-return mutant would collapse.
    assert_ne!(r.to_u64(), w.to_u64());
    // Reserved bits get dropped (the `& SCOPE_KNOWN_MASK` body
    // semantic — both `|` and `^` mutants would corrupt this).
    let masked = PatScopes::single(SCOPE_CACHE_R | (1u64 << 63));
    assert_eq!(masked.to_u64(), SCOPE_CACHE_R, "reserved bit must drop");
}

/// Kill `scopes.rs:131:31 & → |` in `PatScopes::add`. Body is
/// `Self((self.0 | scope) & SCOPE_KNOWN_MASK)`. Mutant: `(... | scope)
/// | SCOPE_KNOWN_MASK` ⇒ always returns SCOPE_KNOWN_MASK regardless
/// of the input. Asserting `empty().add(SCOPE_CACHE_R) == {bit 0}`
/// kills this directly.
#[test]
fn pat_scopes_add_single_bit_preserves_popcount_one() {
    let s = PatScopes::empty().add(SCOPE_CACHE_R);
    assert_eq!(s.to_u64(), SCOPE_CACHE_R);
    assert_eq!(s.to_u64().count_ones(), 1);
    // Add a second disjoint bit → popcount 2.
    let s2 = s.add(SCOPE_CACHE_W);
    assert_eq!(s2.to_u64(), SCOPE_CACHE_R | SCOPE_CACHE_W);
    assert_eq!(s2.to_u64().count_ones(), 2);
    // Adding a reserved bit must be a no-op (the `& SCOPE_KNOWN_MASK`
    // tail drops it).
    let s3 = s.add(1u64 << 63);
    assert_eq!(s3.to_u64(), SCOPE_CACHE_R);
}

/// Kill any "constant return" mutant on the `Ok` / `Err` arms of
/// `parse_env` by exhausting all three env discriminants
/// + asserting distinctness at the public boundary.
#[test]
fn parse_env_distinguishes_all_three_env_discriminants() {
    let pat = parse_env("corelink_pat_").expect("pat");
    let ci = parse_env("corelink_ci_").expect("ci");
    let ro = parse_env("corelink_ro_").expect("ro");
    assert_eq!(pat, PatEnv::Pat);
    assert_eq!(ci, PatEnv::Ci);
    assert_eq!(ro, PatEnv::Ro);
    // Distinctness: a `Ok(_)` constant-return mutant would collapse.
    assert_ne!(pat, ci);
    assert_ne!(ci, ro);
    assert_ne!(pat, ro);
}

/// Kill `mint.rs:80 % → /` and `mint.rs:80 % → +` on the production
/// `mint` (OsRng) path. Same Crockford mapping as line 185.
/// - Under `% → /`: alphabet collapses to `'0'..='7'` (8 chars).
/// - Under `% → +`: every char defaults to `'0'`.
///
/// Mint a single token via the production path and assert it contains
/// at least one char outside `'0'..='7'` (kills `/`; prob ≈ 1 - 0.25^16
/// under proper `%`) and at least one non-`'0'` char (kills `+`;
/// prob ≈ 1 - (1/32)^16 under proper `%`). Both checks are
/// effectively unflakeable.
#[test]
fn mint_production_token_id_spans_upper_half_of_alphabet() {
    use uuid::Uuid;
    let key = PatSigningKey::from_bytes(vec![0x11u8; 32]).expect("signing key");
    // Try a few times to make the test rock-solid; first attempt is
    // already > 1 - 10^-9 likely to pass.
    let mut saw_upper = false;
    let mut saw_non_zero = false;
    for _ in 0..4 {
        let (plaintext, _pat) = corelink_pat::mint::mint(
            PatEnv::Pat,
            corelink_pat::TenantId(Uuid::nil()),
            corelink_pat::PrincipalId(Uuid::nil()),
            PatScopes::from_u64(SCOPE_CACHE_R),
            None,
            &key,
            1,
        )
        .expect("mint");
        let s = plaintext.into_string();
        let prefix = "corelink_pat_";
        let body = &s[prefix.len()..];
        let token_id = &body[..16];
        // Crockford upper half = '8','9','A','B','C','D','E','F','G','H','J','K','M','N','P','Q','R','S','T','V','W','X','Y','Z'
        if token_id.chars().any(|c| !('0'..='7').contains(&c)) {
            saw_upper = true;
        }
        if token_id.chars().any(|c| c != '0') {
            saw_non_zero = true;
        }
        if saw_upper && saw_non_zero {
            break;
        }
    }
    assert!(saw_upper, "mint must occasionally emit chars outside '0'..='7'");
    assert!(saw_non_zero, "mint must emit non-'0' chars");
}

/// Kill `mint.rs:185 % → /` and `mint.rs:185 % → +` on the
/// deterministic `mint_with_entropy` path. Line 185 maps each
/// token_id byte into the 32-element Crockford alphabet via `% 32`.
/// - Under `% → /`: `(u8 as usize) / 32 ∈ 0..=7`, alphabet collapses
///   to the 8 chars `'0'..='7'`.
/// - Under `% → +`: `(u8 as usize) + 32 ∈ 32..=287`, all out of range
///   of the 32-element slice — every char defaults to `'0'`.
///
/// Feed token_id_bytes that span every 8th value across [0, 255]
/// (covering all 32 mod-32 residues exactly once across the 16
/// bytes) so the proper `%` produces 16 distinct alphabet chars —
/// strictly more than the 8 (`/`) or 1 (`+`) the mutants can emit.
#[test]
fn mint_with_entropy_token_id_alphabet_spans_more_than_eight_symbols() {
    use corelink_pat::mint::{mint_with_entropy, DeterministicMintInput};
    use corelink_pat::types::PAT_TOKEN_ID_LEN;
    use password_hash::Salt;
    use std::collections::HashSet;
    use uuid::Uuid;

    let salt: Salt<'static> = Salt::from_b64("Y29yZWxpbmt2MXNhbHQwMQ").expect("salt");
    let key = PatSigningKey::from_bytes(vec![0x42u8; 32]).expect("signing key");
    // 16 bytes spanning all 32 Crockford residues: pick byte i = i * 8
    // so values are 0, 8, 16, ..., 120 — each `% 32` ∈ {0, 8, 16, 24, 0, 8, ...},
    // giving 4 distinct symbols. Better: byte i = i * 16 + (i / 2) so we
    // hit 16 unique mod-32 values directly.
    let mut token_id_bytes = [0u8; PAT_TOKEN_ID_LEN];
    for (i, b) in token_id_bytes.iter_mut().enumerate() {
        // Spread mod-32 hits across the 16 positions:
        // i=0 -> 0; i=1 -> 17; i=2 -> 34 (->2); ... so 16 distinct.
        *b = (i as u8).wrapping_mul(17);
    }
    let secret_bytes = [0xBBu8; 32];

    let (plaintext, _pat) = mint_with_entropy(DeterministicMintInput {
        env: PatEnv::Pat,
        tenant_id: corelink_pat::TenantId(Uuid::nil()),
        principal_id: corelink_pat::PrincipalId(Uuid::nil()),
        scopes: PatScopes::from_u64(SCOPE_CACHE_R),
        ttl: None,
        signing_key: &key,
        signing_key_id: 1,
        token_id_bytes,
        secret_bytes,
        salt,
    })
    .expect("mint_with_entropy");

    let s = plaintext.into_string();
    let prefix = "corelink_pat_";
    assert!(s.starts_with(prefix), "plaintext shape: {}", s);
    let body = &s[prefix.len()..];
    let token_id = &body[..16];
    let alphabet: HashSet<char> = token_id.chars().collect();
    assert!(
        alphabet.len() > 8,
        "mint_with_entropy must span >8 distinct Crockford symbols on a \
         residue-spreading 16-byte input; got {} ({:?})",
        alphabet.len(),
        alphabet
    );
    // Also: under `% → +`, every char defaults to '0'. Pin a stronger
    // upper-half discriminator: the token MUST contain at least one
    // non-'0' char.
    assert!(
        token_id.chars().any(|c| c != '0'),
        "mint_with_entropy token_id must include non-'0' chars; got {}",
        token_id
    );
}
