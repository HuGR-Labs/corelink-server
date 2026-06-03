//! Canonical `LogRecord` shape — CloudEvents 1.0 aligned (id /
//! source / specversion / type / time / data) plus a pseudonymous
//! `tenant_id` slot pinned to the auth-context middleware (S-03
//! WI-S03-003) source-of-truth.
//!
//! ## Why CloudEvents 1.0
//!
//! Per `observability_model.md §5` the canonical log envelope shares
//! the CloudEvents-1.0 attribute set used by the WI-S09-004 audit
//! chain emitter so a single downstream serializer (NDJSON for
//! Logpush + R2 + Loki) covers both surfaces. The `specversion`
//! attribute is hard-pinned to `"1.0"` so the schema-evolution gate
//! can detect drift at the cold-start parse step.
//!
//! ## CTRL-PRIV-001 enforcement at the type system
//!
//! Per privacy_model §6 + sprint contract §5.2 R-S09-4, raw PII fields
//! (`email`, `ip`, `bearer`, `digest`) are FORBIDDEN in the log
//! payload. The `LogRecord` type forces the caller to pass `data` as
//! a `serde_json::Value` that has already been redacted by
//! [`crate::logpush::redaction::PiiRedactor::redact`]; the
//! [`crate::logpush::sink::InMemoryLogSink`] orchestrator enforces this at the
//! emit boundary by running redaction BEFORE serialization on every
//! emit arm. Tests pinned by the 100k synthetic fixture (WI §6.1.5
//! DoD gate; §6.1.7 falsifiability target).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use corelink_analytics::Region;

/// Canonical log-schema version (currently 16; mirrors the
/// `migrations/d1/0016_log_schema.sql` filename prefix). Cold-start
/// log emit asserts the in-memory `SCHEMA_VERSION` const matches the
/// per-region durable mirror BEFORE accepting any emit.
pub const SCHEMA_VERSION: u32 = 16;

/// Canonical CloudEvents `specversion` attribute. Hard-pinned to
/// `"1.0"` per CloudEvents canonical spec.
pub const CLOUDEVENTS_SPECVERSION: &str = "1.0";

/// Canonical 4-event `type` taxonomy aligned with the CloudEvents
/// dotted-name convention. Per WI §6.1.1 the closed set is:
/// RequestServed / AuthAttempt / AdminAction / BillingEvent.
///
/// `#[non_exhaustive]` so follow-on WIs (WI-S09-003 traces /
/// WI-S09-004 audit chain) can extend the taxonomy additively without
/// breaking downstream sinks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum LogEventType {
    /// `request_served` — every CAS / AC / admin HTTP request that
    /// reached a Worker handler completion arm.
    RequestServed,
    /// `auth_attempt` — every Clerk / WebAuthn / PAT auth attempt
    /// (success and failure both emit).
    AuthAttempt,
    /// `admin_action` — every privileged admin-plane action (S-13
    /// admin sprint stub for now; emitted from the audit chain
    /// emitter to provide a duplicate observability surface).
    AdminAction,
    /// `billing_event` — every metered usage event (S-10 billing
    /// sprint surface; the log mirror is sampled INFO-level).
    BillingEvent,
}

impl LogEventType {
    /// Canonical CloudEvents-style dotted slug. Snake_case per
    /// privacy_model §5 + observability_model §5 vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestServed => "request_served",
            Self::AuthAttempt => "auth_attempt",
            Self::AdminAction => "admin_action",
            Self::BillingEvent => "billing_event",
        }
    }
}

impl core::fmt::Display for LogEventType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for LogEventType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for LogEventType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "request_served" => Ok(Self::RequestServed),
            "auth_attempt" => Ok(Self::AuthAttempt),
            "admin_action" => Ok(Self::AdminAction),
            "billing_event" => Ok(Self::BillingEvent),
            other => Err(serde::de::Error::custom(format!(
                "unknown LogEventType: {other}"
            ))),
        }
    }
}

/// Canonical 4-element `LogEventType` list — pinned for cardinality
/// estimate (4 = canonical event-type label cardinality).
#[must_use]
pub const fn canonical_log_event_types() -> &'static [LogEventType; 4] {
    &[
        LogEventType::RequestServed,
        LogEventType::AuthAttempt,
        LogEventType::AdminAction,
        LogEventType::BillingEvent,
    ]
}

fn serialize_region<S: serde::Serializer>(region: &Region, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(region.as_str())
}

fn deserialize_region<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Region, D::Error> {
    let s = String::deserialize(d)?;
    region_from_str(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown Region: {s}")))
}

fn region_from_str(s: &str) -> Option<Region> {
    Some(match s {
        "iad" => Region::Iad,
        "sjc" => Region::Sjc,
        "dfw" => Region::Dfw,
        "sea" => Region::Sea,
        "ord" => Region::Ord,
        "lhr" => Region::Lhr,
        "fra" => Region::Fra,
        "ams" => Region::Ams,
        "cdg" => Region::Cdg,
        "mad" => Region::Mad,
        "gru" => Region::Gru,
        "eze" => Region::Eze,
        "bog" => Region::Bog,
        "nrt" => Region::Nrt,
        "sin" => Region::Sin,
        "syd" => Region::Syd,
        "hkg" => Region::Hkg,
        "bom" => Region::Bom,
        "icn" => Region::Icn,
        "jnb" => Region::Jnb,
        "cpt" => Region::Cpt,
        "dxb" => Region::Dxb,
        _ => return None,
    })
}

/// Pseudonymous tenant identifier slot. The runtime contract (WI §1
/// invariant 3 + Lote 10.4bis TenantCtx-only enforcement) is that
/// tenant_id arrives from the auth-context middleware
/// (S-03 WI-S03-003) ONLY and is the canonical UUIDv7 surface. Raw
/// tenant identifiers (email / Clerk subject / Stripe customer id)
/// NEVER reach this surface.
///
/// The `Option<Uuid>` discriminator covers the canonical anonymous
/// path (pre-auth + edge probe) where the tenant is intentionally
/// absent.
pub type TenantId = Uuid;

/// Canonical CloudEvents-1.0-aligned log record shape.
///
/// `Serialize + Deserialize` so the NDJSON serializer (used by the
/// Logpush → R2 + Loki pipeline) round-trips losslessly. The
/// `data` slot is a `serde_json::Value` so the per-event-type payload
/// can vary while the envelope stays stable; the redaction step
/// (run BEFORE serialization on every emit arm) walks the value tree
/// substituting PII matches with canonical placeholder strings.
///
/// ## Field ordering
///
/// CloudEvents 1.0 §3 defines the canonical attribute set as the
/// declared order: `specversion / type / source / id / time / data`,
/// optionally extended with `subject`. We follow that order here so
/// the NDJSON output reads naturally for ops debugging in Loki.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
    /// CloudEvents `specversion` — hard-pinned `"1.0"`.
    pub specversion: String,

    /// CloudEvents `type` — canonical 4-event taxonomy.
    #[serde(rename = "type")]
    pub event_type: LogEventType,

    /// CloudEvents `source` — the originating Worker name (e.g.
    /// `corelink-cas-iad`). Used as the `source` URI per CloudEvents §3.1.1.
    pub source: String,

    /// CloudEvents `id` — UUIDv7 (time-ordered) per CloudEvents §3.1.4
    /// uniqueness requirement.
    pub id: Uuid,

    /// CloudEvents `time` — Unix epoch milliseconds (canonical
    /// `*_ms` suffix per Lote 10.7bis P0-3 SCHEMA-DRIFT lesson; the
    /// internal SQL column is `time_at` ms — the JSON wire surface
    /// uses `time_ms` because CloudEvents external surfaces are
    /// time-suffix canonical).
    pub time_ms: u64,

    /// Pseudonymous tenant id (UUIDv7 from the S-03 auth-context
    /// middleware). `None` covers the pre-auth / edge-probe path.
    pub tenant_id: Option<TenantId>,

    /// Canonical 3-char CF colocode (mirrors
    /// `corelink_analytics::Region::as_str()`).
    #[serde(
        serialize_with = "serialize_region",
        deserialize_with = "deserialize_region"
    )]
    pub region: Region,

    /// Per-event-type payload AFTER redaction. The `serde_json::Value`
    /// shape mirrors the CloudEvents `data` attribute.
    pub data: serde_json::Value,
}

impl LogRecord {
    /// Construct a canonical log record. `time_ms` MUST be a Unix
    /// epoch ms instant; `id` MUST be a UUIDv7 the production wiring
    /// derives from `Uuid::now_v7()` so events are time-ordered.
    /// `data` MUST be already-redacted (the
    /// [`crate::logpush::sink::InMemoryLogSink`] orchestrator enforces this
    /// at the emit boundary).
    #[must_use]
    pub fn new(
        event_type: LogEventType,
        source: impl Into<String>,
        id: Uuid,
        time_ms: u64,
        tenant_id: Option<TenantId>,
        region: Region,
        data: serde_json::Value,
    ) -> Self {
        Self {
            specversion: CLOUDEVENTS_SPECVERSION.to_string(),
            event_type,
            source: source.into(),
            id,
            time_ms,
            tenant_id,
            region,
            data,
        }
    }

    /// Serialize to canonical NDJSON (one record = one line; no
    /// trailing newline). The Logpush → R2 + Loki pipeline expects
    /// one JSON object per line; the writer concatenates records with
    /// a single `\n` separator on the wire.
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_json::Error` when the embedded
    /// `data` value cannot be serialized (e.g. a non-finite float).
    pub fn to_ndjson_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Round-trip parse from a canonical NDJSON line.
    ///
    /// # Errors
    ///
    /// Returns the underlying `serde_json::Error` when the line is
    /// not a valid `LogRecord` JSON object.
    pub fn from_ndjson_line(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line)
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
    use serde_json::json;

    #[test]
    fn schema_version_pinned_to_16() {
        assert_eq!(SCHEMA_VERSION, 16);
    }

    #[test]
    fn cloudevents_specversion_pinned_to_1_0() {
        assert_eq!(CLOUDEVENTS_SPECVERSION, "1.0");
    }

    #[test]
    fn each_event_type_has_unique_canonical_string() {
        let v = canonical_log_event_types();
        assert_eq!(v.len(), 4);
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(set.insert(t.as_str()));
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn event_type_strings_pinned() {
        assert_eq!(LogEventType::RequestServed.as_str(), "request_served");
        assert_eq!(LogEventType::AuthAttempt.as_str(), "auth_attempt");
        assert_eq!(LogEventType::AdminAction.as_str(), "admin_action");
        assert_eq!(LogEventType::BillingEvent.as_str(), "billing_event");
    }

    #[test]
    fn ndjson_round_trip_preserves_record() {
        let id = Uuid::now_v7();
        let tenant = Some(Uuid::now_v7());
        let r = LogRecord::new(
            LogEventType::RequestServed,
            "corelink-cas-iad",
            id,
            1_700_000_000_000,
            tenant,
            Region::Iad,
            json!({"path": "/v1/cas/put", "status": 200}),
        );
        let line = r.to_ndjson_line().unwrap();
        let r2 = LogRecord::from_ndjson_line(&line).unwrap();
        assert_eq!(r, r2);
    }

    #[test]
    fn ndjson_line_has_no_trailing_newline() {
        let r = LogRecord::new(
            LogEventType::RequestServed,
            "corelink-cas-iad",
            Uuid::now_v7(),
            1,
            None,
            Region::Iad,
            json!({}),
        );
        let line = r.to_ndjson_line().unwrap();
        assert!(!line.ends_with('\n'));
    }

    #[test]
    fn ndjson_line_contains_canonical_attributes() {
        let id = Uuid::now_v7();
        let r = LogRecord::new(
            LogEventType::AuthAttempt,
            "corelink-auth-gru",
            id,
            42,
            None,
            Region::Gru,
            json!({"outcome": "success"}),
        );
        let line = r.to_ndjson_line().unwrap();
        assert!(line.contains("\"specversion\":\"1.0\""));
        assert!(line.contains("\"type\":\"auth_attempt\""));
        assert!(line.contains("\"source\":\"corelink-auth-gru\""));
        assert!(line.contains("\"region\":\"gru\""));
    }

    #[test]
    fn anonymous_tenant_serializes_as_null() {
        let r = LogRecord::new(
            LogEventType::RequestServed,
            "corelink-edge-iad",
            Uuid::now_v7(),
            1,
            None,
            Region::Iad,
            json!({}),
        );
        let line = r.to_ndjson_line().unwrap();
        assert!(line.contains("\"tenant_id\":null"));
    }

    #[test]
    fn unknown_event_type_deserialize_errors() {
        let line = r#"{"specversion":"1.0","type":"unknown_type","source":"x","id":"00000000-0000-0000-0000-000000000000","time_ms":1,"tenant_id":null,"region":"iad","data":{}}"#;
        let err = LogRecord::from_ndjson_line(line).unwrap_err();
        assert!(format!("{err}").contains("unknown LogEventType"));
    }

    #[test]
    fn display_matches_as_str() {
        assert_eq!(format!("{}", LogEventType::AdminAction), "admin_action");
    }
}
