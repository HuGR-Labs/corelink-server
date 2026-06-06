//! Builder + `Clock` seam + handler struct definition.
//!
//! Split from monolith `reapi/ac/handler.rs` (wave-33 stage 2.PRE-A.4).

use core::fmt;
use std::sync::Arc;

use super::super::audit::AuditSink;
use super::super::merkle::MerkleVerifier;
use super::super::meta::AcMetaStore;
use super::super::neg_cache::AcNegCache;
use super::super::outputs::OutputsCheck;
use super::super::sig::Signer;
use super::super::types::ActionResult;
use super::envelope_store::AcEnvelopeStore;
use crate::cache::kv::KvBackend;
use crate::Region;

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
        self.now.store(abs_ms, std::sync::atomic::Ordering::Release);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Builder argument bundle for `ActionCacheHandlerImpl::new`.
///
/// 11 dependencies stay outside `clippy::too_many_arguments` by
/// landing through the `Builder` shape.
#[allow(clippy::module_name_repetitions, missing_debug_implementations)]
pub struct ActionCacheHandlerBuilder<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    /// Region the handler is pinned to.
    pub region: Region,
    /// `ac_meta` row store.
    pub meta: Arc<M>,
    /// AC envelope persistence.
    pub envelope_store: Arc<E>,
    /// Merkle verifier (WI-S04-003 trait).
    pub merkle: Arc<V>,
    /// HKDF signer (WI-S04-004 trait).
    pub signer: Arc<S>,
    /// Outputs aliveness check (delegates to `MetaStore` in production).
    pub outputs: Arc<O>,
    /// Audit sink.
    pub audit: Arc<A>,
    /// AC-flavor negative cache.
    pub neg_cache: Arc<AcNegCache<K>>,
    /// Sig key id (`>= 1` per ADR-0021 §P0-R5-001).
    pub sig_key_id: u32,
    /// Path-derivation TDK version.
    pub path_key_id: u32,
    /// TTL extension (ms) applied on UPSERT + GET refresh-on-hit. Per
    /// S-07 / ADR-0019; for S-04 we accept an injected value so the
    /// handler property tests pin the budget explicitly.
    pub ttl_extend_ms: u64,
    /// Clock seam.
    pub clock: Arc<dyn Clock>,
}

/// Canonical pure-logic AC handler.
///
/// Holds `Arc`-shared dependencies so a single handler instance can
/// be cloned across spawned gRPC + REST tasks. All state-bearing
/// dependencies are interior-mutable — the meta store, envelope store,
/// and audit sink each hold their own locks; the handler itself has
/// no per-request state.
pub struct ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    pub(super) region: Region,
    pub(super) meta: Arc<M>,
    pub(super) envelope_store: Arc<E>,
    pub(super) merkle: Arc<V>,
    pub(super) signer: Arc<S>,
    pub(super) outputs: Arc<O>,
    pub(super) audit: Arc<A>,
    pub(super) neg_cache: Arc<AcNegCache<K>>,
    pub(super) sig_key_id: u32,
    pub(super) path_key_id: u32,
    pub(super) ttl_extend_ms: u64,
    pub(super) clock: Arc<dyn Clock>,
    /// Per-instance sibling store for the [`ActionResult`] proto bytes
    /// (production routes the proto through the envelope JSON; the
    /// in-memory fake keeps a parallel map keyed by the canonical
    /// envelope path). Scoped to the handler instance so concurrent
    /// in-process tests cannot pollute each other's view (closes
    /// F-001 audit finding 2026-05-01: prior process-global
    /// `ACTION_RESULT_STASH` static caused parallel test flakes when
    /// rejected `UpdateActionResult` proto bytes leaked across
    /// instances).
    pub(super) action_result_stash:
        Arc<tokio::sync::Mutex<std::collections::HashMap<String, ActionResult>>>,
}

impl<M, E, V, S, O, A, K> fmt::Debug for ActionCacheHandlerImpl<M, E, V, S, O, A, K>
where
    M: AcMetaStore,
    E: AcEnvelopeStore,
    V: MerkleVerifier,
    S: Signer,
    O: OutputsCheck,
    A: AuditSink,
    K: KvBackend + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ActionCacheHandlerImpl")
            .field("region", &self.region)
            .field("sig_key_id", &self.sig_key_id)
            .field("path_key_id", &self.path_key_id)
            .field("ttl_extend_ms", &self.ttl_extend_ms)
            .finish_non_exhaustive()
    }
}
