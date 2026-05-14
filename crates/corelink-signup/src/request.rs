//! `SignupRequest` — the inbound orchestrator input.
//!
//! Per CTRL-CRED-001 the request carries:
//!
//! - `clerk_event_id` (the Clerk webhook event id; production wiring
//!   verifies the `Clerk-Signature` HMAC before invoking this crate).
//! - `email_hash` (sha256 hex of normalized email; raw email NEVER
//!   reaches this crate).
//! - `locale` (`corelink_locale` cookie value; Lote 10.16 canonical).
//! - `idempotency_key` (HTTP `Idempotency-Key` header).
//! - `correlation_id` (PAT-CORRELATION-ID-001 propagation token; the
//!   Clerk webhook event id doubles as the correlation id in canonical
//!   wiring).
//!
//! The request intentionally has **no PAT field** per CTRL-CRED-001
//! (PAT is issued by the orchestrator, not supplied by the caller).

use crate::correlation::CorrelationId;
use crate::idempotency::IdempotencyKey;
use crate::region::Bcp47Locale;
use crate::tenant::UserEmailHash;

/// Inbound signup request — the orchestrator input. Construct via
/// [`SignupRequest::new`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SignupRequest {
    /// Clerk webhook event id; production wiring verifies signature +
    /// nonce + 5-min timestamp window before invoking the orchestrator.
    pub clerk_event_id: String,
    /// User-email hash (sha256 hex of normalized email).
    pub email_hash: UserEmailHash,
    /// `corelink_locale` cookie value (Lote 10.16 canonical map source).
    pub locale: Bcp47Locale,
    /// HTTP `Idempotency-Key` header value (R-S19-1 enforced).
    pub idempotency_key: IdempotencyKey,
    /// PAT-CORRELATION-ID-001 propagation token (typically the Clerk
    /// event id).
    pub correlation_id: CorrelationId,
}

impl SignupRequest {
    /// Canonical constructor. Every field is required.
    #[must_use]
    pub fn new(
        clerk_event_id: impl Into<String>,
        email_hash: UserEmailHash,
        locale: Bcp47Locale,
        idempotency_key: IdempotencyKey,
        correlation_id: CorrelationId,
    ) -> Self {
        Self {
            clerk_event_id: clerk_event_id.into(),
            email_hash,
            locale,
            idempotency_key,
            correlation_id,
        }
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
    fn request_round_trip() {
        let r = SignupRequest::new(
            "evt_1",
            UserEmailHash::new("deadbeef"),
            Bcp47Locale::new("pt-BR"),
            IdempotencyKey::new("idem-1"),
            CorrelationId::new("evt_1"),
        );
        assert_eq!(r.clerk_event_id, "evt_1");
        assert_eq!(r.email_hash.as_str(), "deadbeef");
        assert_eq!(r.locale.as_str(), "pt-BR");
        assert_eq!(r.idempotency_key.as_str(), "idem-1");
        assert_eq!(r.correlation_id.as_str(), "evt_1");
    }
}
