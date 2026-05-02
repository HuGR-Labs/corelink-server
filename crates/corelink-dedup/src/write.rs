//! Dedup-on-write counter helper (R-S07-2).
//!
//! ## Why a thin helper (and not a method on `DedupIndex`)
//!
//! The dedup-on-write counter increment needs to fire from EVERY chunk
//! upsert path: REAPI v2 `BatchUpdateBlobs`, the multipart `UploadPart`
//! handler (S-05), the SplitBlob assembler (WI-S05-001), and any future
//! direct `chunks` write code path. Pulling the counter logic into a
//! free function lets each caller compose `record_chunk_write(...)` on
//! top of its own `chunks` upsert without taking a hard dependency on
//! the [`crate::index::DedupIndex`] trait (which is the read seam, not
//! the write seam).
//!
//! ## Contract
//!
//! For each chunk upsert outcome:
//!
//! - **Fresh INSERT** → bump `corelink.dedup.chunks_inserted_total{tenant}`
//!   by 1 (counter; monotone — never decremented).
//! - **ON CONFLICT (refcount increment)** → bump
//!   `corelink.dedup.chunks_reused_total{tenant}` by 1.
//!
//! Per S-05 lesson absorbed in WI-S07-001 §6.1.6 (Lote 10.7bis P0-1
//! fix): a duplicate INSERT for `(tenant_id, chunk_digest)` is the
//! NORMAL dedup-hit path (not a 409 reject); the canonical SQL is
//! `INSERT ... ON CONFLICT (tenant_id, chunk_digest) DO UPDATE SET
//! refcount = refcount + 1, deleted_at = NULL`. The fake mirrors this
//! semantic via [`crate::index::InMemoryDedupIndex::upsert_chunk`]
//! returning `bool` (`true` = inserted; `false` = reused).
//!
//! ## Error handling — log-and-continue
//!
//! Per WI §14.s07.001.6 the metrics emit failure is not load-bearing:
//! a metric backend hiccup MUST NOT abort the chunk upsert (which
//! would be observed by the customer as a 5xx). The helper surfaces
//! the [`crate::DedupMetricsObserverError`] so the caller can decide
//! to log-and-continue (production wiring) OR fail-closed (test
//! fixture asserting the helper's atomicity contract).

use uuid::Uuid;

use crate::error::DedupError;
use crate::metrics::DedupMetricsObserver;

/// Outcome of a single chunk upsert. Mirrors the canonical SQL `INSERT
/// ... ON CONFLICT (tenant_id, chunk_digest) DO UPDATE SET refcount =
/// refcount + 1` semantic byte-for-byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChunkWriteOutcome {
    /// Fresh INSERT — no prior `(tenant_id, chunk_digest)` row.
    /// Triggers `chunks_inserted_total` counter increment.
    Inserted,
    /// ON CONFLICT — existing row's refcount incremented (or
    /// deleted_at cleared on undelete). Triggers `chunks_reused_total`
    /// counter increment.
    Reused,
}

impl ChunkWriteOutcome {
    /// Construct from the boolean returned by
    /// [`crate::index::InMemoryDedupIndex::upsert_chunk`] (`true` =>
    /// Inserted, `false` => Reused).
    #[must_use]
    pub const fn from_inserted_bool(inserted: bool) -> Self {
        if inserted {
            Self::Inserted
        } else {
            Self::Reused
        }
    }
}

/// Record a chunk write outcome on the metrics observer.
///
/// Increments exactly ONE counter per call:
///
/// - `outcome = Inserted` → `chunks_inserted_total{tenant}` += 1.
/// - `outcome = Reused`   → `chunks_reused_total{tenant}` += 1.
///
/// The two counters are monotone (never decremented); the dedup ratio
/// downstream is `reused / (reused + inserted)` per spec_contract §1
/// SOTA framing.
///
/// # Errors
///
/// Returns [`DedupError::Metrics`] when the observer surfaces a
/// backend error. Production wiring SHOULD log-and-continue (the
/// metric gap is non-load-bearing; the chunk upsert already
/// succeeded). The test fixtures use [`crate::metrics::FailingDedupMetrics`]
/// to exercise this path.
pub fn record_chunk_write<O>(
    observer: &O,
    tenant_id: Uuid,
    outcome: ChunkWriteOutcome,
) -> Result<(), DedupError>
where
    O: DedupMetricsObserver + ?Sized,
{
    match outcome {
        ChunkWriteOutcome::Inserted => observer.record_chunks_inserted(tenant_id)?,
        ChunkWriteOutcome::Reused => observer.record_chunks_reused(tenant_id)?,
    }
    Ok(())
}

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
    use crate::metrics::{
        DedupMetricKind, FailingDedupMetrics, InMemoryDedupMetrics,
    };

    fn ten() -> Uuid {
        Uuid::from_u128(0x1)
    }

    #[test]
    fn from_inserted_bool_round_trip() {
        assert_eq!(
            ChunkWriteOutcome::from_inserted_bool(true),
            ChunkWriteOutcome::Inserted
        );
        assert_eq!(
            ChunkWriteOutcome::from_inserted_bool(false),
            ChunkWriteOutcome::Reused
        );
    }

    #[test]
    fn inserted_outcome_bumps_inserted_counter_only() {
        let m = InMemoryDedupMetrics::new();
        record_chunk_write(&m, ten(), ChunkWriteOutcome::Inserted).unwrap();
        assert_eq!(m.counter_total(DedupMetricKind::ChunksInserted), 1);
        assert_eq!(m.counter_total(DedupMetricKind::ChunksReused), 0);
    }

    #[test]
    fn reused_outcome_bumps_reused_counter_only() {
        let m = InMemoryDedupMetrics::new();
        record_chunk_write(&m, ten(), ChunkWriteOutcome::Reused).unwrap();
        assert_eq!(m.counter_total(DedupMetricKind::ChunksInserted), 0);
        assert_eq!(m.counter_total(DedupMetricKind::ChunksReused), 1);
    }

    #[test]
    fn repeated_writes_accumulate() {
        let m = InMemoryDedupMetrics::new();
        for _ in 0..5 {
            record_chunk_write(&m, ten(), ChunkWriteOutcome::Inserted).unwrap();
        }
        for _ in 0..15 {
            record_chunk_write(&m, ten(), ChunkWriteOutcome::Reused).unwrap();
        }
        assert_eq!(m.counter_total(DedupMetricKind::ChunksInserted), 5);
        assert_eq!(m.counter_total(DedupMetricKind::ChunksReused), 15);
        // Dedup ratio = 15 / (5 + 15) = 0.75 (4× compression).
        let r = m.dedup_ratio().unwrap();
        assert!((r - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn failing_observer_surfaces_metrics_error() {
        let m = FailingDedupMetrics::new();
        let err = record_chunk_write(&m, ten(), ChunkWriteOutcome::Inserted).unwrap_err();
        assert!(matches!(err, DedupError::Metrics(_)));
        let err = record_chunk_write(&m, ten(), ChunkWriteOutcome::Reused).unwrap_err();
        assert!(matches!(err, DedupError::Metrics(_)));
    }
}
