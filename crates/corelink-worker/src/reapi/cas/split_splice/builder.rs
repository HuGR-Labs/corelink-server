//! Builder + clock seam for [`super::handler::SplitSpliceHandlerImpl`].
//!
//! Split from monolith `reapi/cas/split_splice.rs` (wave-33 stage
//! 2.PRE-A.2).

use core::fmt;
use std::sync::Arc;

use super::super::assembler::BlobAssembler;
use super::super::audit::AuditSink;
use super::super::chunk_store::ChunkStore;
use super::super::session::SessionStore;
use crate::Region;

/// Builder for [`super::handler::SplitSpliceHandlerImpl`].
#[allow(clippy::module_name_repetitions, missing_debug_implementations)]
pub struct SplitSpliceHandlerBuilder<S, C, B, A>
where
    S: SessionStore,
    C: ChunkStore,
    B: BlobAssembler,
    A: AuditSink,
{
    /// Region the handler is pinned to.
    pub region: Region,
    /// Multipart session row store.
    pub sessions: Arc<S>,
    /// Chunk content-addressable store.
    pub chunks: Arc<C>,
    /// Manifest builder + verifier (delegate WI-S05-005 in production).
    pub assembler: Arc<B>,
    /// Audit sink.
    pub audit: Arc<A>,
    /// Wall-clock seam for `created_at_ms` capture.
    pub clock: Arc<dyn Clock>,
}

/// Wall-clock seam — production wires [`SystemClock`]; tests pin a
/// fixed instant via [`FakeClock`].
pub trait Clock: Send + Sync + fmt::Debug {
    /// Current Unix epoch ms.
    fn now_ms(&self) -> u64;
}

/// `SystemTime`-backed clock.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }
}

/// Test-only deterministic clock.
#[derive(Debug)]
pub struct FakeClock {
    now: std::sync::atomic::AtomicU64,
}

impl FakeClock {
    /// Construct a fake clock pinned at `start_ms`.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            now: std::sync::atomic::AtomicU64::new(start_ms),
        }
    }

    /// Move the clock forward by `delta_ms`.
    pub fn advance_ms(&self, delta_ms: u64) {
        self.now
            .fetch_add(delta_ms, std::sync::atomic::Ordering::AcqRel);
    }

    /// Pin the clock to an absolute value.
    pub fn set_ms(&self, abs_ms: u64) {
        self.now
            .store(abs_ms, std::sync::atomic::Ordering::Release);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now.load(std::sync::atomic::Ordering::Acquire)
    }
}
