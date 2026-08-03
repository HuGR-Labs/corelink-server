//! Canonical [`UsageEvent`] CloudEvents 1.0 envelope + 8-element
//! `UsageEventKind` taxonomy + BLAKE3-derived `idem_key` slot.
//!
//! ## Why CloudEvents 1.0 (CNCF spec)
//!
//! Per WI-S10-001 §5.1 + sprint contract §5.1 R-S10-1, the billing emit
//! surface is canonical CloudEvents 1.0 so any downstream finance /
//! billing aggregator (Stripe, Lago) can ingest the events without a
//! custom adapter. The required attributes per CloudEvents §3.1 are:
//!
//! - `id` (canonical UUIDv7 — time-ordered + globally unique)
//! - `source` (URI; e.g. `corelink/region/iad`)
//! - `specversion` (hard-pinned to `"1.0"` per Lote 10.9bis P0-G)
//! - `type` (canonical dotted slug —
//!   `"corelink.billing.usage.recorded"` per WI-S10-001 §1)
//! - `time` (RFC 3339 — represented as Unix epoch ms `time_ms` in the
//!   wire shape; server-side conversion is canonical)
//! - `data` (the typed payload —
//!   `{tenant_id, event_kind, qty, unit, billing_period}`)
//!
//! Optional CloudEvents attributes used:
//!
//! - `datacontenttype` (canonical `"application/json"`)
//! - `subject` (the canonical `tenant:<uuid>` per
//!   WI-S10-001 + audit chain S-09 inheritance)
//!
//! ## CoreLink-specific extensions (CloudEvents extension attributes)
//!
//! - `idem_key` — 32-byte BLAKE3-256 derived from the JCS-canonical
//!   bytes of the event with the `idem_key` slot zeroed for the link
//!   input (mirrors the `prev_hash`-zeroed-link-input pattern from the
//!   audit chain Bitcoin-genesis convention; see
//!   [`crate::idempotency::derive_idem_key`]). Hex-rendered on the wire
//!   for the 64-char canonical form (mirrors `ChainHash` from
//!   `corelink-audit-chain`).
//!
//! ## Why BLAKE3-256 derived idem_key (NOT a random UUID)
//!
//! Per the WI-S10-001 §1 invariant 2 + sprint contract §5.1 R-S10-2:
//! the idempotency key is **deterministic across the canonical event
//! bytes**, so a Cloudflare Worker retry that re-emits the SAME billable
//! event surface returns the SAME idem_key — the deduplication is then
//! a pure set-membership check at the [`crate::idempotency`] layer.
//! Random UUIDs would NOT survive Worker retry (each retry would
//! produce a new UUID + a duplicate billing event). The BLAKE3 / JCS
//! derivation is the canonical Stripe-style content-addressable
//! idempotency key (sprint contract §5.3 R-S10-6 inheritance for
//! external Stripe surface; here we apply the same discipline at the
//! INTERNAL surface).
//!
//! Collision resistance: BLAKE3-256 has 128-bit collision security; the
//! probability of two distinct canonical event bodies mapping to the
//! same idem_key is < 2^-128 — orders of magnitude below ANY production
//! billing volume (10^9 events/yr → < 10^-110 birthday-bound expected
//! collisions over the 7y retention window).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use corelink_analytics::Region;

/// Canonical CloudEvents `specversion` attribute. Hard-pinned to
/// `"1.0"` per CloudEvents 1.0 §3.1.
pub const CLOUDEVENTS_SPECVERSION: &str = "1.0";

/// Canonical CloudEvents `datacontenttype` attribute. Hard-pinned to
/// `"application/json"`.
pub const CLOUDEVENTS_DATACONTENTTYPE: &str = "application/json";

/// Canonical CloudEvents `type` attribute. Hard-pinned per WI-S10-001
/// §1 — every usage event recorded by this emit surface carries the
/// SAME canonical `type` (the operation taxonomy lives in
/// [`UsageEventKind`] which is part of the `data` payload, not the CE
/// envelope `type`). This keeps the CE wire shape uniform across the
/// 6-element kind taxonomy + lets downstream Stripe / Lago routers
/// dispatch on `event_kind` directly without parsing variant-typed
/// envelopes.
pub const USAGE_EVENT_TYPE: &str = "corelink.billing.usage.recorded";

/// Canonical genesis idem_key — 32 bytes of `0x00`. Used as the
/// "zeroed-slot" link input when computing the canonical idem_key (so
/// the BLAKE3 derivation is well-defined for the FIRST canonicalization
/// pass — the second pass writes the derived bytes back into the slot
/// for the on-the-wire shape). Mirrors the
/// `corelink_audit_chain::GENESIS_PREV_HASH` convention.
pub const GENESIS_IDEM_KEY: [u8; 32] = [0u8; 32];

/// Canonical 8-element usage-event kind taxonomy per WI-S10-001 §1
/// (StorageBytesHourly / EgressBytes / AcLookup / CasGet / CasPut /
/// ReplayRequest) + the ASK-2 runner-billing addition
/// (RunnerSlotSeconds — NON-Stripe-billable, like ReplayRequest) + the
/// 2026-08-02 runner-overage addition (RunnerVcpuSeconds — the BILLABLE
/// runner compute unit; owner-ratified the same day, superseding the
/// "concurrency priced, minutes unlimited" model).
///
/// `#[non_exhaustive]` so follow-on WIs (S-13 admin / S-19 Stripe) can
/// extend the taxonomy additively without breaking downstream Stripe /
/// Lago routers. Adding a NEW variant requires Finance + Compliance
/// sign-off (per WI-S10-001 §6.1.6) — schema versioning policy
/// (sprint contract §5.1 R-S10-3) keeps two consecutive versions
/// processable concurrently.
///
/// ## Why this 6-element set (NOT WI's "5 SKUs canonical")
///
/// The WI-S10-001 spec enumerates 5 SKUs at the Stripe surface
/// (`cas_storage_gb_month` / `cas_egress_gb` / `cas_put_op_count` /
/// `cas_get_op_count` / `ac_lookup_op_count`); the emit surface sees
/// 6 KINDS because the replay forensic endpoint
/// (`POST /v1/billing/replay` per WI-S10-006) emits its own
/// `ReplayRequest` events for the audit trail (out-of-band of the
/// Stripe SKU stream). Production wiring at WI-S10-007 maps the 6
/// kinds → 5 Stripe SKUs in the counter aggregator (WI-S10-002).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum UsageEventKind {
    /// `storage_bytes_hourly` — hourly snapshot of bytes_used per
    /// tenant (Stripe SKU `cas_storage_gb_month` source).
    StorageBytesHourly,
    /// `egress_bytes` — bytes flowing OUT of the CoreLink edge
    /// (Stripe SKU `cas_egress_gb` source).
    EgressBytes,
    /// `ac_lookup` — Action Cache lookup events (Stripe SKU
    /// `ac_lookup_op_count` source).
    AcLookup,
    /// `cas_get` — CAS read events (Stripe SKU `cas_get_op_count`
    /// source).
    CasGet,
    /// `cas_put` — CAS write events (Stripe SKU `cas_put_op_count`
    /// source).
    CasPut,
    /// `replay_request` — replay-from-events forensic API hits (NOT
    /// Stripe-billable; audit trail per WI-S10-006).
    ReplayRequest,
    /// `runner_slot_seconds` — per-lease runner wall-clock occupancy in
    /// SLOT-SECONDS, pushed by the corelink-runners fabric per finished
    /// lease (ASK-2 runner billing usage-push ingest).
    ///
    /// **NON-Stripe-billable, and it STAYS that way.** This is capacity /
    /// cost-reconciliation telemetry: how long a slot was held, independent of
    /// how big the box was. It is deliberately NOT the billing unit — see
    /// [`Self::RunnerVcpuSeconds`], which supersedes it for money.
    ///
    /// ⚠️ HISTORICAL NOTE (2026-08-02). This variant used to carry the sentence
    /// *"the owner-ratified 'concurrency priced, minutes unlimited' runner
    /// pricing model … the minutes a tenant burns are unmetered for billing"*.
    /// **That model was superseded by the owner the same day**: minutes above the
    /// tier's included `runners_entitlement.max_vcpu_h` are now billed as
    /// overage. The sentence is preserved here, struck, rather than deleted —
    /// a pricing model that lived in a doc-comment is exactly the kind of
    /// decision that gets silently re-derived from stale code by the next
    /// reader. Concurrency is still an entitlement axis; it is no longer the
    /// ONLY billable one.
    RunnerSlotSeconds,
    /// `runner_vcpu_seconds` — the BILLABLE runner compute unit: wall-clock
    /// seconds a runner box was ALLOCATED, multiplied by that box's vCPU count.
    ///
    /// ## Why vCPU-seconds and not slot-seconds
    ///
    /// The entitlement this meters against is `runners_entitlement.max_vcpu_h`,
    /// denominated in vCPU-HOURS. A slot-second is not a vCPU-second: on the
    /// current 4-vCPU runner box they differ by exactly 4×, so metering
    /// slot-seconds against a vCPU-hour ceiling under-bills by 4× — silently,
    /// because both numbers look like "seconds".
    ///
    /// ## Why the emitter multiplies (and this carries no size field)
    ///
    /// The multiplication happens at the emitter, which is the only component
    /// that KNOWS the box it just tore down. That keeps the wire shape frozen
    /// AND makes the unit correct-by-construction when the fleet stops being
    /// one size: an 8-vCPU or 64-GiB SKU multiplies by its own vCPU count with
    /// no change here, no migration, and no per-kind conversion table that
    /// could drift from the hardware. A fixed ×4 anywhere in this path would
    /// under-bill every future box the day it ships.
    ///
    /// ## Why ALLOCATED wall-clock, not CPU time
    ///
    /// Cloudflare bills memory + disk by ALLOCATION (wall-clock), not by CPU
    /// consumed — measured on a real run at 490 allocated-seconds vs 170
    /// cpu-seconds. Metering `cpuTimeSec` would therefore under-count COGS by
    /// ~3× and break the loss-proof floor the pricing ladder assumes.
    RunnerVcpuSeconds,
}

impl UsageEventKind {
    /// Canonical kind string (the value of the `event_kind` field in
    /// the typed `UsageEventData` payload).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StorageBytesHourly => "storage_bytes_hourly",
            Self::EgressBytes => "egress_bytes",
            Self::AcLookup => "ac_lookup",
            Self::CasGet => "cas_get",
            Self::CasPut => "cas_put",
            Self::ReplayRequest => "replay_request",
            Self::RunnerSlotSeconds => "runner_slot_seconds",
            Self::RunnerVcpuSeconds => "runner_vcpu_seconds",
        }
    }

    /// Canonical billable unit string for this event kind. Production
    /// wiring at the counter aggregator (WI-S10-002) consumes this to
    /// route into per-SKU buckets without reparsing the kind taxonomy.
    #[must_use]
    pub const fn canonical_unit(self) -> UsageUnit {
        match self {
            Self::StorageBytesHourly | Self::EgressBytes => UsageUnit::Bytes,
            Self::AcLookup
            | Self::CasGet
            | Self::CasPut
            | Self::ReplayRequest
            | Self::RunnerSlotSeconds
            | Self::RunnerVcpuSeconds => UsageUnit::OpCount,
        }
    }
}

impl core::fmt::Display for UsageEventKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for UsageEventKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for UsageEventKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "storage_bytes_hourly" => Ok(Self::StorageBytesHourly),
            "egress_bytes" => Ok(Self::EgressBytes),
            "ac_lookup" => Ok(Self::AcLookup),
            "cas_get" => Ok(Self::CasGet),
            "cas_put" => Ok(Self::CasPut),
            "replay_request" => Ok(Self::ReplayRequest),
            "runner_slot_seconds" => Ok(Self::RunnerSlotSeconds),
            "runner_vcpu_seconds" => Ok(Self::RunnerVcpuSeconds),
            other => Err(serde::de::Error::custom(format!(
                "unknown UsageEventKind: {other}"
            ))),
        }
    }
}

/// Canonical 8-element `UsageEventKind` list. Pinned for cardinality
/// estimate + cross-component regression tests.
#[must_use]
pub const fn canonical_usage_event_kinds() -> &'static [UsageEventKind; 8] {
    &[
        UsageEventKind::StorageBytesHourly,
        UsageEventKind::EgressBytes,
        UsageEventKind::AcLookup,
        UsageEventKind::CasGet,
        UsageEventKind::CasPut,
        UsageEventKind::ReplayRequest,
        UsageEventKind::RunnerSlotSeconds,
        UsageEventKind::RunnerVcpuSeconds,
    ]
}

/// Canonical billable-unit taxonomy (Bytes / OpCount). The counter
/// aggregator (WI-S10-002) consumes this to route per-SKU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum UsageUnit {
    /// `bytes` — quantity is a byte count (storage / egress).
    Bytes,
    /// `op_count` — quantity is an integer operation count
    /// (cas_get / cas_put / ac_lookup / replay_request).
    OpCount,
}

impl UsageUnit {
    /// Canonical unit string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bytes => "bytes",
            Self::OpCount => "op_count",
        }
    }
}

impl core::fmt::Display for UsageUnit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for UsageUnit {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for UsageUnit {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        match s.as_str() {
            "bytes" => Ok(Self::Bytes),
            "op_count" => Ok(Self::OpCount),
            other => Err(serde::de::Error::custom(format!(
                "unknown UsageUnit: {other}"
            ))),
        }
    }
}

/// Canonical 32-byte BLAKE3-256 idem_key newtype. Always hex-rendered
/// when serialized to JSON for the CloudEvents wire shape (RFC 4648 §8
/// hex-lowercase canonical form). Mirrors
/// `corelink_audit_chain::ChainHash`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IdemKey(pub [u8; 32]);

impl IdemKey {
    /// The canonical genesis idem_key (32 bytes of `0x00`); used as
    /// the zeroed-slot link input when deriving the canonical idem_key
    /// from the JCS bytes.
    #[must_use]
    pub const fn genesis() -> Self {
        Self(GENESIS_IDEM_KEY)
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

impl Serialize for IdemKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for IdemKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let bytes = hex::decode(&s)
            .map_err(|e| serde::de::Error::custom(format!("IdemKey hex decode failed: {e}")))?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom(format!(
                "IdemKey expected 32 bytes; got {}",
                bytes.len()
            )));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        Ok(Self(out))
    }
}

impl core::fmt::Display for IdemKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_hex())
    }
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

/// Typed payload of a [`UsageEvent`] (the value of the CloudEvents
/// `data` attribute). Per the WI-S10-001 §1 narrative the canonical
/// shape is `{tenant_id, event_kind, qty, unit, billing_period}`.
///
/// `qty` is the BILLABLE QUANTITY (bytes for storage / egress; op count
/// for cas_get / cas_put / ac_lookup / replay_request). `billing_period`
/// is the canonical `YYYY-MM` UTC month bucket; the format is enforced
/// at construction time via [`UsageEvent::new`] (returns an error for
/// any non-`YYYY-MM` shape).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageEventData {
    /// Tenant id of the billable operation. Per the WI-S10-001 §1
    /// invariant 6 + Lote 10.4bis: this MUST come from the
    /// authenticated `TenantCtx` middleware (S-03 inheritance) NOT
    /// the request body — the production wiring at the hot path
    /// extracts it from the JWT-authenticated context. The trait surface
    /// here treats it as opaque.
    pub tenant_id: Uuid,

    /// Canonical operation kind.
    pub event_kind: UsageEventKind,

    /// Billable quantity. Units per [`UsageUnit`].
    pub qty: u64,

    /// Canonical billable unit.
    pub unit: UsageUnit,

    /// Canonical UTC month bucket (`YYYY-MM` per RFC 3339-extended).
    /// Validated at construction in [`UsageEvent::new`].
    pub billing_period: String,
}

/// Canonical CloudEvents-1.0-aligned usage event shape with BLAKE3
/// idempotency key.
///
/// `Serialize + Deserialize` so the NDJSON serializer (used by the R2
/// PutObject pipeline at
/// `usage/{tenant_id}/{billing_period YYYY-MM}/{seq:08}.usage.ndjson`)
/// round-trips losslessly. The idempotency key (`idem_key`) is a
/// first-class CloudEvents extension attribute per CloudEvents §3.1.5.
///
/// ## Field ordering
///
/// CloudEvents 1.0 §3 declares the canonical attribute set; we follow
/// that order so the NDJSON output reads naturally for ops debugging.
/// JCS canonicalization (RFC 8785) at idem_key derivation time sorts
/// keys lexicographically regardless, so the on-the-wire field order is
/// purely a readability concern; the derived idem_key is invariant to it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageEvent {
    /// CloudEvents `specversion` — hard-pinned `"1.0"`.
    pub specversion: String,

    /// CloudEvents `type` — hard-pinned
    /// `"corelink.billing.usage.recorded"`.
    #[serde(rename = "type")]
    pub event_type: String,

    /// CloudEvents `source` — the originating Worker URI (e.g.
    /// `corelink/region/iad`). Used as the `source` URI per
    /// CloudEvents §3.1.1.
    pub source: String,

    /// CloudEvents `subject` — the canonical `tenant:<uuid>` slug
    /// (mirrors the audit chain S-09 inheritance). Allows downstream
    /// SIEM filtering by tenant without parsing the typed `data`.
    pub subject: String,

    /// CloudEvents `id` — UUIDv7 (time-ordered) per CloudEvents §3.1.4
    /// uniqueness requirement.
    pub id: Uuid,

    /// CloudEvents `time` — Unix epoch milliseconds (canonical `_ms`
    /// suffix per CloudEvents external surface).
    pub time_ms: u64,

    /// CloudEvents `datacontenttype` — hard-pinned
    /// `"application/json"`.
    pub datacontenttype: String,

    /// Canonical 3-char CF colocode (mirrors
    /// `corelink_analytics::Region::as_str()`).
    #[serde(
        serialize_with = "serialize_region",
        deserialize_with = "deserialize_region"
    )]
    pub region: Region,

    /// Typed payload `{tenant_id, event_kind, qty, unit,
    /// billing_period}` per the WI-S10-001 §1 spec.
    pub data: UsageEventData,

    /// 32-byte BLAKE3-256 idempotency key derived from the JCS bytes
    /// of the event with this slot zeroed for the link input. See
    /// [`crate::idempotency::derive_idem_key`] + the module-level
    /// rationale on why BLAKE3 / JCS derivation (NOT a random UUID).
    pub idem_key: IdemKey,
}

impl UsageEvent {
    /// Construct a canonical usage event. The `idem_key` slot is
    /// initialized to [`IdemKey::genesis`]; the orchestrator at
    /// [`crate::emitter::UsageEventEmitter::emit`] derives the
    /// canonical key via [`crate::idempotency::derive_idem_key`] +
    /// rewrites the slot before persisting. Callers wanting to
    /// pre-derive the key can invoke
    /// `Self::with_derived_idem_key`.
    ///
    /// Returns [`InvalidBillingPeriod`] when `billing_period` is not
    /// of the canonical `YYYY-MM` shape (e.g. wrong length / non-digit
    /// chars / month out of range).
    ///
    /// Eight-plus parameters reflect the canonical CloudEvents 1.0
    /// attribute set + the typed payload — every parameter is
    /// load-bearing at the data model. The alternative builder pattern
    /// would obscure the canonical CE 1.0 shape; the lint allowance is
    /// intentional.
    ///
    /// # Errors
    ///
    /// - [`InvalidBillingPeriod`] when the period string is not
    ///   `YYYY-MM` (length, digit shape, month range).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: impl Into<String>,
        id: Uuid,
        time_ms: u64,
        region: Region,
        tenant_id: Uuid,
        event_kind: UsageEventKind,
        qty: u64,
        billing_period: impl Into<String>,
    ) -> Result<Self, InvalidBillingPeriod> {
        let billing_period = billing_period.into();
        validate_billing_period(&billing_period)?;
        let unit = event_kind.canonical_unit();
        let subject = format!("tenant:{tenant_id}");
        Ok(Self {
            specversion: CLOUDEVENTS_SPECVERSION.to_string(),
            event_type: USAGE_EVENT_TYPE.to_string(),
            source: source.into(),
            subject,
            id,
            time_ms,
            datacontenttype: CLOUDEVENTS_DATACONTENTTYPE.to_string(),
            region,
            data: UsageEventData {
                tenant_id,
                event_kind,
                qty,
                unit,
                billing_period,
            },
            idem_key: IdemKey::genesis(),
        })
    }

    /// Borrow the canonical billing period (`YYYY-MM`).
    #[must_use]
    pub fn billing_period(&self) -> &str {
        &self.data.billing_period
    }
}

/// Validation error for the canonical `billing_period` `YYYY-MM` shape.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidBillingPeriod {
    /// Wrong length (canonical is exactly 7 chars: `YYYY-MM`).
    #[error("invalid billing_period: expected `YYYY-MM` (7 chars); got {0:?}")]
    WrongLength(String),
    /// Wrong shape (e.g. missing `-` separator, non-digit chars).
    #[error("invalid billing_period: expected `YYYY-MM`; got {0:?}")]
    WrongShape(String),
    /// Month out of range (must be `01..=12`).
    #[error("invalid billing_period month: expected 01..=12; got {0:?}")]
    MonthOutOfRange(String),
}

/// Validate a `billing_period` string matches the canonical `YYYY-MM`
/// shape. Pure-logic implementation; no `chrono` dependency (mirrors
/// `corelink-audit-chain::canonical_date_yyyy_mm_dd` discipline).
///
/// # Errors
///
/// - [`InvalidBillingPeriod::WrongLength`] when the string is not
///   exactly 7 chars.
/// - [`InvalidBillingPeriod::WrongShape`] when the chars don't match
///   `dddd-dd` (digits + `-` separator).
/// - [`InvalidBillingPeriod::MonthOutOfRange`] when the month part is
///   not in `01..=12`.
pub fn validate_billing_period(s: &str) -> Result<(), InvalidBillingPeriod> {
    if s.len() != 7 {
        return Err(InvalidBillingPeriod::WrongLength(s.to_string()));
    }
    let bytes = s.as_bytes();
    // Canonical shape: digit digit digit digit '-' digit digit
    let separator_ok = bytes.get(4) == Some(&b'-');
    let digits_ok = [0, 1, 2, 3, 5, 6]
        .iter()
        .all(|i| bytes.get(*i).is_some_and(u8::is_ascii_digit));
    if !separator_ok || !digits_ok {
        return Err(InvalidBillingPeriod::WrongShape(s.to_string()));
    }
    // Parse the month part (chars 5..7) into u8 + validate range.
    let month_str = s
        .get(5..7)
        .ok_or_else(|| InvalidBillingPeriod::WrongShape(s.to_string()))?;
    let month: u8 = month_str
        .parse()
        .map_err(|_| InvalidBillingPeriod::WrongShape(s.to_string()))?;
    if !(1..=12).contains(&month) {
        return Err(InvalidBillingPeriod::MonthOutOfRange(s.to_string()));
    }
    Ok(())
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
    fn cloudevents_datacontenttype_pinned() {
        assert_eq!(CLOUDEVENTS_DATACONTENTTYPE, "application/json");
    }

    #[test]
    fn usage_event_type_pinned() {
        assert_eq!(USAGE_EVENT_TYPE, "corelink.billing.usage.recorded");
    }

    #[test]
    fn genesis_idem_key_pinned() {
        assert_eq!(GENESIS_IDEM_KEY, [0u8; 32]);
    }

    /// Cardinality pin. It is SUPPOSED to fail when a kind is added — adding one
    /// requires Finance + Compliance sign-off (WI-S10-001 §6.1.6), and a silent
    /// taxonomy growth is exactly what that rule exists to prevent. Bumping this
    /// number is the deliberate act that records the sign-off happened.
    ///
    /// 7 → 8 on 2026-08-02: `RunnerVcpuSeconds`, the billable runner compute
    /// unit, added when the owner superseded the "concurrency priced, minutes
    /// unlimited" model with metered overage above `max_vcpu_h`.
    #[test]
    fn canonical_event_kinds_count_is_eight() {
        let v = canonical_usage_event_kinds();
        assert_eq!(v.len(), 8);
    }

    #[test]
    fn each_event_kind_has_unique_canonical_string() {
        let v = canonical_usage_event_kinds();
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(set.insert(k.as_str()));
        }
        assert_eq!(set.len(), 8);
    }

    #[test]
    fn kind_strings_pinned() {
        assert_eq!(
            UsageEventKind::StorageBytesHourly.as_str(),
            "storage_bytes_hourly"
        );
        assert_eq!(UsageEventKind::EgressBytes.as_str(), "egress_bytes");
        assert_eq!(UsageEventKind::AcLookup.as_str(), "ac_lookup");
        assert_eq!(UsageEventKind::CasGet.as_str(), "cas_get");
        assert_eq!(UsageEventKind::CasPut.as_str(), "cas_put");
        assert_eq!(UsageEventKind::ReplayRequest.as_str(), "replay_request");
        assert_eq!(
            UsageEventKind::RunnerSlotSeconds.as_str(),
            "runner_slot_seconds"
        );
        assert_eq!(
            UsageEventKind::RunnerVcpuSeconds.as_str(),
            "runner_vcpu_seconds"
        );
    }

    /// The two runner kinds are DISTINCT and must never be conflated: one is
    /// capacity telemetry (slot occupancy), the other is money (vCPU-seconds).
    /// They differ by the box's vCPU count — 4× on today's fleet — so a reader
    /// who treats them as synonyms under-bills by exactly that factor.
    #[test]
    fn the_two_runner_kinds_are_distinct() {
        assert_ne!(
            UsageEventKind::RunnerSlotSeconds,
            UsageEventKind::RunnerVcpuSeconds
        );
        assert_ne!(
            UsageEventKind::RunnerSlotSeconds.as_str(),
            UsageEventKind::RunnerVcpuSeconds.as_str()
        );
    }

    #[test]
    fn canonical_units_pinned_per_kind() {
        assert_eq!(
            UsageEventKind::StorageBytesHourly.canonical_unit(),
            UsageUnit::Bytes
        );
        assert_eq!(
            UsageEventKind::EgressBytes.canonical_unit(),
            UsageUnit::Bytes
        );
        assert_eq!(
            UsageEventKind::AcLookup.canonical_unit(),
            UsageUnit::OpCount
        );
        assert_eq!(UsageEventKind::CasGet.canonical_unit(), UsageUnit::OpCount);
        assert_eq!(UsageEventKind::CasPut.canonical_unit(), UsageUnit::OpCount);
        assert_eq!(
            UsageEventKind::ReplayRequest.canonical_unit(),
            UsageUnit::OpCount
        );
        assert_eq!(
            UsageEventKind::RunnerSlotSeconds.canonical_unit(),
            UsageUnit::OpCount
        );
    }

    #[test]
    fn usage_event_kind_serde_round_trip() {
        for k in canonical_usage_event_kinds() {
            let s = serde_json::to_string(k).unwrap();
            let back: UsageEventKind = serde_json::from_str(&s).unwrap();
            assert_eq!(*k, back);
        }
    }

    #[test]
    fn unknown_kind_rejects() {
        let line = r#""ZZ_unknown""#;
        let err = serde_json::from_str::<UsageEventKind>(line).unwrap_err();
        assert!(format!("{err}").contains("unknown UsageEventKind"));
    }

    #[test]
    fn usage_unit_serde_round_trip() {
        for u in [UsageUnit::Bytes, UsageUnit::OpCount] {
            let s = serde_json::to_string(&u).unwrap();
            let back: UsageUnit = serde_json::from_str(&s).unwrap();
            assert_eq!(u, back);
        }
    }

    #[test]
    fn unknown_unit_rejects() {
        let err = serde_json::from_str::<UsageUnit>(r#""bogus""#).unwrap_err();
        assert!(format!("{err}").contains("unknown UsageUnit"));
    }

    #[test]
    fn idem_key_genesis_is_zero() {
        let g = IdemKey::genesis();
        assert_eq!(g.as_bytes(), &[0u8; 32]);
        assert_eq!(g.to_hex(), "0".repeat(64));
    }

    #[test]
    fn idem_key_serde_hex_round_trip() {
        let h = IdemKey([0xCD; 32]);
        let s = serde_json::to_string(&h).unwrap();
        // 64 hex chars + 2 surrounding quotes = 66.
        assert_eq!(s.len(), 66);
        let back: IdemKey = serde_json::from_str(&s).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn idem_key_invalid_hex_rejected() {
        let err = serde_json::from_str::<IdemKey>("\"zz\"").unwrap_err();
        assert!(format!("{err}").contains("hex decode"));
    }

    #[test]
    fn idem_key_wrong_length_rejected() {
        let err = serde_json::from_str::<IdemKey>("\"ab\"").unwrap_err();
        assert!(format!("{err}").contains("expected 32 bytes"));
    }

    #[test]
    fn billing_period_validates_canonical() {
        assert!(validate_billing_period("2026-01").is_ok());
        assert!(validate_billing_period("2026-12").is_ok());
        assert!(validate_billing_period("1970-01").is_ok());
    }

    #[test]
    fn billing_period_rejects_non_canonical() {
        // Wrong length.
        assert!(matches!(
            validate_billing_period("2026"),
            Err(InvalidBillingPeriod::WrongLength(_))
        ));
        assert!(matches!(
            validate_billing_period("2026-001"),
            Err(InvalidBillingPeriod::WrongLength(_))
        ));
        // Wrong shape (missing dash).
        assert!(matches!(
            validate_billing_period("2026/01"),
            Err(InvalidBillingPeriod::WrongShape(_))
        ));
        // Wrong shape (non-digit).
        assert!(matches!(
            validate_billing_period("20a6-01"),
            Err(InvalidBillingPeriod::WrongShape(_))
        ));
        // Month out of range.
        assert!(matches!(
            validate_billing_period("2026-00"),
            Err(InvalidBillingPeriod::MonthOutOfRange(_))
        ));
        assert!(matches!(
            validate_billing_period("2026-13"),
            Err(InvalidBillingPeriod::MonthOutOfRange(_))
        ));
    }

    #[test]
    fn usage_event_new_pins_canonical_attributes() {
        let tenant = Uuid::now_v7();
        let id = Uuid::now_v7();
        let e = UsageEvent::new(
            "corelink/region/iad",
            id,
            1_700_000_000_000,
            Region::Iad,
            tenant,
            UsageEventKind::CasPut,
            4096,
            "2026-05",
        )
        .unwrap();
        assert_eq!(e.specversion, "1.0");
        assert_eq!(e.event_type, USAGE_EVENT_TYPE);
        assert_eq!(e.datacontenttype, "application/json");
        assert_eq!(e.subject, format!("tenant:{tenant}"));
        assert_eq!(e.region, Region::Iad);
        assert_eq!(e.data.tenant_id, tenant);
        assert_eq!(e.data.event_kind, UsageEventKind::CasPut);
        assert_eq!(e.data.qty, 4096);
        assert_eq!(e.data.unit, UsageUnit::OpCount);
        assert_eq!(e.data.billing_period, "2026-05");
        // idem_key initialized to genesis (orchestrator derives + rewrites).
        assert_eq!(e.idem_key, IdemKey::genesis());
    }

    #[test]
    fn usage_event_new_rejects_invalid_period() {
        let tenant = Uuid::now_v7();
        let res = UsageEvent::new(
            "corelink/region/iad",
            Uuid::now_v7(),
            1,
            Region::Iad,
            tenant,
            UsageEventKind::CasPut,
            1,
            "bogus",
        );
        assert!(res.is_err());
    }

    #[test]
    fn usage_event_round_trips_via_serde_json() {
        let tenant = Uuid::now_v7();
        let e = UsageEvent::new(
            "corelink/region/gru",
            Uuid::now_v7(),
            42,
            Region::Gru,
            tenant,
            UsageEventKind::EgressBytes,
            1024 * 1024,
            "2026-05",
        )
        .unwrap();
        let s = serde_json::to_string(&e).unwrap();
        let back: UsageEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(e, back);
    }
}
