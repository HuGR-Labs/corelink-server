//! Pip adapter audit emit helper.
//!
//! Every state-mutating CAS / KV write in the adapter invokes
//! [`emit_pip_audit`] BEFORE returning success to the client, per the
//! charter `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (CRITICAL)
//! fail-CLOSED contract. If the emit fails, the surrounding
//! operation MUST roll back and propagate a 503 to the client.
//!
//! This module is a thin convenience layer over
//! [`corelink_audit::ports::AuditEmitter`] — it adds the canonical
//! pip event-type strings + a unix-millisecond timestamp helper, but
//! does NOT swallow or remap audit emit failures.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use corelink_audit::ports::{AuditEmitError, AuditEmitter, AuditEvent};
use corelink_core::types::tenant::TenantId;

use crate::error::PipAdapterError;

/// Canonical event-type strings emitted by the pip adapter. Kept as
/// `&'static str` constants (not an enum) so the production wiring
/// layer can pin the taxonomy without round-tripping every variant
/// through a central enum (per
/// [`corelink_audit::ports::AuditEvent::event_type`] doc-comment).
pub mod event_types {
    /// Wheel/sdist CAS-store hit (cache-hit, no state mutation).
    pub const WHEEL_CACHE_HIT: &str = "corelink.pip.wheel.cache_hit.v1";
    /// Wheel/sdist downloaded from upstream + stored in CAS.
    pub const WHEEL_STORED: &str = "corelink.pip.wheel.stored.v1";
    /// Wheel/sdist rejected: downloaded bytes' SHA256 did not match
    /// the upstream `#sha256=` fragment.
    pub const WHEEL_INTEGRITY_MISMATCH: &str = "corelink.pip.wheel.integrity_mismatch.v1";
    /// Wheel/sdist rejected: exceeded
    /// `wheel_size_limit_bytes`.
    pub const WHEEL_OVERSIZED: &str = "corelink.pip.wheel.oversized.v1";
    /// Index JSON refreshed from upstream + stored in KV.
    pub const INDEX_REFRESHED: &str = "corelink.pip.index.refreshed.v1";
    /// Index JSON served from KV cache (no upstream call).
    pub const INDEX_CACHE_HIT: &str = "corelink.pip.index.cache_hit.v1";
    /// Forged / malformed / unscoped PAT rejected at the auth layer.
    pub const AUTH_REJECTED: &str = "corelink.pip.auth.rejected.v1";
}

/// Unix-millisecond wall-clock timestamp. Production callers should
/// pass a [`corelink_core::time::Clock`]-derived value instead; this
/// helper is for the audit-emit call sites that have no clock
/// handle threaded through (boot-time PAT auth rejections).
#[must_use]
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Emit a pip-adapter audit event. Returns
/// [`PipAdapterError::Audit`] on emit failure so the caller can
/// short-circuit the surrounding mutation.
///
/// # Errors
///
/// Returns [`PipAdapterError::Audit`] on any
/// [`AuditEmitError`] from the underlying emitter.
pub fn emit_pip_audit(
    auditor: &Arc<dyn AuditEmitter>,
    event_type: &'static str,
    tenant: &TenantId,
    at_unix_ms: u64,
    payload: serde_json::Value,
) -> Result<(), PipAdapterError> {
    let event = AuditEvent::new(event_type, tenant.to_string(), at_unix_ms, payload);
    auditor.emit(event).map_err(|e: AuditEmitError| match e {
        AuditEmitError::Store(msg) => PipAdapterError::Audit(msg),
        AuditEmitError::MutexPoisoned => {
            PipAdapterError::Audit("audit mutex poisoned (test sink)".into())
        }
        other => PipAdapterError::Audit(other.to_string()),
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use corelink_audit::ports::InMemoryAuditEmitter;

    #[test]
    fn event_types_are_well_formed() {
        for et in [
            event_types::WHEEL_CACHE_HIT,
            event_types::WHEEL_STORED,
            event_types::WHEEL_INTEGRITY_MISMATCH,
            event_types::WHEEL_OVERSIZED,
            event_types::INDEX_REFRESHED,
            event_types::INDEX_CACHE_HIT,
            event_types::AUTH_REJECTED,
        ] {
            assert!(et.starts_with("corelink.pip."));
            assert!(et.ends_with(".v1"));
        }
    }

    #[test]
    fn now_unix_ms_is_positive() {
        assert!(now_unix_ms() > 0);
    }

    #[test]
    fn emit_pip_audit_forwards_to_sink() {
        let sink = InMemoryAuditEmitter::default();
        let arc: Arc<dyn AuditEmitter> = Arc::new(sink.clone());
        let tenant = TenantId::from_uuid(uuid::Uuid::now_v7());
        let result = emit_pip_audit(
            &arc,
            event_types::WHEEL_CACHE_HIT,
            &tenant,
            1_700_000_000_000,
            serde_json::json!({ "digest": "abc" }),
        );
        assert!(result.is_ok());
        let drained = sink.snapshot();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].event_type, event_types::WHEEL_CACHE_HIT);
    }
}
