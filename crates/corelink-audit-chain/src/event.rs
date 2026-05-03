//! Canonical [`AuditEvent`] CloudEvents 1.0 envelope + per-event
//! hash-chain link slot (`prev_hash` 32-byte BLAKE3 + monotonic
//! `sequence_number` u64).
//!
//! ## Why CloudEvents 1.0 (CNCF spec)
//!
//! Per WI §1 + sprint contract §5.4 R-S09-10, the audit emit surface
//! is canonical CloudEvents 1.0 so any SIEM consumer (Splunk, Datadog,
//! Sumo) can ingest the events without a custom adapter. The required
//! attributes per CloudEvents §3.1 are:
//!
//! - `id` (canonical UUIDv7 — time-ordered + globally unique)
//! - `source` (URI; e.g. `corelink/region/iad`)
//! - `specversion` (hard-pinned to `"1.0"` per Lote 10.9bis P0-G)
//! - `type` (canonical dotted slug — `dev.hugr.corelink.<subject>.v1`)
//! - `time` (RFC 3339 — represented as Unix epoch ms `time_ms` in the
//!   wire shape; server-side conversion is canonical)
//! - `data` (the typed payload per `AuditEventKind`)
//!
//! Optional CloudEvents attributes used:
//!
//! - `datacontenttype` (canonical `"application/json"`)
//! - `subject` (the canonical CNCF subject string from the 8-element
//!   `AuditEventKind` taxonomy)
//!
//! ## CoreLink-specific extensions (CloudEvents extension attributes)
//!
//! - `tenant_id` — pseudonymous UUID from the auth-context middleware
//!   (S-03 WI-S03-003); used as the per-tenant chain partition key
//!   (per WI §1 invariant 8 + Lote 10.4bis TenantCtx-only enforcement).
//! - `region` — canonical 3-char CF colocode (mirrors
//!   `corelink_analytics::Region::as_str()`).
//! - `sequence_number` — monotonic per-(tenant, region) chain sequence;
//!   `0` is the canonical genesis position.
//! - `prev_hash` — 32-byte BLAKE3-256 of the JCS-canonical bytes of the
//!   previous event in the chain (or `[0u8; 32]` for the genesis event
//!   per WI §1 invariant 1 — Bitcoin-genesis-block convention).
//!
//! ## Why `prev_hash = [0u8; 32]` for genesis (NOT a magic sentinel)
//!
//! The convention mirrors Bitcoin's genesis-block `prev_hash = 0x00..00`.
//! Any non-zero value would require a "first event" sentinel branch in
//! the verifier; the zero-hash convention reduces verifier branching
//! and pins the canonical chain head deterministically.
//!
//! ## CTRL-PRIV-001 + CTRL-AUDIT-001 enforcement
//!
//! Per WI §1 invariant 7, raw PII fields (`email`, `ip`, `bearer`,
//! `digest`) are FORBIDDEN in the audit data payload. The
//! `AuditEventKind` taxonomy is the canonical 8-element CNCF subject
//! list (closed at compile-time per `#[non_exhaustive]` discipline);
//! the runtime data envelope MUST be already-redacted via
//! `corelink-logpush::PiiRedactor::redact_json` before reaching the
//! chain emit (the production wiring + WI-S09-002 inheritance enforce
//! this). The chain emit itself is purely structural — it does NOT
//! re-redact, so a non-redacted body would land in the 7y immutable
//! R2 archive (compliance gap). Tests pinned by `prop_redacted_body`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use corelink_analytics::Region;

/// Canonical CloudEvents `specversion` attribute. Hard-pinned to
/// `"1.0"` per CloudEvents 1.0 §3.1 + WI Lote 10.9bis P0-G correction.
pub const CLOUDEVENTS_SPECVERSION: &str = "1.0";

/// Canonical CloudEvents `datacontenttype` attribute. Hard-pinned to
/// `"application/json"` per WI §1.
pub const CLOUDEVENTS_DATACONTENTTYPE: &str = "application/json";

/// Canonical event-type prefix per WI Lote 10.9bis P0-G corrected from
/// `io.corelink.*` to `dev.hugr.corelink.*` for SIEM consumer
/// compatibility with the S-06 audit events.
pub const EVENT_TYPE_PREFIX: &str = "dev.hugr.corelink.";

/// Canonical genesis `prev_hash` — 32 bytes of `0x00`. Per WI §1
/// invariant 1 + Bitcoin-genesis-block convention.
pub const GENESIS_PREV_HASH: [u8; 32] = [0u8; 32];

/// Canonical genesis `sequence_number` — `0`. Per WI §1 invariant 1.
pub const GENESIS_SEQUENCE_NUMBER: u64 = 0;

/// Canonical 8-element CNCF subject taxonomy per WI §1 + sprint
/// contract §5.4 R-S09-10 (Lote 10.9bis P0-H corrected: 8 subjects
/// canonical aligning DoD §6 + S-08 abuse score events compliance
/// auditing).
///
/// `#[non_exhaustive]` so follow-on WIs (S-11 DSR / S-13 admin /
/// S-19 Stripe) can extend the taxonomy additively without breaking
/// downstream sinks. NEW subject requires a Compliance Officer
/// sign-off mandatory (per WI §6.1.6 R-S09-11 regulatory scoping).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AuditEventKind {
    /// `tenant` — tenant lifecycle (create / delete / upgrade / suspend).
    Tenant,
    /// `cas:put` — CAS write events (S-01 inheritance).
    CasPut,
    /// `cas:get` — CAS read events (compliance-required per SOC 2 CC7.2).
    CasGet,
    /// `ac:lookup` — Action Cache lookups (S-04 inheritance).
    AcLookup,
    /// `gc:purge` — GC sweep events (S-06 inheritance).
    GcPurge,
    /// `auth:login` — PAT auth events (S-03 inheritance).
    AuthLogin,
    /// `quota:exceeded` — S-08 quota enforcement events.
    QuotaExceeded,
    /// `abuse:detected` — S-08 abuse score events (Lote 10.9bis P0-H
    /// added for compliance auditing).
    AbuseDetected,
}

impl AuditEventKind {
    /// Canonical CNCF subject string (the value of the CloudEvents
    /// `subject` attribute).
    #[must_use]
    pub const fn subject(self) -> &'static str {
        match self {
            Self::Tenant => "tenant",
            Self::CasPut => "cas:put",
            Self::CasGet => "cas:get",
            Self::AcLookup => "ac:lookup",
            Self::GcPurge => "gc:purge",
            Self::AuthLogin => "auth:login",
            Self::QuotaExceeded => "quota:exceeded",
            Self::AbuseDetected => "abuse:detected",
        }
    }

    /// Canonical CloudEvents `type` attribute string per WI Lote 10.9bis
    /// P0-G corrected `dev.hugr.corelink.<subject>.v1` shape. The dotted
    /// slug uses `.` as the segment separator (CloudEvents §3.1.3
    /// canonical) — the subject's `:` separator is mapped to `.` for the
    /// type slug.
    #[must_use]
    pub const fn event_type(self) -> &'static str {
        match self {
            Self::Tenant => "dev.hugr.corelink.tenant.v1",
            Self::CasPut => "dev.hugr.corelink.cas.put.v1",
            Self::CasGet => "dev.hugr.corelink.cas.get.v1",
            Self::AcLookup => "dev.hugr.corelink.ac.lookup.v1",
            Self::GcPurge => "dev.hugr.corelink.gc.purge.v1",
            Self::AuthLogin => "dev.hugr.corelink.auth.login.v1",
            Self::QuotaExceeded => "dev.hugr.corelink.quota.exceeded.v1",
            Self::AbuseDetected => "dev.hugr.corelink.abuse.detected.v1",
        }
    }
}

impl core::fmt::Display for AuditEventKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.subject())
    }
}

impl Serialize for AuditEventKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.subject())
    }
}

impl<'de> Deserialize<'de> for AuditEventKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "tenant" => Ok(Self::Tenant),
            "cas:put" => Ok(Self::CasPut),
            "cas:get" => Ok(Self::CasGet),
            "ac:lookup" => Ok(Self::AcLookup),
            "gc:purge" => Ok(Self::GcPurge),
            "auth:login" => Ok(Self::AuthLogin),
            "quota:exceeded" => Ok(Self::QuotaExceeded),
            "abuse:detected" => Ok(Self::AbuseDetected),
            other => Err(serde::de::Error::custom(format!(
                "unknown AuditEventKind subject: {other}"
            ))),
        }
    }
}

/// Canonical 8-element `AuditEventKind` list. Pinned for cardinality
/// estimate (8 = canonical event-kind label cardinality per WI §6.1.6).
#[must_use]
pub const fn canonical_audit_event_kinds() -> &'static [AuditEventKind; 8] {
    &[
        AuditEventKind::Tenant,
        AuditEventKind::CasPut,
        AuditEventKind::CasGet,
        AuditEventKind::AcLookup,
        AuditEventKind::GcPurge,
        AuditEventKind::AuthLogin,
        AuditEventKind::QuotaExceeded,
        AuditEventKind::AbuseDetected,
    ]
}

/// Canonical 32-byte BLAKE3-256 hash newtype. Always hex-rendered when
/// serialized to JSON for the CloudEvents wire shape (RFC 4648 §8
/// hex-lowercase canonical form per CTRL-AUDIT-001 chain integrity
/// discipline).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChainHash(pub [u8; 32]);

impl ChainHash {
    /// The canonical genesis chain hash (32 bytes of `0x00`).
    #[must_use]
    pub const fn genesis() -> Self {
        Self(GENESIS_PREV_HASH)
    }

    /// Borrow the underlying 32-byte array.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Render as a 64-char lowercase hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl Serialize for ChainHash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ChainHash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let bytes = hex::decode(&s).map_err(|e| {
            serde::de::Error::custom(format!("ChainHash hex decode failed: {e}"))
        })?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom(format!(
                "ChainHash expected 32 bytes; got {}",
                bytes.len()
            )));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        Ok(Self(out))
    }
}

impl core::fmt::Display for ChainHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

fn serialize_region<S: serde::Serializer>(region: &Region, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(region.as_str())
}

fn deserialize_region<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Region, D::Error> {
    let s = String::deserialize(d)?;
    region_from_str(&s)
        .ok_or_else(|| serde::de::Error::custom(format!("unknown Region: {s}")))
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

/// Canonical CloudEvents-1.0-aligned audit event shape with hash-chain
/// link.
///
/// `Serialize + Deserialize` so the NDJSON serializer (used by the R2
/// PutObject pipeline at `audit/{tenant_id}/{date}/{seq:08}.cloudevent.ndjson`
/// per WI §6.1.4) round-trips losslessly. The chain link inputs
/// (`sequence_number`, `prev_hash`) are first-class CloudEvents extension
/// attributes per CloudEvents §3.1.5.
///
/// ## Field ordering
///
/// CloudEvents 1.0 §3 declares the canonical attribute set; we follow
/// that order so the NDJSON output reads naturally for ops debugging.
/// JCS canonicalization (RFC 8785) at chain-link time sorts keys
/// lexicographically regardless, so the on-the-wire field order is
/// purely a readability concern; the chain hash is invariant to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// CloudEvents `specversion` — hard-pinned `"1.0"`.
    pub specversion: String,

    /// CloudEvents `type` — canonical `dev.hugr.corelink.<subject>.v1`
    /// dotted slug.
    #[serde(rename = "type")]
    pub event_type: String,

    /// CloudEvents `source` — the originating Worker URI (e.g.
    /// `corelink/region/iad`). Used as the `source` URI per
    /// CloudEvents §3.1.1.
    pub source: String,

    /// CloudEvents `subject` — canonical CNCF subject from the 8-element
    /// taxonomy.
    pub subject: AuditEventKind,

    /// CloudEvents `id` — UUIDv7 (time-ordered) per CloudEvents §3.1.4
    /// uniqueness requirement.
    pub id: Uuid,

    /// CloudEvents `time` — Unix epoch milliseconds (canonical `_ms`
    /// suffix per CloudEvents external surface).
    pub time_ms: u64,

    /// CloudEvents `datacontenttype` — hard-pinned
    /// `"application/json"`.
    pub datacontenttype: String,

    /// Pseudonymous tenant id (UUIDv7 from the S-03 auth-context
    /// middleware). Per-tenant chain partition key per WI §6.1.4.
    pub tenant_id: Uuid,

    /// Canonical 3-char CF colocode (mirrors
    /// `corelink_analytics::Region::as_str()`).
    #[serde(
        serialize_with = "serialize_region",
        deserialize_with = "deserialize_region"
    )]
    pub region: Region,

    /// Monotonic per-(tenant, region) chain sequence. `0` is the
    /// canonical genesis position; subsequent events are `prev + 1`
    /// strictly. Used as the lexicographic-sort R2 key prefix per WI
    /// §6.1.4 NDJSON layout.
    pub sequence_number: u64,

    /// 32-byte BLAKE3-256 of the JCS-canonical bytes of the PREVIOUS
    /// event in this (tenant, region) chain. For the genesis event
    /// (`sequence_number == 0`) this MUST be `[0u8; 32]` per WI §1
    /// invariant 1 (zero-hash convention; Bitcoin genesis pattern).
    pub prev_hash: ChainHash,

    /// Per-event-kind payload AFTER redaction. The `serde_json::Value`
    /// shape mirrors the CloudEvents `data` attribute. Production
    /// wiring (WI-S09-002 inheritance) walks this tree with the
    /// `PiiRedactor` substituting PII matches BEFORE reaching the
    /// chain emit.
    pub data: serde_json::Value,
}

impl AuditEvent {
    /// Construct a canonical audit event. `time_ms` MUST be a Unix
    /// epoch ms instant; `id` MUST be a UUIDv7 the production wiring
    /// derives from `Uuid::now_v7()` so events are time-ordered.
    /// `data` MUST be already-redacted (the chain orchestrator does
    /// NOT re-redact — production wiring + WI-S09-002 inheritance
    /// enforce this BEFORE reaching this surface).
    ///
    /// Nine parameters reflect the canonical CloudEvents 1.0 attribute
    /// set + the chain link inputs + the per-tenant chain partition
    /// key — every parameter is load-bearing at the data model. The
    /// alternative builder pattern would obscure the canonical CE 1.0
    /// shape; the lint allowance here is intentional.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        kind: AuditEventKind,
        source: impl Into<String>,
        id: Uuid,
        time_ms: u64,
        tenant_id: Uuid,
        region: Region,
        sequence_number: u64,
        prev_hash: ChainHash,
        data: serde_json::Value,
    ) -> Self {
        Self {
            specversion: CLOUDEVENTS_SPECVERSION.to_string(),
            event_type: kind.event_type().to_string(),
            source: source.into(),
            subject: kind,
            id,
            time_ms,
            datacontenttype: CLOUDEVENTS_DATACONTENTTYPE.to_string(),
            tenant_id,
            region,
            sequence_number,
            prev_hash,
            data,
        }
    }

    /// Construct the canonical genesis event for a (tenant, region)
    /// chain. `sequence_number == 0` and `prev_hash == [0u8; 32]` per
    /// WI §1 invariant 1.
    #[must_use]
    pub fn genesis(
        kind: AuditEventKind,
        source: impl Into<String>,
        id: Uuid,
        time_ms: u64,
        tenant_id: Uuid,
        region: Region,
        data: serde_json::Value,
    ) -> Self {
        Self::new(
            kind,
            source,
            id,
            time_ms,
            tenant_id,
            region,
            GENESIS_SEQUENCE_NUMBER,
            ChainHash::genesis(),
            data,
        )
    }

    /// Whether this event is the canonical genesis position
    /// (`sequence_number == 0` AND `prev_hash == [0u8; 32]`).
    #[must_use]
    pub fn is_genesis(&self) -> bool {
        self.sequence_number == GENESIS_SEQUENCE_NUMBER
            && self.prev_hash.as_bytes() == &GENESIS_PREV_HASH
    }

    /// Serialize to canonical NDJSON (one event = one line; no
    /// trailing newline). The R2 PutObject pipeline expects one JSON
    /// object per line; the writer concatenates events with a single
    /// `\n` separator on the wire.
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
    /// Returns the underlying `serde_json::Error` when the line is not
    /// a valid `AuditEvent` JSON object.
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
    fn cloudevents_specversion_pinned() {
        assert_eq!(CLOUDEVENTS_SPECVERSION, "1.0");
    }

    #[test]
    fn cloudevents_datacontenttype_pinned() {
        assert_eq!(CLOUDEVENTS_DATACONTENTTYPE, "application/json");
    }

    #[test]
    fn event_type_prefix_pinned() {
        assert_eq!(EVENT_TYPE_PREFIX, "dev.hugr.corelink.");
    }

    #[test]
    fn genesis_constants_pinned() {
        assert_eq!(GENESIS_PREV_HASH, [0u8; 32]);
        assert_eq!(GENESIS_SEQUENCE_NUMBER, 0u64);
    }

    #[test]
    fn canonical_event_kinds_count_is_eight() {
        let v = canonical_audit_event_kinds();
        assert_eq!(v.len(), 8);
    }

    #[test]
    fn each_event_kind_has_unique_subject_string() {
        let v = canonical_audit_event_kinds();
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(set.insert(k.subject()));
        }
        assert_eq!(set.len(), 8);
    }

    #[test]
    fn each_event_kind_has_unique_event_type_string() {
        let v = canonical_audit_event_kinds();
        let mut set = std::collections::HashSet::new();
        for k in v {
            let t = k.event_type();
            assert!(t.starts_with(EVENT_TYPE_PREFIX));
            assert!(t.ends_with(".v1"));
            assert!(set.insert(t));
        }
        assert_eq!(set.len(), 8);
    }

    #[test]
    fn subject_strings_pinned() {
        assert_eq!(AuditEventKind::Tenant.subject(), "tenant");
        assert_eq!(AuditEventKind::CasPut.subject(), "cas:put");
        assert_eq!(AuditEventKind::CasGet.subject(), "cas:get");
        assert_eq!(AuditEventKind::AcLookup.subject(), "ac:lookup");
        assert_eq!(AuditEventKind::GcPurge.subject(), "gc:purge");
        assert_eq!(AuditEventKind::AuthLogin.subject(), "auth:login");
        assert_eq!(AuditEventKind::QuotaExceeded.subject(), "quota:exceeded");
        assert_eq!(AuditEventKind::AbuseDetected.subject(), "abuse:detected");
    }

    #[test]
    fn event_type_strings_pinned() {
        assert_eq!(AuditEventKind::Tenant.event_type(), "dev.hugr.corelink.tenant.v1");
        assert_eq!(AuditEventKind::CasPut.event_type(), "dev.hugr.corelink.cas.put.v1");
        assert_eq!(
            AuditEventKind::AbuseDetected.event_type(),
            "dev.hugr.corelink.abuse.detected.v1"
        );
    }

    #[test]
    fn audit_event_kind_serde_round_trip() {
        for k in canonical_audit_event_kinds() {
            let s = serde_json::to_string(k).unwrap();
            let back: AuditEventKind = serde_json::from_str(&s).unwrap();
            assert_eq!(*k, back);
        }
    }

    #[test]
    fn unknown_subject_rejects() {
        let line = r#""ZZ_unknown""#;
        let err = serde_json::from_str::<AuditEventKind>(line).unwrap_err();
        assert!(format!("{err}").contains("unknown AuditEventKind"));
    }

    #[test]
    fn chain_hash_serde_hex_round_trip() {
        let h = ChainHash([0xAB; 32]);
        let s = serde_json::to_string(&h).unwrap();
        // 64 hex chars + 2 surrounding quotes = 66.
        assert_eq!(s.len(), 66);
        let back: ChainHash = serde_json::from_str(&s).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn chain_hash_genesis_is_zero() {
        let g = ChainHash::genesis();
        assert_eq!(g.as_bytes(), &[0u8; 32]);
        assert_eq!(g.to_hex(), "0".repeat(64));
    }

    #[test]
    fn chain_hash_invalid_hex_rejected() {
        let err = serde_json::from_str::<ChainHash>("\"zzzz\"").unwrap_err();
        assert!(format!("{err}").contains("hex decode"));
    }

    #[test]
    fn chain_hash_wrong_length_rejected() {
        let err =
            serde_json::from_str::<ChainHash>("\"ab\"").unwrap_err();
        assert!(format!("{err}").contains("expected 32 bytes"));
    }

    #[test]
    fn audit_event_genesis_is_genesis() {
        let id = Uuid::now_v7();
        let tenant = Uuid::now_v7();
        let e = AuditEvent::genesis(
            AuditEventKind::Tenant,
            "corelink/region/iad",
            id,
            1_700_000_000_000,
            tenant,
            Region::Iad,
            json!({"action": "created"}),
        );
        assert!(e.is_genesis());
        assert_eq!(e.sequence_number, 0);
        assert_eq!(e.prev_hash.as_bytes(), &[0u8; 32]);
    }

    #[test]
    fn audit_event_non_zero_seq_not_genesis() {
        let id = Uuid::now_v7();
        let tenant = Uuid::now_v7();
        let e = AuditEvent::new(
            AuditEventKind::Tenant,
            "corelink/region/iad",
            id,
            1,
            tenant,
            Region::Iad,
            1,
            ChainHash::genesis(),
            json!({}),
        );
        assert!(!e.is_genesis());
    }

    #[test]
    fn ndjson_round_trip_preserves_event() {
        let id = Uuid::now_v7();
        let tenant = Uuid::now_v7();
        let e = AuditEvent::genesis(
            AuditEventKind::CasPut,
            "corelink/region/iad",
            id,
            1_700_000_000_000,
            tenant,
            Region::Iad,
            json!({"digest_truncated": "abc1234567890def"}),
        );
        let line = e.to_ndjson_line().unwrap();
        let back = AuditEvent::from_ndjson_line(&line).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn ndjson_line_has_no_trailing_newline() {
        let e = AuditEvent::genesis(
            AuditEventKind::Tenant,
            "corelink/region/iad",
            Uuid::now_v7(),
            1,
            Uuid::now_v7(),
            Region::Iad,
            json!({}),
        );
        let line = e.to_ndjson_line().unwrap();
        assert!(!line.ends_with('\n'));
    }

    #[test]
    fn ndjson_line_contains_canonical_cloudevents_attributes() {
        let e = AuditEvent::genesis(
            AuditEventKind::AuthLogin,
            "corelink/region/gru",
            Uuid::now_v7(),
            42,
            Uuid::now_v7(),
            Region::Gru,
            json!({"result": "success"}),
        );
        let line = e.to_ndjson_line().unwrap();
        assert!(line.contains("\"specversion\":\"1.0\""));
        assert!(line.contains("\"type\":\"dev.hugr.corelink.auth.login.v1\""));
        assert!(line.contains("\"subject\":\"auth:login\""));
        assert!(line.contains("\"datacontenttype\":\"application/json\""));
        assert!(line.contains("\"region\":\"gru\""));
        assert!(line.contains("\"sequence_number\":0"));
    }
}
