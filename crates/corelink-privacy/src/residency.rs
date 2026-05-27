//! `corelink-privacy-residency-enforcement` — Residency Pinning E2E (WI-S11-007).
//!
//! # Overview
//!
//! Schrems II (CJEU C-311/18) + LGPD Art. 33 §1º + GDPR Art. 44 runtime enforcement.
//!
//! This crate ships the **pure-logic skeleton** of the residency enforcement
//! primitive (per `trait-abstraction-defer` charter). Production wiring
//! (Cloudflare Workers custom domain routing, D1 trigger, R2 audit Object Lock 7y)
//! is deferred to the Worker layer.
//!
//! # What this crate ships
//!
//! 1. [`region`] — [`Region`] closed enum, 6-region canonical
//!    (`wnam/enam/weur/sam/apac/afr`) per `privacy_model.md §7.1` +
//!    `data_model.md §2.1 L95`. Adding a region requires an ADR + Privacy
//!    Officer + Compliance review (cardinality discipline; anti-pattern §32).
//!
//! 2. [`error`] — [`ResidencyViolation`] `#[non_exhaustive]` error taxonomy.
//!    Mismatch → HTTP 451 `legal_residency_violation` (PAT-ROUTING-PINNED-001
//!    fail-CLOSED canonical). Audit fail → 503 + retry-after=60s (AC-007).
//!
//! 3. [`audit_emit`] — [`ResidencyAuditEventType`] 2-type taxonomy +
//!    [`ResidencyAuditRecord`] + [`ResidencyAuditSink`] trait +
//!    [`InMemoryResidencyAuditSink`] + [`FailingResidencyAuditSink`].
//!    Canonical CloudEvents types: `dev.hugr.corelink.residency.{request_routed,
//!    write_rejected_cross_region}.v1` per Lote 10.9bis P0-G prefix.
//!
//! 4. [`assert_request`] — `assert_request_residency`: Worker pre-flight for
//!    request routing. Custom domain `<tenant_id>.<region>.corelink.humangr.com` regex
//!    match + 451 on mismatch.
//!
//! 5. [`assert_write`] — `assert_write_residency`: backend write pre-flight
//!    assertion (CAS PUT, AC PUT, audit emit, billing-events emit, manifest).
//!    D1 trigger (`trg_blob_meta_region_match`) provides database-level backstop.
//!
//! 6. [`enforcement`] — [`ResidencyEnforcement`] trait + [`InMemoryResidencyEnforcement`]
//!    orchestrator + [`FailClosedResidencyEnforcement`] for chaos tests.
//!    Per-instance `Arc<Mutex<>>` (F-001 closure; NEVER `static LazyLock<Mutex<>>`).
//!
//! 7. [`migration`] — [`RegionMigrationRequest`] + [`InMemoryMigrationStore`].
//!    POST /v1/admin/tenant/region-migration 30d cooldown + Privacy Officer +
//!    Compliance review (ADR-S11-011).
//!
//! # Audit fail-CLOSED ordering (S-06 P0-2 lesson)
//!
//! `lookup → emit_audit → mutate_state`
//!
//! State is **NEVER** mutated if audit emit fails. Tests verify state UNCHANGED
//! on audit emit failure (see `tests/regression_d1_check_constraints.rs`).
//!
//! # wasm32-unknown-unknown compatibility
//!
//! This crate is `wasm32-unknown-unknown` clean. No `ring`, no C toolchain
//! dependencies, no `tokio::spawn`. All deps are pure-Rust.
//!
//! # INV-DATA-RESIDENCY CRITICAL (Lote 10.11.0-bis §3.11)
//!
//! Runtime cobertura: custom domain routing + insert checks + 20k property test.
//! TLA+ scope split (Lote 10.11.0-bis-prime cycle 3): PARTIAL S-11 via
//! `dsr_erasure_atomicity.tla` (InvResidencyPinned + InvResidencyMonotonic);
//! FULL deferred to S-14 (`region_residency.tla` per ADR-S11-010).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
// W35-P2: inherited `//!` docs from the absorbed sibling crate use
// short paths that resolved at the former crate root.
#![allow(rustdoc::broken_intra_doc_links)]

pub mod assert_request;
pub mod assert_write;
pub mod audit_emit;
pub mod enforcement;
pub mod error;
pub mod migration;
pub mod region;

pub use error::ResidencyViolation;
pub use region::Region;

/// Tenant context — primary_region resolved from D1 `tenant.primary_region` column
/// (`data_model.md §4.1 L151`; NOT `tenant_metadata.region_pinned` — that legacy
/// column does not exist per sprint contract v1.2.0 Lote 10.11.0 correction).
#[derive(Debug, Clone)]
pub struct TenantCtx {
    /// Tenant identifier.
    pub tenant_id: String,
    /// Canonical primary region (from `tenant.primary_region`).
    pub primary_region: Region,
}

impl TenantCtx {
    /// Construct a new TenantCtx.
    pub fn new(tenant_id: impl Into<String>, primary_region: Region) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            primary_region,
        }
    }
}

/// Backend kind taxonomy for insert check enforcement.
///
/// All arms are `#[non_exhaustive]` per project convention.
/// Covers the 5+ paths specified in §6.1 C-1.5:
/// CAS PUT, AC PUT, audit emit, billing-events emit, manifest write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// Content-Addressable Storage — R2 CAS PUT.
    Cas,
    /// Action Cache — R2 AC PUT.
    Ac,
    /// Audit event emit — R2 `audit-<region>` Object Lock 7y.
    AuditEmit,
    /// Billing events staging — D1 billing_events_staging emit.
    BillingEvents,
    /// Manifest write — D1/R2 manifest PUT.
    Manifest,
    /// D1 operational metadata — `blob_meta` / `ac_meta` tables.
    D1Metadata,
    /// KV sessions / cached metadata.
    Kv,
}

impl std::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BackendKind::Cas => "cas",
            BackendKind::Ac => "ac",
            BackendKind::AuditEmit => "audit_emit",
            BackendKind::BillingEvents => "billing_events",
            BackendKind::Manifest => "manifest",
            BackendKind::D1Metadata => "d1_metadata",
            BackendKind::Kv => "kv",
        };
        f.write_str(s)
    }
}
