//! Storage-layer metrics observer (WI-S01-003 §6.1.5).
//!
//! The WI requires three counters/histograms for the R2 single-blob hot
//! path:
//!
//! - `corelink.storage.r2.put_total{region, result}`
//!   (`result` ∈ {`ok`, `conflict_duplicate`, `error`}).
//! - `corelink.storage.r2.put_duration_seconds_bucket{region, blob_size_bucket}`.
//! - `corelink.storage.r2.get_duration_seconds_bucket{region}`.
//!
//! The concrete telemetry pipe (Cloudflare Workers Analytics, OTel, …) lands
//! in S-09 (observability sprint). To keep the storage adapter free of
//! telemetry-stack coupling at S-01, this module exposes a small
//! [`MetricsObserver`] trait + a [`NoopMetrics`] default. Production
//! deploys (WI-S01-005 + S-09) will plug their own observer in.
//!
//! Two design properties:
//! 1. The trait is `Send + Sync` so observers can be `Arc`-ed and shared.
//! 2. The trait methods take `&self` (no `&mut`) so observers must use
//!    interior mutability (`AtomicU64`, `dashmap`, …); the storage adapter
//!    never blocks on a metrics-side lock.
//!
//! ## Histogram bucketing
//!
//! Wall-clock duration is reported as a `Duration` per call; observers
//! decide their own histogram bucketing (the WI's
//! `_duration_seconds_bucket` shorthand is a Prometheus-style label, not a
//! constraint on the in-process API). [`PutSizeBucket`] gives the canonical
//! 6-variant layout for the `blob_size_bucket` label (5 inclusive size
//! buckets `[Le1KiB..Le5MiB]` plus an `Oversize` rejection sentinel) so
//! that any observer aggregating durations by size lines up with the
//! dashboard panels in S-09 ahead of time.

use core::sync::atomic::{AtomicU64, Ordering};
use core::time::Duration;

use crate::Region;

/// Outcome label for a PUT operation, mapped 1:1 to the
/// `result=` metric label in WI-S01-003 §6.1.5.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PutResultLabel {
    /// Fresh write — `result="ok"` per WI.
    Ok,
    /// `If-None-Match: *` rejection — `result="conflict_duplicate"`.
    ConflictDuplicate,
    /// Backend / transport fault — `result="error"`.
    Error,
}

impl PutResultLabel {
    /// Stable string label used by the metrics pipe.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::ConflictDuplicate => "conflict_duplicate",
            Self::Error => "error",
        }
    }
}

/// Outcome label for a GET operation (currently HTTP-success vs.
/// not-found vs. backend error). Kept symmetric with [`PutResultLabel`] so
/// the dashboard can reuse the same panel template.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GetResultLabel {
    /// 200 — body returned.
    Ok,
    /// 404 — blob not present.
    NotFound,
    /// Backend / transport fault.
    Error,
}

impl GetResultLabel {
    /// Stable string label used by the metrics pipe.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::NotFound => "not_found",
            Self::Error => "error",
        }
    }
}

/// Canonical `blob_size_bucket` histogram label per WI-S01-003 §6.1.5.
///
/// The buckets are picked to cover the practical CAS write distribution:
/// the long tail is the small / medium body cluster, with a hard ceiling
/// at the 5 MiB single-blob limit. Variant names follow the `Le<N>`
/// "less than or equal to" convention: `Le1KiB` covers `[0, 1024]`
/// inclusive, etc. Bodies above `5 MiB` are rejected at the writer
/// (`R2Error::BlobTooLarge`) and surface as the [`Self::Oversize`] sentinel
/// here so the histogram correctly reflects the rejection rate without
/// folding rejected writes into the largest valid bucket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PutSizeBucket {
    /// `[0, 1024]` bytes (inclusive).
    Le1KiB,
    /// `(1024, 16 384]` bytes.
    Le16KiB,
    /// `(16 384, 262 144]` bytes.
    Le256KiB,
    /// `(262 144, 1 048 576]` bytes.
    Le1MiB,
    /// `(1 048 576, 5 242 880]` bytes (single-blob upper bound, inclusive).
    Le5MiB,
    /// `> 5 242 880` bytes — rejected at the writer with
    /// [`crate::storage::error::R2Error::BlobTooLarge`]; surfaces here so
    /// the histogram can chart the rejection rate.
    Oversize,
}

impl PutSizeBucket {
    /// Pick the bucket for an observed body size, matching the inclusive
    /// `Le<N>` semantics.
    #[must_use]
    pub const fn from_bytes(size: usize) -> Self {
        if size <= 1024 {
            Self::Le1KiB
        } else if size <= 16 * 1024 {
            Self::Le16KiB
        } else if size <= 256 * 1024 {
            Self::Le256KiB
        } else if size <= 1024 * 1024 {
            Self::Le1MiB
        } else if size <= 5 * 1024 * 1024 {
            Self::Le5MiB
        } else {
            Self::Oversize
        }
    }

    /// Stable string label used by the metrics pipe.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Le1KiB => "le1kib",
            Self::Le16KiB => "le16kib",
            Self::Le256KiB => "le256kib",
            Self::Le1MiB => "le1mib",
            Self::Le5MiB => "le5mib",
            Self::Oversize => "oversize",
        }
    }
}

/// Pluggable metrics observer for R2 storage events.
///
/// The S-09 observability stack will provide a concrete implementation; the
/// in-crate default is [`NoopMetrics`] (no-op for unit tests / fake
/// backends). The R2 adapter calls each method exactly once per operation,
/// in the success or failure tail.
///
/// The duration methods are emitted on every PUT/GET call (success or
/// failure path); concrete observers can choose to bucket by latency
/// percentile (Prometheus histogram), by sliding window (CF Workers
/// Analytics), or to forward the raw observation to OTel.
pub trait MetricsObserver: Send + Sync {
    /// Record one PUT outcome counter.
    fn record_put(&self, region: Region, result: PutResultLabel);

    /// Record one GET outcome counter.
    fn record_get(&self, region: Region, result: GetResultLabel);

    /// Record the wall-clock duration of a PUT call, labeled by region and
    /// the body's size bucket. Default impl is no-op so observers that
    /// only care about counters can opt in.
    fn record_put_duration(&self, _region: Region, _bucket: PutSizeBucket, _duration: Duration) {}

    /// Record the wall-clock duration of a GET call, labeled by region.
    /// Default impl is no-op.
    fn record_get_duration(&self, _region: Region, _duration: Duration) {}
}

/// No-op metrics observer — production deploys swap this for a real one
/// in WI-S01-005 / S-09. Used as the default so the writer/reader can be
/// constructed without any telemetry plumbing in unit tests.
#[derive(Debug, Default)]
pub struct NoopMetrics;

impl MetricsObserver for NoopMetrics {
    fn record_put(&self, _region: Region, _result: PutResultLabel) {}
    fn record_get(&self, _region: Region, _result: GetResultLabel) {}
}

/// Process-local counter + duration-sample observer for tests +
/// introspection.
///
/// Tracks counters per `(region, result)` cell, plus per-call duration
/// observation counts per `(region, blob_size_bucket)` for PUT and per
/// `region` for GET. Concrete telemetry sinks (Workers Analytics / OTel)
/// land in S-09 with their own histograms; this struct is the load-bearing
/// integration-test sink + an existence proof that every code path emits
/// the WI's required (region, result, duration) triple.
#[derive(Debug, Default)]
pub struct InMemoryMetrics {
    put_ok: [AtomicU64; 3],
    put_dup: [AtomicU64; 3],
    put_err: [AtomicU64; 3],
    get_ok: [AtomicU64; 3],
    get_nf: [AtomicU64; 3],
    get_err: [AtomicU64; 3],
    // PUT duration observations by (region, size bucket).
    put_dur_le1k: [AtomicU64; 3],
    put_dur_le16k: [AtomicU64; 3],
    put_dur_le256k: [AtomicU64; 3],
    put_dur_le1m: [AtomicU64; 3],
    put_dur_le5m: [AtomicU64; 3],
    put_dur_oversize: [AtomicU64; 3],
    // GET duration observations by region.
    get_dur: [AtomicU64; 3],
}

impl InMemoryMetrics {
    /// Allocate a fresh observer with all counters at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the current PUT counter for `(region, result)`.
    #[must_use]
    pub fn put_count(&self, region: Region, result: PutResultLabel) -> u64 {
        self.bucket_for_put(result, region).load(Ordering::Relaxed)
    }

    /// Read the current GET counter for `(region, result)`.
    #[must_use]
    pub fn get_count(&self, region: Region, result: GetResultLabel) -> u64 {
        self.bucket_for_get(result, region).load(Ordering::Relaxed)
    }

    /// Read the count of PUT duration observations for `(region, bucket)`.
    #[must_use]
    pub fn put_duration_observations(&self, region: Region, bucket: PutSizeBucket) -> u64 {
        self.bucket_for_put_dur(bucket, region)
            .load(Ordering::Relaxed)
    }

    /// Read the count of GET duration observations for `region`.
    #[must_use]
    pub fn get_duration_observations(&self, region: Region) -> u64 {
        let idx = Self::region_idx(region);
        self.get_dur
            .get(idx)
            .map_or(0, |a| a.load(Ordering::Relaxed))
    }

    fn region_idx(region: Region) -> usize {
        match region {
            Region::Wnam => 0,
            Region::Weur => 1,
            Region::Sam => 2,
        }
    }

    fn bucket_for_put(&self, result: PutResultLabel, region: Region) -> &AtomicU64 {
        let idx = Self::region_idx(region);
        let bank = match result {
            PutResultLabel::Ok => &self.put_ok,
            PutResultLabel::ConflictDuplicate => &self.put_dup,
            PutResultLabel::Error => &self.put_err,
        };
        bank.get(idx).unwrap_or(&self.put_ok[0])
    }

    fn bucket_for_get(&self, result: GetResultLabel, region: Region) -> &AtomicU64 {
        let idx = Self::region_idx(region);
        let bank = match result {
            GetResultLabel::Ok => &self.get_ok,
            GetResultLabel::NotFound => &self.get_nf,
            GetResultLabel::Error => &self.get_err,
        };
        bank.get(idx).unwrap_or(&self.get_ok[0])
    }

    fn bucket_for_put_dur(&self, bucket: PutSizeBucket, region: Region) -> &AtomicU64 {
        let idx = Self::region_idx(region);
        let bank = match bucket {
            PutSizeBucket::Le1KiB => &self.put_dur_le1k,
            PutSizeBucket::Le16KiB => &self.put_dur_le16k,
            PutSizeBucket::Le256KiB => &self.put_dur_le256k,
            PutSizeBucket::Le1MiB => &self.put_dur_le1m,
            PutSizeBucket::Le5MiB => &self.put_dur_le5m,
            PutSizeBucket::Oversize => &self.put_dur_oversize,
        };
        bank.get(idx).unwrap_or(&self.put_dur_le1k[0])
    }
}

impl MetricsObserver for InMemoryMetrics {
    fn record_put(&self, region: Region, result: PutResultLabel) {
        self.bucket_for_put(result, region)
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_get(&self, region: Region, result: GetResultLabel) {
        self.bucket_for_get(result, region)
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_put_duration(&self, region: Region, bucket: PutSizeBucket, _duration: Duration) {
        self.bucket_for_put_dur(bucket, region)
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_get_duration(&self, region: Region, _duration: Duration) {
        let idx = Self::region_idx(region);
        if let Some(slot) = self.get_dur.get(idx) {
            slot.fetch_add(1, Ordering::Relaxed);
        }
    }
}
