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

use hmac::{Hmac, Mac as _};
use sha2::{Digest as _, Sha256};

/// Env var holding the server-held salt (HMAC key). Unset/empty ⇒ legacy
/// unsalted SHA-256 (pre-salt parity). Owner registers it in the secrets
/// matrix + DO forward-list (see module docs).
const EMAIL_HASH_SALT_ENV: &str = "EMAIL_HASH_SALT";

/// Canonical pseudonymized email hash (CTRL-PRIV-001). See module docs.
///
/// Normalizes (`trim` + `to_lowercase`) then HMAC-SHA256s under
/// `EMAIL_HASH_SALT` if set+non-empty, else falls back to plain SHA-256 — the
/// pre-salt production scheme (no regression). Hex-encoded, lowercase.
#[must_use]
pub fn hash_email(email: &str) -> String {
    let normalized = email.trim().to_lowercase();
    match std::env::var(EMAIL_HASH_SALT_ENV) {
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
        std::env::remove_var(EMAIL_HASH_SALT_ENV);
        Self(g)
    }

    pub(crate) fn set_salt(&self, salt: &str) {
        std::env::set_var(EMAIL_HASH_SALT_ENV, salt);
    }
}

#[cfg(test)]
impl Drop for EnvGuard {
    fn drop(&mut self) {
        std::env::remove_var(EMAIL_HASH_SALT_ENV);
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
