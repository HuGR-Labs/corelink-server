//! [`Emitter`] trait + [`InMemoryEmitter`] (test sink).
//!
//! ## Production wiring (deferred)
//!
//! The production emitter ladder is composed at S-03 wiring time:
//!
//! - `OutboxEmitter` — INSERTs the [`crate::AuthEvent`] (canonicalized
//!   via [`crate::compute_content_hash`]) into `audit_outbox` in the
//!   **same D1 batch** as the handler's main mutation. This satisfies
//!   `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (the batch either commits
//!   both rows or rolls back, surfacing 503 to the client; no silent
//!   audit gap is possible). Reuses the [WI-S01-005] outbox shape.
//! - `DirectSiemEmitter` — async POST to a SIEM webhook (Datadog /
//!   Splunk / CrowdStrike) for SEV-1 events ([`crate::AuthEventType::is_sev1`]).
//!   Bounded by a circuit breaker; webhook outage triggers SEV-2
//!   alerting via `corelink.audit.outbox_lag_seconds`.
//! - `MultiplexEmitter` — composes both: writes to outbox always, fans
//!   out to direct SIEM only for SEV-1 events. Belt-and-suspenders for
//!   incident response when 60-s outbox drain is too slow.
//!
//! All three live behind the [`Emitter`] trait so unit + property
//! tests + integration tests use the [`InMemoryEmitter`] sink.

use std::sync::{Arc, Mutex, MutexGuard};

use thiserror::Error;

use crate::events::AuthEvent;

/// Errors returned by an [`Emitter`] implementation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum EmitterError {
    /// The underlying store rejected the row (D1 batch failure / disk
    /// full / unique-constraint violation under retry with mismatched
    /// payload, etc.). The handler MUST translate this to a 503-class
    /// response so the audit gap doesn't leak to the client as a 200.
    #[error("audit emitter store error: {0}")]
    Store(String),

    /// The internal mutex of an [`InMemoryEmitter`] was poisoned. Test
    /// sink only — production emitters should not hit this.
    #[error("audit emitter mutex poisoned (test sink only)")]
    MutexPoisoned,
}

/// Trait for any backend that persists an audit envelope.
///
/// Sync rather than async because production callers will INSERT into
/// the D1 batch via the existing `corelink-meta::commit_*` orchestrator
/// which is itself sync at the trait surface (the actual D1 RPC is
/// async but lives behind a host-server feature gate). Keeping this
/// trait sync lets the no-default-features build stay
/// `wasm32-unknown-unknown`-clean for downstream Worker callers,
/// matching the pattern set by `corelink-pat` / `corelink-clerk` /
/// `corelink-webauthn`.
pub trait Emitter: Send + Sync {
    /// Persist `event`. Returns `Ok(())` on durable persistence (in the
    /// outbox emitter that means the D1 batch committed; in the direct
    /// SIEM emitter that means the webhook returned 2xx); `Err` on any
    /// failure that would leave the audit chain incomplete.
    ///
    /// # Errors
    ///
    /// Returns [`EmitterError::Store`] on any backend failure. The
    /// handler MUST translate this to a 503-class response.
    fn emit(&self, event: AuthEvent) -> Result<(), EmitterError>;
}

/// Test-only emitter that captures every emitted event in an in-memory
/// vector. Used by unit + property + integration tests; not for
/// production.
///
/// Cloning shares the underlying buffer (`Arc<Mutex<Vec<…>>>`), so
/// multiple call sites can hold separate handles to the same sink.
#[derive(Clone, Debug, Default)]
pub struct InMemoryEmitter {
    events: Arc<Mutex<Vec<AuthEvent>>>,
}

impl InMemoryEmitter {
    /// Construct an empty in-memory emitter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Snapshot every event captured so far. Returns a clone of the
    /// underlying buffer; the emitter retains its state.
    #[must_use]
    pub fn snapshot(&self) -> Vec<AuthEvent> {
        match self.events.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Number of events captured so far. Mirrors `snapshot().len()`
    /// without cloning the vector.
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

    /// Lock the inner buffer for mutation. Used by adversarial tests
    /// that want to simulate the outbox-drain layer (e.g. clearing
    /// drained rows).
    fn lock(&self) -> Result<MutexGuard<'_, Vec<AuthEvent>>, EmitterError> {
        self.events
            .lock()
            .map_err(|_| EmitterError::MutexPoisoned)
    }
}

impl Emitter for InMemoryEmitter {
    fn emit(&self, event: AuthEvent) -> Result<(), EmitterError> {
        let mut guard = self.lock()?;
        guard.push(event);
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test module — assertions panic by design"
)]
mod tests {
    use super::*;
    use crate::events::{AuthEventData, AuthEventType, RegionTag, TokenKind};
    use crate::redact::PrincipalIdHash;
    use crate::retention::RetentionHint;
    use crate::{RequestId, TenantId};
    use uuid::Uuid;

    fn sample() -> AuthEvent {
        AuthEvent::new(
            AuthEventType::TokenValidated,
            "corelink://wnam/auth/middleware",
            TenantId::from_uuid(Uuid::nil()),
            PrincipalIdHash::derive("user_x").expect("derive"),
            RegionTag::Wnam,
            RequestId::new("req_x"),
            RetentionHint::Team90d,
            1_700_000_000_000,
            AuthEventData::TokenValidated {
                token_kind: TokenKind::Pat,
                scope_bitset: 1,
            },
        )
    }

    #[test]
    fn in_memory_captures_emitted_events() {
        let emitter = InMemoryEmitter::new();
        assert!(emitter.is_empty());
        emitter.emit(sample()).expect("emit");
        emitter.emit(sample()).expect("emit");
        assert_eq!(emitter.len(), 2);
        assert_eq!(emitter.snapshot().len(), 2);
    }

    #[test]
    fn in_memory_is_clone_share() {
        let a = InMemoryEmitter::new();
        let b = a.clone();
        a.emit(sample()).expect("emit");
        // Clones share the same buffer.
        assert_eq!(b.len(), 1);
    }
}
