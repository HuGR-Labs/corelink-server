//! Wave-33 Stage 0 sub-step 4 — Audit chokepoint trait surface.
//!
//! Every state-mutating operation in CoreLink MUST emit an audit row
//! through this trait BEFORE returning success to the caller. This
//! module's [`AuditEmitter`] trait is the canonical chokepoint surface
//! Stage 1 streams will consume.
//!
//! ## Why a new trait when [`crate::Emitter`] already exists?
//!
//! [`crate::Emitter`] is `AuthEvent`-specific — its `emit` signature
//! takes [`crate::AuthEvent`] directly. That fits the S-03 auth event
//! family but does NOT generalize to the broader audit surfaces Stage
//! 1 streams need to instrument (billing audit rows, BYOK revocation
//! audit, replication failover audit, DSR erasure audit, etc.).
//!
//! [`AuditEmitter`] is the generic chokepoint that abstracts the
//! audit-fail-CLOSED contract over any [`AuditEvent`]-shaped row. The
//! existing [`crate::Emitter`] trait is preserved unchanged for
//! backwards compatibility; Stage 1 streams MAY adopt [`AuditEmitter`]
//! incrementally where the auth-event-specific surface is too narrow.
//!
//! ## Fail-CLOSED contract (charter-mandated)
//!
//! Per `specs/03_architecture/invariant_registry.md`
//! INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (CRITICAL): a failed emit MUST
//! surface as the operation's error (typically mapped to 503 by the
//! route layer), never as silent success. The `Result` return on
//! [`AuditEmitter::emit`] enforces this at the type level — there is
//! no way for a caller to silently swallow a failed emit without an
//! explicit `.ok()` / `.unwrap_or_default()` / `let _ = emit()`. The
//! charter's "Audit fail-CLOSED preserved" rule forbids those
//! patterns in production code; CI lints + adversarial review enforce.
//!
//! ## What this trait does NOT do
//!
//! - Define a new event taxonomy. The [`AuditEvent`] struct here is a
//!   generic envelope (event_type / tenant_id / at_unix_ms / payload).
//!   Auth-specific events continue to use [`crate::AuthEvent`] +
//!   [`crate::AuthEventType`] through the [`crate::Emitter`] trait.
//! - Re-implement CloudEvents 1.0. The CE envelope already exists in
//!   [`crate::events::AuthEvent`]; this trait is the orthogonal
//!   "any-context-can-emit" surface. Stage 1 streams may wrap a
//!   context-specific row type into [`AuditEvent::payload`] as
//!   `serde_json::Value`, then bridge to the CloudEvents envelope at
//!   the production emitter wiring layer.
//!
//! ## Migration plan (Stage 0 → Stage 1)
//!
//! - Stage 0: trait surface defined here. No consumers updated yet
//!   (per the wave-33 reorg spec sub-step 4 explicit directive: "DO
//!   NOT yet update consumers of the moved types").
//! - Stage 1: each stream's audit-fail-CLOSED handler call site
//!   migrates from its context-local trait (e.g.
//!   `corelink-handler-cas::AuditSink`, `corelink-handler-ac::AuditSink`,
//!   `corelink-handler-admin::AuditSink`,
//!   `corelink-worker::reapi::cas::AuditSink`,
//!   `corelink-worker::reapi::ac::AuditSink`) to
//!   [`AuditEmitter`].
//! - Stage 2 / Stage 3: the existing context-local sinks become
//!   type-aliases for [`AuditEmitter`]; eventually the aliases are
//!   removed once consumer migration is complete.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Generic audit-event envelope used by [`AuditEmitter`].
///
/// Wave-33 Stage 0 intentionally keeps this envelope MINIMAL — just
/// the four fields every audit row currently shares. Context-specific
/// payloads live in [`Self::payload`] as `serde_json::Value`; the
/// production wiring layer transforms this into the CloudEvents 1.0
/// envelope shape ([`crate::events::AuthEvent`]) at row-insert time.
///
/// `#[non_exhaustive]`: future fields (e.g. `request_id`, `region`,
/// `retention_hint`) may be added without breaking call sites that
/// only set the four current fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditEvent {
    /// CloudEvents 1.0 `type` attribute, e.g.
    /// `"corelink.cas.blob.upload.v1"`,
    /// `"corelink.byok.cmk.revoke.v1"`,
    /// `"corelink.billing.tier.upgrade.v1"`.
    ///
    /// Stored as `String` rather than an enum so Stage 1 streams can
    /// land their context-specific event types without round-tripping
    /// every new variant through this central enum (which would
    /// recreate the coupling the modular-monolith refactor is trying
    /// to break). The actual taxonomy is enforced at the production
    /// emitter wiring layer.
    pub event_type: String,

    /// Canonical D1 tenant id (UUIDv7 hyphenated lowercase text form).
    ///
    /// Stage 1 streams MAY adopt `corelink_core::TenantId` here once
    /// the consumer migration completes; for Stage 0 this is a plain
    /// `String` so the trait surface doesn't force every existing
    /// caller to convert at the call site.
    pub tenant_id: String,

    /// Emit timestamp in unix milliseconds. The producer SHOULD use
    /// the canonical `corelink_core::Clock` surface to obtain this
    /// value — production wall-clock or test fake.
    pub at_unix_ms: u64,

    /// Context-specific payload (e.g. blob digest, BYOK CMK ARN,
    /// Stripe customer id). Always JSON-canonical at emit time.
    pub payload: serde_json::Value,
}

impl AuditEvent {
    /// Construct a minimal [`AuditEvent`] from the four required
    /// fields. Convenience constructor — most call sites should use
    /// struct-literal syntax to surface every field at the
    /// audit-emit ordering site.
    #[must_use]
    pub fn new(
        event_type: impl Into<String>,
        tenant_id: impl Into<String>,
        at_unix_ms: u64,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            event_type: event_type.into(),
            tenant_id: tenant_id.into(),
            at_unix_ms,
            payload,
        }
    }
}

/// Errors returned by [`AuditEmitter::emit`].
///
/// Mirrors [`crate::EmitterError`] but at the chokepoint surface so
/// callers that only depend on `corelink_audit::ports::AuditEmitter`
/// don't need to import the auth-event-specific error type.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AuditEmitError {
    /// The backend storage (D1 batch / R2 / KV) refused or failed to
    /// persist the audit row. MUST surface as 503-class to the
    /// customer; the handler's main mutation MUST be rolled back.
    #[error("audit emitter store error: {0}")]
    Store(String),

    /// An internal `Mutex` was poisoned (a panic occurred while
    /// another thread held the lock). Test sinks only — production
    /// emitters should not reach this.
    #[error("audit emitter mutex poisoned (test sink only)")]
    MutexPoisoned,
}

/// The canonical wave-33 audit chokepoint trait.
///
/// Every state-mutating handler in every context crate MUST call
/// [`Self::emit`] BEFORE returning success. A failed emit MUST
/// propagate as the operation's error (typically mapped to 503 by the
/// route layer). The `Result` return type enforces this at compile
/// time: there is no way to silently succeed past an emit failure
/// without explicit code that the charter forbids in production src/
/// (e.g. `.ok()`, `.unwrap_or_default()`, `let _ = emit()`).
///
/// ## Choice of sync over async
///
/// Sync matches [`crate::Emitter`] for the same reason: the
/// no-default-features build path must stay `wasm32-unknown-unknown`
/// clean for CF Worker consumers. Async machinery (tokio /
/// `async-trait`) is platform-gated and lives in the binding crates
/// (`corelink-adapters-cloud`), not in the audit chokepoint trait.
pub trait AuditEmitter: Send + Sync {
    /// Persist `event`. Returns `Ok(())` on durable persistence (in
    /// the production outbox emitter that means the D1 batch
    /// committed; in the SIEM direct emitter that means the webhook
    /// returned 2xx); `Err` on any failure that would leave the
    /// audit chain incomplete.
    ///
    /// # Errors
    ///
    /// Returns [`AuditEmitError::Store`] on any backend failure. The
    /// caller MUST propagate the error rather than swallow it — the
    /// audit-fail-CLOSED contract is the only mechanism backing
    /// INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (CRITICAL).
    fn emit(&self, event: AuditEvent) -> Result<(), AuditEmitError>;
}

/// In-memory test sink for [`AuditEmitter`]. Captures every emitted
/// event in an internal buffer.
///
/// Cloning shares the underlying buffer (`Arc<Mutex<Vec<…>>>`) so
/// multiple call sites can hold separate handles to the same sink.
#[derive(Clone, Debug, Default)]
pub struct InMemoryAuditEmitter {
    events: Arc<Mutex<Vec<AuditEvent>>>,
}

impl InMemoryAuditEmitter {
    /// Construct an empty in-memory audit emitter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Snapshot every event captured so far. Returns a clone of the
    /// underlying buffer; the emitter retains its state.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AuditEvent> {
        match self.events.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Number of events captured so far.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.events.lock() {
            Ok(guard) => guard.len(),
            Err(poisoned) => poisoned.into_inner().len(),
        }
    }

    /// Whether no events have been captured yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AuditEmitter for InMemoryAuditEmitter {
    fn emit(&self, event: AuditEvent) -> Result<(), AuditEmitError> {
        match self.events.lock() {
            Ok(mut guard) => {
                guard.push(event);
                Ok(())
            }
            Err(_) => Err(AuditEmitError::MutexPoisoned),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn audit_event_new_constructs_canonical_envelope() {
        let e = AuditEvent::new(
            "corelink.cas.blob.upload.v1",
            "01938af0-abcd-7123-8456-000000000001",
            1_700_000_000_000,
            serde_json::json!({"digest_hex": "ab".repeat(32)}),
        );
        assert_eq!(e.event_type, "corelink.cas.blob.upload.v1");
        assert_eq!(e.tenant_id, "01938af0-abcd-7123-8456-000000000001");
        assert_eq!(e.at_unix_ms, 1_700_000_000_000);
    }

    #[test]
    fn in_memory_emitter_captures_events() {
        let sink = InMemoryAuditEmitter::new();
        assert!(sink.is_empty());

        let e = AuditEvent::new(
            "corelink.byok.cmk.revoke.v1",
            "01938af0-abcd-7123-8456-000000000002",
            1_700_000_001_000,
            serde_json::json!({}),
        );
        sink.emit(e.clone()).unwrap();
        assert_eq!(sink.len(), 1);
        let snap = sink.snapshot();
        assert_eq!(snap.first(), Some(&e));
    }

    #[test]
    fn in_memory_emitter_clone_shares_buffer() {
        let a = InMemoryAuditEmitter::new();
        let b = a.clone();
        a.emit(AuditEvent::new("t.v1", "tid", 1, serde_json::json!({})))
            .unwrap();
        // Cloned handle sees the same buffer.
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn audit_emit_error_message_contains_detail() {
        let e = AuditEmitError::Store("d1 batch rolled back".into());
        let s = format!("{e}");
        assert!(s.contains("d1 batch rolled back"));
    }

    #[test]
    fn audit_event_round_trips_through_serde_json() {
        let e = AuditEvent::new(
            "corelink.dsr.erasure.completed.v1",
            "tid",
            42,
            serde_json::json!({"k": "v"}),
        );
        let json = serde_json::to_string(&e).unwrap();
        let back: AuditEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }

    /// Compile-time check: `AuditEmitter` is object-safe. The
    /// production wiring layer stores it as `Arc<dyn AuditEmitter>`
    /// so handlers can take `&dyn AuditEmitter`-style parameters
    /// without generic explosion at the route layer.
    #[allow(dead_code)]
    fn _audit_emitter_is_dyn_safe(_: &dyn AuditEmitter) {}
}
