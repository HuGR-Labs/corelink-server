//! Production wiring: assemble the four real CF binding wrappers and
//! attach a shared audit sink.
//!
//! This module is the single place where the Cloudflare Worker boot path
//! (the `#[worker::event(fetch)]` entry in [`crate::health`]) constructs
//! production binding adapters. Every binding-access site in the rest of
//! the crate flows through [`CfRealBindings`] so the tenant-prefix-
//! enforced + audit-fenced wrappers are reached at *every* call site —
//! no raw `env.bucket(...)` / `env.kv(...)` / `env.d1(...)` /
//! `env.durable_object(...)` lookups remain outside this module.
//!
//! # Tenant resolution
//!
//! The tenant identifier is the validated subject from the request's
//! `Authorization: Bearer <JWT>` once `corelink-clerk` has resolved it.
//! For the PoC clerk-cf Worker, the tenant prefix is sourced from the
//! `x-corelink-tenant` request header — a JWT-validated principal claim
//! the upstream gateway has already verified. The header value is
//! anchored through the per-binding `TenantPrefix::new` / `TenantId::new`
//! constructors, each of which validates shape (non-empty, no NUL, no
//! separator collisions) and bottoms-out the equality check via
//! `subtle::ConstantTimeEq` on the leading prefix probe.
//!
//! # Binding name map
//!
//! - `CAS_BUCKET`     → R2 (`CfR2BucketReal`)
//! - `CLERK_DB`       → D1 (`CfD1DatabaseReal`)
//! - `CLERK_JWKS_KV`  → KV (`CfKvNamespaceReal`)
//! - `CLERK_DO`       → DO (`CfDurableObjectReal`)
//!
//! See `wrangler.toml` for the canonical declaration of each binding.

use corelink_cf_bindings::{
    CfD1DatabaseReal, CfDurableObjectReal, CfKvNamespaceReal, CfR2BucketReal, DoTenantPrefix,
    KvTenantPrefix, TenantId, TenantPrefix,
};

use crate::audit_sink::AuditSink;

/// Error type for production wiring failures.
///
/// `#[non_exhaustive]` so future binding additions (Queues, Vectorize,
/// ...) can extend the variant set without breaking downstream callers.
#[non_exhaustive]
#[derive(Debug)]
pub enum WiringError {
    /// A binding lookup against `worker::Env` failed (binding missing or
    /// of the wrong type). Wraps the `worker::Error` string-form.
    BindingLookup {
        /// Name of the missing / wrongly-typed binding (e.g. `CAS_BUCKET`).
        binding: &'static str,
        /// `worker::Error::to_string()` payload.
        cause: String,
    },
    /// Tenant identifier validation failed (empty / NUL / wrong shape).
    /// Translates the per-binding constructor error onto a single
    /// variant so callers don't have to match on four different error
    /// enums at the boot path.
    TenantValidation(String),
}

impl std::fmt::Display for WiringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BindingLookup { binding, cause } => {
                write!(f, "binding lookup failed: {binding}: {cause}")
            }
            Self::TenantValidation(msg) => write!(f, "tenant validation: {msg}"),
        }
    }
}

impl std::error::Error for WiringError {}

/// Validated tenant context used to construct all four wrappers.
///
/// Constructed via [`TenantContext::from_header_value`] which performs
/// the same shape validation the canonical 16-hex HMAC tenant prefix
/// satisfies (non-empty, no `/`, no `:`, no NUL). Each binding-specific
/// anchor (R2 `TenantPrefix`, D1 `TenantId`, KV `TenantPrefix`, DO
/// `DoTenantPrefix`) is derived once at construction so the per-request
/// hot path does not revalidate.
#[derive(Clone, Debug)]
pub struct TenantContext {
    /// Stable label used for audit emission.
    pub label: String,
    /// R2 tenant prefix (separator `/`).
    pub r2: TenantPrefix,
    /// D1 tenant identifier (bind-time CT-eq anchor).
    pub d1: TenantId,
    /// KV tenant prefix (separator `:`).
    pub kv: KvTenantPrefix,
    /// DO tenant prefix (`tenant:<id>:<purpose>` scoped name root).
    pub do_: DoTenantPrefix,
}

impl TenantContext {
    /// Build a tenant context from a validated header / JWT claim value.
    ///
    /// `raw` is the tenant identifier already resolved by the upstream
    /// authentication layer. We re-validate here at the binding-anchor
    /// boundary — defense-in-depth, since each per-binding constructor
    /// has its own shape rules (R2 forbids `/`, KV forbids `:`, etc.).
    ///
    /// # Errors
    ///
    /// Returns `WiringError::TenantValidation` if any of the four
    /// per-binding constructors rejects the input.
    pub fn from_header_value(raw: &str) -> Result<Self, WiringError> {
        let r2 = TenantPrefix::new(raw)
            .map_err(|e| WiringError::TenantValidation(format!("r2 tenant: {e}")))?;
        let d1 = TenantId::new(raw)
            .map_err(|e| WiringError::TenantValidation(format!("d1 tenant: {e}")))?;
        let kv = KvTenantPrefix::new(raw)
            .map_err(|e| WiringError::TenantValidation(format!("kv tenant: {e}")))?;
        let do_ = DoTenantPrefix::new(raw)
            .map_err(|e| WiringError::TenantValidation(format!("do tenant: {e}")))?;
        Ok(Self {
            label: raw.to_owned(),
            r2,
            d1,
            kv,
            do_,
        })
    }
}

/// Bundle of the four real-binding wrappers, all sharing a single
/// [`AuditSink`] and anchored to a single validated [`TenantContext`].
///
/// Held by the request handler chain instead of raw `worker::*` types so
/// the tenant-prefix enforcement and audit fence are unavoidable at the
/// type-system level.
#[derive(Debug)]
pub struct CfRealBindings {
    /// R2 bucket (audit-fenced; tenant prefix `tenant/`).
    pub r2: CfR2BucketReal,
    /// D1 database (audit-fenced mutations; tenant id bind-time check).
    pub d1: CfD1DatabaseReal,
    /// KV namespace (audit-fenced; tenant prefix `tenant:`).
    pub kv: CfKvNamespaceReal,
    /// Durable Object namespace (audit-fenced fetches; tenant-scoped name).
    pub do_: CfDurableObjectReal,
    /// The audit sink shared by all four. Retained on the struct so the
    /// closures keep their `Arc<...>` reference live until the request
    /// handler drops the bundle.
    pub audit: AuditSink,
}

/// Production boot path — wasm32 only. Reads the four bindings from
/// `worker::Env`, wraps each in its real-impl adapter, attaches the
/// shared audit sink, and returns the bundle.
///
/// # Errors
///
/// Returns `WiringError::BindingLookup` if any binding is missing from
/// `wrangler.toml` or is of the wrong type. Returns
/// `WiringError::TenantValidation` if the resolved tenant identifier
/// fails any per-binding constructor's shape check.
#[cfg(target_arch = "wasm32")]
pub fn build_real_bindings(
    env: &worker::Env,
    tenant: &TenantContext,
) -> Result<CfRealBindings, WiringError> {
    let bucket = env
        .bucket("CAS_BUCKET")
        .map_err(|e| WiringError::BindingLookup {
            binding: "CAS_BUCKET",
            cause: e.to_string(),
        })?;
    let db = env.d1("CLERK_DB").map_err(|e| WiringError::BindingLookup {
        binding: "CLERK_DB",
        cause: e.to_string(),
    })?;
    let kv_store = env
        .kv("CLERK_JWKS_KV")
        .map_err(|e| WiringError::BindingLookup {
            binding: "CLERK_JWKS_KV",
            cause: e.to_string(),
        })?;
    let do_ns = env
        .durable_object("CLERK_DO")
        .map_err(|e| WiringError::BindingLookup {
            binding: "CLERK_DO",
            cause: e.to_string(),
        })?;

    let audit = AuditSink::console_ndjson(tenant.label.clone());
    let r2 = CfR2BucketReal::new(bucket, tenant.r2.clone()).with_audit(audit.r2());
    let d1 = CfD1DatabaseReal::new(db, tenant.d1.clone()).with_audit(audit.d1());
    let kv = CfKvNamespaceReal::new(kv_store, tenant.kv.clone()).with_audit(audit.kv());
    let do_ = CfDurableObjectReal::new(do_ns, tenant.do_.clone()).with_audit(audit.do_());

    Ok(CfRealBindings {
        r2,
        d1,
        kv,
        do_,
        audit,
    })
}

/// Native test boot path — builds the four wrappers from
/// `stub_for_native_tests` and attaches the supplied recorder audit
/// sink. Used by `tests/prod_wiring.rs` to exercise the boot path
/// without the wasm32 toolchain.
#[cfg(not(target_arch = "wasm32"))]
pub fn build_real_bindings_for_tests(
    tenant: &TenantContext,
    audit: AuditSink,
) -> CfRealBindings {
    let r2 = CfR2BucketReal::stub_for_native_tests(tenant.r2.clone()).with_audit(audit.r2());
    let d1 = CfD1DatabaseReal::stub_for_native_tests(tenant.d1.clone()).with_audit(audit.d1());
    let kv = CfKvNamespaceReal::stub_for_native_tests(tenant.kv.clone()).with_audit(audit.kv());
    let do_ =
        CfDurableObjectReal::stub_for_native_tests(tenant.do_.clone()).with_audit(audit.do_());
    CfRealBindings {
        r2,
        d1,
        kv,
        do_,
        audit,
    }
}

// ---------------------------------------------------------------------------
// Wave-18 follow-on: billing materializer real bindings.
//
// When the `cf-billing-real` feature is on, the CF Worker boot path
// additionally constructs the `CfD1BillingWriter` +
// `ArchiveProducerBillingEmitter` trait objects from the canonical
// `CfRealBindings` bundle. The result is plugged into
// `corelink-stripe-real::webhook_dispatch::WebhookDispatcher::new` so
// the materializer + audit emitter route through wave-14
// `CfD1DatabaseReal` + wave-15 `ArchiveProducer` at every call site.
//
// The construction lives here so the binding-access boundary stays
// single-source-of-truth — `apps/server::main.rs` keeps the wave-17
// InMemory* wiring for native (D1 is not reachable outside the CF
// Worker isolate), and the wasm32 binary in `apps/corelink-worker`
// calls `build_billing_real_bindings` at fetch-event boot.
// ---------------------------------------------------------------------------

/// Wave-18 wasm32 production billing-materializer bindings bundle.
///
/// Holds the `CfD1BillingWriter` + `ArchiveProducerBillingEmitter`
/// trait objects + the underlying `ArchiveProducer` so the CF Worker
/// fetch handler can construct a `WebhookDispatcher` without
/// re-importing the wave-18 binder types at every call site.
///
/// The producer is exposed so the boot layer can call
/// `force_flush` on shutdown drain / scheduled flush per the wave-15
/// archive-flush policy.
#[cfg(feature = "cf-billing-real")]
#[derive(Debug)]
pub struct CfBillingRealBindings {
    /// `BillingD1Writer` impl routing through `CfD1DatabaseReal`.
    pub billing_d1: std::sync::Arc<corelink_billing_stripe_materializer::CfD1BillingWriter>,
    /// `BillingAuditEmitter` impl routing through `ArchiveProducer` +
    /// `R2AuditSink`.
    pub billing_audit: std::sync::Arc<
        corelink_billing_stripe_materializer::ArchiveProducerBillingEmitter,
    >,
    /// Borrow of the underlying producer for shutdown-drain hooks.
    pub archive_producer: std::sync::Arc<corelink_audit_chain::ArchiveProducer>,
}

/// Construct the wave-18 billing-real bindings bundle from a wrapped
/// `CfD1DatabaseReal` + the wave-15 archive `producer` + `r2_sink`.
///
/// `tenant` is the validated `TenantId` the D1 wrapper is anchored to;
/// the same tenant is propagated to the `CfD1BillingWriter` so per-row
/// `MaterializedRow.tenant_id` ct-eq checks share one source of truth.
///
/// # Errors
///
/// Infallible at the binding-construction layer (all validation
/// happens upstream at `TenantContext::from_header_value` and the
/// `CfD1DatabaseReal::new` constructor). Returns `Self` directly.
#[cfg(feature = "cf-billing-real")]
#[must_use]
pub fn build_billing_real_bindings(
    d1: std::sync::Arc<corelink_cf_bindings::CfD1DatabaseReal>,
    tenant: corelink_cf_bindings::TenantId,
    producer: std::sync::Arc<corelink_audit_chain::ArchiveProducer>,
    r2_sink: std::sync::Arc<dyn corelink_audit_chain::R2AuditSink>,
) -> CfBillingRealBindings {
    let billing_d1 = std::sync::Arc::new(
        corelink_billing_stripe_materializer::CfD1BillingWriter::new(d1, tenant),
    );
    let billing_audit = std::sync::Arc::new(
        corelink_billing_stripe_materializer::ArchiveProducerBillingEmitter::new(
            producer.clone(),
            r2_sink,
        ),
    );
    CfBillingRealBindings {
        billing_d1,
        billing_audit,
        archive_producer: producer,
    }
}

// ---------------------------------------------------------------------------
// Wave-25 follow-on: tenant-config region resolver wire for the Neon
// analytics shadow.
//
// Wave-21 shipped the `D1TenantRegionResolver` trait surface in
// `corelink-audit-chain::neon_shadow::tenant_region` but left the CF
// Worker production boot path on an `InMemoryTenantRegionResolver`
// IAD fallback (audit doc §7 caveat: "`corelink-clerk-cf::prod_wiring`
// not yet updated to construct `D1TenantRegionResolver`"). This module
// closes that caveat: when the CF Worker boot path runs with the
// `tenant-region-real` feature on, the resolver wraps a
// `D1TenantConfigStore` (which queries the wave-21
// `tenant_config.region` column) behind the canonical
// `D1TenantRegionResolver`. When the feature is off OR the D1 store
// is empty (dev mode), the boot path falls back to
// `InMemoryTenantRegionResolver` with an explicit IAD default —
// mirroring the wave-20 native gRPC boot path's behaviour.
// ---------------------------------------------------------------------------

/// Wave-25 production tenant-region resolver wire. Constructs a
/// [`corelink_audit_chain::D1TenantRegionResolver`] backed by a
/// [`crate::tenant_region_real::D1TenantConfigStore`] when the
/// `store` is supplied (i.e. the D1 binding is present), and falls
/// back to a populated
/// [`corelink_audit_chain::InMemoryTenantRegionResolver`] otherwise.
///
/// Returns the resolver as an `Arc<dyn TenantRegionResolver>` so it
/// can be threaded into the wave-20 `TokioPgShadowSinkFactory`
/// (mirrored on the CF Worker side by the analogous synchronous
/// `ShadowSinkFactory::for_tenant` dispatch).
///
/// `fallback_region` is the legacy-tenant / cache-miss landing
/// region — the wave-21 audit doc §7 charter pins this to `Region::Iad`
/// for production rollouts (the wave-20 hard-coded default) so a
/// fresh deployment doesn't 503 every legacy tenant.
///
/// # Modes
///
/// - `Some(store)` — production wire. The resolver routes every
///   `resolve_region` through the D1-backed store. The CF Worker
///   boot path MUST call
///   [`crate::tenant_region_real::D1TenantConfigStore::prefetch`]
///   in the request-prelude to populate the cache for the current
///   request's tenant BEFORE the synchronous `ShadowSinkFactory::
///   for_tenant` dispatch.
/// - `None` — dev mode wire. The resolver routes every
///   `resolve_region` through an empty
///   [`corelink_audit_chain::InMemoryTenantRegionResolver`] that
///   always falls back to `fallback_region`. Used when the
///   `CLERK_DB` D1 binding is absent (local `wrangler dev` without
///   D1 binding wired).
///
/// # Errors
///
/// Infallible — both the `D1TenantConfigStore::new` constructor and
/// the `InMemoryTenantRegionResolver::new().with_fallback(...)`
/// chain are infallible. Returns `Self` directly.
#[cfg(feature = "tenant-region-real")]
#[must_use]
pub fn build_tenant_region_resolver(
    store: Option<std::sync::Arc<crate::tenant_region_real::D1TenantConfigStore>>,
    fallback_region: corelink_audit_chain::Region,
) -> std::sync::Arc<dyn corelink_audit_chain::TenantRegionResolver> {
    match store {
        Some(store) => {
            // Wrap the concrete `D1TenantConfigStore` as a
            // trait-object `TenantConfigStore` for the resolver.
            let trait_store: std::sync::Arc<dyn corelink_audit_chain::TenantConfigStore> =
                store;
            std::sync::Arc::new(corelink_audit_chain::D1TenantRegionResolver::new(
                trait_store,
                fallback_region,
            ))
        }
        None => std::sync::Arc::new(
            corelink_audit_chain::InMemoryTenantRegionResolver::new()
                .with_fallback(fallback_region),
        ),
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: panics on assertion failure are the canonical signal"
)]
mod tests {
    use super::*;

    #[test]
    fn tenant_context_accepts_canonical_16hex() {
        // 16-hex matches the canonical `corelink_tenant_path::TenantPrefix`
        // shape (HMAC-derived). All four per-binding constructors must
        // accept it.
        let ctx = TenantContext::from_header_value("0123456789abcdef").expect("valid tenant");
        assert_eq!(ctx.label, "0123456789abcdef");
    }

    #[test]
    fn tenant_context_rejects_empty() {
        let err = TenantContext::from_header_value("").expect_err("empty rejected");
        match err {
            WiringError::TenantValidation(_) => {}
            other => panic!("expected TenantValidation, got {other:?}"),
        }
    }

    #[test]
    fn tenant_context_rejects_nul() {
        let err = TenantContext::from_header_value("a\0b").expect_err("nul rejected");
        match err {
            WiringError::TenantValidation(_) => {}
            other => panic!("expected TenantValidation, got {other:?}"),
        }
    }

    #[test]
    fn tenant_context_rejects_r2_separator() {
        // `/` is forbidden by the R2 TenantPrefix constructor.
        let err = TenantContext::from_header_value("a/b").expect_err("slash rejected");
        match err {
            WiringError::TenantValidation(_) => {}
            other => panic!("expected TenantValidation, got {other:?}"),
        }
    }

    #[test]
    fn tenant_context_rejects_kv_separator() {
        // `:` is forbidden by the KV TenantPrefix constructor.
        let err = TenantContext::from_header_value("a:b").expect_err("colon rejected");
        match err {
            WiringError::TenantValidation(_) => {}
            other => panic!("expected TenantValidation, got {other:?}"),
        }
    }
}
