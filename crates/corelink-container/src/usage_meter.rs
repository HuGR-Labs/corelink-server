//! In-process **display** usage aggregator (BE-1 reads/writes/daily + BE-2 cache
//! hit-rate / $-saved) for the customer dashboard ROI surface.
//!
//! # Why this is not on the hot path
//!
//! The customer ROI surface needs per-tenant reads/writes/hits/misses. Counting
//! those with a synchronous D1 write per cache op would add a D1-over-HTTP
//! round-trip to the **read** hot path (today only OCI pays a sync counter, and
//! only for request-cap billing — see [`crate::request_count`]). That is the one
//! thing we must not do.
//!
//! Instead [`UsageMeter::record`] is a cheap in-memory `lock + increment` (no
//! `await`, no I/O) called fire-and-forget from a handler AFTER its hit/miss
//! decision. A background task ([`UsageMeter::spawn_flusher`]) drains the
//! accumulated deltas every ~30s and applies them with an **additive** `+=`
//! UPSERT into `usage_daily` (migration 0089). Additive means N container
//! instances flushing the same `(tenant, day)` row simply sum — no coordination.
//!
//! # Honest approximation
//!
//! This is DISPLAY telemetry, never billing. Billing stays authoritative on the
//! synchronous paths (`monthly_request_counts` request caps, `tenant_quota` /
//! `tenant_storage_state` bytes). A container eviction can drop at most one
//! un-flushed window (~30s) of counts for that instance; a transient D1 fault on
//! flush re-queues the delta (so a fault does not lose it). The displayed
//! hit-rate / savings is therefore eventually-consistent and may slightly
//! under-count — acceptable for a dashboard estimate, and it is NEVER read on a
//! request path or used to gate/bill anything.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

/// One classified cache op, recorded fire-and-forget by a surface handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageEvent {
    /// A read that HIT the cache (e.g. GET → 200 / found).
    ReadHit,
    /// A read that MISSED (e.g. GET → 404 / not found).
    ReadMiss,
    /// A write (e.g. PUT / upload).
    Write,
}

/// A per-`(tenant, day)` accumulation of classified ops. All monotonic,
/// non-negative, additive across container instances.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta {
    /// Cache read ops (`hits + misses`).
    pub reads: u64,
    /// Cache write ops.
    pub writes: u64,
    /// Reads that hit.
    pub hits: u64,
    /// Reads that missed.
    pub misses: u64,
}

impl Delta {
    fn apply(&mut self, ev: UsageEvent) {
        match ev {
            UsageEvent::ReadHit => {
                self.reads += 1;
                self.hits += 1;
            }
            UsageEvent::ReadMiss => {
                self.reads += 1;
                self.misses += 1;
            }
            UsageEvent::Write => self.writes += 1,
        }
    }

    fn merge(&mut self, other: Delta) {
        self.reads += other.reads;
        self.writes += other.writes;
        self.hits += other.hits;
        self.misses += other.misses;
    }

    fn is_empty(&self) -> bool {
        self.reads == 0 && self.writes == 0 && self.hits == 0 && self.misses == 0
    }
}

/// The durable sink the flusher applies deltas to (an additive `+=` UPSERT into
/// `usage_daily`). Errors are surfaced so the flusher can re-queue the delta;
/// they never propagate to a request.
#[async_trait]
pub trait UsageDailySink: Send + Sync + std::fmt::Debug {
    /// Additively apply one `(tenant, day)` delta. `now_ms` stamps
    /// `updated_at_ms`.
    async fn upsert(&self, tenant: &str, day: &str, delta: Delta, now_ms: i64) -> Result<(), String>;
}

/// The in-process meter. Cheap to `record`; flushed in the background.
///
/// `sink == None` (dev/CI, no D1 env) makes [`record`](Self::record) a no-op so
/// the map never grows unbounded without a flusher to drain it.
pub struct UsageMeter {
    buckets: Mutex<HashMap<(String, String), Delta>>,
    sink: Option<Arc<dyn UsageDailySink>>,
    clock: fn() -> i64,
}

impl std::fmt::Debug for UsageMeter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsageMeter")
            .field("wired", &self.sink.is_some())
            .finish_non_exhaustive()
    }
}

impl UsageMeter {
    /// Build over an explicit sink + clock (tests inject both).
    #[must_use]
    pub fn new(sink: Option<Arc<dyn UsageDailySink>>, clock: fn() -> i64) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            sink,
            clock,
        }
    }

    /// Production meter from process env: wires a D1-backed sink when
    /// [`crate::storage::StorageEnv`] is present, else an inert (record-is-noop)
    /// meter for dev/CI. Mirrors [`crate::request_count::RequestCountGate::from_env`].
    #[must_use]
    pub fn from_env() -> Arc<Self> {
        let sink: Option<Arc<dyn UsageDailySink>> = crate::storage::StorageEnv::from_env()
            .and_then(|env| {
                crate::storage::d1_http::D1HttpClient::new(&env)
                    .map_err(|e| tracing::warn!(error = %e, "usage-meter: D1 client init failed"))
                    .ok()
            })
            .map(|client| Arc::new(D1UsageDailySink::new(Arc::new(client))) as Arc<dyn UsageDailySink>);
        if sink.is_none() {
            tracing::warn!("StorageEnv unset/invalid; usage-meter INERT (dev/CI — record is a no-op)");
        }
        Arc::new(Self::new(sink, || {
            i64::try_from(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0),
            )
            .unwrap_or(i64::MAX)
        }))
    }

    /// Record one classified cache op. **Cheap and non-blocking**: a single
    /// mutex `lock + increment`, no `await`, no I/O. Call it fire-and-forget
    /// from a handler after the hit/miss decision. A no-op when the meter is
    /// inert (dev/CI) or the tenant is empty/sentinel.
    pub fn record(&self, tenant: &str, ev: UsageEvent) {
        if self.sink.is_none() || tenant.is_empty() {
            return;
        }
        let day = day_bucket_utc((self.clock)());
        let key = (tenant.to_owned(), day);
        // Poisoned lock (a prior panic while holding it) must never take down a
        // request path — recover the guard and continue.
        let mut g = match self.buckets.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        g.entry(key).or_default().apply(ev);
    }

    /// Drain the accumulated deltas and apply them to the sink. On a transient
    /// sink error the delta is re-queued (merged back) so a D1 blip never loses
    /// counts. No lock is held across the `await`.
    pub async fn flush(&self) {
        let Some(sink) = self.sink.as_ref() else {
            return;
        };
        let drained: Vec<((String, String), Delta)> = {
            let mut g = match self.buckets.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            g.drain().filter(|(_, d)| !d.is_empty()).collect()
        };
        if drained.is_empty() {
            return;
        }
        let now_ms = (self.clock)();
        for ((tenant, day), delta) in drained {
            if let Err(e) = sink.upsert(&tenant, &day, delta, now_ms).await {
                tracing::warn!(error = %e, tenant = %tenant, day = %day, "usage_daily flush failed; re-queuing delta");
                let mut g = match self.buckets.lock() {
                    Ok(g) => g,
                    Err(p) => p.into_inner(),
                };
                g.entry((tenant, day)).or_default().merge(delta);
            }
        }
    }

    /// Spawn the background flush loop (every `period`). No-op when inert.
    pub fn spawn_flusher(self: Arc<Self>, period: Duration) {
        if self.sink.is_none() {
            return;
        }
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(period);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                self.flush().await;
            }
        });
    }
}

/// Production [`UsageDailySink`] over the `usage_daily` D1 table (migration
/// 0089), reached via [`crate::storage::d1_http::D1HttpClient`]. The additive
/// UPSERT mirrors the request-count store's shape (positional binds; the tenant
/// scope rides the conflict key — INV-TENANT-ISOLATION).
#[derive(Debug)]
pub struct D1UsageDailySink {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl D1UsageDailySink {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl UsageDailySink for D1UsageDailySink {
    async fn upsert(&self, tenant: &str, day: &str, delta: Delta, now_ms: i64) -> Result<(), String> {
        // Saturating i64 casts: the counters are display telemetry and can never
        // realistically overflow i64, but never panic on the request-adjacent path.
        let reads = i64::try_from(delta.reads).unwrap_or(i64::MAX);
        let writes = i64::try_from(delta.writes).unwrap_or(i64::MAX);
        let hits = i64::try_from(delta.hits).unwrap_or(i64::MAX);
        let misses = i64::try_from(delta.misses).unwrap_or(i64::MAX);
        self.client
            .query(
                "INSERT INTO usage_daily \
                     (tenant_id, day, reads, writes, hits, misses, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
                 ON CONFLICT(tenant_id, day) DO UPDATE SET \
                     reads = reads + ?3, writes = writes + ?4, \
                     hits = hits + ?5, misses = misses + ?6, \
                     updated_at_ms = ?7",
                &[
                    serde_json::Value::String(tenant.to_owned()),
                    serde_json::Value::String(day.to_owned()),
                    serde_json::Value::from(reads),
                    serde_json::Value::from(writes),
                    serde_json::Value::from(hits),
                    serde_json::Value::from(misses),
                    serde_json::Value::from(now_ms),
                ],
            )
            .await
            .map(|_| ())
    }
}

/// The UTC calendar day as a fixed `YYYY-MM-DD` string (e.g. `2026-07-07`), the
/// `usage_daily.day` bucket key. Howard Hinnant's branchless `civil_from_days`
/// (proleptic Gregorian) — matches `new Date().toISOString().slice(0, 10)` and
/// the month-bucket derivation in [`crate::request_count`].
#[must_use]
pub fn day_bucket_utc(now_ms: i64) -> String {
    let (y, m, d) = civil_from_days(now_ms.div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Hinnant `civil_from_days` returning full `(year, month, day)`.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
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
    use std::sync::Mutex as StdMutex;

    /// Records every upsert; optionally fails the first N calls to exercise the
    /// re-queue path.
    #[derive(Debug, Default)]
    struct RecordingSink {
        calls: StdMutex<Vec<(String, String, Delta)>>,
        fail_next: StdMutex<usize>,
    }

    #[async_trait]
    impl UsageDailySink for RecordingSink {
        async fn upsert(&self, tenant: &str, day: &str, delta: Delta, _now_ms: i64) -> Result<(), String> {
            {
                let mut f = self.fail_next.lock().unwrap();
                if *f > 0 {
                    *f -= 1;
                    return Err("transient D1 fault".to_owned());
                }
            }
            self.calls.lock().unwrap().push((tenant.to_owned(), day.to_owned(), delta));
            Ok(())
        }
    }

    // A fixed clock: 2026-07-07T00:00:00Z = 1783382400000 ms.
    const FIXED_MS: i64 = 1_783_382_400_000;
    fn fixed_clock() -> i64 {
        FIXED_MS
    }

    #[test]
    fn day_bucket_is_utc_yyyy_mm_dd() {
        assert_eq!(day_bucket_utc(FIXED_MS), "2026-07-07");
        assert_eq!(day_bucket_utc(0), "1970-01-01");
        // 1 ms before the next UTC midnight stays on the same day.
        assert_eq!(day_bucket_utc(FIXED_MS + 86_400_000 - 1), "2026-07-07");
        // Next UTC midnight rolls over.
        assert_eq!(day_bucket_utc(FIXED_MS + 86_400_000), "2026-07-08");
    }

    #[tokio::test]
    async fn record_aggregates_then_flush_upserts_one_additive_row() {
        let sink = Arc::new(RecordingSink::default());
        let meter = UsageMeter::new(Some(sink.clone()), fixed_clock);
        meter.record("t1", UsageEvent::ReadHit);
        meter.record("t1", UsageEvent::ReadHit);
        meter.record("t1", UsageEvent::ReadMiss);
        meter.record("t1", UsageEvent::Write);
        meter.flush().await;

        let calls = sink.calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "one coalesced (tenant, day) row");
        let (tenant, day, delta) = &calls[0];
        assert_eq!(tenant, "t1");
        assert_eq!(day, "2026-07-07");
        assert_eq!(delta.reads, 3);
        assert_eq!(delta.writes, 1);
        assert_eq!(delta.hits, 2);
        assert_eq!(delta.misses, 1);
    }

    #[tokio::test]
    async fn separate_tenants_flush_as_separate_rows() {
        let sink = Arc::new(RecordingSink::default());
        let meter = UsageMeter::new(Some(sink.clone()), fixed_clock);
        meter.record("t1", UsageEvent::ReadHit);
        meter.record("t2", UsageEvent::Write);
        meter.flush().await;
        assert_eq!(sink.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn flush_drains_so_a_second_flush_is_a_noop() {
        let sink = Arc::new(RecordingSink::default());
        let meter = UsageMeter::new(Some(sink.clone()), fixed_clock);
        meter.record("t1", UsageEvent::ReadHit);
        meter.flush().await;
        meter.flush().await;
        assert_eq!(sink.calls.lock().unwrap().len(), 1, "drained — second flush upserts nothing");
    }

    #[tokio::test]
    async fn sink_error_requeues_the_delta_no_loss() {
        let sink = Arc::new(RecordingSink::default());
        *sink.fail_next.lock().unwrap() = 1; // fail the first upsert
        let meter = UsageMeter::new(Some(sink.clone()), fixed_clock);
        meter.record("t1", UsageEvent::ReadHit);
        meter.record("t1", UsageEvent::ReadHit);
        meter.flush().await; // fails → re-queued
        assert_eq!(sink.calls.lock().unwrap().len(), 0, "first flush failed");
        meter.flush().await; // succeeds with the SAME delta
        let calls = sink.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2.hits, 2, "no counts lost across the fault");
    }

    #[tokio::test]
    async fn inert_meter_never_records_or_flushes() {
        let meter = UsageMeter::new(None, fixed_clock);
        meter.record("t1", UsageEvent::ReadHit); // no-op
        meter.flush().await; // no-op (no sink)
        // Nothing to assert beyond "does not panic / does not accumulate": a
        // record on an inert meter must not grow the map.
        assert!(meter.buckets.lock().unwrap().is_empty());
    }

    #[test]
    fn empty_tenant_is_never_recorded() {
        let sink = Arc::new(RecordingSink::default());
        let meter = UsageMeter::new(Some(sink), fixed_clock);
        meter.record("", UsageEvent::ReadHit);
        assert!(meter.buckets.lock().unwrap().is_empty());
    }
}
