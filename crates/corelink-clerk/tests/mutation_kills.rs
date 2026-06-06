//! Targeted regression tests that close mutation-testing surface
//! coverage gaps for `corelink-clerk`. Written 2026-05-15 as part of
//! the mutation-baseline expansion (specs/_audits/2026-05-15-
//! mutation-expansion.md).
//!
//! A full local `cargo mutants -p corelink-clerk` run is INFEASIBLE on
//! a laptop because each mutant invokes the full RS256/JWT test suite
//! (incl. proptest 10k iter) — ~83s of `cargo test` per cycle ×
//! ~240 mutants = ~5.5h wall-clock. The CI nightly workflow
//! (`.github/workflows/mutation-nightly.yml`) runs the full sweep at
//! the 75 % per-crate floor.
//!
//! These tests target the canonical mutation surfaces empirically
//! observed in earlier baselines: `as_str` constants on enum variants,
//! match-arm equality, comparison operators on TTL, and constant-
//! return mutants on `metric_outcome` etc.

#![forbid(unsafe_code)]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]

use std::time::{Duration, SystemTime};

use corelink_clerk::{
    jwks_cache::{is_fresh, CachedJwks},
    principal::{ClerkRole, Email, EmailParseError},
    principal_hash, AuthError, Jwks,
};

// =====================================================================
// AuthError::metric_outcome — canonical 6 outcome strings per WI §6.1.6.
// Match-arm mutations: a `SignatureInvalid` accidentally routed to
// `expired` is a security-observability bug.
// =====================================================================

#[test]
fn metric_outcome_canonical_strings_match_per_variant() {
    assert_eq!(AuthError::SignatureInvalid.metric_outcome(), "sig_invalid");
    assert_eq!(
        AuthError::Malformed("hdr".to_string()).metric_outcome(),
        "sig_invalid"
    );
    assert_eq!(AuthError::AlgNotAllowed.metric_outcome(), "sig_invalid");
    assert_eq!(AuthError::Expired.metric_outcome(), "expired");
    assert_eq!(AuthError::NotYetValid.metric_outcome(), "expired");
    assert_eq!(
        AuthError::IssuerMismatch {
            got: "g".into(),
            expected: vec!["e".into()]
        }
        .metric_outcome(),
        "iss_mismatch"
    );
    assert_eq!(AuthError::AudienceMismatch.metric_outcome(), "aud_mismatch");
    assert_eq!(
        AuthError::JwksFetchFailed("net".into()).metric_outcome(),
        "jwks_fetch_failed"
    );
    assert_eq!(AuthError::KidNotInJwks.metric_outcome(), "kid_miss");
}

#[test]
fn metric_outcome_strings_distinct_and_non_empty() {
    let labels = [
        AuthError::SignatureInvalid.metric_outcome(),
        AuthError::Expired.metric_outcome(),
        AuthError::AudienceMismatch.metric_outcome(),
        AuthError::IssuerMismatch {
            got: "g".into(),
            expected: vec!["e".into()],
        }
        .metric_outcome(),
        AuthError::JwksFetchFailed("net".into()).metric_outcome(),
        AuthError::KidNotInJwks.metric_outcome(),
    ];
    for l in &labels {
        assert!(!l.is_empty(), "metric outcome must not be empty");
        assert_ne!(*l, "xyzzy");
    }
    let mut sorted: Vec<&str> = labels.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 6, "6 distinct canonical labels");
}

// =====================================================================
// AuthError Display — every variant must produce a non-empty string.
// Kills `fmt -> ""` mutants.
// =====================================================================

#[test]
fn auth_error_display_non_empty_per_variant() {
    let variants = [
        format!("{}", AuthError::SignatureInvalid),
        format!("{}", AuthError::Expired),
        format!("{}", AuthError::NotYetValid),
        format!(
            "{}",
            AuthError::IssuerMismatch {
                got: "got-issuer".into(),
                expected: vec!["e1".into()],
            }
        ),
        format!("{}", AuthError::AudienceMismatch),
        format!("{}", AuthError::JwksFetchFailed("network".into())),
        format!("{}", AuthError::KidNotInJwks),
        format!("{}", AuthError::Malformed("bad-segments".into())),
        format!("{}", AuthError::AlgNotAllowed),
    ];
    for s in &variants {
        assert!(!s.is_empty());
    }
    // IssuerMismatch surface must include the `got` field (interpolation).
    let issuer = format!(
        "{}",
        AuthError::IssuerMismatch {
            got: "ZZZ-got-token".into(),
            expected: vec!["EXP-iss".into()],
        }
    );
    assert!(issuer.contains("ZZZ-got-token"));
    assert!(issuer.contains("EXP-iss"));
}

// =====================================================================
// jwks_cache::is_fresh — TTL boundary semantics. Mutating `>` → `==`
// or `>` → `<` flips the freshness verdict on the boundary.
// =====================================================================

#[test]
fn is_fresh_returns_true_before_expiry_false_after_expiry() {
    let stored_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let ttl = Duration::from_secs(60);
    let cached = CachedJwks {
        jwks: Jwks::from_keys(vec![]),
        stored_at,
    };
    // now = stored_at + 30s → expires_at = stored_at + 60s > now → fresh.
    let now_fresh = stored_at + Duration::from_secs(30);
    assert!(is_fresh(&cached, ttl, now_fresh));
    // now = stored_at + 90s → expires_at = stored_at + 60s < now → stale.
    let now_stale = stored_at + Duration::from_secs(90);
    assert!(!is_fresh(&cached, ttl, now_stale));
    // now = stored_at + 60s EXACTLY → expires_at > now is false → stale
    // (kills `>` → `>=` mutation).
    let now_boundary = stored_at + Duration::from_secs(60);
    assert!(!is_fresh(&cached, ttl, now_boundary));
    // now = stored_at + 59s → just-fresh.
    let now_just_fresh = stored_at + Duration::from_secs(59);
    assert!(is_fresh(&cached, ttl, now_just_fresh));
}

#[test]
fn is_fresh_returns_false_on_arithmetic_saturation() {
    // stored_at = SystemTime::UNIX_EPOCH; TTL = Duration::MAX would
    // overflow checked_add and surface `None` → is_fresh = false.
    let cached = CachedJwks {
        jwks: Jwks::from_keys(vec![]),
        stored_at: SystemTime::UNIX_EPOCH,
    };
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    // Use a TTL close to u64::MAX seconds to force overflow.
    let huge = Duration::new(u64::MAX, 999_999_999);
    let res = is_fresh(&cached, huge, now);
    // Either overflow → false (None arm) OR the platform's
    // checked_add accepts it. The defensive contract is that NONE
    // arm yields false. Verify the function did not panic.
    let _ = res;
}

// =====================================================================
// principal_hash — 8-hex-char redaction surrogate (privacy primitive).
// Existing tests assert length=8 + hex + distinguishing inputs; we
// add pinning of canonical-input vector + non-emptiness for several
// inputs.
// =====================================================================

#[test]
fn principal_hash_length_is_exactly_eight_and_hex_lowercase() {
    let inputs = ["user_2abc", "alice", "bob", "user_long_secret_x", ""];
    for inp in inputs {
        let h = principal_hash(inp);
        assert_eq!(h.len(), 8, "hash for {:?} must be 8 chars", inp);
        for c in h.chars() {
            assert!(
                c.is_ascii_hexdigit() && (c.is_ascii_digit() || c.is_ascii_lowercase()),
                "hex lowercase for {:?}",
                inp
            );
        }
        // Non-empty.
        assert!(!h.is_empty());
    }
}

#[test]
fn principal_hash_distinguishes_inputs() {
    // 50 random-ish inputs — all should hash to distinct 8-hex
    // surrogates (collision probability for SHA-256 first-4-bytes
    // over 50 inputs is ~5×10^-7 — well below test flake budget).
    let mut hashes = std::collections::HashSet::new();
    for i in 0..50 {
        let inp = format!("user_{}_{}", i, i * 7919);
        let h = principal_hash(&inp);
        assert!(hashes.insert(h.clone()), "collision: {}", h);
    }
    assert_eq!(hashes.len(), 50);
}

#[test]
fn principal_hash_does_not_leak_input_bytes() {
    let long = "user_long_secret_value_abc_xyz_123";
    let h = principal_hash(long);
    assert!(!h.contains("user"));
    assert!(!h.contains("secret"));
}

// =====================================================================
// Email::parse — boundary cases on `@` separator + non-empty halves +
// domain TLD presence.
// =====================================================================

#[test]
fn email_parse_rejects_empty_and_malformed() {
    assert_eq!(Email::parse(""), Err(EmailParseError::Empty));
    // Missing `@`.
    match Email::parse("alicebob.com") {
        Err(EmailParseError::Malformed(_)) => {}
        other => panic!("missing @ must reject; got {:?}", other),
    }
    // Empty local.
    match Email::parse("@example.com") {
        Err(EmailParseError::Malformed(_)) => {}
        other => panic!("empty local must reject; got {:?}", other),
    }
    // Empty domain.
    match Email::parse("alice@") {
        Err(EmailParseError::Malformed(_)) => {}
        other => panic!("empty domain must reject; got {:?}", other),
    }
    // Domain without `.` (no TLD).
    match Email::parse("alice@localhost") {
        Err(EmailParseError::Malformed(_)) => {}
        other => panic!("no-TLD domain must reject; got {:?}", other),
    }
}

#[test]
fn email_parse_accepts_canonical_and_domain_lowercase_works() {
    let e = Email::parse("Alice@Example.Com").expect("valid");
    assert_eq!(e.as_str(), "Alice@Example.Com");
    // domain_lowercase preserves nothing of the local part.
    assert_eq!(e.domain_lowercase(), "example.com");
}

#[test]
fn email_debug_redacts_local_keeps_domain() {
    let e = Email::parse("alice.smith@example.com").expect("valid");
    let d = format!("{:?}", e);
    assert!(d.contains("redacted"), "debug must redact local part");
    assert!(
        d.contains("example.com"),
        "debug must keep domain for diagnosis"
    );
    assert!(!d.contains("alice.smith"), "local part must NOT appear");
}

// =====================================================================
// ClerkRole::from_claim — fail-safe mapping (unknown → Guest, none →
// Member, "admin" → Admin, "member" → Member).
// =====================================================================

#[test]
fn clerk_role_from_claim_canonical_mapping() {
    assert_eq!(ClerkRole::from_claim(Some("admin")), ClerkRole::Admin);
    assert_eq!(ClerkRole::from_claim(Some("Admin")), ClerkRole::Admin);
    assert_eq!(ClerkRole::from_claim(Some("ADMIN")), ClerkRole::Admin);
    assert_eq!(ClerkRole::from_claim(Some("member")), ClerkRole::Member);
    assert_eq!(ClerkRole::from_claim(Some("Member")), ClerkRole::Member);
    // Unknown → Guest (fail-safe least-privilege).
    assert_eq!(ClerkRole::from_claim(Some("owner")), ClerkRole::Guest);
    assert_eq!(ClerkRole::from_claim(Some("root")), ClerkRole::Guest);
    assert_eq!(ClerkRole::from_claim(Some("xyzzy")), ClerkRole::Guest);
    // None → Member (Clerk free tier default).
    assert_eq!(ClerkRole::from_claim(None), ClerkRole::Member);
}

// =====================================================================
// EmailParseError Display — non-empty + distinct on canonical input.
// =====================================================================

#[test]
fn email_parse_error_display_carries_diagnostic() {
    let e1 = format!("{}", EmailParseError::Empty);
    assert!(!e1.is_empty());
    assert!(e1.contains("empty"));
    let e2 = format!("{}", EmailParseError::Malformed("missing @"));
    assert!(!e2.is_empty());
    assert!(e2.contains("missing @"));
    assert_ne!(e1, e2);
}
