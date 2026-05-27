//! Wave-21 closure: per-tenant pinned-region resolver for the Neon
//! analytics shadow.
//!
//! Wave-20 wired the production `TokioPgShadowSinkFactory` in
//! `apps/server/src/main.rs` to a hard-coded `Region::Iad` default —
//! every tenant resolved to the IAD pool regardless of their pinned
//! region (a TODO was left explicit at the swap point). This module
//! closes that TODO by introducing a thin abstraction that the boot
//! path consumes:
//!
//! - [`TenantRegionResolver`] — trait surface (`tenant_id → Region`).
//! - [`InMemoryTenantRegionResolver`] — staging / dev / test impl
//!   backed by a `HashMap<Uuid, Region>` + a configurable fallback
//!   region. Mirrors the wave-19 [`crate::neon_shadow::real::StaticResolver`]
//!   pattern for per-region DSN resolution.
//! - [`D1TenantRegionResolver`] — production impl that delegates to a
//!   [`TenantConfigStore`] (the trait that the CF Worker binder layer
//!   wires up against `worker::D1Database` via
//!   `corelink-cf-bindings::d1_real::CfD1DatabaseReal`). The store is
//!   queried for the `region` column of the `tenant_config` D1 table
//!   added by migration `0052_tenant_config_region.sql`.
//!
//! ## Why a trait and not a free function
//!
//! The trait-abstraction-defer charter (see `corelink-billing-stripe-materializer`
//! + `corelink-byok-aws-real` for the canonical pattern) keeps the
//! Postgres driver out of the wasm32 build AND lets the native gRPC
//! server stay on the in-memory mirror for dev/CI. The CF Worker
//! production boot path (`corelink-clerk-cf`) is the only call site
//! that ever constructs a real D1-backed
//! [`TenantConfigStore`]; everywhere else uses
//! [`InMemoryTenantRegionResolver`].
//!
//! ## Tenant-id type
//!
//! The trait takes `&Uuid` to match the existing
//! `ShadowSinkFactory::for_tenant(&self, tenant_id: Uuid)` signature
//! at the route layer (`apps/server/src/routes/audit_analytics.rs`).
//! The canonical newtype lives in `corelink-meta::TenantId`; pulling
//! the meta dep into this crate would create a cycle (meta → analytics
//! → audit-chain). The `Uuid` is the storage canonical form per
//! `data_model.md §2.1` and is the value bound at the SQL parameter
//! site by `RealNeonShadowSink`.
//!
//! ## Cross-refs
//!
//! - `specs/_audits/sealed/2026-05-16-neon-shadow-real-driver.md` §7
//!   (wave-21 closure note).
//! - `migrations/d1/0052_tenant_config_region.sql` — D1 column add.

use std::collections::HashMap;
use std::fmt;

use uuid::Uuid;

use corelink_analytics::Region;

// ---------------------------------------------------------------------------
// TenantRegionError — failure modes for the resolver surface.
// ---------------------------------------------------------------------------

/// Failure modes surfaced by [`TenantRegionResolver::resolve_region`].
///
/// `Unresolved` is the "no row present, no fallback configured"
/// terminal state — callers (boot path) MUST translate this to a
/// SEV-2 503 at the affected route. `BackendUnavailable` is the
/// transient-failure state (D1 query refused, network blip); the
/// route layer surfaces it as 503 too but operators distinguish it
/// from `Unresolved` in audit-log dashboards.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TenantRegionError {
    /// No region pin recorded for the tenant AND no fallback
    /// configured by the resolver.
    Unresolved {
        /// Canonical tenant id (hyphenated lowercase UUID text form).
        tenant_id: String,
    },
    /// The backing store (D1 in production) refused the lookup.
    /// Stable prefix string for log/audit pattern matching.
    BackendUnavailable {
        /// Human-readable backend diagnostic (no PII).
        diagnostic: String,
    },
}

impl fmt::Display for TenantRegionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unresolved { tenant_id } => {
                write!(f, "tenant_region: unresolved tenant_id={tenant_id}")
            }
            Self::BackendUnavailable { diagnostic } => {
                write!(f, "tenant_region: backend_unavailable {diagnostic}")
            }
        }
    }
}

impl std::error::Error for TenantRegionError {}

// ---------------------------------------------------------------------------
// TenantRegionResolver — the swap-point trait.
// ---------------------------------------------------------------------------

/// Resolve a tenant's pinned analytics-shadow region.
///
/// Implementations are bound at boot — see
/// `apps/server/src/main.rs` `--feature neon-real` branch.
pub trait TenantRegionResolver: Send + Sync + core::fmt::Debug {
    /// Look up the pinned region for `tenant_id`.
    ///
    /// # Errors
    ///
    /// Returns [`TenantRegionError::Unresolved`] when no region is
    /// recorded for the tenant AND no fallback is configured;
    /// returns [`TenantRegionError::BackendUnavailable`] when the
    /// backing store refused the lookup.
    fn resolve_region(&self, tenant_id: &Uuid) -> Result<Region, TenantRegionError>;
}

// ---------------------------------------------------------------------------
// InMemoryTenantRegionResolver — dev / test impl.
// ---------------------------------------------------------------------------

/// Static-map resolver backed by a `HashMap<Uuid, Region>` plus an
/// optional fallback region. Mirrors the wave-19
/// [`crate::neon_shadow::real::StaticResolver`] pattern for per-region
/// DSN resolution.
///
/// # Modes
///
/// - **strict** (no fallback): unknown tenant → `Unresolved`.
/// - **fallback** (`with_fallback(region)`): unknown tenant → falls
///   back to the configured region (dev/staging friendly — every
///   tenant resolves SOMETHING).
#[derive(Clone, Debug, Default)]
pub struct InMemoryTenantRegionResolver {
    entries: HashMap<Uuid, Region>,
    fallback: Option<Region>,
}

impl InMemoryTenantRegionResolver {
    /// Construct an empty strict-mode resolver. Unknown tenants
    /// surface [`TenantRegionError::Unresolved`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            fallback: None,
        }
    }

    /// Construct a resolver from `(tenant_id, region)` pairs. Useful
    /// for inline boot-time defaults (e.g. the wave-20 hard-coded
    /// `[("tenant-id", Region::Iad)]` dev wire).
    #[must_use]
    pub fn from_pairs<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (Uuid, Region)>,
    {
        Self {
            entries: pairs.into_iter().collect(),
            fallback: None,
        }
    }

    /// Builder: install a fallback region for unknown tenants.
    #[must_use]
    pub fn with_fallback(mut self, region: Region) -> Self {
        self.fallback = Some(region);
        self
    }

    /// Builder: insert a `(tenant_id, region)` pin.
    #[must_use]
    pub fn with(mut self, tenant_id: Uuid, region: Region) -> Self {
        self.entries.insert(tenant_id, region);
        self
    }

    /// Borrow the configured fallback region, if any. Used by the
    /// boot path log line so operators can see what unknown tenants
    /// land on.
    #[must_use]
    pub fn fallback_region(&self) -> Option<Region> {
        self.fallback
    }

    /// Number of explicit pins in the map. Useful for the boot-time
    /// info log so operators can confirm the dev seed loaded.
    #[must_use]
    pub fn pin_count(&self) -> usize {
        self.entries.len()
    }
}

impl TenantRegionResolver for InMemoryTenantRegionResolver {
    fn resolve_region(&self, tenant_id: &Uuid) -> Result<Region, TenantRegionError> {
        if let Some(region) = self.entries.get(tenant_id) {
            return Ok(*region);
        }
        if let Some(fallback) = self.fallback {
            return Ok(fallback);
        }
        Err(TenantRegionError::Unresolved {
            tenant_id: tenant_id.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// TenantConfigStore — abstraction over the D1 backing store.
// ---------------------------------------------------------------------------

/// Thin abstraction over the `tenant_config` D1 table.
///
/// The CF Worker production boot path
/// (`corelink-clerk-cf::prod_wiring`) constructs an impl that wraps
/// `corelink_cf_bindings::d1_real::CfD1DatabaseReal::scoped_query`
/// against the SQL:
///
/// ```sql
/// SELECT region FROM tenant_config WHERE tenant_id = ?
/// ```
///
/// (column added by `migrations/d1/0052_tenant_config_region.sql`).
///
/// Native binaries DO NOT construct a real impl — D1 is a
/// `worker::D1Database` binding, only reachable inside the Worker
/// isolate. The native gRPC server stays on
/// [`InMemoryTenantRegionResolver`] (matching the `BillingD1Writer` /
/// `InMemoryBillingD1` pattern in `corelink-billing-stripe-materializer`).
pub trait TenantConfigStore: Send + Sync + core::fmt::Debug {
    /// Look up the pinned region label (3-letter colocode) for the
    /// tenant. Returns:
    ///
    /// - `Ok(Some(label))` — row present.
    /// - `Ok(None)` — tenant has no `tenant_config` row yet (legacy
    ///   tenant pre-`0052_tenant_config_region`); resolver applies
    ///   its fallback.
    /// - `Err(diag)` — backing store unavailable. Stable diagnostic
    ///   string surfaced via [`TenantRegionError::BackendUnavailable`].
    ///
    /// # Errors
    ///
    /// Backend errors propagate as `Err(String)` — the resolver
    /// wraps the string in [`TenantRegionError::BackendUnavailable`].
    fn region_label(&self, tenant_id: &Uuid) -> Result<Option<String>, String>;
}

// ---------------------------------------------------------------------------
// D1TenantRegionResolver — production impl.
// ---------------------------------------------------------------------------

/// Production resolver: queries the D1 `tenant_config.region` column
/// via a [`TenantConfigStore`] and parses the label string back to
/// the canonical [`Region`] enum.
///
/// A configurable fallback region handles the "legacy tenant, no row
/// yet" path (Ok(None) from the store) so a fresh deployment doesn't
/// 503 every legacy tenant. The fallback path is observable: every
/// fallback resolution emits an `info!` log at the boot wire path
/// so operators can confirm the legacy backfill is progressing.
#[derive(Debug)]
pub struct D1TenantRegionResolver {
    store: std::sync::Arc<dyn TenantConfigStore>,
    fallback: Region,
}

impl D1TenantRegionResolver {
    /// Construct a D1-backed resolver with the supplied store and
    /// fallback region.
    #[must_use]
    pub fn new(store: std::sync::Arc<dyn TenantConfigStore>, fallback: Region) -> Self {
        Self { store, fallback }
    }

    /// Borrow the configured fallback region.
    #[must_use]
    pub fn fallback_region(&self) -> Region {
        self.fallback
    }
}

impl TenantRegionResolver for D1TenantRegionResolver {
    fn resolve_region(&self, tenant_id: &Uuid) -> Result<Region, TenantRegionError> {
        match self.store.region_label(tenant_id) {
            Ok(Some(label)) => parse_region_label(&label).ok_or_else(|| {
                TenantRegionError::BackendUnavailable {
                    diagnostic: format!("region_label_invalid label={label}"),
                }
            }),
            Ok(None) => Ok(self.fallback),
            Err(diag) => Err(TenantRegionError::BackendUnavailable { diagnostic: diag }),
        }
    }
}

// ---------------------------------------------------------------------------
// parse_region_label — canonical 3-letter colocode → Region enum.
// ---------------------------------------------------------------------------

/// Parse a 3-letter colocode label (e.g. `"iad"`, `"fra"`) into the
/// canonical [`Region`] enum. Case-insensitive (D1 may store mixed
/// case; `tenant_config.region DEFAULT 'IAD'` is uppercase per the
/// migration). Returns `None` when the label does not match a known
/// colocode — the resolver lifts that to
/// [`TenantRegionError::BackendUnavailable`] so a stale enum value
/// from D1 doesn't silently map every tenant to IAD.
#[must_use]
pub fn parse_region_label(label: &str) -> Option<Region> {
    let lower = label.trim().to_ascii_lowercase();
    match lower.as_str() {
        "iad" => Some(Region::Iad),
        "sjc" => Some(Region::Sjc),
        "dfw" => Some(Region::Dfw),
        "sea" => Some(Region::Sea),
        "ord" => Some(Region::Ord),
        "lhr" => Some(Region::Lhr),
        "fra" => Some(Region::Fra),
        "ams" => Some(Region::Ams),
        "cdg" => Some(Region::Cdg),
        "mad" => Some(Region::Mad),
        "gru" => Some(Region::Gru),
        "eze" => Some(Region::Eze),
        "bog" => Some(Region::Bog),
        "nrt" => Some(Region::Nrt),
        "sin" => Some(Region::Sin),
        "syd" => Some(Region::Syd),
        "hkg" => Some(Region::Hkg),
        "bom" => Some(Region::Bom),
        "icn" => Some(Region::Icn),
        "jnb" => Some(Region::Jnb),
        "cpt" => Some(Region::Cpt),
        "dxb" => Some(Region::Dxb),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    /// Wave-21 net-new #1: trait round-trip — `InMemoryTenantRegionResolver`
    /// returns the exact pinned region for a tenant present in the map.
    #[test]
    fn in_memory_resolver_returns_pinned_region_for_known_tenant() {
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let resolver = InMemoryTenantRegionResolver::from_pairs([
            (tenant_a, Region::Fra),
            (tenant_b, Region::Gru),
        ]);
        assert_eq!(resolver.resolve_region(&tenant_a), Ok(Region::Fra));
        assert_eq!(resolver.resolve_region(&tenant_b), Ok(Region::Gru));
        assert_eq!(resolver.pin_count(), 2);
        assert_eq!(resolver.fallback_region(), None);
    }

    /// Wave-21 net-new #2: fallback path — strict mode surfaces
    /// `Unresolved` for an unknown tenant; `with_fallback(IAD)`
    /// flips that to a successful `Ok(Region::Iad)` resolution.
    #[test]
    fn in_memory_resolver_fallback_flips_unresolved_to_ok() {
        let known = Uuid::now_v7();
        let unknown = Uuid::now_v7();
        let strict = InMemoryTenantRegionResolver::new().with(known, Region::Fra);
        match strict.resolve_region(&unknown) {
            Err(TenantRegionError::Unresolved { tenant_id }) => {
                assert_eq!(tenant_id, unknown.to_string());
            }
            other => panic!("expected Unresolved, got {other:?}"),
        }
        let with_fb = strict.with_fallback(Region::Iad);
        assert_eq!(with_fb.resolve_region(&unknown), Ok(Region::Iad));
        // Pinned tenant still resolves to its explicit pin.
        assert_eq!(with_fb.resolve_region(&known), Ok(Region::Fra));
    }

    /// Wave-21 net-new #3: 503 path — when the D1 store returns
    /// `Err`, the `D1TenantRegionResolver` surfaces
    /// `BackendUnavailable`, which the route layer maps to 503
    /// (`SERVICE_UNAVAILABLE`) via the `ShadowSinkFactory::for_tenant`
    /// `Err` propagation.
    #[test]
    fn d1_resolver_surfaces_backend_unavailable_when_store_errors() {
        #[derive(Debug)]
        struct FailingStore;
        impl TenantConfigStore for FailingStore {
            fn region_label(&self, _tenant_id: &Uuid) -> Result<Option<String>, String> {
                Err("d1: connection refused".to_owned())
            }
        }
        let resolver = D1TenantRegionResolver::new(
            std::sync::Arc::new(FailingStore),
            Region::Iad,
        );
        let outcome = resolver.resolve_region(&Uuid::now_v7());
        match outcome {
            Err(TenantRegionError::BackendUnavailable { diagnostic }) => {
                assert!(
                    diagnostic.contains("d1:"),
                    "expected D1 diagnostic prefix, got {diagnostic}"
                );
            }
            other => panic!("expected BackendUnavailable, got {other:?}"),
        }
    }

    /// Defense-in-depth: store returns `Ok(None)` (legacy tenant pre-`0052`)
    /// — resolver falls back to the configured default region.
    #[test]
    fn d1_resolver_falls_back_on_missing_row() {
        #[derive(Debug)]
        struct EmptyStore;
        impl TenantConfigStore for EmptyStore {
            fn region_label(&self, _tenant_id: &Uuid) -> Result<Option<String>, String> {
                Ok(None)
            }
        }
        let resolver =
            D1TenantRegionResolver::new(std::sync::Arc::new(EmptyStore), Region::Iad);
        assert_eq!(resolver.resolve_region(&Uuid::now_v7()), Ok(Region::Iad));
        assert_eq!(resolver.fallback_region(), Region::Iad);
    }

    /// Defense-in-depth: store returns an unknown label — resolver
    /// lifts that to `BackendUnavailable` so a stale enum value
    /// doesn't silently coerce to the default region.
    #[test]
    fn d1_resolver_rejects_unknown_region_label() {
        #[derive(Debug)]
        struct WeirdStore;
        impl TenantConfigStore for WeirdStore {
            fn region_label(&self, _tenant_id: &Uuid) -> Result<Option<String>, String> {
                Ok(Some("mars".to_owned()))
            }
        }
        let resolver =
            D1TenantRegionResolver::new(std::sync::Arc::new(WeirdStore), Region::Iad);
        match resolver.resolve_region(&Uuid::now_v7()) {
            Err(TenantRegionError::BackendUnavailable { diagnostic }) => {
                assert!(diagnostic.contains("region_label_invalid"));
                assert!(diagnostic.contains("label=mars"));
            }
            other => panic!("expected BackendUnavailable, got {other:?}"),
        }
    }

    /// Defense-in-depth: store returns an uppercase label (the
    /// migration default is `'IAD'`); parser is case-insensitive.
    #[test]
    fn d1_resolver_parses_uppercase_migration_default() {
        #[derive(Debug)]
        struct UpperStore;
        impl TenantConfigStore for UpperStore {
            fn region_label(&self, _tenant_id: &Uuid) -> Result<Option<String>, String> {
                Ok(Some("IAD".to_owned()))
            }
        }
        let resolver =
            D1TenantRegionResolver::new(std::sync::Arc::new(UpperStore), Region::Fra);
        assert_eq!(resolver.resolve_region(&Uuid::now_v7()), Ok(Region::Iad));
    }

    #[test]
    fn parse_region_label_covers_every_canonical_colocode() {
        // Spot-check the parser against the canonical `Region::as_str`
        // round-trip — every variant of `Region` MUST parse back.
        for region in [
            Region::Iad,
            Region::Fra,
            Region::Gru,
            Region::Nrt,
            Region::Syd,
        ] {
            let label = region.as_str();
            assert_eq!(parse_region_label(label), Some(region));
            assert_eq!(parse_region_label(&label.to_uppercase()), Some(region));
        }
        assert_eq!(parse_region_label("not-a-region"), None);
    }
}
