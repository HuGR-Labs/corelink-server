//! `INV-AC-OUTPUTS-VALID` enforcement (WI-S04-003 §6.1.6).
//!
//! Builder-side strict aliveness check: every output digest the
//! envelope references must exist in `blob_meta` AND not be
//! tombstoned for the owning tenant. The trait abstraction lets the
//! handler hand in either a real D1 reader (production) or an
//! in-memory map (tests + fakes).

use core::fmt;

use corelink_hash::Digest;

use crate::ac_core::error::OutputsCheckError;
use crate::ac_core::types::ActionResult;

/// Reader trait for the `blob_meta` aliveness check (WI-S04-003 §6.1.6).
///
/// Implementations MUST:
///
/// 1. Return `Ok(Vec<bool>)` whose length equals `digests.len()`.
/// 2. The `bool` at index `i` is `true` IFF `(tenant_id, digests[i])`
///    is alive (`deleted_at IS NULL`) in the backing store.
/// 3. Issue a single batched lookup (no per-digest round-trip) so the
///    p99 latency budget (≤ 50 ms @ 100 outputs per WI §3) is met.
///
/// The trait is **synchronous** to keep the crate wasm32-clean (no
/// tokio dependency); production wirings that need async I/O wrap
/// the call in their own runtime context (the worker handler does
/// the await on its side; this trait runs against a pre-fetched
/// snapshot).
pub trait BlobMetaReader: Send + Sync + fmt::Debug {
    /// Tenant id type — opaque string newtype provided by the caller
    /// (the worker handler uses `corelink-meta::TenantId` here).
    type TenantId;

    /// Look up the aliveness of every `(tenant_id, digest)` pair.
    /// See trait-level rustdoc for contract.
    ///
    /// # Errors
    ///
    /// Surface [`OutputsCheckError::BackendError`] on any backend
    /// round-trip failure; the handler maps to 503.
    fn batch_check_alive(
        &self,
        tenant_id: &Self::TenantId,
        digests: &[Digest],
    ) -> Result<Vec<bool>, OutputsCheckError>;
}

/// Validator trait — composes a [`BlobMetaReader`] into the
/// builder-side enforcement step. Handlers can swap implementations
/// (strict for `UpdateActionResult`, warn-only for
/// `GetActionResult` per WI §9.6) by selecting a different validator
/// instance.
pub trait OutputsValidator: Send + Sync + fmt::Debug {
    /// Tenant id passthrough.
    type TenantId;

    /// Validate every output digest in `result` is alive in `blob_meta`.
    ///
    /// # Errors
    ///
    /// Surface [`OutputsCheckError::BlobMissing`] on any tombstoned /
    /// missing digest; surface [`OutputsCheckError::BackendError`]
    /// on a backend round-trip failure.
    fn validate(
        &self,
        tenant_id: &Self::TenantId,
        result: &ActionResult,
    ) -> Result<(), OutputsCheckError>;
}

/// Canonical strict validator (server-side `UpdateActionResult`
/// pre-persist enforcement). Generic over the [`BlobMetaReader`] so
/// the same struct serves the production D1 path and the test
/// fixture path.
#[derive(Clone, Copy, Debug)]
pub struct StrictOutputsValidator<R> {
    reader: R,
}

impl<R> StrictOutputsValidator<R>
where
    R: BlobMetaReader,
{
    /// Construct a strict validator wrapping `reader`.
    #[must_use]
    pub const fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Borrow the inner reader (used by integration tests + the
    /// audit emit path that wants to reuse the same reader for
    /// observability counters).
    #[must_use]
    pub const fn reader(&self) -> &R {
        &self.reader
    }
}

impl<R> OutputsValidator for StrictOutputsValidator<R>
where
    R: BlobMetaReader,
{
    type TenantId = R::TenantId;

    fn validate(
        &self,
        tenant_id: &Self::TenantId,
        result: &ActionResult,
    ) -> Result<(), OutputsCheckError> {
        let digests: Vec<Digest> = result.iter_output_digests().collect();
        if digests.is_empty() {
            return Ok(());
        }
        let aliveness = self.reader.batch_check_alive(tenant_id, &digests)?;
        if aliveness.len() != digests.len() {
            return Err(OutputsCheckError::BackendError(format!(
                "batch_check_alive returned {} flags for {} digests",
                aliveness.len(),
                digests.len()
            )));
        }
        for (alive, digest) in aliveness.iter().zip(digests.iter()) {
            if !alive {
                return Err(OutputsCheckError::BlobMissing {
                    digest: digest.to_hex(),
                });
            }
        }
        Ok(())
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
    use crate::ac_core::types::{ActionResult, OutputFileDigest};
    use std::collections::HashSet;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct FakeReader {
        alive: Mutex<HashSet<(String, [u8; 32])>>,
    }

    impl FakeReader {
        fn new() -> Self {
            Self {
                alive: Mutex::new(HashSet::new()),
            }
        }

        fn add(&self, tenant: &str, d: &Digest) {
            let mut g = self.alive.lock().unwrap();
            g.insert((tenant.to_string(), *d.as_bytes()));
        }
    }

    impl BlobMetaReader for FakeReader {
        type TenantId = String;

        fn batch_check_alive(
            &self,
            tenant_id: &String,
            digests: &[Digest],
        ) -> Result<Vec<bool>, OutputsCheckError> {
            let g = self.alive.lock().unwrap();
            Ok(digests
                .iter()
                .map(|d| g.contains(&(tenant_id.clone(), *d.as_bytes())))
                .collect())
        }
    }

    #[test]
    fn empty_result_validates_ok() {
        let reader = FakeReader::new();
        let v = StrictOutputsValidator::new(reader);
        let r = ActionResult::new(Vec::new(), Vec::new(), 0, Vec::new());
        v.validate(&"t".to_string(), &r).unwrap();
    }

    #[test]
    fn all_alive_validates_ok() {
        let reader = FakeReader::new();
        let d_a = Digest::compute(b"a");
        let d_b = Digest::compute(b"b");
        reader.add("t", &d_a);
        reader.add("t", &d_b);
        let v = StrictOutputsValidator::new(reader);
        let r = ActionResult::new(
            vec![
                OutputFileDigest::new(d_a, 1),
                OutputFileDigest::new(d_b, 2),
            ],
            Vec::new(),
            0,
            Vec::new(),
        );
        v.validate(&"t".to_string(), &r).unwrap();
    }

    #[test]
    fn one_tombstoned_rejects() {
        let reader = FakeReader::new();
        let d_a = Digest::compute(b"a");
        let d_b = Digest::compute(b"b");
        reader.add("t", &d_a); // d_b NOT registered
        let v = StrictOutputsValidator::new(reader);
        let r = ActionResult::new(
            vec![
                OutputFileDigest::new(d_a, 1),
                OutputFileDigest::new(d_b, 2),
            ],
            Vec::new(),
            0,
            Vec::new(),
        );
        let err = v.validate(&"t".to_string(), &r).unwrap_err();
        assert!(matches!(err, OutputsCheckError::BlobMissing { .. }));
    }

    #[test]
    fn cross_tenant_aliveness_isolated() {
        // Tenant A owns d; tenant B asks for d → must reject.
        let reader = FakeReader::new();
        let d = Digest::compute(b"x");
        reader.add("A", &d);
        let v = StrictOutputsValidator::new(reader);
        let r = ActionResult::new(
            vec![OutputFileDigest::new(d, 1)],
            Vec::new(),
            0,
            Vec::new(),
        );
        let err = v.validate(&"B".to_string(), &r).unwrap_err();
        assert!(matches!(err, OutputsCheckError::BlobMissing { .. }));
    }
}
