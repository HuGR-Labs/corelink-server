//! Pure-logic AC handler (WI-S04-001 §1, §6.1).
//!
//! [`ActionCacheHandlerImpl`] is the canonical handler that orchestrates
//! the 7-step GET + 10-step UPDATE flows. The trait surface
//! ([`ActionCacheHandler`]) is the integration seam consumed by the
//! gRPC tonic + axum REST wrappers (deferred to a follow-up
//! integration WI; see [`crate::reapi`] module rustdoc).
//!
//! ## 5-Layer Defense (auth_model.md §8.1 + ADR-0035)
//!
//! Every handler call enforces:
//!
//! 1. **Layer 1 — auth verify** is upstream (`AuthLayer`); the handler
//!    receives an `AuthCtx` reference whose construction was gated
//!    by the verifier.
//! 2. **Layer 2 — D1 enforcement**: the
//!    `super::meta::AcMetaStore` surface is `(tenant_id,
//!    action_digest)`-PK-keyed.
//! 3. **Layer 3 — scope check**: `auth_ctx.has_scope(SCOPE_CACHE_R)` /
//!    `SCOPE_CACHE_W` enforced as the first step of every flow.
//! 4. **Layer 4 — HMAC tenant prefix**: read off the `ac_meta` row on
//!    GET; computed via `corelink_tenant_path::derive_prefix(tdk,
//!    tenant_id)` once at INSERT time on UPDATE (per ADR-0035 H-3).
//! 5. **Layer 5 — audit emit**: every terminal flow path emits a
//!    typed record via `super::audit::AuditSink`.
//!
//! The 5-layer enforcement is structural — the handler refuses to
//! compile against an `AuthCtx` that is missing any field; the
//! storage adapter refuses to construct a `TenantCtx` without the
//! TDK + tenant_id pair; the AC trait surface refuses to hit a row
//! without an `AcKey::new(tenant_id, _)` build. Property tests
//! exercise the structural enforcement at 10k iter.
//!
//! ## Internal layout (wave-33 stage 2.PRE-A.4)
//!
//! The original 1578-LOC monolith is decomposed into seven submodule
//! files + a tests module, re-exported through this parent file so
//! every `reapi::ac::handler::*` path consumer keeps resolving at
//! parity:
//!
//! - [`errors`] — `AcError` + `From` impls + `GetActionResult` +
//!   `UpdateActionResult` + constants.
//! - [`handler_trait`] — `ActionCacheHandler` trait.
//! - [`envelope_store`] — `AcEnvelopeStore` trait + `InMemoryAcEnvelopeStore`.
//! - [`builder`] — `ActionCacheHandlerImpl` struct + `Clock` seam +
//!   `SystemClock` + `FakeClock` + `ActionCacheHandlerBuilder`.
//! - [`handler_impl`] — constructor + helper methods + trait-impl
//!   trampolines.
//! - [`methods`] — the 7-step GET + 10-step UPDATE inner async
//!   methods.
//! - [`stash`] — `ActionResult` sideband persistence
//!   (`persist_action_result` / `recover_action_result` + the
//!   `ActionResultStash` canonical key generator).

pub mod builder;
pub mod envelope_store;
pub mod errors;
pub mod handler_impl;
pub mod handler_trait;
pub mod methods;
pub mod stash;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Canonical re-exports — preserve the pre-split
// `reapi::ac::handler::*` public surface verbatim.
// ---------------------------------------------------------------------------

pub use builder::{
    ActionCacheHandlerBuilder, ActionCacheHandlerImpl, Clock, FakeClock, SystemClock,
};
pub use envelope_store::{AcEnvelopeStore, InMemoryAcEnvelopeStore};
pub use errors::{
    AcError, GetActionResult, UpdateActionResult, AC_ENVELOPE_VERSION,
    DEFAULT_AC_TTL_EXTEND_MS,
};
pub use handler_trait::ActionCacheHandler;
