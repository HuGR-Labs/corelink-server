//! `corelink-signup` correlation id canonical newtype.
//!
//! Per PAT-CORRELATION-ID-001 + spec contract S-19 §5.1 + WI §9.5: a
//! single opaque token threads the Clerk webhook event id through the D1
//! transaction, Stripe customer metadata, first PAT audit emit, and any
//! downstream CAS PUT correlation header. Debug + audit chain integrity
//! depend on this propagating unmodified through every arm of the
//! orchestrator.

/// Correlation id newtype carrying a Clerk webhook event id (or any
/// caller-supplied opaque string). The newtype prevents accidental
/// stringly-typed mixing with `IdempotencyKey` / `TenantId` / etc.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CorrelationId(String);

impl CorrelationId {
    /// Construct a new correlation id from any string-like value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the underlying string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for CorrelationId {
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
    fn round_trip_str() {
        let c = CorrelationId::new("evt_abc123");
        assert_eq!(c.as_str(), "evt_abc123");
        assert_eq!(c.to_string(), "evt_abc123");
    }

    #[test]
    fn correlation_id_equality() {
        assert_eq!(CorrelationId::new("a"), CorrelationId::new("a"));
        assert_ne!(CorrelationId::new("a"), CorrelationId::new("b"));
    }
}
