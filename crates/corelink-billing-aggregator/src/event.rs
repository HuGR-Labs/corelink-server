//! Canonical [`AggregatedCounter`] CloudEvents 1.0 envelope + per-aggregate
//! hash-chain link slot (`prev_hash` 32-byte BLAKE3 + monotonic
//! `sequence_number` u64) + per-aggregate `total_qty` / `event_count` /
//! `idem_keys_seen` summary.
//!
//! ## Why CloudEvents 1.0 (CNCF spec)
//!
//! Per WI-S10-002 §1 + sprint contract §5.4, the counter aggregator
//! emits canonical CloudEvents 1.0 so the downstream Stripe billing
//! adapter (WI-S10-003) + reconciliation worker (WI-S10-004) + replay
//! forensic API (WI-S10-006) consume the same wire shape. The required
//! attributes per CloudEvents §3.1 are:
//!
//! - `id` (canonical UUIDv7 — time-ordered + globally unique)
//! - `source` (URI; e.g. `corelink/region/iad/aggregator`)
//! - `specversion` (hard-pinned `"1.0"`)
//! - `type` (canonical `corelink.billing.counter.aggregated`)
//! - `time` (RFC 3339 → Unix epoch ms `time_ms` on the wire)
//! - `data` (the typed `{tenant_id, billing_period, event_kind,
//!   total_qty, event_count, period_start_ms, period_end_ms,
//!   idem_keys_seen}` payload)
//!
//! Optional CloudEvents attributes used:
//!
//! - `datacontenttype` (canonical `"application/json"`)
//! - `subject` (the canonical `tenant:<uuid>` slug; mirrors the
//!   audit-chain S-09 inheritance + the WI-S10-001 emit surface)
//!
//! ## CoreLink-specific extensions (CloudEvents extension attributes)
//!
//! - `prev_hash` — 32-byte BLAKE3-256 of the JCS-canonical bytes of the
//!   previous aggregate in the (tenant, billing_period) chain (or
//!   `[0u8; 32]` for the genesis aggregate per WI-S10-002 §1 invariant 3
//!   — Bitcoin-genesis-block convention; canonical inheritance from the
//!   S-09 audit chain pattern).
//! - `sequence_number` — monotonic per-(tenant, billing_period) chain
//!   sequence; `0` is the canonical genesis position.
//!
//! ## Why `prev_hash = [0u8; 32]` for genesis
//!
//! Mirrors the canonical convention from `corelink-audit-chain`
//! (S-09 inheritance); the zero-hash convention reduces verifier
//! branching. See `corelink_audit_chain::GENESIS_PREV_HASH`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use corelink_billing_emit::{IdemKey, UsageEventKind};

/// Canonical CloudEvents `specversion` attribute. Hard-pinned to
/// `"1.0"` per CloudEvents 1.0 §3.1.
pub const CLOUDEVENTS_SPECVERSION: &str = "1.0";

/// Canonical CloudEvents `datacontenttype` attribute. Hard-pinned to
/// `"application/json"`.
pub const CLOUDEVENTS_DATACONTENTTYPE: &str = "application/json";

/// Canonical CloudEvents `type` attribute. Hard-pinned per WI-S10-002
/// §1 — every aggregate emitted by this surface carries the same
/// canonical `type` (the per-kind dispatch lives in the typed
/// [`AggregatedCounterData::event_kind`] field).
pub const COUNTER_AGGREGATED_EVENT_TYPE: &str = "corelink.billing.counter.aggregated";

/// Canonical genesis `prev_hash` — 32 bytes of `0x00`. Mirrors
/// `corelink_audit_chain::GENESIS_PREV_HASH`.
pub const GENESIS_PREV_HASH: [u8; 32] = [0u8; 32];

/// Canonical genesis `sequence_number` — `0`.
pub const GENESIS_SEQUENCE_NUMBER: u64 = 0;

/// Canonical 32-byte BLAKE3-256 chain-hash newtype. Always hex-rendered
/// when serialized to JSON for the CloudEvents wire shape (RFC 4648 §8
/// hex-lowercase canonical form). Mirrors
/// `corelink_audit_chain::ChainHash` discipline.
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

fn serialize_event_kind<S: serde::Serializer>(
    kind: &UsageEventKind,
    s: S,
) -> Result<S::Ok, S::Error> {
    s.serialize_str(kind.as_str())
}

fn deserialize_event_kind<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<UsageEventKind, D::Error> {
    let s = String::deserialize(d)?;
    match s.as_str() {
        "storage_bytes_hourly" => Ok(UsageEventKind::StorageBytesHourly),
        "egress_bytes" => Ok(UsageEventKind::EgressBytes),
        "ac_lookup" => Ok(UsageEventKind::AcLookup),
        "cas_get" => Ok(UsageEventKind::CasGet),
        "cas_put" => Ok(UsageEventKind::CasPut),
        "replay_request" => Ok(UsageEventKind::ReplayRequest),
        other => Err(serde::de::Error::custom(format!(
            "unknown UsageEventKind: {other}"
        ))),
    }
}

/// Typed payload of an [`AggregatedCounter`] (the value of the
/// CloudEvents `data` attribute). Per WI-S10-002 §1 the canonical shape
/// is `{tenant_id, billing_period, event_kind, total_qty, event_count,
/// period_start_ms, period_end_ms, idem_keys_seen}`.
///
/// `total_qty` is the SUM of the contributing
/// `corelink_billing_emit::UsageEvent::data.qty` field over every event
/// in the (tenant, billing_period, event_kind) bucket. `event_count` is
/// the cardinality of the contributing event set (DEDUPed by
/// `idem_key`). `idem_keys_seen` lists the canonical idem_keys of every
/// contributing event in the deterministic input order
/// (`(time_ms, idem_key)` lexicographic) — the chain digest is
/// determined by this canonical list, so re-aggregation reproduces the
/// same digest provided the input set matches (WI-S10-002 §1 invariant
/// 9 idempotent re-run guarantee).
///
/// ## Why `period_start_ms` inclusive + `period_end_ms` exclusive
///
/// Canonical Prometheus bucket boundary semantics (mirrors histogram
/// upper-bound exclusive convention): an event with `time_ms ==
/// period_end_ms` belongs to the NEXT period bucket. This avoids
/// double-counting at period boundaries — WI-S10-002 chaos scenario 4
/// (late-arriving event `7h+` over the boundary) relies on the
/// exclusive-end semantics for correct routing to `usage_counter_late`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregatedCounterData {
    /// Tenant id of the aggregate (per-tenant chain partition; matches
    /// the WI-S10-001 emit surface tenant partition + the audit-chain
    /// S-09 inheritance).
    pub tenant_id: Uuid,

    /// Canonical UTC month bucket (`YYYY-MM` per RFC 3339-extended).
    /// Validated upstream at the [`corelink_billing_emit::UsageEvent`]
    /// construction surface; the aggregator never sees a non-canonical
    /// period.
    pub billing_period: String,

    /// Canonical event kind (the contributing event taxonomy).
    /// Hex-rendered via the `UsageEventKind::as_str()` discriminant
    /// string per WI-S10-001 §1 + sprint contract §5.1 R-S10-1.
    #[serde(
        serialize_with = "serialize_event_kind",
        deserialize_with = "deserialize_event_kind"
    )]
    pub event_kind: UsageEventKind,

    /// Aggregated billable quantity (sum of contributing events' `qty`).
    pub total_qty: u128,

    /// Cardinality of the contributing event set (DEDUPed by
    /// `idem_key`).
    pub event_count: u64,

    /// Inclusive period start (Unix epoch ms; canonical `_ms` suffix).
    pub period_start_ms: u64,

    /// Exclusive period end (Unix epoch ms; canonical `_ms` suffix).
    /// An event with `time_ms == period_end_ms` belongs to the NEXT
    /// period bucket (canonical Prometheus boundary semantics).
    pub period_end_ms: u64,

    /// Canonical list of contributing event idem_keys in deterministic
    /// input order (`(time_ms, idem_key)` lexicographic). The chain
    /// digest is determined by this list, so re-aggregation of the same
    /// input set reproduces the same digest (WI-S10-002 §1 invariant 9
    /// idempotent re-run guarantee).
    pub idem_keys_seen: Vec<IdemKey>,
}

/// Canonical CloudEvents-1.0-aligned aggregated-counter shape with
/// BLAKE3 hash-chain link slot.
///
/// `Serialize + Deserialize` so the canonical chain digest is computed
/// over the JCS bytes via `serde_jcs` (mirrors the audit-chain
/// canonical-bytes discipline; see [`crate::chain`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregatedCounter {
    /// CloudEvents `specversion` — hard-pinned `"1.0"`.
    pub specversion: String,

    /// CloudEvents `type` — hard-pinned
    /// `"corelink.billing.counter.aggregated"`.
    #[serde(rename = "type")]
    pub event_type: String,

    /// CloudEvents `source` — the originating aggregator URI (e.g.
    /// `corelink/region/iad/aggregator`). Used as the `source` URI per
    /// CloudEvents §3.1.1.
    pub source: String,

    /// CloudEvents `subject` — the canonical `tenant:<uuid>` slug
    /// (mirrors the WI-S10-001 emit surface + the audit chain S-09
    /// inheritance).
    pub subject: String,

    /// CloudEvents `id` — UUIDv7 (time-ordered) per CloudEvents §3.1.4
    /// uniqueness requirement.
    pub id: Uuid,

    /// CloudEvents `time` — Unix epoch ms (canonical `_ms` suffix per
    /// CloudEvents external surface). Wall-clock instant of the
    /// aggregate emit.
    pub time_ms: u64,

    /// CloudEvents `datacontenttype` — hard-pinned
    /// `"application/json"`.
    pub datacontenttype: String,

    /// Per-(tenant, billing_period) monotonic chain sequence; `0` is
    /// canonical genesis.
    pub sequence_number: u64,

    /// 32-byte BLAKE3-256 chain link to the previous aggregate (or
    /// genesis `[0u8; 32]` for the first aggregate in the (tenant,
    /// billing_period) chain).
    pub prev_hash: ChainHash,

    /// Typed payload `{tenant_id, billing_period, event_kind,
    /// total_qty, event_count, period_start_ms, period_end_ms,
    /// idem_keys_seen}`.
    pub data: AggregatedCounterData,
}

impl AggregatedCounter {
    /// Construct a canonical aggregated counter. The orchestrator at
    /// [`crate::aggregator::InMemoryCounterAggregator::run`] is the
    /// canonical caller; this constructor is exposed so adversarial
    /// tests can build aggregates outside the orchestrator (e.g. to
    /// exercise `prop_chain_break_detected_on_tamper` directly at the
    /// chain primitive surface).
    ///
    /// Eight-plus parameters reflect the canonical CloudEvents 1.0
    /// attribute set + the typed payload — every parameter is
    /// load-bearing at the data model. The lint allowance is intentional
    /// (mirrors `corelink_billing_emit::UsageEvent::new` discipline).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: impl Into<String>,
        id: Uuid,
        time_ms: u64,
        sequence_number: u64,
        prev_hash: ChainHash,
        tenant_id: Uuid,
        billing_period: impl Into<String>,
        event_kind: UsageEventKind,
        total_qty: u128,
        event_count: u64,
        period_start_ms: u64,
        period_end_ms: u64,
        idem_keys_seen: Vec<IdemKey>,
    ) -> Self {
        let billing_period = billing_period.into();
        let subject = format!("tenant:{tenant_id}");
        Self {
            specversion: CLOUDEVENTS_SPECVERSION.to_string(),
            event_type: COUNTER_AGGREGATED_EVENT_TYPE.to_string(),
            source: source.into(),
            subject,
            id,
            time_ms,
            datacontenttype: CLOUDEVENTS_DATACONTENTTYPE.to_string(),
            sequence_number,
            prev_hash,
            data: AggregatedCounterData {
                tenant_id,
                billing_period,
                event_kind,
                total_qty,
                event_count,
                period_start_ms,
                period_end_ms,
                idem_keys_seen,
            },
        }
    }

    /// Canonical `(tenant_id, billing_period, event_kind)` grouping key
    /// (used by the in-memory store for UPSERT semantics).
    #[must_use]
    pub fn group_key(&self) -> CounterGroupKey {
        CounterGroupKey {
            tenant_id: self.data.tenant_id,
            billing_period: self.data.billing_period.clone(),
            event_kind: self.data.event_kind,
        }
    }
}

/// Canonical grouping coordinate for an aggregated counter row.
///
/// Production wiring binds this to the D1 PRIMARY KEY (tenant_id,
/// region, sku, hour) UNIQUE — the trait surface here uses the
/// (tenant, billing_period, event_kind) triple at the period (`YYYY-MM`)
/// granularity, which is the canonical reconciliation Layer 1 grouping
/// (sprint contract §5.4 R-S10-8 — Σ events vs Σ counters per (tenant,
/// region, sku, hour) collapsed across the period). The hour-level
/// production grouping lands at WI-S10-007 alongside the CF Cron DO
/// binding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CounterGroupKey {
    /// Tenant id of the aggregate.
    pub tenant_id: Uuid,
    /// Canonical `YYYY-MM` period.
    pub billing_period: String,
    /// Canonical event kind discriminant.
    pub event_kind: UsageEventKind,
}

/// Decision returned by [`crate::aggregator::CounterAggregator::run`]:
/// either an aggregate landed, or the run was a no-op.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AggregationDecision {
    /// Aggregate landed: the chain advanced + the counter store
    /// recorded the new row + the audit fired
    /// `corelink.billing_aggregator.run_completed`.
    Aggregated(AggregatedCounter),
    /// No events matched the (tenant, billing_period, event_kind) +
    /// period window: the run was a no-op (chain head unchanged, counter
    /// store unchanged); audit fires
    /// `corelink.billing_aggregator.run_completed` with `event_count == 0`.
    SkippedNoEvents {
        /// Tenant id of the no-op run.
        tenant_id: Uuid,
        /// Canonical billing period that had no contributing events.
        billing_period: String,
        /// Canonical event kind that had no contributing events.
        event_kind: UsageEventKind,
    },
    /// Duplicate run detected (same `(tenant, billing_period,
    /// event_kind)` already aggregated this period with the SAME
    /// canonical input set): the chain head is unchanged + the counter
    /// store is unchanged + the audit fires
    /// `corelink.billing_aggregator.run_completed` with the existing
    /// aggregate echoed for observability. WI-S10-002 §6.1.9 watermark
    /// replay protocol relies on this arm for cron retry idempotency.
    SkippedDuplicateRun {
        /// The pre-existing aggregate (echoed for observability).
        existing: AggregatedCounter,
    },
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

    #[test]
    fn cloudevents_specversion_pinned() {
        assert_eq!(CLOUDEVENTS_SPECVERSION, "1.0");
    }

    #[test]
    fn counter_aggregated_event_type_pinned() {
        assert_eq!(COUNTER_AGGREGATED_EVENT_TYPE, "corelink.billing.counter.aggregated");
    }

    #[test]
    fn genesis_prev_hash_pinned() {
        assert_eq!(GENESIS_PREV_HASH, [0u8; 32]);
    }

    #[test]
    fn genesis_sequence_pinned() {
        assert_eq!(GENESIS_SEQUENCE_NUMBER, 0);
    }

    #[test]
    fn chain_hash_genesis_is_zero() {
        let g = ChainHash::genesis();
        assert_eq!(g.as_bytes(), &[0u8; 32]);
        assert_eq!(g.to_hex(), "0".repeat(64));
    }

    #[test]
    fn chain_hash_serde_hex_round_trip() {
        let h = ChainHash([0xCD; 32]);
        let s = serde_json::to_string(&h).unwrap();
        // 64 hex chars + 2 surrounding quotes = 66.
        assert_eq!(s.len(), 66);
        let back: ChainHash = serde_json::from_str(&s).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn chain_hash_invalid_hex_rejected() {
        let err = serde_json::from_str::<ChainHash>("\"zz\"").unwrap_err();
        assert!(format!("{err}").contains("hex decode"));
    }

    #[test]
    fn chain_hash_wrong_length_rejected() {
        let err = serde_json::from_str::<ChainHash>("\"ab\"").unwrap_err();
        assert!(format!("{err}").contains("expected 32 bytes"));
    }

    #[test]
    fn aggregated_counter_new_pins_canonical_attributes() {
        let tenant = Uuid::now_v7();
        let id = Uuid::now_v7();
        let a = AggregatedCounter::new(
            "corelink/region/iad/aggregator",
            id,
            1_700_000_000_000,
            0,
            ChainHash::genesis(),
            tenant,
            "2026-05",
            UsageEventKind::CasPut,
            100,
            10,
            1_700_000_000_000,
            1_703_000_000_000,
            vec![IdemKey::genesis()],
        );
        assert_eq!(a.specversion, "1.0");
        assert_eq!(a.event_type, COUNTER_AGGREGATED_EVENT_TYPE);
        assert_eq!(a.datacontenttype, "application/json");
        assert_eq!(a.subject, format!("tenant:{tenant}"));
        assert_eq!(a.sequence_number, 0);
        assert_eq!(a.prev_hash, ChainHash::genesis());
        assert_eq!(a.data.tenant_id, tenant);
        assert_eq!(a.data.billing_period, "2026-05");
        assert_eq!(a.data.event_kind, UsageEventKind::CasPut);
        assert_eq!(a.data.total_qty, 100);
        assert_eq!(a.data.event_count, 10);
    }

    #[test]
    fn aggregated_counter_group_key_extraction() {
        let tenant = Uuid::now_v7();
        let a = AggregatedCounter::new(
            "src",
            Uuid::now_v7(),
            1,
            0,
            ChainHash::genesis(),
            tenant,
            "2026-05",
            UsageEventKind::EgressBytes,
            0,
            0,
            0,
            1,
            vec![],
        );
        let k = a.group_key();
        assert_eq!(k.tenant_id, tenant);
        assert_eq!(k.billing_period, "2026-05");
        assert_eq!(k.event_kind, UsageEventKind::EgressBytes);
    }

    #[test]
    fn aggregated_counter_round_trips_via_serde_json() {
        let tenant = Uuid::now_v7();
        let a = AggregatedCounter::new(
            "src",
            Uuid::now_v7(),
            42,
            7,
            ChainHash([0xAB; 32]),
            tenant,
            "2026-05",
            UsageEventKind::EgressBytes,
            1024 * 1024,
            16,
            0,
            1000,
            vec![IdemKey([0xCD; 32]), IdemKey([0xEF; 32])],
        );
        let s = serde_json::to_string(&a).unwrap();
        let back: AggregatedCounter = serde_json::from_str(&s).unwrap();
        assert_eq!(a, back);
    }

    #[test]
    fn aggregation_decision_variants_distinct() {
        let tenant = Uuid::now_v7();
        let no_events = AggregationDecision::SkippedNoEvents {
            tenant_id: tenant,
            billing_period: "2026-05".to_string(),
            event_kind: UsageEventKind::CasPut,
        };
        let agg = AggregationDecision::Aggregated(AggregatedCounter::new(
            "src",
            Uuid::now_v7(),
            1,
            0,
            ChainHash::genesis(),
            tenant,
            "2026-05",
            UsageEventKind::CasPut,
            0,
            0,
            0,
            1,
            vec![],
        ));
        assert_ne!(format!("{no_events:?}"), format!("{agg:?}"));
    }
}
