//! `corelink bench` — micro-benchmark vs cluster (WI-S15-001).
//!
//! Default `--full` = 100 write+read round-trips; reports p50/p95/p99 latency
//! + throughput.

use std::fmt;
use std::time::Instant;

use serde::Serialize;

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// Benchmark mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BenchMode {
    /// Only write operations.
    Write,
    /// Only read operations.
    Read,
    /// Full round-trip (write + read; default).
    Full,
}

/// Latency percentile stats (milliseconds).
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct LatencyStats {
    /// p50 latency in ms.
    pub p50_ms: u64,
    /// p95 latency in ms.
    pub p95_ms: u64,
    /// p99 latency in ms.
    pub p99_ms: u64,
    /// Minimum observed latency in ms.
    pub min_ms: u64,
    /// Maximum observed latency in ms.
    pub max_ms: u64,
}

/// Benchmark result.
#[derive(Debug, Serialize)]
#[non_exhaustive]
pub struct BenchResult {
    /// Mode used.
    pub mode: String,
    /// Number of operations executed.
    pub ops: u32,
    /// Total elapsed time in ms.
    pub total_ms: u64,
    /// Throughput in ops/sec.
    pub ops_per_sec: f64,
    /// Write latency stats (None for read-only mode).
    pub write_latency: Option<LatencyStats>,
    /// Read latency stats (None for write-only mode).
    pub read_latency: Option<LatencyStats>,
}

impl fmt::Display for BenchResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Mode:        {}", self.mode)?;
        writeln!(f, "Ops:         {}", self.ops)?;
        writeln!(f, "Total time:  {}ms", self.total_ms)?;
        writeln!(f, "Throughput:  {:.1} ops/sec", self.ops_per_sec)?;
        if let Some(w) = &self.write_latency {
            writeln!(
                f,
                "Write p50/p95/p99: {}ms / {}ms / {}ms",
                w.p50_ms, w.p95_ms, w.p99_ms
            )?;
        }
        if let Some(r) = &self.read_latency {
            write!(
                f,
                "Read  p50/p95/p99: {}ms / {}ms / {}ms",
                r.p50_ms, r.p95_ms, r.p99_ms
            )?;
        }
        Ok(())
    }
}

/// Run `corelink bench [--write|--read|--full]`.
pub async fn run(
    client: &CorelinkClient,
    write: bool,
    read: bool,
    full: bool,
    format: OutputFormat,
) -> Result<(), CliError> {
    let mode = match (write, read, full) {
        (true, false, false) => BenchMode::Write,
        (false, true, false) => BenchMode::Read,
        _ => BenchMode::Full,
    };

    let ops: u32 = 100;
    let blob_size: usize = 1024; // 1 KB

    let overall_start = Instant::now();
    let mut write_latencies: Vec<u64> = Vec::with_capacity(ops as usize);
    let mut read_latencies: Vec<u64> = Vec::with_capacity(ops as usize);
    let mut digests: Vec<String> = Vec::with_capacity(ops as usize);

    if mode == BenchMode::Write || mode == BenchMode::Full {
        for i in 0..ops {
            let blob = bench_blob(blob_size, i);
            let digest = blob_digest(&blob);
            let t = Instant::now();
            // Real CAS write: PUT /v1/cas/<tenant>/<blake3>. A failed op is
            // surfaced (never a fake latency for a swallowed 404).
            client.cas_put(&digest, bytes::Bytes::from(blob)).await?;
            write_latencies.push(t.elapsed().as_millis() as u64);
            digests.push(digest);
        }
    }

    if mode == BenchMode::Read || mode == BenchMode::Full {
        // Read-only mode has no freshly-written digests; seed one real blob
        // (untimed) so the read benchmark exercises a genuine CAS GET rather
        // than a guaranteed 404.
        if digests.is_empty() {
            let blob = bench_blob(blob_size, 0);
            let digest = blob_digest(&blob);
            client.cas_put(&digest, bytes::Bytes::from(blob)).await?;
            digests.push(digest);
        }
        for i in 0..ops as usize {
            // `digests` is non-empty here (seeded above / filled by writes).
            let idx = i % digests.len();
            let digest = digests.get(idx).map(String::as_str).unwrap_or_default();
            let t = Instant::now();
            // Real CAS read: GET /v1/cas/<tenant>/<blake3>. Errors surface.
            client.cas_get(digest).await?;
            read_latencies.push(t.elapsed().as_millis() as u64);
        }
    }

    let total_ms = overall_start.elapsed().as_millis() as u64;
    let total_ops = if mode == BenchMode::Full {
        ops * 2
    } else {
        ops
    };
    let ops_per_sec = if total_ms > 0 {
        (total_ops as f64) * 1000.0 / (total_ms as f64)
    } else {
        0.0
    };

    let result = BenchResult {
        mode: format!("{mode:?}").to_lowercase(),
        ops,
        total_ms,
        ops_per_sec,
        write_latency: if write_latencies.is_empty() {
            None
        } else {
            Some(compute_stats(&mut write_latencies))
        },
        read_latency: if read_latencies.is_empty() {
            None
        } else {
            Some(compute_stats(&mut read_latencies))
        },
    };

    let fmt = Formatter::new(format);
    fmt.emit(&result).map_err(CliError::Json)?;

    Ok(())
}

/// Deterministic synthetic blob of `size` bytes, salted by `seed` so distinct
/// iterations write distinct CAS objects (not an idempotent re-PUT of one
/// blob) — a more representative write benchmark.
fn bench_blob(size: usize, seed: u32) -> Vec<u8> {
    let salt = seed as u8;
    (0..size).map(|j| (j as u8).wrapping_add(salt)).collect()
}

/// BLAKE3 hex digest of a blob (64-char lowercase) — the CAS content address.
fn blob_digest(blob: &[u8]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(blob);
    hasher.finalize().to_hex().to_string()
}

fn compute_stats(samples: &mut [u64]) -> LatencyStats {
    samples.sort_unstable();
    let p50 = percentile(samples, 50);
    let p95 = percentile(samples, 95);
    let p99 = percentile(samples, 99);
    let min = samples.first().copied().unwrap_or(0);
    let max = samples.last().copied().unwrap_or(0);
    LatencyStats {
        p50_ms: p50,
        p95_ms: p95,
        p99_ms: p99,
        min_ms: min,
        max_ms: max,
    }
}

fn percentile(sorted: &[u64], pct: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    // Index = ceil(len * pct / 100) - 1, clamped to [0, len-1].
    // For pct=0 we return the minimum (index 0).
    let raw = (sorted.len() * pct).div_ceil(100);
    let idx = raw.saturating_sub(1).min(sorted.len() - 1);
    sorted.get(idx).copied().unwrap_or(0)
}

#[cfg(test)]
#[allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    #[test]
    fn percentile_basic() {
        let data = vec![10u64, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&data, 50), 50);
        assert_eq!(percentile(&data, 99), 100);
        assert_eq!(percentile(&data, 0), 10);
    }

    #[test]
    fn percentile_single_element() {
        let data = vec![42u64];
        assert_eq!(percentile(&data, 99), 42);
    }

    #[test]
    fn percentile_empty() {
        let data: Vec<u64> = vec![];
        assert_eq!(percentile(&data, 50), 0);
    }

    #[test]
    fn bench_result_serialises() {
        let r = BenchResult {
            mode: "full".to_owned(),
            ops: 100,
            total_ms: 5000,
            ops_per_sec: 40.0,
            write_latency: None,
            read_latency: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"ops_per_sec\""));
    }

    #[test]
    fn blob_digest_is_blake3() {
        // The bench write loop addresses each PUT by this digest; it MUST be
        // BLAKE3 (native CAS 422s a non-BLAKE3 claim), matching `blake3::hash`.
        let blob = bench_blob(1024, 7);
        assert_eq!(blob_digest(&blob), blake3::hash(&blob).to_hex().to_string());
        assert_eq!(blob_digest(&blob).len(), 64);
    }

    #[test]
    fn bench_blob_varies_by_seed() {
        // Distinct iterations write distinct CAS objects (not idempotent).
        assert_ne!(bench_blob(64, 0), bench_blob(64, 1));
        assert_eq!(bench_blob(64, 3).len(), 64);
    }

    #[test]
    fn mode_defaults_to_full_when_no_flags() {
        // write=false, read=false, full=false → Full mode.
        let mode = match (false, false, false) {
            (true, false, false) => BenchMode::Write,
            (false, true, false) => BenchMode::Read,
            _ => BenchMode::Full,
        };
        assert_eq!(mode, BenchMode::Full);
    }
}
