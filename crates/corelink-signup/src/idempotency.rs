//! `corelink-signup` idempotency-key canonical newtype.
//!
//! Per spec contract S-19 §5.1 R-S19-1 + WI acceptance Gherkin: HTTP
//! `Idempotency-Key` header is enforced on `POST /v1/signup`; duplicate
//! requests (same key) resolve to the original `signup_id`, regardless
//! of payload variation in non-essential fields. Property test
//! `prop_idempotency_key_same_signup_id` pins this invariant.

/// `Idempotency-Key` header value newtype.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Construct a new idempotency key from any string-like value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True if the key is the empty string (caller-side validation
    /// guard — header MUST NOT be empty).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl core::fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
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
    fn round_trip() {
        let k = IdempotencyKey::new("idem-001");
        assert_eq!(k.as_str(), "idem-001");
        assert!(!k.is_empty());
    }

    #[test]
    fn empty_detected() {
        let k = IdempotencyKey::new("");
        assert!(k.is_empty());
    }
}
