//! [`JitterPolicy`] + miss-arm marker types ([`MissMarker`] + [`MissArm`]).
//!
//! Split from monolith `middleware/timing_padding.rs` (wave-33 stage
//! 2.PRE-A.3).

/// Per-request jitter source policy (ADR-0023 §"Decision" 1.2).
#[derive(Clone, Debug)]
pub enum JitterPolicy {
    /// Production policy: deterministic per-request seed derived from
    /// the request's `x-request-id` header **mixed with the
    /// per-layer server-side secret** (codex round-1 P1 fix —
    /// preventing client-controlled-seed attacks). When the header is
    /// absent, the fallback monotonic counter feeds the same mix.
    /// Independent across requests + unpredictable to the attacker.
    Seeded,
    /// Test policy: identical jitter every call. **Never** use in
    /// production — defeats the correlation-resistance property. The
    /// adversarial test in `tests/timing_indistinguishability.rs`
    /// uses [`Self::Seeded`]; this variant is reserved for property
    /// tests asserting padding-window arithmetic alone.
    FixedForTests {
        /// Forced jitter offset in `[-jitter_pct%, +jitter_pct%]`.
        signed_pct: i8,
    },
}

/// Marker extension a handler attaches to its [`http::Response`] to opt
/// the response into miss-classified padding regardless of HTTP
/// `StatusCode`. The optional [`MissArm`] discriminator lets the
/// timing-padding emit hook record which canonical
/// `corelink-reapi::read::MissReason` arm produced the miss — the
/// load-bearing field for the S-09 aggregation that computes
/// `corelink_cas_side_channel_timing_diff_ms` as a pairwise median
/// across arms (codex round-5 P1 fix; without `miss_arm` the
/// aggregation cannot reconstruct per-arm distributions from the
/// per-request stream).
///
/// Production wiring: gRPC + HTTP handlers that surface
/// `MissReason` insert
/// `response.extensions_mut().insert(MissMarker::for_arm(MissArm::Tombstoned))`
/// (or the appropriate arm) before returning. The
/// [`super::predicate::miss_predicates::extension_marker`] predicate
/// keys off the presence of the marker; the emit hook reads the
/// optional [`MissArm`] for the aggregation discriminator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MissMarker {
    /// Optional arm discriminator for S-09 aggregation; `None`
    /// surfaces in the emit log as `unknown` (still padded, but
    /// without per-arm attribution).
    pub arm: Option<MissArm>,
}

impl MissMarker {
    /// Construct an arm-less marker (used when the handler cannot
    /// classify the arm — pre-S-02-001 paths or future REST
    /// endpoints).
    #[must_use]
    pub const fn new() -> Self {
        Self { arm: None }
    }

    /// Construct a marker carrying a canonical arm discriminator.
    /// Production wiring: pass the
    /// `corelink-reapi::read::MissReason → MissArm` mapping.
    #[must_use]
    pub const fn for_arm(arm: MissArm) -> Self {
        Self { arm: Some(arm) }
    }
}

/// Canonical `corelink-reapi::read::MissReason` discriminator used
/// by the timing-padding emit hook for S-09 aggregation. Mirrors
/// the read orchestrator's enum at the layer boundary so
/// `corelink-worker` does not depend on `corelink-reapi` (cycles
/// would block wasm32 builds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MissArm {
    /// `NeverExisted` — folds in the conflated `CrossTenantMasked`
    /// arm at the orchestrator surface (per ADR-0028 v1.1.0).
    NeverExisted,
    /// `Tombstoned` — D1 row found, `deleted_at != NULL`.
    Tombstoned,
    /// `R2OrphanRow` — D1 row alive + AuthZ pass + R2 NotFound.
    R2OrphanRow,
}

impl MissArm {
    /// Static label used in structured logs + aggregations.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::NeverExisted => "never_existed",
            Self::Tombstoned => "tombstoned",
            Self::R2OrphanRow => "r2_orphan_row",
        }
    }
}
