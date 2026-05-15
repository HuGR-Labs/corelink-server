//! `INV-AC-OUTPUTS-VALID` aliveness check trait + InMemory fake
//! (WI-S04-001 §6.1.5–§6.1.6).
//!
//! Production wiring uses [`corelink_meta::MetaStore`] to look up
//! every `output_file` / `output_directory` digest under the same
//! tenant_id. The fake here lets the property tests pre-populate
//! "alive" and "tombstoned" digest sets without spinning up the
//! `InMemoryMetaStore` for every iteration.
//!
//! ## Strict UPDATE semantics
//!
//! [`OutputsCheck::assert_alive`] returns
//! [`OutputsCheckOutcome::AllAlive`] only when **every** output digest
//! is alive in `blob_meta`. Any tombstoned OR never-existed digest →
//! [`OutputsCheckOutcome::SomeMissing { missing }`] with the canonical
//! list (digests in input order). The handler maps to 422 +
//! `COR_AC_OUTPUTS_MISSING` + audit `ac.update.outputs_missing` per
//! WI §8 Gherkin.
//!
//! ## Warn-only GET semantics (1% sampled)
//!
//! [`OutputsCheck::warn_if_missing`] runs on a sampled subset of GETs
//! per ADR-0035 H-7 / Lote 10.4bis P0. It returns the same outcome
//! enum but the handler **does not** reject the GET on missing — it
//! emits an audit warning + increments the
//! `corelink.ac.outputs.tombstoned_warning_total{sampled=true}`
//! metric. Reconcile diário in S-06 closes the drift.

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore pattern"
)]

use core::future::Future;
use std::collections::{HashMap, HashSet};
// DEBT-013 OPT-04 phase 1 — `parking_lot::Mutex` (infallible lock).
// `OutputsCheckError::Backend("mutex poisoned")` is unreachable here.
use parking_lot::Mutex;

use corelink_hash::Digest;
use thiserror::Error;
use uuid::Uuid;

use super::types::ActionResult;

/// Errors surfaced by [`OutputsCheck`] methods.
#[derive(Debug, Error)]
pub enum OutputsCheckError {
    /// Backend transport failure (D1 RPC error).
    #[error("outputs check backend error: {0}")]
    Backend(String),
}

/// Outcome of an [`OutputsCheck::assert_alive`] /
/// [`OutputsCheck::warn_if_missing`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputsCheckOutcome {
    /// Every output digest is alive in `blob_meta`. Handler proceeds
    /// without rejecting.
    AllAlive,
    /// Some output digests are tombstoned OR never existed in
    /// `blob_meta`. UPDATE handler rejects 422; GET handler emits a
    /// warning (1% sampled) and proceeds.
    SomeMissing {
        /// The subset of digests that are missing — in canonical
        /// input order.
        missing: Vec<Digest>,
    },
}

impl OutputsCheckOutcome {
    /// Helper: `true` iff [`Self::AllAlive`].
    #[must_use]
    pub const fn is_all_alive(&self) -> bool {
        matches!(self, Self::AllAlive)
    }

    /// Helper: borrow the missing-digests slice (empty when
    /// [`Self::AllAlive`]).
    #[must_use]
    pub fn missing(&self) -> &[Digest] {
        match self {
            Self::AllAlive => &[],
            Self::SomeMissing { missing } => missing,
        }
    }
}

/// Outputs aliveness check trait. Production impl walks
/// `corelink_meta::MetaStore::get` per digest in parallel (analogous
/// to `corelink-reapi::find_missing::FindMissingOrchestrator`).
pub trait OutputsCheck: Send + Sync {
    /// Strict aliveness check used by the UPDATE handler.
    ///
    /// # Errors
    ///
    /// Returns [`OutputsCheckError::Backend`] on transport faults.
    fn assert_alive<'a>(
        &'a self,
        tenant_id: Uuid,
        result: &'a ActionResult,
    ) -> impl Future<Output = Result<OutputsCheckOutcome, OutputsCheckError>> + Send + 'a;

    /// Warn-only check used by the GET handler (1% sampled per ADR-
    /// 0035 H-7). Same outcome shape as [`Self::assert_alive`]; the
    /// handler does not reject on `SomeMissing` — it emits an audit
    /// warning + increments the drift counter.
    ///
    /// # Errors
    ///
    /// Returns [`OutputsCheckError::Backend`] on transport faults.
    fn warn_if_missing<'a>(
        &'a self,
        tenant_id: Uuid,
        result: &'a ActionResult,
    ) -> impl Future<Output = Result<OutputsCheckOutcome, OutputsCheckError>> + Send + 'a {
        // Default: same logic as the strict path; the handler is the
        // one that decides whether to reject. Subclasses can override
        // when production wiring routes the warn-only path to a
        // sampled D1 read.
        async move { self.assert_alive(tenant_id, result).await }
    }
}

/// In-memory outputs check that mirrors the `(tenant_id, digest) ⇒ alive
/// | tombstoned | absent` matrix without spinning up a real `MetaStore`.
///
/// Use [`Self::insert_alive`] to pre-populate alive digests and
/// [`Self::insert_tombstoned`] to mark digests as tombstoned. Anything
/// not registered counts as "absent" (== never existed).
pub struct InMemoryOutputsCheck {
    inner: Mutex<InnerOutputs>,
}

#[derive(Default)]
struct InnerOutputs {
    /// `(tenant_id, digest)` ⇒ alive (true) / tombstoned (false).
    rows: HashMap<(Uuid, Digest), bool>,
}

impl Default for InMemoryOutputsCheck {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for InMemoryOutputsCheck {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryOutputsCheck")
            .finish_non_exhaustive()
    }
}

impl InMemoryOutputsCheck {
    /// Construct a fresh empty check (every digest is "absent").
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(InnerOutputs::default()),
        }
    }

    /// Mark `digest` as alive under `tenant_id`.
    pub fn insert_alive(&self, tenant_id: Uuid, digest: Digest) {
        let mut guard = self.inner.lock();
        guard.rows.insert((tenant_id, digest), true);
    }

    /// Mark `digest` as tombstoned (alive in blob_meta but with
    /// `deleted_at IS NOT NULL`).
    pub fn insert_tombstoned(&self, tenant_id: Uuid, digest: Digest) {
        let mut guard = self.inner.lock();
        guard.rows.insert((tenant_id, digest), false);
    }

    /// Bulk-mark every digest in `result.iter_output_digests()` as
    /// alive under `tenant_id`. Convenience helper for tests.
    pub fn insert_all_alive(&self, tenant_id: Uuid, result: &ActionResult) {
        for d in result.iter_output_digests() {
            self.insert_alive(tenant_id, d);
        }
    }

    fn check(
        &self,
        tenant_id: Uuid,
        result: &ActionResult,
    ) -> Result<OutputsCheckOutcome, OutputsCheckError> {
        // parking_lot lock is infallible — Backend mutex-poisoned arm
        // is preserved on the enum for non-in-memory implementations.
        let guard = self.inner.lock();
        let mut missing: Vec<Digest> = Vec::new();
        let mut seen = HashSet::new();
        for d in result.iter_output_digests() {
            if !seen.insert(d) {
                continue; // dedupe
            }
            match guard.rows.get(&(tenant_id, d)) {
                Some(true) => {}                       // alive
                Some(false) => missing.push(d),        // tombstoned
                None => missing.push(d),               // never existed
            }
        }
        if missing.is_empty() {
            Ok(OutputsCheckOutcome::AllAlive)
        } else {
            Ok(OutputsCheckOutcome::SomeMissing { missing })
        }
    }
}

impl OutputsCheck for InMemoryOutputsCheck {
    fn assert_alive<'a>(
        &'a self,
        tenant_id: Uuid,
        result: &'a ActionResult,
    ) -> impl Future<Output = Result<OutputsCheckOutcome, OutputsCheckError>> + Send + 'a {
        async move { self.check(tenant_id, result) }
    }

    fn warn_if_missing<'a>(
        &'a self,
        tenant_id: Uuid,
        result: &'a ActionResult,
    ) -> impl Future<Output = Result<OutputsCheckOutcome, OutputsCheckError>> + Send + 'a {
        async move { self.check(tenant_id, result) }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]
mod tests {
    use super::*;
    use crate::reapi::ac::types::{OutputDirectoryDigest, OutputFileDigest};

    fn fixed_tenant() -> Uuid {
        Uuid::parse_str("01938af0-abcd-7123-8456-000000000a01").unwrap()
    }

    #[tokio::test]
    async fn all_alive_when_every_digest_pre_inserted() {
        let check = InMemoryOutputsCheck::new();
        let result = ActionResult::new(
            vec![
                OutputFileDigest::new(Digest::compute(b"o1"), 1),
                OutputFileDigest::new(Digest::compute(b"o2"), 2),
            ],
            vec![OutputDirectoryDigest::new(Digest::compute(b"d1"), 3)],
            0,
            Vec::new(),
        );
        check.insert_all_alive(fixed_tenant(), &result);
        let out = check
            .assert_alive(fixed_tenant(), &result)
            .await
            .unwrap();
        assert!(out.is_all_alive());
    }

    #[tokio::test]
    async fn tombstoned_digest_surfaces_as_missing() {
        let check = InMemoryOutputsCheck::new();
        let alive = Digest::compute(b"alive");
        let dead = Digest::compute(b"dead");
        let result = ActionResult::new(
            vec![
                OutputFileDigest::new(alive, 1),
                OutputFileDigest::new(dead, 2),
            ],
            Vec::new(),
            0,
            Vec::new(),
        );
        check.insert_alive(fixed_tenant(), alive);
        check.insert_tombstoned(fixed_tenant(), dead);
        let out = check
            .assert_alive(fixed_tenant(), &result)
            .await
            .unwrap();
        match out {
            OutputsCheckOutcome::SomeMissing { missing } => {
                assert_eq!(missing, vec![dead]);
            }
            other => panic!("expected SomeMissing, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cross_tenant_alive_is_not_alive_for_other_tenant() {
        let check = InMemoryOutputsCheck::new();
        let blob = Digest::compute(b"blob");
        check.insert_alive(fixed_tenant(), blob);
        let other = Uuid::parse_str("01938af0-abcd-7123-8456-000000000b02").unwrap();
        let result = ActionResult::new(
            vec![OutputFileDigest::new(blob, 1)],
            Vec::new(),
            0,
            Vec::new(),
        );
        let out = check.assert_alive(other, &result).await.unwrap();
        assert_eq!(
            out,
            OutputsCheckOutcome::SomeMissing {
                missing: vec![blob]
            }
        );
    }
}
