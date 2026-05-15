//! Stripe Customer Portal session creation — `BillingPortalSessionCreator`
//! trait abstraction + in-memory fake.
//!
//! # Why a separate trait (not `StripeClient::create_portal_session`)?
//!
//! Per the `trait-abstraction-defer` charter: portal sessions are a
//! distinct capability surface from the
//! Checkout / Subscription / Customer plumbing that
//! [`corelink_tier_selection::stripe::StripeClient`] already abstracts.
//! Tier mutation drives Checkout; tenant self-service drives the
//! Customer Portal. Separating the trait keeps fail-CLOSED scope tight:
//! a portal outage MUST NOT take down checkout, and vice-versa.
//!
//! # Audit fail-CLOSED contract (INV-BILLING-PORTAL-AUDIT)
//!
//! Implementations MUST emit
//! `corelink.billing.portal_session_created` to their bound
//! audit sink **before** returning a [`PortalSessionUrl`]. If the audit
//! sink rejects, the session URL is dropped and
//! [`PortalSessionError::AuditFailed`] is returned. The Stripe-side
//! session, if already created, becomes orphaned-but-harmless:
//! single-use, time-bounded, and tied to a customer that itself is
//! authn-protected behind Clerk on return.
//!
//! # URL contract (INV-BILLING-PORTAL-URL-SINGLE-USE)
//!
//! - Every successful call returns a **unique** [`PortalSessionUrl`]
//!   even when the `(customer_id, return_url)` tuple repeats. Stripe
//!   itself enforces single-use semantics; the fake mirrors that so
//!   property tests catch any caller-side caching mistake.
//! - The URL MUST be HTTPS.
//! - The URL MUST embed an opaque session id (so the URL itself
//!   functions as the bearer credential — leaking it is equivalent to
//!   leaking a one-shot login link).
//!
//! # Time-bounded
//!
//! Stripe's hosted session URLs expire after **5 minutes** by default
//! (verified empirically and documented at
//! <https://stripe.com/docs/api/customer_portal/sessions>). Callers
//! MUST redirect promptly; do NOT cache or email the URL.

use core::fmt;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stripe Customer Portal session URL — newtype to prevent accidental
/// string-typed swap with checkout/invoice URLs.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PortalSessionUrl(String);

impl PortalSessionUrl {
    /// Construct a [`PortalSessionUrl`] after light validation.
    ///
    /// # Errors
    ///
    /// Returns [`PortalSessionError::InvalidUrl`] if the URL does not
    /// start with `https://`.
    pub fn new(s: impl Into<String>) -> Result<Self, PortalSessionError> {
        let s = s.into();
        if !s.starts_with("https://") {
            return Err(PortalSessionError::InvalidUrl(
                "portal URL must be HTTPS".to_string(),
            ));
        }
        Ok(Self(s))
    }

    /// Borrow the underlying URL.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the wrapper and return the owned URL.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Debug for PortalSessionUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // URL is bearer-equivalent. Redact in Debug to avoid leaking
        // via tracing / panic output.
        f.debug_tuple("PortalSessionUrl")
            .field(&"<redacted>")
            .finish()
    }
}

/// Errors returned by [`BillingPortalSessionCreator::create_session`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PortalSessionError {
    /// Stripe API rejected the request (mapped from `StripeError`).
    #[error("stripe API error: {0}")]
    Stripe(String),

    /// Audit emission failed; per fail-CLOSED contract the URL is
    /// dropped before this error is surfaced.
    #[error("audit emission failed: {0}")]
    AuditFailed(String),

    /// The customer id was empty or malformed.
    #[error("invalid customer id: {0}")]
    InvalidCustomerId(String),

    /// The return URL was empty or not HTTPS.
    #[error("invalid return URL: {0}")]
    InvalidReturnUrl(String),

    /// The portal URL returned by Stripe failed validation.
    #[error("invalid portal URL: {0}")]
    InvalidUrl(String),
}

/// Minimal audit-sink contract for portal creation events.
///
/// Decoupled from `corelink-audit` to keep this crate's dep graph
/// small; production callers wire an adapter that forwards to the
/// canonical audit ledger.
pub trait PortalAuditSink: fmt::Debug + Send + Sync {
    /// Emit a `corelink.billing.portal_session_created` event.
    ///
    /// MUST return `Err` on durable failure; the creator will surface
    /// [`PortalSessionError::AuditFailed`] and drop the session URL.
    ///
    /// # Errors
    ///
    /// Returns an opaque string on failure. The string is propagated
    /// verbatim into `PortalSessionError::AuditFailed`; callers MUST
    /// NOT include secrets.
    fn emit_portal_session_created(
        &self,
        event: &PortalAuditEvent<'_>,
    ) -> Result<(), String>;
}

/// Audit-event payload for portal session creation.
#[derive(Debug)]
#[non_exhaustive]
pub struct PortalAuditEvent<'a> {
    /// `cus_...` id the session was opened for.
    pub customer_id: &'a str,
    /// Clerk-bound tenant id (multi-tenant isolation).
    pub tenant_id: &'a str,
    /// Where Stripe will send the customer when they close the portal.
    pub return_url: &'a str,
    /// Unix-epoch seconds when the session was issued.
    pub issued_at_unix: u64,
}

/// Stripe Customer Portal session-creation trait.
///
/// Implementations MUST be fail-CLOSED on transport AND on audit
/// emission: every successful return value implies the audit row has
/// been written.
pub trait BillingPortalSessionCreator: fmt::Debug + Send + Sync {
    /// Create a Customer Portal session for `customer_id`. The user
    /// will be redirected to `return_url` when they close the portal.
    ///
    /// # Errors
    ///
    /// See [`PortalSessionError`] taxonomy.
    fn create_session(
        &self,
        customer_id: &str,
        return_url: &str,
        tenant_id: &str,
    ) -> Result<PortalSessionUrl, PortalSessionError>;
}

/// In-memory fake — for unit + property tests.
///
/// Mirrors Stripe-side single-use + uniqueness semantics so callers
/// that accidentally cache URLs will be caught by `assert_ne!`.
pub struct InMemoryPortalSessionCreator {
    audit: Arc<dyn PortalAuditSink>,
    next_seq: Arc<Mutex<u64>>,
    fail_next_stripe: Arc<Mutex<bool>>,
    fail_next_audit: Arc<Mutex<bool>>,
    issued: Arc<Mutex<Vec<PortalSessionUrl>>>,
}

impl fmt::Debug for InMemoryPortalSessionCreator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryPortalSessionCreator")
            .field("audit", &self.audit)
            .finish()
    }
}

impl InMemoryPortalSessionCreator {
    /// Construct a fake bound to `audit`.
    #[must_use]
    pub fn new(audit: Arc<dyn PortalAuditSink>) -> Self {
        Self {
            audit,
            next_seq: Arc::new(Mutex::new(0)),
            fail_next_stripe: Arc::new(Mutex::new(false)),
            fail_next_audit: Arc::new(Mutex::new(false)),
            issued: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Adversarial: cause the next `create_session` to fail with a
    /// simulated Stripe API error.
    pub fn arm_stripe_failure(&self) {
        if let Ok(mut g) = self.fail_next_stripe.lock() {
            *g = true;
        }
    }

    /// Adversarial: cause the next `create_session` to fail in the
    /// audit phase — exercises the fail-CLOSED contract.
    pub fn arm_audit_failure(&self) {
        if let Ok(mut g) = self.fail_next_audit.lock() {
            *g = true;
        }
    }

    /// Snapshot of every URL ever issued (in insertion order).
    #[must_use]
    pub fn issued(&self) -> Vec<PortalSessionUrl> {
        match self.issued.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }
}

impl BillingPortalSessionCreator for InMemoryPortalSessionCreator {
    fn create_session(
        &self,
        customer_id: &str,
        return_url: &str,
        tenant_id: &str,
    ) -> Result<PortalSessionUrl, PortalSessionError> {
        if customer_id.is_empty() || !customer_id.starts_with("cus_") {
            return Err(PortalSessionError::InvalidCustomerId(
                customer_id.to_string(),
            ));
        }
        if !return_url.starts_with("https://") {
            return Err(PortalSessionError::InvalidReturnUrl(
                return_url.to_string(),
            ));
        }
        if tenant_id.is_empty() {
            return Err(PortalSessionError::InvalidCustomerId(
                "missing tenant_id".to_string(),
            ));
        }

        // Adversarial: simulated transport failure.
        let stripe_armed = match self.fail_next_stripe.lock() {
            Ok(mut g) => {
                let v = *g;
                *g = false;
                v
            }
            Err(_) => false,
        };
        if stripe_armed {
            return Err(PortalSessionError::Stripe(
                "in-memory fake: armed transport failure".to_string(),
            ));
        }

        // Generate a unique, opaque session id. Uniqueness MUST hold
        // across calls — Stripe's portal URLs are single-use and a
        // collision would imply the cache deduped illegally.
        let seq = {
            let mut g = self.next_seq.lock().map_err(|e| {
                PortalSessionError::Stripe(format!("seq mutex poisoned: {e}"))
            })?;
            *g = g.saturating_add(1);
            *g
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Nanosecond entropy mixed into the id so even a clock that
        // doesn't advance produces unique URLs.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let session_id = format!("bps_fake_{seq:016x}_{nanos:08x}");
        let url_str = format!("https://billing.stripe.com/p/session/{session_id}");
        let url = PortalSessionUrl::new(url_str)?;

        // Audit fail-CLOSED — emit BEFORE returning the URL. If this
        // fails, drop the URL on the floor.
        let audit_armed = match self.fail_next_audit.lock() {
            Ok(mut g) => {
                let v = *g;
                *g = false;
                v
            }
            Err(_) => false,
        };
        if audit_armed {
            return Err(PortalSessionError::AuditFailed(
                "in-memory fake: armed audit failure".to_string(),
            ));
        }
        let event = PortalAuditEvent {
            customer_id,
            tenant_id,
            return_url,
            issued_at_unix: now,
        };
        self.audit
            .emit_portal_session_created(&event)
            .map_err(PortalSessionError::AuditFailed)?;

        // Only after audit success do we record + return.
        if let Ok(mut g) = self.issued.lock() {
            g.push(url.clone());
        }
        Ok(url)
    }
}

/// In-memory audit sink — records every emitted event.
#[derive(Debug, Default)]
pub struct InMemoryPortalAuditSink {
    events: Arc<Mutex<Vec<RecordedPortalEvent>>>,
    fail_next: Arc<Mutex<bool>>,
}

/// Recorded audit event (owned snapshot).
#[derive(Clone, Debug)]
pub struct RecordedPortalEvent {
    /// Customer id from the event.
    pub customer_id: String,
    /// Tenant id from the event.
    pub tenant_id: String,
    /// Return URL from the event.
    pub return_url: String,
    /// Issuance timestamp from the event.
    pub issued_at_unix: u64,
}

impl InMemoryPortalAuditSink {
    /// Construct an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of every event ever recorded.
    #[must_use]
    pub fn events(&self) -> Vec<RecordedPortalEvent> {
        match self.events.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Adversarial: cause the next emit to return `Err`.
    pub fn arm_failure(&self) {
        if let Ok(mut g) = self.fail_next.lock() {
            *g = true;
        }
    }
}

impl PortalAuditSink for InMemoryPortalAuditSink {
    fn emit_portal_session_created(
        &self,
        event: &PortalAuditEvent<'_>,
    ) -> Result<(), String> {
        let armed = match self.fail_next.lock() {
            Ok(mut g) => {
                let v = *g;
                *g = false;
                v
            }
            Err(_) => false,
        };
        if armed {
            return Err("in-memory sink: armed failure".to_string());
        }
        if let Ok(mut g) = self.events.lock() {
            g.push(RecordedPortalEvent {
                customer_id: event.customer_id.to_string(),
                tenant_id: event.tenant_id.to_string(),
                return_url: event.return_url.to_string(),
                issued_at_unix: event.issued_at_unix,
            });
        }
        Ok(())
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

    fn make() -> (Arc<InMemoryPortalAuditSink>, InMemoryPortalSessionCreator) {
        let sink = Arc::new(InMemoryPortalAuditSink::new());
        let creator = InMemoryPortalSessionCreator::new(sink.clone());
        (sink, creator)
    }

    #[test]
    fn happy_path_emits_audit_before_url() {
        let (sink, creator) = make();
        let url = creator
            .create_session("cus_abc", "https://app.corelink.dev/billing", "tenant_acme")
            .unwrap();
        assert!(url.as_str().starts_with("https://billing.stripe.com/p/session/"));
        let events = sink.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].customer_id, "cus_abc");
        assert_eq!(events[0].tenant_id, "tenant_acme");
    }

    #[test]
    fn audit_failure_fails_closed_drops_url() {
        let (sink, creator) = make();
        creator.arm_audit_failure();
        let err = creator
            .create_session("cus_abc", "https://app.corelink.dev/billing", "tenant_acme")
            .unwrap_err();
        assert!(matches!(err, PortalSessionError::AuditFailed(_)));
        // No URL handed out + no event recorded (audit-armed path
        // short-circuits BEFORE the sink emit).
        assert!(sink.events().is_empty());
        assert!(creator.issued().is_empty());
    }

    #[test]
    fn rejects_non_https_return_url() {
        let (_sink, creator) = make();
        let err = creator
            .create_session("cus_abc", "http://app.corelink.dev/billing", "tenant_acme")
            .unwrap_err();
        assert!(matches!(err, PortalSessionError::InvalidReturnUrl(_)));
    }

    #[test]
    fn rejects_malformed_customer_id() {
        let (_sink, creator) = make();
        let err = creator
            .create_session("not_a_customer", "https://app.corelink.dev/billing", "tenant_acme")
            .unwrap_err();
        assert!(matches!(err, PortalSessionError::InvalidCustomerId(_)));
    }

    #[test]
    fn debug_redacts_url() {
        let url = PortalSessionUrl::new("https://billing.stripe.com/p/session/bps_secret").unwrap();
        let dbg = format!("{url:?}");
        assert!(dbg.contains("<redacted>"));
        assert!(!dbg.contains("bps_secret"));
    }
}
