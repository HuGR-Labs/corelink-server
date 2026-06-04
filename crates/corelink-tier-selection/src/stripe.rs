//! Stripe Checkout Session creation trait + in-memory fake +
//! canonical webhook signature verification.
//!
//! Production HTTPS Stripe API client deferred to PRR ship gate per
//! `trait-abstraction-defer` charter pattern.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::TierError;
use crate::tenant::{StripeCustomerId, TenantId};
use crate::tier::TierKind;

type HmacSha256 = Hmac<Sha256>;

/// Canonical Stripe webhook replay window per Stripe spec:
/// 5 minutes (300_000 ms).
pub const STRIPE_REPLAY_WINDOW_MS: u64 = 300_000;

/// Stripe Checkout Session request (typed payload passed to
/// [`StripeClient::create_checkout_session`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CheckoutSessionRequest {
    /// Tenant on whose behalf we create the session.
    pub tenant_id: TenantId,
    /// Tier the session is for (one of Starter / Team / Pro).
    pub tier: TierKind,
    /// Customer email pre-filled into Checkout (pulled from the
    /// tenant signup payload per WI §6.2).
    pub customer_email: String,
    /// Success redirect URL.
    pub success_url: String,
    /// Cancel redirect URL.
    pub cancel_url: String,
}

impl CheckoutSessionRequest {
    /// Construct a [`CheckoutSessionRequest`]. Downstream crates cannot
    /// brace-init `#[non_exhaustive]` structs across the crate boundary, so
    /// the real tier-select Checkout adapter in `corelink-container` needs
    /// this constructor (mirrors [`CheckoutSessionResponse::new`] and
    /// [`StripeCheckoutSessionCompletedEvent::new`]).
    #[must_use]
    pub fn new(
        tenant_id: TenantId,
        tier: TierKind,
        customer_email: impl Into<String>,
        success_url: impl Into<String>,
        cancel_url: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            tier,
            customer_email: customer_email.into(),
            success_url: success_url.into(),
            cancel_url: cancel_url.into(),
        }
    }
}

/// Stripe Checkout Session response (returned by the fake; production
/// will return the parsed JSON from Stripe API).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CheckoutSessionResponse {
    /// Stripe-assigned session id (e.g. `cs_test_...`).
    pub session_id: String,
    /// Stripe-assigned customer id (e.g. `cus_...`) — mapped to
    /// `tenant.stripe_customer_id` atomically per WI §6.7.
    pub stripe_customer_id: StripeCustomerId,
    /// URL the caller redirects the user to.
    pub url: String,
}

impl CheckoutSessionResponse {
    /// Construct a [`CheckoutSessionResponse`] (downstream crates
    /// cannot brace-init `#[non_exhaustive]` structs across the crate
    /// boundary; R2-1 real Stripe client needs this).
    #[must_use]
    pub fn new(
        session_id: impl Into<String>,
        stripe_customer_id: StripeCustomerId,
        url: impl Into<String>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            stripe_customer_id,
            url: url.into(),
        }
    }
}

/// `checkout.session.completed` event payload (subset; production
/// will parse the full Stripe event envelope).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StripeCheckoutSessionCompletedEvent {
    /// Stripe event id (e.g. `evt_...`). Used for idempotency
    /// dedup per WI §6.3.
    pub event_id: String,
    /// Session id this event refers to.
    pub session_id: String,
    /// Tenant id pulled from `metadata.tenant_id` (set by us when
    /// the session was created).
    pub tenant_id: TenantId,
    /// Tier pulled from `metadata.tier`.
    pub tier: TierKind,
    /// Stripe customer id.
    pub stripe_customer_id: StripeCustomerId,
    /// Event timestamp (ms since epoch).
    pub ts_ms: u64,
}

impl StripeCheckoutSessionCompletedEvent {
    /// Construct a [`StripeCheckoutSessionCompletedEvent`] (downstream
    /// tests cannot brace-init `#[non_exhaustive]` structs across the
    /// crate boundary).
    #[must_use]
    pub fn new(
        event_id: impl Into<String>,
        session_id: impl Into<String>,
        tenant_id: TenantId,
        tier: TierKind,
        stripe_customer_id: StripeCustomerId,
        ts_ms: u64,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            session_id: session_id.into(),
            tenant_id,
            tier,
            stripe_customer_id,
            ts_ms,
        }
    }
}

/// Stripe API client trait. Implementations MUST be fail-CLOSED: any
/// transport failure returns [`TierError::Stripe`] so the orchestrator
/// can abort without partial state.
pub trait StripeClient: core::fmt::Debug + Send + Sync {
    /// Create a Checkout Session for `req` and return the session
    /// metadata.
    fn create_checkout_session(
        &self,
        req: &CheckoutSessionRequest,
    ) -> Result<CheckoutSessionResponse, TierError>;
}

/// In-memory Stripe fake. Records every created session for property
/// inspection; supports adversarial modes (force transport failure).
#[derive(Clone, Debug, Default)]
pub struct InMemoryStripeClient {
    sessions: Arc<Mutex<HashMap<String, CheckoutSessionResponse>>>,
    next_seq: Arc<Mutex<u64>>,
    fail_next: Arc<Mutex<bool>>,
}

impl InMemoryStripeClient {
    /// Construct an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adversarial mode: cause the next `create_checkout_session`
    /// call to return [`TierError::Stripe`]. Used in chaos tests.
    pub fn arm_failure(&self) {
        if let Ok(mut g) = self.fail_next.lock() {
            *g = true;
        }
    }

    /// Snapshot of every session ever created (in insertion order
    /// by `session_id`).
    #[must_use]
    pub fn sessions(&self) -> Vec<CheckoutSessionResponse> {
        match self.sessions.lock() {
            Ok(g) => g.values().cloned().collect(),
            Err(p) => p.into_inner().values().cloned().collect(),
        }
    }
}

impl StripeClient for InMemoryStripeClient {
    fn create_checkout_session(
        &self,
        req: &CheckoutSessionRequest,
    ) -> Result<CheckoutSessionResponse, TierError> {
        // Adversarial path.
        let armed = {
            match self.fail_next.lock() {
                Ok(mut g) => {
                    let v = *g;
                    *g = false;
                    v
                }
                Err(_) => false,
            }
        };
        if armed {
            return Err(TierError::Stripe(
                "in-memory fake: armed transport failure".to_string(),
            ));
        }

        let seq = {
            let mut g = self
                .next_seq
                .lock()
                .map_err(|e| TierError::Internal(format!("seq mutex poisoned: {e}")))?;
            *g += 1;
            *g
        };

        let session_id = format!("cs_fake_{seq:08x}");
        let stripe_customer_id =
            StripeCustomerId::new(format!("cus_fake_{}", req.tenant_id.as_str()));
        let url = format!("https://checkout.stripe.com/c/pay/{session_id}");

        let resp = CheckoutSessionResponse {
            session_id: session_id.clone(),
            stripe_customer_id,
            url,
        };

        let mut g = self
            .sessions
            .lock()
            .map_err(|e| TierError::Internal(format!("sessions mutex poisoned: {e}")))?;
        g.insert(session_id, resp.clone());
        Ok(resp)
    }
}

/// Compute the canonical Stripe HMAC-SHA256 signature for `payload`
/// at `timestamp_seconds`.
///
/// Per Stripe spec: `signed_payload = "{t}.{payload}"`; signature =
/// HMAC-SHA256(signed_payload, secret); hex-encoded.
#[must_use]
pub fn compute_stripe_signature(secret: &[u8], timestamp_seconds: u64, payload: &[u8]) -> String {
    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => {
            // HMAC accepts any key length; this branch is
            // unreachable in practice but kept fail-CLOSED.
            return String::new();
        }
    };
    mac.update(timestamp_seconds.to_string().as_bytes());
    mac.update(b".");
    mac.update(payload);
    let bytes = mac.finalize().into_bytes();
    hex::encode(bytes)
}

/// Parse a `Stripe-Signature: t=<unix_ts>,v1=<hex>` header.
///
/// Returns `(timestamp_seconds, hex_signature)` on success.
pub fn parse_stripe_signature_header(header: &str) -> Result<(u64, String), TierError> {
    let mut ts: Option<u64> = None;
    let mut v1: Option<String> = None;
    for part in header.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("t=") {
            ts = rest.parse::<u64>().ok();
        } else if let Some(rest) = part.strip_prefix("v1=") {
            v1 = Some(rest.to_string());
        }
    }
    let ts = ts.ok_or_else(|| TierError::InvalidSignature("missing t=".to_string()))?;
    let v1 = v1.ok_or_else(|| TierError::InvalidSignature("missing v1=".to_string()))?;
    Ok((ts, v1))
}

/// Verify a Stripe webhook signature header against `payload` using
/// `secret`. Enforces the canonical 5-min replay window
/// ([`STRIPE_REPLAY_WINDOW_MS`]).
///
/// Constant-time compare via [`subtle::ConstantTimeEq`].
pub fn verify_stripe_signature(
    secret: &[u8],
    header: &str,
    payload: &[u8],
    now_ms: u64,
) -> Result<(), TierError> {
    let (ts_seconds, sig_hex) = parse_stripe_signature_header(header)?;
    // Replay window check (5-min).
    let ts_ms = ts_seconds.saturating_mul(1000);
    let skew = now_ms.saturating_sub(ts_ms);
    if skew > STRIPE_REPLAY_WINDOW_MS {
        return Err(TierError::InvalidSignature(format!(
            "replay window exceeded: skew={skew}ms"
        )));
    }
    // Future-dated check (clock skew the other way).
    if ts_ms > now_ms.saturating_add(STRIPE_REPLAY_WINDOW_MS) {
        return Err(TierError::InvalidSignature(
            "timestamp future-dated".to_string(),
        ));
    }

    let expected = compute_stripe_signature(secret, ts_seconds, payload);
    let provided = hex::decode(&sig_hex)
        .map_err(|e| TierError::InvalidSignature(format!("hex decode: {e}")))?;
    let expected_bytes = hex::decode(&expected)
        .map_err(|e| TierError::InvalidSignature(format!("hex encode: {e}")))?;
    if provided.ct_eq(&expected_bytes).into() {
        Ok(())
    } else {
        Err(TierError::InvalidSignature("HMAC mismatch".to_string()))
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
    fn signature_roundtrip_ok() {
        let secret = b"whsec_test";
        let payload = b"{\"event\":\"checkout.session.completed\"}";
        let ts = 1_700_000_000_u64;
        let sig = compute_stripe_signature(secret, ts, payload);
        let header = format!("t={ts},v1={sig}");
        verify_stripe_signature(secret, &header, payload, ts * 1000).unwrap();
    }

    #[test]
    fn signature_replay_rejected() {
        let secret = b"whsec_test";
        let payload = b"{}";
        let ts = 1_700_000_000_u64;
        let sig = compute_stripe_signature(secret, ts, payload);
        let header = format!("t={ts},v1={sig}");
        // now = ts + 10 min → outside 5-min window.
        let now = (ts + 600) * 1000;
        let err = verify_stripe_signature(secret, &header, payload, now).unwrap_err();
        assert!(matches!(err, TierError::InvalidSignature(_)));
    }

    #[test]
    fn signature_tampered_payload_rejected() {
        let secret = b"whsec_test";
        let payload = b"{}";
        let ts = 1_700_000_000_u64;
        let sig = compute_stripe_signature(secret, ts, payload);
        let header = format!("t={ts},v1={sig}");
        let tampered = b"{\"evil\":true}";
        let err = verify_stripe_signature(secret, &header, tampered, ts * 1000).unwrap_err();
        assert!(matches!(err, TierError::InvalidSignature(_)));
    }

    #[test]
    fn missing_header_components_rejected() {
        let err = parse_stripe_signature_header("v1=abc").unwrap_err();
        assert!(matches!(err, TierError::InvalidSignature(_)));
        let err = parse_stripe_signature_header("t=123").unwrap_err();
        assert!(matches!(err, TierError::InvalidSignature(_)));
    }

    #[test]
    fn stripe_fake_create_session() {
        let s = InMemoryStripeClient::new();
        let req = CheckoutSessionRequest {
            tenant_id: TenantId::new("tenant_1"),
            tier: TierKind::Starter,
            customer_email: "u@example.com".to_string(),
            success_url: "https://app/ok".to_string(),
            cancel_url: "https://app/cancel".to_string(),
        };
        let r = s.create_checkout_session(&req).unwrap();
        assert!(r.session_id.starts_with("cs_fake_"));
        assert_eq!(r.stripe_customer_id.as_str(), "cus_fake_tenant_1");
        assert_eq!(s.sessions().len(), 1);
    }

    #[test]
    fn stripe_fake_armed_failure() {
        let s = InMemoryStripeClient::new();
        s.arm_failure();
        let req = CheckoutSessionRequest {
            tenant_id: TenantId::new("tenant_1"),
            tier: TierKind::Starter,
            customer_email: "u@example.com".to_string(),
            success_url: "https://app/ok".to_string(),
            cancel_url: "https://app/cancel".to_string(),
        };
        let err = s.create_checkout_session(&req).unwrap_err();
        assert!(matches!(err, TierError::Stripe(_)));
    }
}
