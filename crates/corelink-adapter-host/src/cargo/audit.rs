//! Audit-emit chokepoint wrapper for the cargo adapter.
//!
//! Each CAS write emits one `corelink.cargo.cache.write.v1` event
//! BEFORE the CAS put. The wrapper exists so the request handler
//! can stay agnostic of the underlying [`AuditEmitter`] implementation
//! and so the per-event payload shape is documented in exactly one place.
//!
//! ## Fail-CLOSED contract
//!
//! If `emit_cache_write` returns `Err`, the handler MUST NOT proceed
//! with the CAS put. This is the audit-fail-CLOSED invariant from the
//! Wave-33 Stage 0 SEAL §4 rationale.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use corelink_audit::ports::{AuditEmitError, AuditEmitter, AuditEvent};
use serde_json::json;

use crate::cargo::error::CargoAdapterError;

/// CloudEvents `type` value for a cargo cache write.
pub const EVENT_TYPE_CACHE_WRITE: &str = "corelink.cargo.cache.write.v1";

/// CloudEvents `type` value for a failed auth attempt.
pub const EVENT_TYPE_AUTH_FAILED: &str = "corelink.cargo.adapter.auth_failed.v1";

/// Pluggable clock — returns unix-ms.
pub type ClockFn = Arc<dyn Fn() -> u64 + Send + Sync>;

/// Audit chokepoint orchestrator. Holds the shared
/// [`AuditEmitter`] plus a pluggable clock so tests can pin
/// timestamps.
#[non_exhaustive]
pub struct AuditOrchestrator {
    emitter: Arc<dyn AuditEmitter>,
    clock: ClockFn,
}

impl std::fmt::Debug for AuditOrchestrator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditOrchestrator")
            .field("emitter", &"Arc<dyn AuditEmitter>")
            .field("clock", &"Fn() -> u64")
            .finish()
    }
}

impl AuditOrchestrator {
    /// Construct with the production wall-clock.
    #[must_use]
    pub fn new(emitter: Arc<dyn AuditEmitter>) -> Self {
        Self {
            emitter,
            clock: Arc::new(wall_clock_unix_ms),
        }
    }

    /// Construct with a test clock.
    #[must_use]
    pub fn with_clock(emitter: Arc<dyn AuditEmitter>, clock: ClockFn) -> Self {
        Self { emitter, clock }
    }

    /// Emit a cache write audit row. MUST be called BEFORE the CAS put.
    /// Returns [`CargoAdapterError::Audit`] on emitter failure — the
    /// route layer maps this to HTTP 503 and the caller MUST NOT proceed
    /// with the CAS put (fail-CLOSED).
    pub fn emit_cache_write(
        &self,
        tenant_id: &str,
        digest_hex: &str,
        body_size_bytes: u64,
    ) -> Result<(), CargoAdapterError> {
        let payload = json!({
            "digest_hex": digest_hex,
            "body_size_bytes": body_size_bytes,
            "adapter": "cargo",
            "operation": "put",
        });

        let event = AuditEvent::new(EVENT_TYPE_CACHE_WRITE, tenant_id, (self.clock)(), payload);

        self.emit_event(event)
    }

    /// Emit an auth-failed audit row. Best-effort (logged but not
    /// fail-CLOSED — auth itself already rejected the request).
    pub fn emit_auth_failed(&self, reason: &str) {
        let payload = json!({
            "reason": reason,
            "adapter": "cargo",
        });
        let event = AuditEvent::new(EVENT_TYPE_AUTH_FAILED, "unknown", (self.clock)(), payload);
        // Auth-failed audit is best-effort; ignore emitter errors here
        // (the request was already rejected; we log only).
        let _ = self.emitter.emit(event);
    }

    fn emit_event(&self, event: AuditEvent) -> Result<(), CargoAdapterError> {
        match self.emitter.emit(event) {
            Ok(()) => Ok(()),
            Err(AuditEmitError::Store(msg)) => Err(CargoAdapterError::Audit(msg)),
            Err(AuditEmitError::MutexPoisoned) => Err(CargoAdapterError::Audit(
                "test sink mutex poisoned".to_owned(),
            )),
            Err(other) => Err(CargoAdapterError::Audit(format!("{other}"))),
        }
    }
}

/// Production wall-clock — unix milliseconds. Failures (clock before
/// epoch) saturate to `0` so the audit row still emits with a clearly
/// invalid timestamp rather than panicking.
fn wall_clock_unix_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::uninlined_format_args
)]
mod tests {
    use super::*;
    use corelink_audit::ports::InMemoryAuditEmitter;

    #[test]
    fn emit_cache_write_appends_event_with_expected_shape() {
        let sink: Arc<InMemoryAuditEmitter> = Arc::new(InMemoryAuditEmitter::new());
        let orchestrator =
            AuditOrchestrator::with_clock(sink.clone(), Arc::new(|| 1_716_700_000_000));
        orchestrator
            .emit_cache_write("tenant-abc", "deadbeef".repeat(8).as_str(), 4096)
            .unwrap();
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.event_type, EVENT_TYPE_CACHE_WRITE);
        assert_eq!(event.tenant_id, "tenant-abc");
        assert_eq!(event.at_unix_ms, 1_716_700_000_000);
        assert_eq!(event.payload["body_size_bytes"], 4096);
        assert_eq!(event.payload["adapter"], "cargo");
        assert_eq!(event.payload["operation"], "put");
    }

    #[test]
    fn emit_auth_failed_is_best_effort() {
        let sink: Arc<InMemoryAuditEmitter> = Arc::new(InMemoryAuditEmitter::new());
        let orchestrator = AuditOrchestrator::new(sink.clone());
        // Should not panic or return error.
        orchestrator.emit_auth_failed("invalid PAT");
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, EVENT_TYPE_AUTH_FAILED);
    }
}
