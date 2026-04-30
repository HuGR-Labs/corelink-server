//! CloudEvents 1.0 audit envelope builder for `corelink.cas.put_completed`.
//!
//! Per WI-S01-005 §6.1.6 + INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER, every
//! `BatchUpdateBlobs` per-blob success is paired atomically with an
//! `audit_outbox` row whose `payload_json` is a CloudEvents 1.0 envelope.
//! This module owns the envelope shape so:
//!
//! 1. The handler does not hand-craft JSON strings (cargo-fuzz hostile).
//! 2. The envelope shape is round-tripped + asserted in unit tests, so
//!    drift between the audit chain consumer (S-09) and the producer
//!    (this crate) is caught at CI time, not in production.
//! 3. The `request_id` propagation contract (WI §6.1.6) is enforced at the
//!    type level: callers cannot construct an [`AuditEnvelope`] without
//!    declaring the request_id, tenant_id, principal_id, digest, and
//!    `event_type`.
//!
//! ## CloudEvents 1.0 attribute mapping (canonical for S-01)
//!
//! | CE attribute | CoreLink value |
//! |---|---|
//! | `specversion` | `"1.0"` |
//! | `id`          | UUIDv7 (the `audit_outbox.id` PK; same value) |
//! | `source`      | `corelink://tenant/<tenant_id>` |
//! | `type`        | `corelink.cas.put_completed` (or `…poisoning_attempt`) |
//! | `subject`     | `<digest_canonical_text>` (`blake3:<hex>`) |
//! | `datacontenttype` | `application/json` |
//! | `data`        | `{ tenant_id, principal_id, digest_hash, size_bytes,
//!                    region, request_id }` |
//!
//! ### Why `time` is OMITTED from the envelope
//!
//! CloudEvents 1.0 §3.1 declares `time` OPTIONAL. We **deliberately** omit
//! it from the envelope so the `payload_json` column in `audit_outbox` is
//! content-stable across legitimate retries (same `request_id`, possibly
//! seconds apart on wall-clock). The actual ingest timestamp is recorded
//! in the `audit_outbox.enqueued_at_ms` column (separate from the
//! envelope) and is the canonical row-level T+0; the audit chain consumer
//! (S-09) uses the column, not the envelope's `time`.
//!
//! Without this carve-out, every retry would build a different
//! `payload_json` and the meta layer's `(request_id, event_type)` dedup
//! would surface as `AuditIdempotencyConflict` — a false positive.
//!
//! Body bytes are NEVER in the envelope (INV-NO-BODY-IN-LOGS, WI §26).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Canonical CE `type` for a successful CAS write. Mirrors
/// `corelink_meta::AuditEventType::CasPutCompleted::as_str()`.
pub const REAPI_PUT_COMPLETED: &str = "corelink.cas.put_completed";

/// Canonical CE `type` for a hash-mismatch (cache poisoning attempt) audit
/// event. Note: hash mismatch DOES NOT touch R2 / D1 (the handler short-
/// circuits before storage), so the audit is **best-effort** logged via the
/// outbox only when a `MetaStore` handle is available — for S-01 we emit it
/// as a tracing event + counter; the chain insert lands in S-09 once the
/// audit pipeline supports out-of-band events.
pub const REAPI_POISONING_ATTEMPT: &str = "corelink.cas.poisoning_attempt";

/// Build an `audit_outbox.payload_json` envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEnvelopeBuilder {
    tenant_id: Uuid,
    principal_id: Uuid,
    request_id: String,
    region: &'static str,
    digest_canonical_text: String,
    size_bytes: u64,
    event_type: &'static str,
    /// CE `id`. Caller-provided (UUIDv7 minted alongside the
    /// `audit_outbox.id` PK so both fields share the same value).
    id: Uuid,
}

/// Identity bundle for the principal performing the audited action.
/// Folding `tenant_id` + `principal_id` + `region` into a single value
/// keeps [`AuditEnvelopeBuilder`] constructor signatures within the
/// 7-argument clippy bound and ensures a "no rogue principal_id field"
/// invariant: the three identity fields always travel together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditPrincipal {
    /// Tenant UUID (from validated PAT).
    pub tenant_id: Uuid,
    /// PAT-owner UUID.
    pub principal_id: Uuid,
    /// Region label as a `'static` string (e.g. `"wnam"`).
    pub region: &'static str,
}

/// Identity of the envelope itself (CE `id`). The CE `time` attribute is
/// **omitted** from the envelope by design (see module-level rustdoc on
/// idempotent retry semantics); wall-clock is captured by the
/// `audit_outbox.enqueued_at_ms` column instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditTime {
    /// CE `id` (UUIDv7); equals `audit_outbox.id` PK so consumer-side
    /// dedup via id is trivial.
    pub envelope_id: Uuid,
}

impl AuditEnvelopeBuilder {
    /// Construct a fresh builder for `corelink.cas.put_completed`.
    #[must_use]
    pub fn put_completed(
        principal: AuditPrincipal,
        request_id: impl Into<String>,
        digest_canonical_text: impl Into<String>,
        size_bytes: u64,
        time: AuditTime,
    ) -> Self {
        Self {
            id: time.envelope_id,
            tenant_id: principal.tenant_id,
            principal_id: principal.principal_id,
            request_id: request_id.into(),
            region: principal.region,
            digest_canonical_text: digest_canonical_text.into(),
            size_bytes,
            event_type: REAPI_PUT_COMPLETED,
        }
    }

    /// Construct a fresh builder for `corelink.cas.poisoning_attempt`.
    #[must_use]
    pub fn poisoning_attempt(
        principal: AuditPrincipal,
        request_id: impl Into<String>,
        digest_canonical_text: impl Into<String>,
        time: AuditTime,
    ) -> Self {
        Self {
            id: time.envelope_id,
            tenant_id: principal.tenant_id,
            principal_id: principal.principal_id,
            request_id: request_id.into(),
            region: principal.region,
            digest_canonical_text: digest_canonical_text.into(),
            size_bytes: 0,
            event_type: REAPI_POISONING_ATTEMPT,
        }
    }

    /// Build a [`AuditEnvelope`] suitable for both serialization (via
    /// `serde_json`) and direct field inspection in tests.
    #[must_use]
    pub fn build(self) -> AuditEnvelope {
        AuditEnvelope {
            specversion: "1.0",
            id: self.id,
            source: format!("corelink://tenant/{}", self.tenant_id),
            r#type: self.event_type,
            subject: self.digest_canonical_text.clone(),
            datacontenttype: "application/json",
            data: AuditEnvelopeData {
                tenant_id: self.tenant_id,
                principal_id: self.principal_id,
                digest: self.digest_canonical_text,
                size_bytes: self.size_bytes,
                region: self.region,
                request_id: self.request_id,
            },
        }
    }
}

/// CloudEvents 1.0 envelope for CoreLink CAS audit events. Serializes via
/// `serde_json::to_string` into the `audit_outbox.payload_json` column.
///
/// **CE `time` is OMITTED by design** — see module-level rustdoc. Wall-
/// clock is captured by the `audit_outbox.enqueued_at_ms` column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEnvelope {
    /// CE `specversion`. Always `"1.0"`.
    pub specversion: &'static str,
    /// CE `id`. UUIDv7; equals `audit_outbox.id`.
    pub id: Uuid,
    /// CE `source`. `corelink://tenant/<tenant_id>`.
    pub source: String,
    /// CE `type`. `corelink.cas.put_completed` or
    /// `corelink.cas.poisoning_attempt`.
    pub r#type: &'static str,
    /// CE `subject`. Canonical digest text `blake3:<hex>`.
    pub subject: String,
    /// CE `datacontenttype`. Always `"application/json"`.
    pub datacontenttype: &'static str,
    /// CE `data`. CoreLink-specific payload.
    pub data: AuditEnvelopeData,
}

/// CoreLink-specific CE `data` payload. No PII beyond `tenant_id` /
/// `principal_id` (UUIDs) and the deliberately-canonicalized digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEnvelopeData {
    /// Tenant UUID.
    pub tenant_id: Uuid,
    /// PAT owner UUID.
    pub principal_id: Uuid,
    /// Canonical digest (`blake3:<hex>`).
    pub digest: String,
    /// Body size in bytes.
    pub size_bytes: u64,
    /// Region (e.g. `wnam`).
    pub region: &'static str,
    /// Correlation id propagated from the gRPC `x-request-id` header.
    pub request_id: String,
}

impl AuditEnvelope {
    /// Serialize to compact JSON. The `audit_outbox.payload_json` row uses
    /// the compact form so D1 storage is minimal.
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_json::Error` only if a struct member
    /// (none in `AuditEnvelope`) becomes non-serializable in the future;
    /// in S-01 every field is a primitive and `to_string` is infallible.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixed_id() -> Uuid {
        Uuid::from_u128(0x019384A0_ABCD_7000_8000_000000000001)
    }

    fn fixed_tenant() -> Uuid {
        Uuid::from_u128(0x019384A0_BEEF_7000_8000_000000000001)
    }

    fn fixed_principal_uuid() -> Uuid {
        Uuid::from_u128(0x019384A0_F00D_7000_8000_000000000001)
    }

    fn fixed_principal() -> AuditPrincipal {
        AuditPrincipal {
            tenant_id: fixed_tenant(),
            principal_id: super::tests::fixed_principal_uuid(),
            region: "wnam",
        }
    }

    fn fixed_time_bundle() -> AuditTime {
        AuditTime {
            envelope_id: fixed_id(),
        }
    }

    #[test]
    fn put_completed_envelope_round_trips_and_redacts_body() {
        let envelope = AuditEnvelopeBuilder::put_completed(
            fixed_principal(),
            "req-001",
            "blake3:d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24",
            5000,
            fixed_time_bundle(),
        )
        .build();

        let json = envelope.to_json().unwrap();

        // Round trip
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["specversion"], "1.0");
        assert_eq!(parsed["type"], REAPI_PUT_COMPLETED);
        assert_eq!(
            parsed["source"],
            format!("corelink://tenant/{}", fixed_tenant())
        );
        assert_eq!(parsed["data"]["request_id"], "req-001");
        assert_eq!(parsed["data"]["region"], "wnam");
        assert_eq!(parsed["data"]["size_bytes"], 5000);
        assert_eq!(
            parsed["subject"],
            "blake3:d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
        );

        // Body redaction: envelope MUST NOT carry body bytes.
        assert!(!json.contains("\"body\""));
        assert!(!json.contains("\"data_bytes\""));
        assert!(!json.contains("\"data\":\"")); // No raw bytes embedded.
    }

    #[test]
    fn poisoning_attempt_uses_distinct_event_type() {
        let envelope = AuditEnvelopeBuilder::poisoning_attempt(
            fixed_principal(),
            "req-002",
            "blake3:cafebabe000000000000000000000000000000000000000000000000deadbeef",
            fixed_time_bundle(),
        )
        .build();
        assert_eq!(envelope.r#type, REAPI_POISONING_ATTEMPT);
        assert_eq!(envelope.data.size_bytes, 0);
    }

    #[test]
    fn ce_id_pinned_to_audit_outbox_pk() {
        // The CE `id` MUST equal the audit_outbox.id PK so the consumer
        // (S-09) can de-duplicate by id without re-reading the row.
        let id = fixed_id();
        let envelope = AuditEnvelopeBuilder::put_completed(
            fixed_principal(),
            "req-003",
            "blake3:00",
            1,
            AuditTime { envelope_id: id },
        )
        .build();
        assert_eq!(envelope.id, id);
    }

    #[test]
    fn envelope_omits_time_for_idempotent_retry_stability() {
        let envelope = AuditEnvelopeBuilder::put_completed(
            fixed_principal(),
            "req-time-test",
            "blake3:00",
            42,
            fixed_time_bundle(),
        )
        .build();
        let json = envelope.to_json().unwrap();
        // CE `time` is deliberately absent — see module rustdoc.
        assert!(
            !json.contains("\"time\""),
            "audit envelope must NOT include CE `time` (idempotent retry stability)"
        );
    }

    #[test]
    fn json_is_compact_no_whitespace() {
        let envelope = AuditEnvelopeBuilder::put_completed(
            fixed_principal(),
            "req-004",
            "blake3:00",
            1,
            fixed_time_bundle(),
        )
        .build();
        let json = envelope.to_json().unwrap();
        assert!(!json.contains('\n'));
        assert!(!json.contains("  "));
    }
}
