//! Audit-emit chokepoint wrapper for the brew adapter.
//!
//! Each CAS-fill emits one `corelink.brew.bottle.cache_fill.v1` event
//! BEFORE the CAS put. The wrapper exists so the bottle-fetch
//! orchestrator can stay agnostic of the underlying [`AuditEmitter`]
//! implementation and so the per-event payload shape is documented in
//! exactly one place.
//!
//! ## Best-effort integrity note (audit metadata)
//!
//! Unlike the `pip` and `npm` adapter siblings, the brew adapter does
//! NOT verify a pre-store SHA — brew URLs do not embed the upstream
//! digest. The audit row therefore carries `integrity = "best-effort"`
//! so downstream SIEM rules can flag this lane separately from
//! adapters that do enforce pre-store verification. See
//! `specs/_proposals/adapters/brew.md` §3 for the rationale.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use corelink_audit::ports::{AuditEmitError, AuditEmitter, AuditEvent};
use serde_json::json;

use crate::error::BrewAdapterError;

/// CloudEvents `type` value for a brew bottle cache-fill.
pub const EVENT_TYPE_CACHE_FILL: &str = "corelink.brew.bottle.cache_fill.v1";

/// Audit chokepoint orchestrator. Holds the shared
/// [`AuditEmitter`] plus a pluggable clock so tests can pin
/// timestamps.
#[non_exhaustive]
pub struct AuditOrchestrator {
    emitter: Arc<dyn AuditEmitter>,
    clock: ClockFn,
}

/// Pluggable clock — returns unix-ms.
pub type ClockFn = Arc<dyn Fn() -> u64 + Send + Sync>;

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

    /// Emit a bottle cache-fill audit row. Returns
    /// [`BrewAdapterError::Audit`] on emitter failure (the route layer
    /// maps this to HTTP 503 and the caller MUST NOT proceed with the
    /// CAS put).
    pub fn emit_bottle_cache_fill(
        &self,
        tenant_id: &str,
        cas_key: &str,
        canonical_path: &str,
        bottle_size_bytes: u64,
    ) -> Result<(), BrewAdapterError> {
        let payload = json!({
            "cas_key": cas_key,
            "canonical_path": canonical_path,
            "bottle_size_bytes": bottle_size_bytes,
            // Brew adapter does NOT verify a pre-store SHA — the brew
            // client verifies against the formula DSL post-download.
            // See spec §3.
            "integrity": "best-effort",
            "adapter": "brew",
        });

        let event = AuditEvent::new(
            EVENT_TYPE_CACHE_FILL,
            tenant_id,
            (self.clock)(),
            payload,
        );

        match self.emitter.emit(event) {
            Ok(()) => Ok(()),
            Err(AuditEmitError::Store(msg)) => Err(BrewAdapterError::Audit(msg)),
            Err(AuditEmitError::MutexPoisoned) => Err(BrewAdapterError::Audit(
                "test sink mutex poisoned".to_owned(),
            )),
            Err(other) => Err(BrewAdapterError::Audit(format!("{other}"))),
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
    fn emit_appends_event_with_expected_shape() {
        let sink: Arc<InMemoryAuditEmitter> = Arc::new(InMemoryAuditEmitter::new());
        let orchestrator = AuditOrchestrator::with_clock(
            sink.clone(),
            Arc::new(|| 1_716_700_000_000),
        );
        orchestrator
            .emit_bottle_cache_fill(
                "tenant-abc",
                "deadbeef",
                "v2/homebrew/core/curl",
                4096,
            )
            .unwrap();
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.event_type, EVENT_TYPE_CACHE_FILL);
        assert_eq!(event.tenant_id, "tenant-abc");
        assert_eq!(event.at_unix_ms, 1_716_700_000_000);
        assert_eq!(event.payload["cas_key"], "deadbeef");
        assert_eq!(event.payload["bottle_size_bytes"], 4096);
        assert_eq!(event.payload["integrity"], "best-effort");
        assert_eq!(event.payload["adapter"], "brew");
    }
}
