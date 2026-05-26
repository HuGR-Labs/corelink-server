//! npm adapter audit emit helper.
//!
//! Every state-mutating CAS / KV write in the adapter invokes
//! [`emit_npm_audit`] BEFORE returning success to the client, per the
//! charter `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (CRITICAL)
//! fail-CLOSED contract. If the emit fails, the surrounding
//! operation MUST roll back and propagate a 503 to the client.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use corelink_audit::ports::{AuditEmitError, AuditEmitter, AuditEvent};
use corelink_core::types::tenant::TenantId;

use crate::error::NpmAdapterError;

/// Canonical event-type strings emitted by the npm adapter.
pub mod event_types {
    /// Tarball CAS-store hit (cache-hit, no state mutation).
    pub const TARBALL_CACHE_HIT: &str = "corelink.npm.tarball.cache_hit.v1";
    /// Tarball downloaded from upstream + stored in CAS.
    pub const TARBALL_STORED: &str = "corelink.npm.tarball.stored.v1";
    /// Tarball rejected: downloaded bytes' SHA256 did not match
    /// `dist.shasum` from npm metadata.
    pub const TARBALL_INTEGRITY_MISMATCH: &str = "corelink.npm.tarball.integrity_mismatch.v1";
    /// Tarball rejected: exceeded `tarball_size_limit_bytes`.
    pub const TARBALL_OVERSIZED: &str = "corelink.npm.tarball.oversized.v1";
    /// Package metadata refreshed from upstream + stored in KV.
    pub const METADATA_REFRESHED: &str = "corelink.npm.metadata.refreshed.v1";
    /// Package metadata served from KV cache (no upstream call).
    pub const METADATA_CACHE_HIT: &str = "corelink.npm.metadata.cache_hit.v1";
    /// Forged / malformed / unscoped PAT rejected at the auth layer.
    pub const AUTH_REJECTED: &str = "corelink.npm.auth.rejected.v1";
}

/// Unix-millisecond wall-clock timestamp.
#[must_use]
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Emit an npm-adapter audit event. Returns
/// [`NpmAdapterError::Audit`] on emit failure so the caller can
/// short-circuit the surrounding mutation.
///
/// # Errors
///
/// Returns [`NpmAdapterError::Audit`] on any
/// [`AuditEmitError`] from the underlying emitter.
pub fn emit_npm_audit(
    auditor: &Arc<dyn AuditEmitter>,
    event_type: &'static str,
    tenant: &TenantId,
    at_unix_ms: u64,
    payload: serde_json::Value,
) -> Result<(), NpmAdapterError> {
    let event = AuditEvent::new(event_type, tenant.to_string(), at_unix_ms, payload);
    auditor.emit(event).map_err(|e: AuditEmitError| match e {
        AuditEmitError::Store(msg) => NpmAdapterError::Audit(msg),
        AuditEmitError::MutexPoisoned => {
            NpmAdapterError::Audit("audit mutex poisoned (test sink)".into())
        }
        other => NpmAdapterError::Audit(other.to_string()),
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
            event_types::TARBALL_CACHE_HIT,
            event_types::TARBALL_STORED,
            event_types::TARBALL_INTEGRITY_MISMATCH,
            event_types::TARBALL_OVERSIZED,
            event_types::METADATA_REFRESHED,
            event_types::METADATA_CACHE_HIT,
            event_types::AUTH_REJECTED,
        ] {
            assert!(et.starts_with("corelink.npm."));
            assert!(et.ends_with(".v1"));
        }
    }

    #[test]
    fn now_unix_ms_is_positive() {
        assert!(now_unix_ms() > 0);
    }

    #[test]
    fn emit_npm_audit_forwards_to_sink() {
        let sink = InMemoryAuditEmitter::default();
        let arc: Arc<dyn AuditEmitter> = Arc::new(sink.clone());
        let tenant = TenantId::from_uuid(uuid::Uuid::now_v7());
        let result = emit_npm_audit(
            &arc,
            event_types::TARBALL_CACHE_HIT,
            &tenant,
            1_700_000_000_000,
            serde_json::json!({ "package": "lodash" }),
        );
        assert!(result.is_ok());
        let drained = sink.snapshot();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].event_type, event_types::TARBALL_CACHE_HIT);
    }
}
