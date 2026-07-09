//! Canonical pseudonymized email-hash helper (CTRL-PRIV-001).
//!
//! ONE scheme for turning an account/invite email into the `email_hash`
//! pseudonym stored in D1. EVERY email-hash site MUST call [`hash_email`] so
//! that the team-invite write, the later accept-time MATCH, and the DSR
//! rectification all compute a **byte-identical** value for the same input —
//! if they diverge, the invite→match flow and Art.16 rectification break.
//!
//! ## Scheme
//! Normalize (`trim` + `to_lowercase`), then:
//! - `EMAIL_HASH_SALT` set, non-empty → `hex(HMAC-SHA256(key = EMAIL_HASH_SALT, msg = normalized))`
//! - `EMAIL_HASH_SALT` unset / empty  → `hex(SHA-256(normalized))`
//!
//! The unsalted branch is **byte-identical to the pre-salt production scheme**,
//! so until the owner registers `EMAIL_HASH_SALT` there is ZERO regression and
//! all existing hashes keep matching. Because both the write and the match read
//! the same env through this one helper, salted/unsalted stays consistent
//! across sites at all times.
//!
//! ## Salting is forward-only (no retro-salt)
//! Existing un-salted hashes CANNOT be retro-salted: the raw email is never
//! persisted (CTRL-PRIV-001), so there is no plaintext to re-hash. Once
//! `EMAIL_HASH_SALT` is set, NEW invites + their accept-matches + rectifications
//! all use the salted scheme consistently (the match key is recomputed on both
//! sides from the same email under the same salt). Any invite written *before*
//! the salt is set and accepted *after* would not match — set the salt during a
//! quiet window. `EMAIL_HASH_SALT` is a **server secret** (the whole point: an
//! attacker without it cannot rainbow-table the pseudonyms).
//!
//! NOTE FOR INTEGRATION: the signup-worker accept side (`emailHashFor`,
//! TypeScript) computes the same `email_hash` to MATCH on. It must read the
//! SAME `EMAIL_HASH_SALT` and apply the same HMAC-SHA256 scheme, or salted
//! invites will never bind. That cross-language parity is an integration
//! follow-up (this module covers only the Rust container sites).

use hmac::{Hmac, KeyInit, Mac as _};
use sha2::{Digest as _, Sha256};

/// Canonical pseudonymized email hash (CTRL-PRIV-001). See module docs.
///
/// Normalizes (`trim` + `to_lowercase`) then HMAC-SHA256s under
/// `EMAIL_HASH_SALT` if set+non-empty, else falls back to plain SHA-256 — the
/// pre-salt production scheme (no regression). Hex-encoded, lowercase.
#[must_use]
pub fn hash_email(email: &str) -> String {
    let normalized = email.trim().to_lowercase();
    match std::env::var("EMAIL_HASH_SALT") {
        Ok(salt) if !salt.is_empty() => {
            // HMAC-SHA256 accepts a key of any length; this never errors (same
            // idiom as `storage::byok_cas::harden_digest`).
            let mut mac = <Hmac<Sha256>>::new_from_slice(salt.as_bytes())
                .unwrap_or_else(|_| unreachable!("HMAC-SHA256 accepts any key length"));
            mac.update(normalized.as_bytes());
            hex::encode(mac.finalize().into_bytes())
        }
        _ => hex::encode(Sha256::digest(normalized.as_bytes())),
    }
}

/// The **legacy, always-unsalted** pseudonymized email hash — `hex(SHA-256(trim +
/// lowercase))`, byte-identical to the pre-salt production scheme regardless of
/// whether `EMAIL_HASH_SALT` is set.
///
/// This is NOT a write helper. It exists solely so a LOOKUP can also match rows
/// that were written *before* the salt was registered: once `EMAIL_HASH_SALT` is
/// set, [`hash_email`] emits the salted pseudonym, but the 123 legacy tenant
/// rows + 5 pending team-invites still carry the unsalted hash. A lookup that
/// only tried the salted candidate would false-negative every one of them
/// (invite-accept fails to bind; DSAR-by-email misses the tenant). See
/// [`email_hash_candidates`].
#[must_use]
pub fn hash_email_legacy(email: &str) -> String {
    let normalized = email.trim().to_lowercase();
    hex::encode(Sha256::digest(normalized.as_bytes()))
}

/// The set of `email_hash` values a LOOKUP for `email` must match against — the
/// salted candidate ([`hash_email`], the current WRITE scheme) plus the legacy
/// unsalted candidate ([`hash_email_legacy`]), **deduplicated**.
///
/// - Salt UNSET → both candidates are byte-identical, so this returns a SINGLE
///   value and dual-read is behaviorally identical to today (zero regression).
/// - Salt SET → returns `[salted, legacy]`, so a lookup finds BOTH a row written
///   under the new salted scheme AND a row written under the pre-salt scheme
///   (no false-negative on the legacy rows).
///
/// WRITES never call this — they stay on [`hash_email`] (salted-if-set). Only
/// the accept-match / DSAR-by-email LOOKUP sites fan out over these candidates.
#[must_use]
pub fn email_hash_candidates(email: &str) -> Vec<String> {
    let salted = hash_email(email);
    let legacy = hash_email_legacy(email);
    if salted == legacy {
        vec![salted]
    } else {
        vec![salted, legacy]
    }
}

// ── Test-only env serialization (crate-visible) ──────────────────────────────
// `EMAIL_HASH_SALT` is process-global, so EVERY test that reads OR mutates it
// (here AND in `customer_d1` / `routes::dsr::access`) must serialize on the one
// lock below — otherwise a salted test bleeds into an unsalted assertion. No
// `serial_test` dependency: a crate-visible module static + RAII guard suffices.

/// The single process-wide lock guarding `EMAIL_HASH_SALT` across all tests.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII guard: acquires [`ENV_LOCK`] (recovering from poisoning so one panicking
/// test can't cascade), forces `EMAIL_HASH_SALT` UNSET on entry, and clears it
/// again on drop — so neither the holding test nor the next observes a stale
/// salt. Use [`EnvGuard::set_salt`] to switch into salted mode mid-test.
#[cfg(test)]
pub(crate) struct EnvGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

#[cfg(test)]
impl EnvGuard {
    pub(crate) fn acquire() -> Self {
        let g = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::remove_var("EMAIL_HASH_SALT");
        Self(g)
    }

    pub(crate) fn set_salt(&self, salt: &str) {
        std::env::set_var("EMAIL_HASH_SALT", salt);
    }
}

#[cfg(test)]
impl Drop for EnvGuard {
    fn drop(&mut self) {
        std::env::remove_var("EMAIL_HASH_SALT");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_unsalted(email: &str) -> String {
        hex::encode(Sha256::digest(email.trim().to_lowercase().as_bytes()))
    }

    #[test]
    fn unset_salt_is_byte_identical_to_legacy_sha256() {
        let _g = EnvGuard::acquire(); // salt UNSET
        // No-regression proof: the helper with no salt == the old inline scheme.
        assert_eq!(hash_email("user@example.com"), legacy_unsalted("user@example.com"));
        assert_eq!(
            hash_email("  Alice@Example.com  "),
            legacy_unsalted("alice@example.com")
        );
    }

    #[test]
    fn set_salt_differs_from_unsalted_and_is_deterministic() {
        let g = EnvGuard::acquire();
        let unsalted = hash_email("user@example.com");
        assert_eq!(unsalted, legacy_unsalted("user@example.com"));

        g.set_salt("server-secret-salt");
        let salted = hash_email("user@example.com");
        // Salted output is NOT the rainbow-attackable unsalted hash...
        assert_ne!(salted, unsalted);
        // ...and is deterministic for a fixed salt (so a match recomputes equal).
        assert_eq!(salted, hash_email("user@example.com"));
        // Sanity: HMAC-SHA256 output is 32 bytes / 64 hex chars.
        assert_eq!(salted.len(), 64);
    }

    #[test]
    fn different_salts_yield_different_hashes() {
        let g = EnvGuard::acquire();
        g.set_salt("salt-A");
        let a = hash_email("user@example.com");
        g.set_salt("salt-B");
        let b = hash_email("user@example.com");
        // Rainbow-resistance: the pseudonym is salt-dependent.
        assert_ne!(a, b);
    }

    #[test]
    fn empty_salt_falls_back_to_unsalted() {
        let g = EnvGuard::acquire();
        g.set_salt(""); // set-but-empty must behave as UNSET (no empty-key HMAC)
        assert_eq!(hash_email("user@example.com"), legacy_unsalted("user@example.com"));
    }

    // ── Dual-read candidate helper (safe EMAIL_HASH_SALT activation) ─────────

    #[test]
    fn legacy_helper_is_always_unsalted_regardless_of_salt() {
        let g = EnvGuard::acquire();
        // Legacy helper == the pre-salt inline scheme with salt UNSET…
        assert_eq!(hash_email_legacy("user@example.com"), legacy_unsalted("user@example.com"));
        // …and STAYS unsalted even after the salt is set (it never reads the env).
        g.set_salt("server-secret-salt");
        assert_eq!(hash_email_legacy("user@example.com"), legacy_unsalted("user@example.com"));
        // Normalizes identically (trim + lowercase).
        assert_eq!(
            hash_email_legacy("  Alice@Example.com  "),
            legacy_unsalted("alice@example.com")
        );
    }

    /// INVARIANT (a): salt UNSET → dual-read collapses to ONE candidate == today.
    #[test]
    fn candidates_unset_salt_is_single_and_equals_legacy() {
        let _g = EnvGuard::acquire(); // salt UNSET
        let c = email_hash_candidates("user@example.com");
        // Unset salt → exactly ONE candidate, byte-identical to today's single hash.
        assert_eq!(c.len(), 1, "unset salt must dedupe to a single candidate");
        assert!(c.contains(&hash_email("user@example.com")));
        assert!(c.contains(&legacy_unsalted("user@example.com")));
    }

    /// INVARIANT (b)+(c): salt SET → candidates contain BOTH the salted write
    /// scheme (finds a SALTED-stored row) AND the legacy unsalted hash (finds a
    /// LEGACY-stored row — the 5-invite / 123-DSAR case, no false-negative).
    #[test]
    fn candidates_set_salt_covers_both_salted_and_legacy() {
        let g = EnvGuard::acquire();
        g.set_salt("server-secret-salt");
        let c = email_hash_candidates("user@example.com");
        assert_eq!(c.len(), 2, "salt set must yield two distinct candidates");
        // (c) a row stored under the current salted WRITE scheme is matched.
        assert!(c.contains(&hash_email("user@example.com")));
        // (b) a row stored under the pre-salt LEGACY scheme is STILL matched.
        assert!(c.contains(&legacy_unsalted("user@example.com")));
    }

    /// INVARIANT (d): distinct emails never collide across the two candidates —
    /// no legacy-vs-salted candidate of email A equals any candidate of email B,
    /// so dual-read can't false-POSITIVE onto a different subject's row.
    #[test]
    fn candidates_do_not_collide_across_different_emails() {
        let g = EnvGuard::acquire();
        g.set_salt("server-secret-salt");
        let a = email_hash_candidates("alice@example.com");
        let b = email_hash_candidates("bob@example.com");
        for x in &a {
            assert!(!b.contains(x), "candidate {x} collided across distinct emails");
        }
    }

    #[test]
    fn candidates_are_normalized_and_deduped() {
        let g = EnvGuard::acquire();
        g.set_salt("salt-X");
        // Same normalized email → identical candidate set regardless of casing.
        assert_eq!(
            email_hash_candidates("  USER@Example.COM  "),
            email_hash_candidates("user@example.com")
        );
        // Empty salt behaves as UNSET → one candidate.
        g.set_salt("");
        assert_eq!(email_hash_candidates("user@example.com").len(), 1);
    }

    #[test]
    fn normalization_is_consistent_in_both_modes() {
        let g = EnvGuard::acquire();
        // Unsalted: trim + case fold to the same value.
        assert_eq!(hash_email("  USER@Example.COM  "), hash_email("user@example.com"));
        // Salted: the same normalization holds.
        g.set_salt("salt-X");
        assert_eq!(hash_email("  USER@Example.COM  "), hash_email("user@example.com"));
    }
}
// The MATCHING INVARIANT (invite-write hash == rectification hash for the same
// email+salt) is proven transitively: each site is asserted equal to this one
// `hash_email` helper in its own module's tests — `customer_d1` (write site,
// `team_invite_inserts_invited_member_row`) and `routes::dsr::access`
// (rectification site, `email_hash_matches_canonical_scheme`) — so both route
// through this single source of truth and therefore equal each other.
