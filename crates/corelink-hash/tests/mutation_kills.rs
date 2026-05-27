//! Wave-21 DEBT-008 mutation-sweep follow-on: targeted tests that kill the
//! 7 non-equivalent surviving mutants surfaced by the 2026-05-16 empirical
//! `cargo mutants -p corelink-hash` run (8 missed / 36 viable → 77.78 %
//! kill rate). See `specs/_audits/sealed/2026-05-16-debt-008-mutation-sweep.md`.
//!
//! The 8th miss (`crates/corelink-hash/src/digest.rs:59:31: replace | with ^
//! in Digest::from_hex`) is a mathematically **equivalent mutation**:
//! `(hi << 4)` always has its low four bits zero (hi is a nibble), and `lo`
//! occupies only the low four bits, so `(hi << 4) | lo` and `(hi << 4) ^ lo`
//! produce bit-identical outputs across the entire input domain. It is
//! documented (not killed) in the audit doc §1.4.
//!
//! Coverage map (mutant → killing assertion):
//!
//! - `digest.rs:57 i*2 → i/2`     → [`invalid_hex_byte_position_kills_mul_to_div_at_hi_nibble`]
//! - `digest.rs:57 i*2 → i+2`     → [`invalid_hex_byte_position_kills_mul_to_add_at_hi_nibble`]
//! - `digest.rs:58 i*2+1 → i+2+1` → [`invalid_hex_byte_position_kills_mul_to_add_at_lo_nibble`]
//! - `digest.rs:94 as_bytes → &[0;32]` → [`as_bytes_returns_real_blake3_not_all_zero`]
//! - `digest.rs:94 as_bytes → &[1;32]` → [`as_bytes_returns_real_blake3_not_all_one`]
//! - `digest.rs:100 Display::fmt` → [`display_renders_full_hex_not_default`]
//! - `digest.rs:106 Debug::fmt`   → [`debug_renders_wrapped_hex_not_default`]

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    missing_docs,
    reason = "test code; panic on assertion failure is the contract"
)]

use corelink_hash::{Digest, ParseError};

// ---------------------------------------------------------------------------
// `from_hex` error-position arithmetic — `i * 2` and `i * 2 + 1`
//
// The 32-byte digest decode walks `bytes.chunks_exact(2).enumerate()` so
// chunk `i` covers character positions `i*2` (hi nibble) and `i*2 + 1`
// (lo nibble). A test at i==2 cannot distinguish `i*2` from `i+2` (both
// yield 4); we therefore poison the hi nibble at chunk i=2 (pos 4) AND
// at chunk i=3 (pos 6) for the `*→+` variant, and assert the exact
// reported error position. `*→/` is killed by any i ≥ 2 since
// `i*2 != i/2` for i != 0 (and 0 collides at zero).
// ---------------------------------------------------------------------------

/// `i*2 → i/2`: position-4 char (chunk i=2, hi nibble) reports `4`, the
/// `i/2` mutant would report `1`. Distinguishes the two.
#[test]
fn invalid_hex_byte_position_kills_mul_to_div_at_hi_nibble() {
    // 64-char hex string with a non-hex 'Z' at byte position 4 (chunk i=2,
    // hi nibble). i*2 == 4; i/2 == 1; i+2 == 4 (collides — see sibling test).
    let mut s = String::from("d749Z1efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24");
    debug_assert_eq!(s.len(), 64);
    let err = Digest::from_hex(&s).expect_err("non-hex at pos 4");
    assert_eq!(
        err,
        ParseError::InvalidHexByte(4),
        "expected position 4 (i*2 where i=2); i/2 mutant would report 1"
    );
    // Mutate the input to also confirm the chunk-3 hi nibble path (kills
    // `*→+` at i=3 below).
    s.replace_range(4..5, "8"); // restore pos 4 to valid hex
    s.replace_range(6..7, "Q"); // poison chunk i=3 hi nibble (pos 6)
    let err2 = Digest::from_hex(&s).expect_err("non-hex at pos 6");
    assert_eq!(
        err2,
        ParseError::InvalidHexByte(6),
        "expected position 6 (i*2 where i=3); i+2 mutant would report 5"
    );
}

/// `i*2 → i+2` at i=3 (pos 6): i*2 == 6; i+2 == 5. Distinguishes mul from
/// add. (At i=2 both yield 4 → tested above via the chunk-3 path.)
#[test]
fn invalid_hex_byte_position_kills_mul_to_add_at_hi_nibble() {
    let s = String::from("d74981Zfa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24");
    debug_assert_eq!(s.len(), 64);
    let err = Digest::from_hex(&s).expect_err("non-hex at pos 6");
    assert_eq!(
        err,
        ParseError::InvalidHexByte(6),
        "expected position 6 (i*2 where i=3); i+2 mutant would report 5"
    );
}

/// `i*2+1 → i+2+1` (the `*→+` mutation on line 58) at i=3 (pos 7):
/// `i*2+1 == 7`; `i+2+1 == 6`. Distinguishes mul from add on the lo nibble.
#[test]
fn invalid_hex_byte_position_kills_mul_to_add_at_lo_nibble() {
    let s = String::from("d7498ef-0a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e2400");
    // Position 7 = chunk i=3, lo nibble. char '-' is non-hex.
    debug_assert_eq!(s.len(), 64);
    let err = Digest::from_hex(&s).expect_err("non-hex at pos 7");
    assert_eq!(
        err,
        ParseError::InvalidHexByte(7),
        "expected position 7 (i*2+1 where i=3); i+2+1 mutant would report 6"
    );
}

// ---------------------------------------------------------------------------
// `as_bytes` — the doc(hidden) escape hatch must return the real 32-byte
// BLAKE3 output, not a sentinel `[0; 32]` or `[1; 32]`.
// ---------------------------------------------------------------------------

/// BLAKE3("hello world") = `d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24`
/// (canonical RFC 9106-equivalent test vector). Confirms `as_bytes` does
/// not collapse to `[0; 32]`.
#[test]
fn as_bytes_returns_real_blake3_not_all_zero() {
    let d = Digest::compute(b"hello world");
    let bytes = d.as_bytes();
    assert_ne!(
        bytes, &[0u8; 32],
        "as_bytes must return the real BLAKE3 output, not [0; 32]"
    );
    // Pin the exact expected vector so a constant-return mutant would
    // need to coincidentally match BLAKE3("hello world").
    let expected = [
        0xd7, 0x49, 0x81, 0xef, 0xa7, 0x0a, 0x0c, 0x88, 0x0b, 0x8d, 0x8c, 0x19, 0x85, 0xd0,
        0x75, 0xdb, 0xcb, 0xf6, 0x79, 0xb9, 0x9a, 0x5f, 0x99, 0x14, 0xe5, 0xaa, 0xf9, 0x6b,
        0x83, 0x1a, 0x9e, 0x24,
    ];
    assert_eq!(bytes, &expected, "BLAKE3(\"hello world\") canonical vector");
}

/// Sibling test against the `[1; 32]` constant-return mutant.
#[test]
fn as_bytes_returns_real_blake3_not_all_one() {
    let d = Digest::compute(b"corelink");
    let bytes = d.as_bytes();
    assert_ne!(
        bytes, &[1u8; 32],
        "as_bytes must return the real BLAKE3 output, not [1; 32]"
    );
    // Also confirm round-trip: as_bytes → to_hex → from_hex must restore.
    let hex = d.to_hex();
    let parsed = Digest::from_hex(&hex).expect("roundtrip");
    assert_eq!(
        parsed.as_bytes(),
        bytes,
        "as_bytes must be the inverse of hex round-trip"
    );
}

// ---------------------------------------------------------------------------
// `Display` / `Debug` formatters — must render real hex, not the
// `Ok(Default::default())` mutant which returns success with no writes.
// ---------------------------------------------------------------------------

/// `Display::fmt -> Ok(Default::default())` mutant short-circuits without
/// writing to the formatter. Asserting the formatted string is the
/// full 64-char hex digest kills it (the mutant produces an empty string).
#[test]
fn display_renders_full_hex_not_default() {
    let d = Digest::compute(b"display-renders-real-hex");
    let s = format!("{d}");
    assert_eq!(
        s.len(),
        64,
        "Display must emit a 64-character lowercase hex digest, got {s:?}"
    );
    assert_eq!(
        s,
        d.to_hex(),
        "Display must render exactly to_hex(); empty/default would be 0 chars"
    );
    // Belt-and-suspenders: the Default::default() mutant produces "" which
    // is asserted-against by the length check, but pin the prefix anyway
    // so a mutant returning a fixed non-empty short string is also killed.
    assert!(
        s.chars().all(|c| c.is_ascii_hexdigit()),
        "Display output must be ASCII hex; got {s:?}"
    );
}

/// `Debug::fmt -> Ok(Default::default())` mutant short-circuits without
/// writing to the formatter. The legitimate impl writes
/// `Digest(<hex>)`; the mutant writes nothing.
#[test]
fn debug_renders_wrapped_hex_not_default() {
    let d = Digest::compute(b"debug-renders-wrapped-hex");
    let s = format!("{d:?}");
    assert!(
        s.starts_with("Digest("),
        "Debug must wrap with `Digest(...)`; got {s:?}"
    );
    assert!(s.ends_with(')'), "Debug must close with `)`; got {s:?}");
    let inner = &s["Digest(".len()..s.len() - 1];
    assert_eq!(
        inner,
        d.to_hex(),
        "Debug inner must be exactly to_hex(); empty/default would fail"
    );
    assert_eq!(
        s.len(),
        "Digest(".len() + 64 + 1,
        "Debug output must be `Digest(` + 64 hex + `)`; got {s:?}"
    );
}
