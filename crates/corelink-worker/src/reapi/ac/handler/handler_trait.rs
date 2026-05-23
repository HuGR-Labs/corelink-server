//! Canonical AC handler trait.
//!
//! Split from monolith `reapi/ac/handler.rs` (wave-33 stage 2.PRE-A.4).

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + corelink-worker R2Backend canonical pattern"
)]

use core::future::Future;

use super::super::types::{ActionDigest, ActionResult};
use super::errors::{AcError, GetActionResult, UpdateActionResult};
use crate::middleware::auth_ctx::AuthCtx;

/// Canonical AC handler trait. The gRPC tonic service wrapper + axum
/// REST router (deferred to follow-up integration WI) both consume
/// this trait.
pub trait ActionCacheHandler: Send + Sync {
    /// `GetActionResult` flow per WI §1 (steps \[0\]..\[7\]).
    ///
    /// # Errors
    ///
    /// See [`AcError`] for the canonical taxonomy.
    fn get_action_result<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        action_digest: &'a ActionDigest,
        request_id: &'a str,
    ) -> impl Future<Output = Result<GetActionResult, AcError>> + Send + 'a;

    /// `UpdateActionResult` flow per WI §1 (steps \[0\]..\[10\]).
    ///
    /// # Errors
    ///
    /// See [`AcError`] for the canonical taxonomy.
    fn update_action_result<'a>(
        &'a self,
        ctx: &'a AuthCtx,
        action_digest: &'a ActionDigest,
        action_result: ActionResult,
        request_id: &'a str,
    ) -> impl Future<Output = Result<UpdateActionResult, AcError>> + Send + 'a;
}
