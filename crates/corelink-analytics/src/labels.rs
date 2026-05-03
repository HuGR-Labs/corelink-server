//! Enum-typed label cartesian — every label value is a closed
//! `#[non_exhaustive]` enum so the cartesian cardinality is bounded
//! at compile time.
//!
//! Lote 10.8bis cardinality discipline: forbidden labels (`trace_id`,
//! `tenant_id`, `request_id`, `blob_digest`) are NOT representable in
//! [`MetricLabelTuple`] by construction — there is no `String` slot.
//! The CI gate `scripts/cardinality_check.py` is the secondary defense
//! against PR amendments that would add such a slot.
//!
//! Per WI-S09-001 §1 invariant 3 + §1 invariant 7: tenant-tier
//! aggregation (5-tier canonical: free / solo / team / business /
//! enterprise; Lote 10.7bis P0-7 vocabulary FROZEN at the data model
//! layer); per-tenant attribution rides logs/Tempo (sampled), NOT
//! Prometheus.

use core::fmt;

/// 5-tier canonical pricing tier (Lote 10.7bis P0-7) — re-exported
/// from `corelink-eviction` to avoid drift; the analytics emit
/// surface uses the SAME enum the rate-limit + quota crates use, so
/// `tenant_tier` label values match across components by
/// construction.
pub use corelink_eviction::Tier;

/// Canonical 5-element `Tier` list — pinned for cardinality estimate
/// (5 = canonical tier label cardinality).
#[must_use]
pub const fn tier_canonical_list() -> &'static [Tier; 5] {
    &[
        Tier::Free,
        Tier::Solo,
        Tier::Team,
        Tier::Business,
        Tier::Enterprise,
    ]
}

/// Cloudflare colocode region — bounded canonical list (~30 sites).
///
/// Closed `#[non_exhaustive]` enum so cartesian cardinality is bounded;
/// adding a NEW region requires PR + cardinality re-estimate per
/// WI-S09-001 §6.1.13 chaos scenario 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Region {
    /// `iad` — US East (Ashburn).
    Iad,
    /// `sjc` — US West (San Jose).
    Sjc,
    /// `dfw` — US Central (Dallas).
    Dfw,
    /// `sea` — US Pacific NW (Seattle).
    Sea,
    /// `ord` — US Midwest (Chicago).
    Ord,
    /// `lhr` — EU (London).
    Lhr,
    /// `fra` — EU (Frankfurt).
    Fra,
    /// `ams` — EU (Amsterdam).
    Ams,
    /// `cdg` — EU (Paris).
    Cdg,
    /// `mad` — EU (Madrid).
    Mad,
    /// `gru` — SAM (São Paulo).
    Gru,
    /// `eze` — SAM (Buenos Aires).
    Eze,
    /// `bog` — SAM (Bogotá).
    Bog,
    /// `nrt` — APAC (Tokyo).
    Nrt,
    /// `sin` — APAC (Singapore).
    Sin,
    /// `syd` — APAC (Sydney).
    Syd,
    /// `hkg` — APAC (Hong Kong).
    Hkg,
    /// `bom` — APAC (Mumbai).
    Bom,
    /// `icn` — APAC (Seoul).
    Icn,
    /// `jnb` — AFR (Johannesburg).
    Jnb,
    /// `cpt` — AFR (Cape Town).
    Cpt,
    /// `dxb` — MENA (Dubai).
    Dxb,
}

impl Region {
    /// Canonical 3-letter colocode label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Iad => "iad",
            Self::Sjc => "sjc",
            Self::Dfw => "dfw",
            Self::Sea => "sea",
            Self::Ord => "ord",
            Self::Lhr => "lhr",
            Self::Fra => "fra",
            Self::Ams => "ams",
            Self::Cdg => "cdg",
            Self::Mad => "mad",
            Self::Gru => "gru",
            Self::Eze => "eze",
            Self::Bog => "bog",
            Self::Nrt => "nrt",
            Self::Sin => "sin",
            Self::Syd => "syd",
            Self::Hkg => "hkg",
            Self::Bom => "bom",
            Self::Icn => "icn",
            Self::Jnb => "jnb",
            Self::Cpt => "cpt",
            Self::Dxb => "dxb",
        }
    }
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `result` label for RED counter histograms (Success / 4xx / 5xx).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum RedResultLabel {
    /// `result=success`.
    Success,
    /// `result=client_error_4xx`.
    ClientError4xx,
    /// `result=server_error_5xx`.
    ServerError5xx,
}

impl RedResultLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::ClientError4xx => "client_error_4xx",
            Self::ServerError5xx => "server_error_5xx",
        }
    }
}

impl fmt::Display for RedResultLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `hit_miss` label for `corelink_ac_lookup_requests_total`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum AcHitMissLabel {
    /// `hit_miss=hit`.
    Hit,
    /// `hit_miss=miss`.
    Miss,
}

impl AcHitMissLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hit => "hit",
            Self::Miss => "miss",
        }
    }
}

/// `phase` label for `corelink_gc_runs_total` (S-06 GC pipeline).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum GcPhaseLabel {
    /// `phase=mark`.
    Mark,
    /// `phase=sweep`.
    Sweep,
    /// `phase=soft_delete`.
    SoftDelete,
    /// `phase=phys_delete`.
    PhysDelete,
}

impl GcPhaseLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mark => "mark",
            Self::Sweep => "sweep",
            Self::SoftDelete => "soft_delete",
            Self::PhysDelete => "phys_delete",
        }
    }
}

/// `layer` label for `corelink_rate_limit_rejects_total` — the 4-layer
/// PAT-RATE-LIMIT-001 bulkhead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum RateLimitLayerLabel {
    /// `layer=global` — camada-0 global circuit breaker.
    Global,
    /// `layer=per_tenant` — camada-1 per-tenant DO bucket.
    PerTenant,
    /// `layer=per_ip` — camada-2 per-IP edge.
    PerIp,
    /// `layer=per_tenant_per_endpoint` — camada-3 per-endpoint scope.
    PerTenantPerEndpoint,
}

impl RateLimitLayerLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::PerTenant => "per_tenant",
            Self::PerIp => "per_ip",
            Self::PerTenantPerEndpoint => "per_tenant_per_endpoint",
        }
    }
}

/// `reason` label for `corelink_rate_limit_rejects_total`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum RateLimitReasonLabel {
    /// `reason=bucket_drained`.
    BucketDrained,
    /// `reason=cidr_blocked`.
    CidrBlocked,
    /// `reason=quota_exceeded`.
    QuotaExceeded,
    /// `reason=abuse_score_high`.
    AbuseScoreHigh,
    /// `reason=global_circuit_open`.
    GlobalCircuitOpen,
    /// `reason=canceled_tenant`.
    CanceledTenant,
}

impl RateLimitReasonLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BucketDrained => "bucket_drained",
            Self::CidrBlocked => "cidr_blocked",
            Self::QuotaExceeded => "quota_exceeded",
            Self::AbuseScoreHigh => "abuse_score_high",
            Self::GlobalCircuitOpen => "global_circuit_open",
            Self::CanceledTenant => "canceled_tenant",
        }
    }
}

/// `dsr_type` label for `corelink_privacy_dsr_active_total` (S-11
/// LGPD/GDPR data subject request types).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DsrTypeLabel {
    /// `dsr_type=access`.
    Access,
    /// `dsr_type=erasure`.
    Erasure,
    /// `dsr_type=portability`.
    Portability,
    /// `dsr_type=rectification`.
    Rectification,
    /// `dsr_type=objection`.
    Objection,
}

impl DsrTypeLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Erasure => "erasure",
            Self::Portability => "portability",
            Self::Rectification => "rectification",
            Self::Objection => "objection",
        }
    }
}

/// `event_type` label for `corelink_billing_events_emitted_total` (S-10
/// metered usage events).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BillingEventTypeLabel {
    /// `event_type=cas_bytes_stored`.
    CasBytesStored,
    /// `event_type=cas_bytes_egress`.
    CasBytesEgress,
    /// `event_type=ac_lookups`.
    AcLookups,
    /// `event_type=requests_total`.
    RequestsTotal,
    /// `event_type=overage_charge`.
    OverageCharge,
}

impl BillingEventTypeLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CasBytesStored => "cas_bytes_stored",
            Self::CasBytesEgress => "cas_bytes_egress",
            Self::AcLookups => "ac_lookups",
            Self::RequestsTotal => "requests_total",
            Self::OverageCharge => "overage_charge",
        }
    }
}

/// `bucket` label for `corelink_r2_ops_total` (USE metric).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum R2BucketLabel {
    /// `bucket=cas`.
    Cas,
    /// `bucket=ac`.
    Ac,
    /// `bucket=audit`.
    Audit,
    /// `bucket=manifest`.
    Manifest,
    /// `bucket=multipart`.
    Multipart,
}

impl R2BucketLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cas => "cas",
            Self::Ac => "ac",
            Self::Audit => "audit",
            Self::Manifest => "manifest",
            Self::Multipart => "multipart",
        }
    }
}

/// `op_type` label for `corelink_r2_ops_total` (USE metric).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum R2OpTypeLabel {
    /// `op_type=put`.
    Put,
    /// `op_type=get`.
    Get,
    /// `op_type=delete`.
    Delete,
    /// `op_type=list`.
    List,
}

impl R2OpTypeLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Put => "put",
            Self::Get => "get",
            Self::Delete => "delete",
            Self::List => "list",
        }
    }
}

/// `namespace` label for `corelink_kv_*_quota_used` (USE metric).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum KvNamespaceLabel {
    /// `namespace=tenant_meta`.
    TenantMeta,
    /// `namespace=session_cache`.
    SessionCache,
    /// `namespace=feature_flags`.
    FeatureFlags,
    /// `namespace=rate_limit_state`.
    RateLimitState,
    /// `namespace=blocklist`.
    Blocklist,
}

impl KvNamespaceLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TenantMeta => "tenant_meta",
            Self::SessionCache => "session_cache",
            Self::FeatureFlags => "feature_flags",
            Self::RateLimitState => "rate_limit_state",
            Self::Blocklist => "blocklist",
        }
    }
}

/// `do_class` label for `corelink_do_storage_size_bytes` (USE metric).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DoClassLabel {
    /// `do_class=rate_limiter`.
    RateLimiter,
    /// `do_class=quota`.
    Quota,
    /// `do_class=global_circuit`.
    GlobalCircuit,
    /// `do_class=tenant_state`.
    TenantState,
}

impl DoClassLabel {
    /// Canonical label string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RateLimiter => "rate_limiter",
            Self::Quota => "quota",
            Self::GlobalCircuit => "global_circuit",
            Self::TenantState => "tenant_state",
        }
    }
}

/// Canonical label tuple — all 9 RED + 6 USE label slots inhabit this
/// shape; per-metric only the relevant slots are populated. The
/// validator hashes the populated tuple to derive the unique-tuple
/// count for a metric.
///
/// Lote 10.8bis discipline reminder: there is NO `String` slot on this
/// struct. Forbidden labels (`trace_id` / `tenant_id` / `request_id` /
/// `blob_digest`) are not representable by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MetricLabelTuple {
    /// `tenant_tier` (5-tier canonical; Lote 10.7bis P0-7).
    pub tenant_tier: Option<Tier>,
    /// `region` (canonical CF colocode).
    pub region: Option<Region>,
    /// `result` (Success / ClientError4xx / ServerError5xx).
    pub result: Option<RedResultLabel>,
    /// `hit_miss` (Hit / Miss) — `corelink_ac_lookup_requests_total`.
    pub hit_miss: Option<AcHitMissLabel>,
    /// `phase` (Mark / Sweep / SoftDelete / PhysDelete) —
    /// `corelink_gc_runs_total`.
    pub phase: Option<GcPhaseLabel>,
    /// `layer` (4-layer bulkhead) — `corelink_rate_limit_rejects_total`.
    pub layer: Option<RateLimitLayerLabel>,
    /// `reason` — `corelink_rate_limit_rejects_total`.
    pub reason: Option<RateLimitReasonLabel>,
    /// `dsr_type` — `corelink_privacy_dsr_active_total`.
    pub dsr_type: Option<DsrTypeLabel>,
    /// `event_type` — `corelink_billing_events_emitted_total`.
    pub event_type: Option<BillingEventTypeLabel>,
    /// `bucket` — `corelink_r2_ops_total`.
    pub bucket: Option<R2BucketLabel>,
    /// `op_type` — `corelink_r2_ops_total`.
    pub op_type: Option<R2OpTypeLabel>,
    /// `namespace` — `corelink_kv_*_quota_used`.
    pub namespace: Option<KvNamespaceLabel>,
    /// `do_class` — `corelink_do_storage_size_bytes`.
    pub do_class: Option<DoClassLabel>,
}

impl MetricLabelTuple {
    /// Empty label tuple — every slot `None`.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            tenant_tier: None,
            region: None,
            result: None,
            hit_miss: None,
            phase: None,
            layer: None,
            reason: None,
            dsr_type: None,
            event_type: None,
            bucket: None,
            op_type: None,
            namespace: None,
            do_class: None,
        }
    }

    /// Convenience: tenant_tier + region (covers most RED metrics).
    #[must_use]
    pub const fn tenant_region(tenant_tier: Tier, region: Region) -> Self {
        Self {
            tenant_tier: Some(tenant_tier),
            region: Some(region),
            ..Self::empty()
        }
    }
}

/// List of label-name strings that MUST NOT appear in any metric label
/// slot per Lote 10.8bis cardinality discipline. The Rust type system
/// rejects these at compile time (no `String` slot in
/// [`MetricLabelTuple`]); this list is exposed for the CI lint
/// `scripts/cardinality_check.py` to scan PR diffs for label-name
/// regressions.
#[must_use]
pub const fn forbidden_label_names() -> &'static [&'static str; 4] {
    &["trace_id", "tenant_id", "request_id", "blob_digest"]
}

/// Public re-export of the forbidden-label list.
pub const FORBIDDEN_LABEL_NAMES: &[&str; 4] = forbidden_label_names();

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
    fn tier_canonical_5_strings() {
        let v = tier_canonical_list();
        assert_eq!(v.len(), 5);
        let mut set = std::collections::HashSet::new();
        for t in v {
            assert!(set.insert(t.as_str()));
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn region_strings_unique_3char() {
        let regions = [
            Region::Iad,
            Region::Sjc,
            Region::Dfw,
            Region::Sea,
            Region::Ord,
            Region::Lhr,
            Region::Fra,
            Region::Ams,
            Region::Cdg,
            Region::Mad,
            Region::Gru,
            Region::Eze,
            Region::Bog,
            Region::Nrt,
            Region::Sin,
            Region::Syd,
            Region::Hkg,
            Region::Bom,
            Region::Icn,
            Region::Jnb,
            Region::Cpt,
            Region::Dxb,
        ];
        let mut set = std::collections::HashSet::new();
        for r in regions {
            assert_eq!(r.as_str().len(), 3);
            assert!(set.insert(r.as_str()));
        }
        assert_eq!(set.len(), 22);
    }

    #[test]
    fn forbidden_labels_pinned_to_4_names() {
        let f = forbidden_label_names();
        assert_eq!(f.len(), 4);
        assert!(f.contains(&"trace_id"));
        assert!(f.contains(&"tenant_id"));
        assert!(f.contains(&"request_id"));
        assert!(f.contains(&"blob_digest"));
    }

    #[test]
    fn tuple_empty_has_all_none_slots() {
        let t = MetricLabelTuple::empty();
        assert!(t.tenant_tier.is_none());
        assert!(t.region.is_none());
        assert!(t.result.is_none());
        assert!(t.hit_miss.is_none());
        assert!(t.phase.is_none());
        assert!(t.layer.is_none());
        assert!(t.reason.is_none());
        assert!(t.dsr_type.is_none());
        assert!(t.event_type.is_none());
        assert!(t.bucket.is_none());
        assert!(t.op_type.is_none());
        assert!(t.namespace.is_none());
        assert!(t.do_class.is_none());
    }

    #[test]
    fn tuple_tenant_region_constructor() {
        let t = MetricLabelTuple::tenant_region(Tier::Team, Region::Iad);
        assert_eq!(t.tenant_tier, Some(Tier::Team));
        assert_eq!(t.region, Some(Region::Iad));
        assert!(t.result.is_none());
    }

    #[test]
    fn red_result_label_canonical_strings() {
        assert_eq!(RedResultLabel::Success.as_str(), "success");
        assert_eq!(
            RedResultLabel::ClientError4xx.as_str(),
            "client_error_4xx"
        );
        assert_eq!(
            RedResultLabel::ServerError5xx.as_str(),
            "server_error_5xx"
        );
    }

    #[test]
    fn rate_limit_layer_canonical_4() {
        let layers = [
            RateLimitLayerLabel::Global,
            RateLimitLayerLabel::PerTenant,
            RateLimitLayerLabel::PerIp,
            RateLimitLayerLabel::PerTenantPerEndpoint,
        ];
        let mut set = std::collections::HashSet::new();
        for l in layers {
            assert!(set.insert(l.as_str()));
        }
        assert_eq!(set.len(), 4);
    }

    #[test]
    fn dsr_type_canonical_5() {
        let v = [
            DsrTypeLabel::Access,
            DsrTypeLabel::Erasure,
            DsrTypeLabel::Portability,
            DsrTypeLabel::Rectification,
            DsrTypeLabel::Objection,
        ];
        let mut set = std::collections::HashSet::new();
        for d in v {
            assert!(set.insert(d.as_str()));
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn tuple_eq_and_hash_consistent() {
        use std::collections::HashSet;
        let a = MetricLabelTuple::tenant_region(Tier::Team, Region::Iad);
        let b = MetricLabelTuple::tenant_region(Tier::Team, Region::Iad);
        let c = MetricLabelTuple::tenant_region(Tier::Free, Region::Iad);
        assert_eq!(a, b);
        assert_ne!(a, c);
        let mut set = HashSet::new();
        set.insert(a);
        set.insert(b);
        assert_eq!(set.len(), 1, "Hash/Eq drift");
        set.insert(c);
        assert_eq!(set.len(), 2);
    }
}
