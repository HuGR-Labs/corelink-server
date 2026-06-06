//! `StripeWebhookHandler` trait + `InMemoryStripeWebhookHandler`
//! orchestrator.
//!
//! The orchestrator wires the four primitives — [`crate::signature`]
//! HMAC-SHA256 verification + 5-min replay window enforcement,
//! [`crate::audit`] fail-CLOSED envelope, [`crate::webhook_log`] event
//! log dedup, and the typed [`crate::event::WebhookEvent`] payload —
//! into a single dispatch pipeline that satisfies every load-bearing
//! invariant the production CF Worker route handler will depend on (per
//! the `trait-abstraction-defer` charter pattern).
//!
//! ## Dispatch pipeline (per webhook delivery)
//!
//! For each input `(payload, header, webhook_secret, now_ms,
//! WebhookEvent)`:
//!
//! 1. **Audit `webhook_received`** BEFORE any signature verification (the
//!    arrival watermark is recorded regardless of signature outcome so
//!    SEV-3 monitor can detect unexpected delivery spikes).
//! 2. **Signature verify** via `verify_stripe_signature`. On reject:
//!    emit `signature_rejected` audit; return decision arm.
//! 3. **Skew check** is folded into `verify_stripe_signature`. On
//!    skew-reject: emit `signature_skew_rejected` audit; return error.
//! 4. **Audit `signature_verified`** BEFORE the dispatch state mutation.
//! 5. **Webhook event log INSERT** (idempotent on `stripe_event_id`
//!    UNIQUE). Already-exists arm short-circuits without re-dispatch.
//! 6. **Return** `WebhookProcessed` decision.
//!
//! ## Audit-fail-CLOSED at the trait surface
//!
//! Per WI-S10-003 §6.1 + sprint contract, every decision arm fires its
//! canonical audit BEFORE the state-mutating step. Audit failure on any
//! arm aborts the orchestrator + propagates `StripeError::Audit`
//! (caller sees no state mutation past the point of the failure).
//!
//! ## F-001 closure
//!
//! The orchestrator holds the audit sink + webhook log as `Arc` handles
//! passed at construction; per-instance state lives inside those Arc'd
//! primitives. Tests instantiate fresh orchestrators per case so
//! cross-test contamination is structurally impossible.

use std::sync::Arc;

use crate::audit::{StripeAuditEventType, StripeAuditRecord, StripeAuditSink};
use crate::error::StripeError;
use crate::event::{StripeAdapterDecision, WebhookEvent};
use crate::signature::verify_stripe_signature;
use crate::webhook_log::{StripeWebhookLog, WebhookInsertOutcome};

/// Stripe webhook handler trait. Production wiring composes:
///
/// - `WorkerStripeWebhookHandler` — Cloudflare Worker `fetch` route at
///   `POST /v1/webhooks/stripe`; reads the canonical `Stripe-Signature`
///   header + raw body bytes; deferred to WI-S10-007 PRR ship gate per
///   `trait-abstraction-defer` charter pattern.
pub trait StripeWebhookHandler: Send + Sync + core::fmt::Debug {
    /// Handle a Stripe webhook delivery.
    ///
    /// # Errors
    ///
    /// - [`StripeError::SignatureRejected`] when HMAC-SHA256 verify
    ///   fails / header malformed.
    /// - [`StripeError::SignatureSkewRejected`] when timestamp delta
    ///   exceeds 5-min canonical replay window.
    /// - [`StripeError::Audit`] when the audit envelope rejects any
    ///   decision arm (fail-CLOSED at the trait surface).
    /// - [`StripeError::WebhookLog`] when the webhook event log
    ///   rejects the INSERT.
    fn handle(
        &self,
        request: WebhookHandleRequest<'_>,
    ) -> Result<StripeAdapterDecision, StripeError>;
}

/// Canonical webhook-handle request shape. Mirrors the production CF
/// Worker route input; the trait surface accepts the typed shape so
/// adversarial tests can inject malformed payloads / skewed timestamps
/// without an HTTP layer.
#[derive(Debug)]
pub struct WebhookHandleRequest<'a> {
    /// Canonical `Stripe-Signature` header value
    /// (`t=<unix_seconds>,v1=<hex>`).
    pub signature_header: &'a str,
    /// Raw webhook payload bytes (the unparsed JSON body Stripe POSTed).
    pub payload: &'a [u8],
    /// Webhook secret (`whsec_*`) provisioned at the Stripe dashboard
    /// + stored as a Worker secret per WI-S10-003 §6.1 R-S10-7.
    pub webhook_secret: &'a [u8],
    /// Receiver-side wall-clock instant (Unix epoch ms; canonical
    /// `_ms` suffix). Production wiring reads `Date::now()` at request
    /// entry; the trait surface accepts the typed `u64` so adversarial
    /// tests can pin specific skew scenarios.
    pub now_ms: u64,
    /// Typed Stripe webhook event (the production wiring deserializes
    /// the canonical Stripe payload into this typed shape with PII
    /// wrappers per WI-S10-003 §6.1 invariant 6 / Lote 10.9-quinquies
    /// NEW-P0-2 absorption).
    pub event: WebhookEvent,
}

/// In-memory Stripe webhook handler orchestrator. Composes the audit
/// sink + the webhook log via `Arc` handles; both are generic over the
/// trait so test fakes (e.g. [`crate::audit::FailingStripeAuditSink`] +
/// [`crate::webhook_log::FailingStripeWebhookLog`]) compose directly at
/// construction.
#[derive(Clone, Debug)]
pub struct InMemoryStripeWebhookHandler<A, W>
where
    A: StripeAuditSink + 'static,
    W: StripeWebhookLog + 'static,
{
    audit: Arc<A>,
    log: Arc<W>,
}

impl<A, W> InMemoryStripeWebhookHandler<A, W>
where
    A: StripeAuditSink + 'static,
    W: StripeWebhookLog + 'static,
{
    /// Construct with explicit audit + webhook log handles.
    pub fn new(audit: Arc<A>, log: Arc<W>) -> Self {
        Self { audit, log }
    }

    /// Borrow the audit sink (for tests + production observability).
    #[must_use]
    pub fn audit(&self) -> &Arc<A> {
        &self.audit
    }

    /// Borrow the webhook log.
    #[must_use]
    pub fn log(&self) -> &Arc<W> {
        &self.log
    }

    fn audit_record(
        event_type: StripeAuditEventType,
        now_ms: u64,
        context: String,
    ) -> StripeAuditRecord {
        StripeAuditRecord {
            event_type,
            tenant_id: None,
            now_ms,
            context,
        }
    }
}

impl<A, W> StripeWebhookHandler for InMemoryStripeWebhookHandler<A, W>
where
    A: StripeAuditSink + 'static,
    W: StripeWebhookLog + 'static,
{
    fn handle(
        &self,
        request: WebhookHandleRequest<'_>,
    ) -> Result<StripeAdapterDecision, StripeError> {
        // Audit `webhook_received` BEFORE any signature verification:
        // the arrival watermark is recorded regardless of signature
        // outcome so SEV-3 dashboards can detect unexpected delivery
        // spikes (potential webhook flooding / DDoS signal).
        let received_rec = Self::audit_record(
            StripeAuditEventType::WebhookReceived,
            request.now_ms,
            format!(
                "webhook delivery received stripe_event_id={} kind={}",
                request.event.stripe_event_id, request.event.kind
            ),
        );
        self.audit.emit(received_rec)?;

        // Signature verification + 5-min skew enforcement folded into
        // a single canonical primitive.
        match verify_stripe_signature(
            request.signature_header,
            request.payload,
            request.webhook_secret,
            request.now_ms,
        ) {
            Ok(()) => {}
            Err(StripeError::SignatureRejected(reason)) => {
                let rejected_rec = Self::audit_record(
                    StripeAuditEventType::SignatureRejected,
                    request.now_ms,
                    format!(
                        "signature rejected: {reason}; stripe_event_id={}",
                        request.event.stripe_event_id
                    ),
                );
                self.audit.emit(rejected_rec)?;
                return Err(StripeError::SignatureRejected(reason));
            }
            Err(StripeError::SignatureSkewRejected {
                now_ms,
                signature_ts_ms,
                delta_ms,
            }) => {
                let skew_rec = Self::audit_record(
                    StripeAuditEventType::SignatureSkewRejected,
                    request.now_ms,
                    format!(
                        "signature timestamp skew exceeded 5-min tolerance: delta_ms={delta_ms}; stripe_event_id={}",
                        request.event.stripe_event_id
                    ),
                );
                self.audit.emit(skew_rec)?;
                return Err(StripeError::SignatureSkewRejected {
                    now_ms,
                    signature_ts_ms,
                    delta_ms,
                });
            }
            Err(other) => return Err(other),
        }

        // Signature verified + within 5-min replay window. Emit the
        // canonical signature_verified audit BEFORE the webhook log
        // INSERT.
        let verified_rec = Self::audit_record(
            StripeAuditEventType::SignatureVerified,
            request.now_ms,
            format!(
                "signature verified stripe_event_id={} kind={}",
                request.event.stripe_event_id, request.event.kind
            ),
        );
        self.audit.emit(verified_rec)?;

        // Webhook log INSERT (idempotent on stripe_event_id UNIQUE).
        let outcome = self.log.insert(&request.event)?;

        match outcome {
            WebhookInsertOutcome::Inserted | WebhookInsertOutcome::AlreadyExists => {
                // Both arms surface as WebhookProcessed at the
                // decision boundary; the orchestrator already
                // short-circuited the duplicate path via the log
                // outcome (the production dispatch handler can
                // re-query the log to discriminate on the AlreadyExists
                // arm if the downstream dispatch is non-idempotent).
                Ok(StripeAdapterDecision::WebhookProcessed {
                    stripe_event_id: request.event.stripe_event_id,
                    kind: request.event.kind,
                })
            }
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
    use crate::audit::{FailingStripeAuditSink, InMemoryStripeAuditSink};
    use crate::event::WebhookEventKind;
    use crate::signature::compute_signature;
    use crate::webhook_log::{FailingStripeWebhookLog, InMemoryStripeWebhookLog};

    type Handler = InMemoryStripeWebhookHandler<InMemoryStripeAuditSink, InMemoryStripeWebhookLog>;

    fn fresh_handler() -> (
        Handler,
        Arc<InMemoryStripeAuditSink>,
        Arc<InMemoryStripeWebhookLog>,
    ) {
        let audit = Arc::new(InMemoryStripeAuditSink::new());
        let log = Arc::new(InMemoryStripeWebhookLog::new());
        let h = InMemoryStripeWebhookHandler::new(Arc::clone(&audit), Arc::clone(&log));
        (h, audit, log)
    }

    fn build_event(id: &str) -> WebhookEvent {
        WebhookEvent {
            stripe_event_id: id.to_string(),
            kind: WebhookEventKind::InvoicePaid,
            event_ts_ms: 1_700_000_000_000,
            payload_redacted: b"{\"redacted\":true}".to_vec(),
        }
    }

    fn build_valid_header(payload: &[u8], ts_seconds: u64, secret: &[u8]) -> String {
        let tag = compute_signature(secret, ts_seconds, payload).unwrap();
        format!("t={},v1={}", ts_seconds, hex::encode(tag))
    }

    #[test]
    fn handle_accepts_canonical_signature() {
        let (h, audit, log) = fresh_handler();
        let secret = b"whsec_test";
        let payload = b"{\"id\":\"evt_canonical\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts, secret);
        let now_ms = ts.saturating_mul(1000);
        let req = WebhookHandleRequest {
            signature_header: &header,
            payload,
            webhook_secret: secret,
            now_ms,
            event: build_event("evt_canonical"),
        };
        let dec = h.handle(req).unwrap();
        let processed = matches!(dec, StripeAdapterDecision::WebhookProcessed { .. });
        assert!(processed);
        assert_eq!(log.len(), 1);
        assert_eq!(
            audit
                .snapshot_of(StripeAuditEventType::WebhookReceived)
                .len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(StripeAuditEventType::SignatureVerified)
                .len(),
            1
        );
    }

    #[test]
    fn handle_rejects_tampered_signature() {
        let (h, audit, log) = fresh_handler();
        let secret = b"whsec_test";
        let payload = b"{\"id\":\"evt\"}";
        let ts = 1_700_000_000_u64;
        let mut tag = compute_signature(secret, ts, payload).unwrap();
        tag[0] ^= 0xFF;
        let header = format!("t={},v1={}", ts, hex::encode(tag));
        let now_ms = ts.saturating_mul(1000);
        let req = WebhookHandleRequest {
            signature_header: &header,
            payload,
            webhook_secret: secret,
            now_ms,
            event: build_event("evt"),
        };
        let err = h.handle(req).unwrap_err();
        assert!(matches!(err, StripeError::SignatureRejected(_)));
        assert_eq!(log.len(), 0);
        assert_eq!(
            audit
                .snapshot_of(StripeAuditEventType::SignatureRejected)
                .len(),
            1
        );
    }

    #[test]
    fn handle_rejects_skewed_timestamp() {
        let (h, audit, log) = fresh_handler();
        let secret = b"whsec_test";
        let payload = b"{\"id\":\"evt\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts, secret);
        // now is 6 minutes after ts → exceed 5-min window.
        let now_ms = ts.saturating_mul(1000).saturating_add(6 * 60 * 1000);
        let req = WebhookHandleRequest {
            signature_header: &header,
            payload,
            webhook_secret: secret,
            now_ms,
            event: build_event("evt"),
        };
        let err = h.handle(req).unwrap_err();
        assert!(matches!(err, StripeError::SignatureSkewRejected { .. }));
        assert_eq!(log.len(), 0);
        assert_eq!(
            audit
                .snapshot_of(StripeAuditEventType::SignatureSkewRejected)
                .len(),
            1
        );
    }

    #[test]
    fn handle_audits_received_before_any_verify() {
        let (h, audit, _log) = fresh_handler();
        let secret = b"whsec_test";
        let payload = b"";
        let header = "t=garbage,v1=ab";
        let req = WebhookHandleRequest {
            signature_header: header,
            payload,
            webhook_secret: secret,
            now_ms: 0,
            event: build_event("evt"),
        };
        let _ = h.handle(req).unwrap_err();
        assert_eq!(
            audit
                .snapshot_of(StripeAuditEventType::WebhookReceived)
                .len(),
            1
        );
        assert_eq!(
            audit
                .snapshot_of(StripeAuditEventType::SignatureRejected)
                .len(),
            1
        );
    }

    #[test]
    fn handle_idempotent_on_stripe_event_id_redelivery() {
        let (h, _audit, log) = fresh_handler();
        let secret = b"whsec_test";
        let payload = b"{\"id\":\"evt_dup\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts, secret);
        let now_ms = ts.saturating_mul(1000);
        let event = build_event("evt_dup");
        let req1 = WebhookHandleRequest {
            signature_header: &header,
            payload,
            webhook_secret: secret,
            now_ms,
            event: event.clone(),
        };
        let req2 = WebhookHandleRequest {
            signature_header: &header,
            payload,
            webhook_secret: secret,
            now_ms,
            event,
        };
        let _ = h.handle(req1).unwrap();
        let _ = h.handle(req2).unwrap();
        // Log unchanged after the duplicate delivery.
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn audit_failure_aborts_handle_no_state_mutation() {
        let audit: Arc<FailingStripeAuditSink> = Arc::new(FailingStripeAuditSink::new());
        let log: Arc<InMemoryStripeWebhookLog> = Arc::new(InMemoryStripeWebhookLog::new());
        let h = InMemoryStripeWebhookHandler::new(Arc::clone(&audit), Arc::clone(&log));
        let secret = b"whsec_test";
        let payload = b"{\"id\":\"evt\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts, secret);
        let now_ms = ts.saturating_mul(1000);
        let req = WebhookHandleRequest {
            signature_header: &header,
            payload,
            webhook_secret: secret,
            now_ms,
            event: build_event("evt"),
        };
        let err = h.handle(req).unwrap_err();
        assert!(matches!(err, StripeError::Audit(_)));
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn webhook_log_failure_propagates() {
        let audit: Arc<InMemoryStripeAuditSink> = Arc::new(InMemoryStripeAuditSink::new());
        let log: Arc<FailingStripeWebhookLog> = Arc::new(FailingStripeWebhookLog::new());
        let h = InMemoryStripeWebhookHandler::new(Arc::clone(&audit), Arc::clone(&log));
        let secret = b"whsec_test";
        let payload = b"{\"id\":\"evt\"}";
        let ts = 1_700_000_000_u64;
        let header = build_valid_header(payload, ts, secret);
        let now_ms = ts.saturating_mul(1000);
        let req = WebhookHandleRequest {
            signature_header: &header,
            payload,
            webhook_secret: secret,
            now_ms,
            event: build_event("evt"),
        };
        let err = h.handle(req).unwrap_err();
        assert!(matches!(err, StripeError::WebhookLog(_)));
        // Audit verified arm fired BEFORE the log failure.
        assert_eq!(
            audit
                .snapshot_of(StripeAuditEventType::SignatureVerified)
                .len(),
            1
        );
    }
}
