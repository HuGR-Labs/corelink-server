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
