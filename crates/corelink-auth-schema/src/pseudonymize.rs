//! Pseudonymisation primitives for the audit chain (S-09 forward).
//!
//! INV-AUTH-AUDIT-PSEUDONYMIZATION (CRITICAL) requires that the audit
//! chain retain identifiers in a form that is:
//!
//! - **Linkable** — two audit rows that mention the same `account_id`
//!   must produce the same pseudonym so investigators can follow a
//!   trail across events.
//! - **Non-reversible without the key** — DSR-erased rows cannot be
//!   re-identified from the audit corpus without access to the
//!   pseudonymisation key.
//!
//! The construction is HMAC-SHA256 with a domain-separated key, a tag
//! truncated to 16 bytes (128 bits — enough collision resistance for
//! the pseudonym alphabet), and hex-encoded for canonical text storage.
//!
//! The actual pseudonymisation key lives in S-09 forward; this module
//! exposes the deterministic primitive so the schema simulator can
//! exercise the cascade-vs-pseudonymisation invariant in property
//! tests today.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use uuid::Uuid;

const PSEUDONYM_LEN: usize = 16;

/// Pseudonymise an account UUID with the supplied key.
///
/// Returns a 32-character hex string (`HMAC-SHA256(key, "account:" ||
/// uuid)[..16]`).
#[must_use]
pub fn pseudonymize_account_id(key: &[u8], account_id: &Uuid) -> String {
    pseudonymize(key, b"account:", account_id)
}

/// Pseudonymise a user UUID with the supplied key.
#[must_use]
pub fn pseudonymize_user_id(key: &[u8], user_id: &Uuid) -> String {
    pseudonymize(key, b"user:", user_id)
}

fn pseudonymize(key: &[u8], domain: &[u8], id: &Uuid) -> String {
    #[allow(
        clippy::expect_used,
        reason = "Hmac<Sha256>::new_from_slice never returns Err for any byte slice"
    )]
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key)
        .expect("HMAC-SHA256 accepts any key length");
    mac.update(domain);
    mac.update(id.as_bytes());
    let tag = mac.finalize().into_bytes();
    // SHA-256 output is 32 bytes; truncate to the canonical 16-byte
    // pseudonym width via `.get(..n)` so `indexing_slicing = "deny"`
    // stays clean.
    let truncated = tag.get(..PSEUDONYM_LEN).unwrap_or(&[]);
    hex::encode(truncated)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn determinism() {
        let key = [0x55_u8; 32];
        let id = Uuid::nil();
        let p1 = pseudonymize_account_id(&key, &id);
        let p2 = pseudonymize_account_id(&key, &id);
        assert_eq!(p1, p2);
        assert_eq!(p1.len(), PSEUDONYM_LEN * 2);
    }

    #[test]
    fn domain_separates_account_vs_user() {
        let key = [0x55_u8; 32];
        let id = Uuid::nil();
        let acc = pseudonymize_account_id(&key, &id);
        let usr = pseudonymize_user_id(&key, &id);
        assert_ne!(acc, usr);
    }

    #[test]
    fn key_changes_pseudonym() {
        let id = Uuid::nil();
        let p1 = pseudonymize_account_id(&[0x01; 32], &id);
        let p2 = pseudonymize_account_id(&[0x02; 32], &id);
        assert_ne!(p1, p2);
    }
}
