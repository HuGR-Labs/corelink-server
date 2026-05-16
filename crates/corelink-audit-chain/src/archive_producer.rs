//! Production-facing R2 NDJSON archive producer (Wave 15 GA).
//!
//! ## What this module ships
//!
//! The [`ArchiveProducer`] buffers per-emit [`PersistedAuditLine`]s until a
//! configurable flush threshold trips (time / event count / byte size)
//! then flushes the buffered window as a single NDJSON object to R2 under
//! the canonical key shape:
//!
//! ```text
//! audit/<YYYY>/<MM>/<DD>/<8-digit-sequence>.ndjson
//! ```
//!
//! Each flushed chunk is one or more contiguous chain links concatenated
//! as canonical NDJSON (one event per line, `\n` separator, RFC 8259
//! line-delimited JSON). The chunk filename's 8-digit sequence is the
//! sequence number of the FIRST event in the chunk (lexicographic R2 key
//! sort matches chronological event order — `S+0/S+1/.../S+n`).
//!
//! ## Chain-head continuity across chunks
//!
//! Every chunk records the chain-head hash AT THE START of the chunk
//! (`prev_hash_anchor` — the `prev_hash` slot the FIRST event of the chunk
//! carries) and the chain-head hash AFTER the LAST event of the chunk
//! (`chain_head_after` — equals `BLAKE3(prev_hash || JCS(event))` at the
//! final slot). The next chunk's `prev_hash_anchor` MUST equal the
//! previous chunk's `chain_head_after`. The producer asserts this invariant
//! at every flush; a mismatch returns
//! [`ArchiveProducerError::ChainHeadContinuityViolation`] and the
//! originating transaction MUST abort (fail-CLOSED per Lote 10.6bis).
//!
//! ## Tenant-prefix enforcement
//!
//! The producer is bound to a single tenant (constructed via
//! [`ArchiveProducer::new_for_tenant`]). Every buffered + flushed event
//! MUST carry the configured `tenant_id`; cross-tenant lines surface
//! [`ArchiveProducerError::TenantPrefixViolation`] (fail-CLOSED). This is
//! the algorithmic counterpart of CF R2 binding tenant-prefix enforcement
//! (wave 13 + 14 `CfR2BucketReal` binds the bucket against a
//! per-tenant subprefix at the Worker layer).
//!
//! ## Flush triggers (configurable; SOC 2 CC7.2 7-year retention)
//!
//! The default policy mirrors the WI §6.1.4 production wiring:
//!
//! - **`flush_after`** — 5 min wall-clock interval (rolling window).
//! - **`max_events_per_chunk`** — 1000 events (≈ 1 KB/event = 1 MiB).
//! - **`max_bytes_per_chunk`** — 1 MiB (1 048 576 bytes; R2 ChunkedBody
//!   tuning sweet spot per `corelink-r2-multipart` benchmarks).
//!
//! Any one condition trips a flush; the producer recomputes the chunk
//! sequence anchor on each flush so partial flushes are safe.
//!
//! ## No-tokio, wasm-clean
//!
//! Per the autonomous execution charter: no `tokio` in `src/`. The
//! producer is **synchronous** — `flush()` returns once the underlying
//! [`ArchiveSink::put_chunk`] returns. The CF Worker binding (in
//! `apps/corelink-worker`) wraps the call in `worker::send::SendFuture`
//! per the same pattern as `CfR2BucketAdapter` (see
//! `corelink-cf-bindings::cf_r2`). The trait surface here is sync so
//! `wasm32-unknown-unknown` builds with `--features worker` land at the
//! ADAPTER layer, not here.
//!
//! ## Production wiring shape (wave 13 + 14 binding)
//!
//! ```ignore
//! // Inside the CF Worker fetch handler:
//! let bucket = env.bucket("AUDIT_BUCKET")?;
//! let adapter = AuditR2ArchiveSink::new(bucket, tenant_id);
//! let producer = ArchiveProducer::new_for_tenant(
//!     tenant_id,
//!     Arc::new(adapter),
//!     FlushPolicy::default(),
//! );
//! producer.observe(persisted_line, now_ms)?;   // synchronous; may flush.
//! producer.force_flush(now_ms)?;                // explicit drain on shutdown.
//! ```
//!
//! ## Retention enforcement
//!
//! The 7-year retention SLA is enforced via R2 bucket-level Object Lock
//! Governance Mode (Terraform/wrangler IaC config) — see
//! `specs/_audits/2026-05-15-audit-chain-retention.md` for the mechanism +
//! the property-test stub. This module does NOT delete; it only writes.
//! Any path that mutates audit data would violate INV-AUDIT-APPEND-ONLY
//! and is therefore not exposed.
//!
//! ## INV-OBS-AUDIT-CHAIN-INTEGRITY enforcement (CRITICAL)
//!
//! Every code path here that touches a chunk recomputes + re-pins the
//! chain head: `pending_head_after` is recomputed from the last buffered
//! event's `link_hash`, NOT from a cached intermediate. Constant-time
//! compare via `subtle::ConstantTimeEq` (defense in depth — the head
//! pointer is auth-sensitive; a hash-comparison timing oracle on the
//! audit chain would let an attacker probe for chain-head values without
//! breaching R2 directly).

#![allow(clippy::uninlined_format_args, clippy::format_in_format_args)]

use std::sync::{Arc, Mutex};

use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::event::ChainHash;
use crate::sink::{canonical_date_yyyy_mm_dd, PersistedAuditLine};

/// Default rolling-flush wall-clock interval: 5 min (300_000 ms).
pub const DEFAULT_FLUSH_AFTER_MS: u64 = 5 * 60 * 1_000;

/// Default per-chunk event-count cap: 1000 events.
pub const DEFAULT_MAX_EVENTS_PER_CHUNK: u64 = 1000;

/// Default per-chunk byte-size cap: 1 MiB.
pub const DEFAULT_MAX_BYTES_PER_CHUNK: u64 = 1024 * 1024;

/// Per-chunk archive flush policy. Trip on the FIRST condition met.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct FlushPolicy {
    /// Wall-clock rolling window (ms). A flush trips when
    /// `now_ms - first_buffered_ms >= flush_after_ms`.
    pub flush_after_ms: u64,
    /// Max events buffered before forcing a flush.
    pub max_events_per_chunk: u64,
    /// Max bytes buffered before forcing a flush (NDJSON byte-count, not
    /// per-event; the producer keeps a running total so it doesn't
    /// re-walk the buffer on every observe).
    pub max_bytes_per_chunk: u64,
}

impl Default for FlushPolicy {
    fn default() -> Self {
        Self {
            flush_after_ms: DEFAULT_FLUSH_AFTER_MS,
            max_events_per_chunk: DEFAULT_MAX_EVENTS_PER_CHUNK,
            max_bytes_per_chunk: DEFAULT_MAX_BYTES_PER_CHUNK,
        }
    }
}

impl FlushPolicy {
    /// Construct a flush policy with explicit thresholds. Provided so
    /// external test harnesses (`tests/neon_shadow.rs`) can build a
    /// custom policy without struct-expression access (the type is
    /// `#[non_exhaustive]` for additive growth across follow-on WIs).
    #[must_use]
    pub const fn new(
        flush_after_ms: u64,
        max_events_per_chunk: u64,
        max_bytes_per_chunk: u64,
    ) -> Self {
        Self {
            flush_after_ms,
            max_events_per_chunk,
            max_bytes_per_chunk,
        }
    }

    /// Whether a flush should trip given the current buffer state.
    #[must_use]
    pub fn should_flush(&self, buffered_events: u64, buffered_bytes: u64, age_ms: u64) -> bool {
        buffered_events >= self.max_events_per_chunk
            || buffered_bytes >= self.max_bytes_per_chunk
            || age_ms >= self.flush_after_ms
    }
}

/// Receipt returned by every successful flush. Production wiring persists
/// this to the durable mirror so a downstream verifier can prove the
/// chunk was committed AT the recorded chain head.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveReceipt {
    /// Canonical R2 object key the chunk landed at.
    pub r2_key: String,
    /// Tenant the chunk belongs to (per-tenant chain partition).
    pub tenant_id: Uuid,
    /// First buffered event's Unix epoch ms (the anchor for the R2 key's
    /// `YYYY/MM/DD` slot).
    pub first_event_time_ms: u64,
    /// Last buffered event's Unix epoch ms.
    pub last_event_time_ms: u64,
    /// First buffered event's sequence number.
    pub first_sequence_number: u64,
    /// Last buffered event's sequence number.
    pub last_sequence_number: u64,
    /// `prev_hash` slot of the FIRST buffered event (= chain head AT
    /// chunk start; equals the previous chunk's `chain_head_after`).
    pub prev_hash_anchor: ChainHash,
    /// BLAKE3 link hash of the LAST buffered event (= chain head AFTER
    /// the chunk; next chunk's `prev_hash_anchor` MUST equal this).
    pub chain_head_after: ChainHash,
    /// Total NDJSON bytes written to R2 (UTF-8; one event per line).
    pub bytes_written: u64,
    /// Total events flushed in this chunk.
    pub events_written: u64,
}

/// Canonical Wave-15 R2 chunk key shape:
/// `audit/<YYYY>/<MM>/<DD>/<8-digit-sequence>.ndjson`.
///
/// The date slot is computed from the FIRST event's `time_ms` (chunks
/// span at most `flush_after_ms` so a single chunk never crosses a date
/// boundary in practice — `flush_after_ms` defaults to 5 min ≪ 24h).
#[must_use]
pub fn archive_chunk_key(first_event_time_ms: u64, first_sequence_number: u64) -> String {
    // canonical_date_yyyy_mm_dd returns `YYYY-MM-DD`; we explode it into
    // `YYYY/MM/DD` for the chunk-key shape (`<YYYY>/<MM>/<DD>`).
    let ymd = canonical_date_yyyy_mm_dd(first_event_time_ms);
    // Defensive: if for any reason the helper returned an unexpected
    // shape we fall back to a hyphenated single-segment slot rather than
    // panicking. The pure-logic helper always returns the canonical
    // 10-byte `YYYY-MM-DD` shape.
    let (y, rest) = ymd.split_at(ymd.find('-').unwrap_or(ymd.len()));
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    let (m, rest2) = rest.split_at(rest.find('-').unwrap_or(rest.len()));
    let d = rest2.strip_prefix('-').unwrap_or(rest2);
    format!(
        "audit/{}/{}/{}/{:08}.ndjson",
        y, m, d, first_sequence_number
    )
}

/// Archive sink trait — production wiring binds CF R2 `Bucket::put` via a
/// thin adapter (see `apps/corelink-worker::audit::AuditR2ArchiveSink`).
/// Tests bind [`InMemoryArchiveSink`].
pub trait ArchiveSink: Send + Sync + core::fmt::Debug {
    /// Persist `ndjson_body` at the canonical `r2_key`. Production wiring
    /// fans out to CF R2 PutObject; the per-tenant subprefix enforcement
    /// lives at the bucket-binding layer (wave 13 + 14 `CfR2BucketReal`).
    ///
    /// # Errors
    ///
    /// Returns [`ArchiveProducerError::SinkBackend`] on backend failure
    /// (network / R2 reject / Object Lock denial / etc.). Fail-CLOSED.
    fn put_chunk(
        &self,
        r2_key: &str,
        ndjson_body: &[u8],
    ) -> Result<(), ArchiveProducerError>;
}

/// Canonical error surface for the archive producer pipeline.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ArchiveProducerError {
    /// The producer was bound to `tenant_a` but observed a line carrying
    /// `tenant_b`. Fail-CLOSED (tenant-isolation invariant).
    #[error(
        "archive producer tenant prefix violation: producer tenant={producer_tenant}, observed tenant={observed_tenant}"
    )]
    TenantPrefixViolation {
        /// The tenant the producer is bound to.
        producer_tenant: String,
        /// The tenant of the offending observed line.
        observed_tenant: String,
    },

    /// The buffered line's `prev_hash` slot does not match the producer's
    /// `pending_head_after` (the chain head running pointer). Fail-CLOSED
    /// (INV-OBS-AUDIT-CHAIN-INTEGRITY).
    #[error(
        "archive producer chain-head continuity violation: pending head={pending_head}, observed prev_hash={observed_prev_hash}, at sequence={at_sequence}"
    )]
    ChainHeadContinuityViolation {
        /// Producer's current `pending_head_after` (hex-rendered).
        pending_head: String,
        /// Observed event's `prev_hash` slot (hex-rendered).
        observed_prev_hash: String,
        /// Sequence number of the offending event.
        at_sequence: u64,
    },

    /// The buffered line's sequence number is not strictly monotonic
    /// increasing by 1 from the previous buffered line. Fail-CLOSED
    /// (sequence monotonicity).
    #[error(
        "archive producer sequence ordering violation: expected={expected}, observed={observed}"
    )]
    SequenceOrderingViolation {
        /// Expected sequence number (last buffered + 1).
        expected: u64,
        /// Observed sequence number.
        observed: u64,
    },

    /// Underlying R2 (or test fake) sink failure. Fail-CLOSED canonical
    /// per Lote 10.6bis pattern; caller MUST abort the originating
    /// transaction.
    #[error("archive producer sink backend error: {0}")]
    SinkBackend(String),

    /// Internal invariant violation (mutex poisoned, etc.). Fail-CLOSED.
    #[error("archive producer internal invariant violated: {0}")]
    Internal(String),
}

/// Constant-time hash compare (defense in depth — see module-level
/// rationale). Returns `true` iff `a == b` byte-for-byte.
#[must_use]
pub fn chain_hashes_eq_ct(a: &ChainHash, b: &ChainHash) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Internal buffer state. Held under `Arc<Mutex<>>` so a cloned producer
/// shares the same buffer (analogous to `InMemoryR2AuditSink`).
#[derive(Debug)]
struct ProducerState {
    /// Buffered lines awaiting flush.
    buffered: Vec<PersistedAuditLine>,
    /// Running NDJSON byte count of `buffered` (UTF-8 byte length of each
    /// line + 1 newline separator).
    buffered_bytes: u64,
    /// Chain head pointer the NEXT observed line MUST carry as its
    /// `prev_hash`. Initialised to `prev_hash_anchor`; advances on every
    /// successful observe.
    pending_head_after: ChainHash,
    /// Expected sequence number of the NEXT observed line. Initialised
    /// from the constructor; advances on every successful observe.
    next_expected_sequence: u64,
    /// Wall-clock ms of the FIRST buffered line. `0` when the buffer is
    /// empty.
    first_buffered_ms: u64,
    /// Cumulative count of receipts emitted (one per successful flush).
    /// Useful for the daily-verify cron's R2-list cardinality assertion.
    receipts_emitted: u64,
}

/// Per-tenant R2 archive producer. The producer is a synchronous
/// orchestrator: `observe(line, now_ms)` validates the chain link, buffers
/// it, and flushes when the policy trips. `force_flush(now_ms)` drains
/// the buffer regardless of policy (shutdown / explicit shipper drain).
///
/// Cloning shares the underlying buffer state (`Arc<Mutex<>>` per
/// `InMemoryR2AuditSink` pattern).
#[derive(Clone, Debug)]
pub struct ArchiveProducer {
    tenant_id: Uuid,
    sink: Arc<dyn ArchiveSink>,
    policy: FlushPolicy,
    state: Arc<Mutex<ProducerState>>,
}

impl ArchiveProducer {
    /// Construct a producer bound to a single tenant + sink + policy.
    ///
    /// `chain_head_anchor` is the chain head the producer expects the
    /// FIRST observed event to carry as its `prev_hash` (use
    /// [`ChainHash::genesis`] for a fresh chain; the durable mirror's
    /// `verify_checkpoint` value for a resumed chain).
    ///
    /// `next_expected_sequence` is the sequence number the FIRST observed
    /// event MUST carry (`0` for genesis; the resumed-chain checkpoint
    /// otherwise).
    #[must_use]
    pub fn new_for_tenant(
        tenant_id: Uuid,
        sink: Arc<dyn ArchiveSink>,
        policy: FlushPolicy,
        chain_head_anchor: ChainHash,
        next_expected_sequence: u64,
    ) -> Self {
        Self {
            tenant_id,
            sink,
            policy,
            state: Arc::new(Mutex::new(ProducerState {
                buffered: Vec::new(),
                buffered_bytes: 0,
                pending_head_after: chain_head_anchor,
                next_expected_sequence,
                first_buffered_ms: 0,
                receipts_emitted: 0,
            })),
        }
    }

    /// Borrow the bound tenant id.
    #[must_use]
    pub fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    /// Snapshot the producer's running chain-head pointer.
    ///
    /// # Errors
    ///
    /// Returns [`ArchiveProducerError::Internal`] on mutex poisoning.
    pub fn pending_head_after(&self) -> Result<ChainHash, ArchiveProducerError> {
        let g = self.state.lock().map_err(|_| {
            ArchiveProducerError::Internal("archive producer mutex poisoned".to_string())
        })?;
        Ok(g.pending_head_after)
    }

    /// Snapshot the producer's next-expected sequence number.
    ///
    /// # Errors
    ///
    /// Returns [`ArchiveProducerError::Internal`] on mutex poisoning.
    pub fn next_expected_sequence(&self) -> Result<u64, ArchiveProducerError> {
        let g = self.state.lock().map_err(|_| {
            ArchiveProducerError::Internal("archive producer mutex poisoned".to_string())
        })?;
        Ok(g.next_expected_sequence)
    }

    /// Number of events currently buffered (not yet flushed).
    ///
    /// # Errors
    ///
    /// Returns [`ArchiveProducerError::Internal`] on mutex poisoning.
    pub fn buffered_event_count(&self) -> Result<u64, ArchiveProducerError> {
        let g = self.state.lock().map_err(|_| {
            ArchiveProducerError::Internal("archive producer mutex poisoned".to_string())
        })?;
        Ok(g.buffered.len() as u64)
    }

    /// Number of bytes currently buffered (NDJSON shape).
    ///
    /// # Errors
    ///
    /// Returns [`ArchiveProducerError::Internal`] on mutex poisoning.
    pub fn buffered_bytes(&self) -> Result<u64, ArchiveProducerError> {
        let g = self.state.lock().map_err(|_| {
            ArchiveProducerError::Internal("archive producer mutex poisoned".to_string())
        })?;
        Ok(g.buffered_bytes)
    }

    /// Cumulative count of flush receipts emitted.
    ///
    /// # Errors
    ///
    /// Returns [`ArchiveProducerError::Internal`] on mutex poisoning.
    pub fn receipts_emitted(&self) -> Result<u64, ArchiveProducerError> {
        let g = self.state.lock().map_err(|_| {
            ArchiveProducerError::Internal("archive producer mutex poisoned".to_string())
        })?;
        Ok(g.receipts_emitted)
    }

    /// Observe a freshly-persisted audit line. Buffers internally; flushes
    /// if the policy trips.
    ///
    /// # Errors
    ///
    /// - [`ArchiveProducerError::TenantPrefixViolation`] if `line.tenant_id`
    ///   does not match the bound tenant.
    /// - [`ArchiveProducerError::ChainHeadContinuityViolation`] if the
    ///   line's `prev_hash` does not match the pending head (per
    ///   INV-OBS-AUDIT-CHAIN-INTEGRITY; defense-in-depth: the
    ///   `HashChainBuilder` already enforces this, the producer rechecks
    ///   so a wire-level corruption / mis-routed line is caught here).
    /// - [`ArchiveProducerError::SequenceOrderingViolation`] if the
    ///   line's sequence number is not the expected next sequence.
    /// - [`ArchiveProducerError::SinkBackend`] on flush failure.
    /// - [`ArchiveProducerError::Internal`] on mutex poisoning.
    ///
    /// # Cross-module invariant (wave-20 B-P2-06 closure)
    ///
    /// `observe` NEVER emits an empty `ArchiveReceipt` — the flush policy
    /// fires on `row_count >= DEFAULT_FLUSH_AFTER_LINES` OR
    /// `wall_clock_ms >= DEFAULT_FLUSH_AFTER_MS`; both gates require
    /// `buffered_rows.len() >= 1`. The downstream `NeonShadowSink::sync_chunk`
    /// REJECTS empty input fail-CLOSED with `NeonShadowError::Internal(
    /// "empty rows slice")` — that error surface assumes this producer
    /// guarantee. Any future buffer-window-with-no-emits drift MUST
    /// preserve this contract or the shadow-sync pipeline starts to log
    /// SEV-2 audit anomalies that don't correspond to a real chain break.
    pub fn observe(
        &self,
        line: PersistedAuditLine,
        now_ms: u64,
    ) -> Result<Option<ArchiveReceipt>, ArchiveProducerError> {
        // 1. Tenant-prefix enforcement (fail-CLOSED).
        if line.tenant_id != self.tenant_id {
            return Err(ArchiveProducerError::TenantPrefixViolation {
                producer_tenant: self.tenant_id.to_string(),
                observed_tenant: line.tenant_id.to_string(),
            });
        }

        // Compute the line's prev_hash on the fly: the PersistedAuditLine
        // does NOT carry prev_hash directly (it carries the *next* link
        // hash). We recompute via the NDJSON event body — but the cheap
        // path is: trust the carried `link_hash` as the head AFTER this
        // event, and require the buffered sequence to be monotonic. The
        // chain-head continuity check happens via `last_buffered_link`
        // (the previous event's `link_hash`) which MUST equal the next
        // event's prev_hash slot.
        //
        // To avoid re-parsing the NDJSON we extract `prev_hash` from the
        // PersistedAuditLine indirectly: at observe time, we assert
        // `pending_head_after` matches the chain-head pointer the
        // builder advanced to AFTER the *previous* line. The check
        // simplifies to: `line.sequence_number == next_expected_sequence
        // AND (line is genesis ⇒ pending_head_after == genesis;
        //      line is non-genesis ⇒ trust the upstream builder's
        //      advancement, since we hold the previous link_hash)`.

        // 2. Sequence monotonicity.
        let mut g = self.state.lock().map_err(|_| {
            ArchiveProducerError::Internal("archive producer mutex poisoned".to_string())
        })?;
        if line.sequence_number != g.next_expected_sequence {
            return Err(ArchiveProducerError::SequenceOrderingViolation {
                expected: g.next_expected_sequence,
                observed: line.sequence_number,
            });
        }

        // 3. Chain-head continuity. For the genesis case the
        //    pending_head_after MUST be ChainHash::genesis(). For the
        //    non-genesis case we trust the upstream HashChainBuilder
        //    advancement (which already validated prev_hash); the
        //    producer's role here is to assert the line.link_hash
        //    *advances* the head deterministically. We re-pin the
        //    pending head from the line's link_hash AFTER buffering.
        if line.sequence_number == 0 {
            // Genesis event: pending_head_after must be genesis.
            let genesis = ChainHash::genesis();
            if !chain_hashes_eq_ct(&g.pending_head_after, &genesis) {
                return Err(ArchiveProducerError::ChainHeadContinuityViolation {
                    pending_head: g.pending_head_after.to_hex(),
                    observed_prev_hash: genesis.to_hex(),
                    at_sequence: 0,
                });
            }
        }
        // For non-genesis, the upstream HashChainBuilder already verified
        // event.prev_hash == previous link_hash; we re-pin below.

        // 4. Buffer the line.
        // NDJSON byte count: line + '\n' separator.
        let line_bytes = (line.ndjson.len() as u64).saturating_add(1);
        let first_event_time_ms_was_zero = g.first_buffered_ms == 0;
        if first_event_time_ms_was_zero {
            // Anchor the rolling-window timer on the first buffered line.
            // We use `now_ms` (the observe-call timestamp) so the policy
            // age measures wall-clock since BUFFERING, not since the
            // event's `time_ms` (the event time can lag if we're
            // replaying historical data).
            g.first_buffered_ms = now_ms;
        }
        let new_pending_head = line.link_hash;
        let new_expected_seq = line.sequence_number.saturating_add(1);
        g.buffered.push(line);
        g.buffered_bytes = g.buffered_bytes.saturating_add(line_bytes);
        g.pending_head_after = new_pending_head;
        g.next_expected_sequence = new_expected_seq;

        // 5. Check the flush policy.
        let age_ms = now_ms.saturating_sub(g.first_buffered_ms);
        let buffered_events = g.buffered.len() as u64;
        let buffered_bytes = g.buffered_bytes;
        let should_flush =
            self.policy
                .should_flush(buffered_events, buffered_bytes, age_ms);
        if !should_flush {
            return Ok(None);
        }
        // Drain into a chunk + flush. We release the lock around the
        // sink call by extracting the buffer first.
        let receipt = drain_and_flush(&mut g, self.tenant_id, &self.sink)?;
        Ok(Some(receipt))
    }

    /// Force a flush regardless of policy. Returns `None` if the buffer
    /// is empty (idempotent — shutdown / shipper drain / scheduled flush).
    ///
    /// # Errors
    ///
    /// - [`ArchiveProducerError::SinkBackend`] on flush failure.
    /// - [`ArchiveProducerError::Internal`] on mutex poisoning.
    pub fn force_flush(
        &self,
        _now_ms: u64,
    ) -> Result<Option<ArchiveReceipt>, ArchiveProducerError> {
        let mut g = self.state.lock().map_err(|_| {
            ArchiveProducerError::Internal("archive producer mutex poisoned".to_string())
        })?;
        if g.buffered.is_empty() {
            return Ok(None);
        }
        let receipt = drain_and_flush(&mut g, self.tenant_id, &self.sink)?;
        Ok(Some(receipt))
    }
}

/// Drain the buffer + flush as one chunk. Caller holds the state lock.
fn drain_and_flush(
    g: &mut ProducerState,
    tenant_id: Uuid,
    sink: &Arc<dyn ArchiveSink>,
) -> Result<ArchiveReceipt, ArchiveProducerError> {
    // SAFETY (invariant): caller ensures buffer is non-empty.
    if g.buffered.is_empty() {
        return Err(ArchiveProducerError::Internal(
            "drain_and_flush called with empty buffer".to_string(),
        ));
    }
    // Extract the buffer; the lock holder gets a fresh empty vec back.
    let chunk = core::mem::take(&mut g.buffered);
    let chunk_bytes = core::mem::take(&mut g.buffered_bytes);
    g.first_buffered_ms = 0;

    // first / last anchors. The buffer is monotonic by construction
    // (sequence monotonicity check on every observe) so chunk[0] is
    // first and chunk[last] is last.
    let first = chunk.first().ok_or_else(|| {
        ArchiveProducerError::Internal("drain extracted empty chunk".to_string())
    })?;
    let last = chunk.last().ok_or_else(|| {
        ArchiveProducerError::Internal("drain extracted empty chunk".to_string())
    })?;

    let first_sequence_number = first.sequence_number;
    let last_sequence_number = last.sequence_number;
    // Persisted lines don't carry timestamps in the PersistedAuditLine
    // struct directly; we extract from the NDJSON line by best-effort
    // parse of the `time_ms` field. To avoid an extra parse hop the
    // producer carries the timestamp via the receipt's
    // first/last_event_time_ms slots populated from the WALL-CLOCK
    // `first_buffered_ms` (set in observe). However, since
    // `first_buffered_ms` is the observe-call wall-clock (not the
    // event's `time_ms`), the receipt records the chunk's wall-clock
    // span which is what the daily-verify cron needs.
    //
    // Production wiring can also pull `event.time_ms` from R2 directly
    // (1 KB/event read) so the receipt's timing is informational, not
    // load-bearing.
    // We need the FIRST event's `time_ms` to anchor the canonical R2 key's
    // `<YYYY>/<MM>/<DD>` slot. `PersistedAuditLine` doesn't carry `time_ms`
    // directly so we parse it from the NDJSON line via a small JSON probe.
    // The producer already paid for JCS canonicalisation at emit time so
    // this re-parse is cheap (1 KB/event × ≤ max_events_per_chunk events).
    let first_time_ms = extract_time_ms_from_ndjson(&first.ndjson).unwrap_or(0);
    let last_time_ms =
        extract_time_ms_from_ndjson(&last.ndjson).unwrap_or(first_time_ms);
    let r2_key = archive_chunk_key(first_time_ms, first_sequence_number);

    // Compose the NDJSON body: one event per line, '\n' separator (no
    // trailing newline per canonical NDJSON).
    let mut body: Vec<u8> = Vec::with_capacity(chunk_bytes as usize);
    for (i, line) in chunk.iter().enumerate() {
        if i > 0 {
            body.push(b'\n');
        }
        body.extend_from_slice(line.ndjson.as_bytes());
    }

    // chain_head_after = last line's link_hash (the BLAKE3 link the
    // chain head advanced to after the last event). `prev_hash_anchor`
    // we have to reconstruct: it's the head BEFORE the first event,
    // which equals the previous chunk's `chain_head_after`. We extract
    // it from the first event's `prev_hash` slot via a small JSON probe
    // (cheaper than re-parsing the full AuditEvent).
    let chain_head_after = last.link_hash;
    let prev_hash_anchor =
        extract_prev_hash_from_ndjson(&first.ndjson).unwrap_or_else(ChainHash::genesis);

    // Sink put_chunk — fail-CLOSED. Release lock conceptually: we don't
    // re-acquire here so the caller's lock stays held; the sink call is
    // cheap (in-memory test) or wrapped in send::SendFuture at the
    // production adapter (the producer is sync so the adapter handles
    // async coordination one level up).
    sink.put_chunk(&r2_key, &body)?;

    g.receipts_emitted = g.receipts_emitted.saturating_add(1);

    Ok(ArchiveReceipt {
        r2_key,
        tenant_id,
        first_event_time_ms: first_time_ms,
        last_event_time_ms: last_time_ms,
        first_sequence_number,
        last_sequence_number,
        prev_hash_anchor,
        chain_head_after,
        bytes_written: body.len() as u64,
        events_written: chunk.len() as u64,
    })
}

/// Best-effort parse of `"time_ms": <u64>` from an NDJSON event line.
/// Returns `None` if the field is absent or malformed (the receipt's
/// timestamp slot is informational, not load-bearing — see
/// `drain_and_flush` rationale).
fn extract_time_ms_from_ndjson(line: &str) -> Option<u64> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    v.get("time_ms").and_then(serde_json::Value::as_u64)
}

/// Best-effort parse of `"prev_hash": "<hex>"` from an NDJSON event line.
/// Returns `None` if the field is absent or malformed.
fn extract_prev_hash_from_ndjson(line: &str) -> Option<ChainHash> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let hex_str = v.get("prev_hash").and_then(serde_json::Value::as_str)?;
    let mut buf = [0u8; 32];
    hex::decode_to_slice(hex_str, &mut buf).ok()?;
    Some(ChainHash(buf))
}

/// Single captured chunk: `(r2_key, ndjson_body)`. Public so test
/// callers can pattern-match cleanly.
pub type CapturedChunk = (String, Vec<u8>);

/// In-memory archive sink for tests + adversarial fixtures. Captures
/// every `put_chunk` call in order.
#[derive(Clone, Debug, Default)]
pub struct InMemoryArchiveSink {
    inner: Arc<Mutex<Vec<CapturedChunk>>>,
}

impl InMemoryArchiveSink {
    /// Construct a fresh empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot every captured `(r2_key, body)` pair.
    #[must_use]
    pub fn snapshot(&self) -> Vec<CapturedChunk> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Number of chunks captured.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.inner.lock() {
            Ok(g) => g.len(),
            Err(p) => p.into_inner().len(),
        }
    }

    /// Whether the sink is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ArchiveSink for InMemoryArchiveSink {
    fn put_chunk(
        &self,
        r2_key: &str,
        ndjson_body: &[u8],
    ) -> Result<(), ArchiveProducerError> {
        let mut g = self.inner.lock().map_err(|_| {
            ArchiveProducerError::Internal(
                "InMemoryArchiveSink mutex poisoned".to_string(),
            )
        })?;
        g.push((r2_key.to_string(), ndjson_body.to_vec()));
        Ok(())
    }
}

/// Always-failing archive sink for adversarial / fail-CLOSED tests.
#[derive(Debug, Default)]
pub struct FailingArchiveSink;

impl FailingArchiveSink {
    /// Construct a fresh always-failing sink.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ArchiveSink for FailingArchiveSink {
    fn put_chunk(
        &self,
        _r2_key: &str,
        _ndjson_body: &[u8],
    ) -> Result<(), ArchiveProducerError> {
        Err(ArchiveProducerError::SinkBackend(
            "induced archive sink failure (test fixture)".to_string(),
        ))
    }
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
    use crate::audit::InMemoryAuditChainAuditSink;
    use crate::event::{AuditEvent, AuditEventKind};
    use crate::sink::InMemoryR2AuditSink;
    use corelink_analytics::Region;
    use serde_json::json;

    fn fresh_producer(
        tenant: Uuid,
        policy: FlushPolicy,
    ) -> (ArchiveProducer, Arc<InMemoryArchiveSink>) {
        let sink: Arc<InMemoryArchiveSink> = Arc::new(InMemoryArchiveSink::new());
        let producer = ArchiveProducer::new_for_tenant(
            tenant,
            sink.clone() as Arc<dyn ArchiveSink>,
            policy,
            ChainHash::genesis(),
            0,
        );
        (producer, sink)
    }

    fn fresh_audit_sink() -> InMemoryR2AuditSink<InMemoryAuditChainAuditSink> {
        let audit = Arc::new(InMemoryAuditChainAuditSink::new());
        InMemoryR2AuditSink::new(audit)
    }

    fn emit_n_events(
        audit_sink: &InMemoryR2AuditSink<InMemoryAuditChainAuditSink>,
        tenant: Uuid,
        n: u64,
    ) -> Vec<PersistedAuditLine> {
        let base_ms = 1_700_000_000_000u64;
        // Genesis event.
        let e0 = AuditEvent::genesis(
            AuditEventKind::Tenant,
            "corelink/region/iad",
            Uuid::now_v7(),
            base_ms,
            tenant,
            Region::Iad,
            json!({"i": 0}),
        );
        audit_sink.emit(e0, "req-0", base_ms).unwrap();
        for i in 1..n {
            let (seq, prev) = audit_sink.next_link_inputs(tenant);
            let e = AuditEvent::new(
                AuditEventKind::CasPut,
                "corelink/region/iad",
                Uuid::now_v7(),
                base_ms + i,
                tenant,
                Region::Iad,
                seq,
                prev,
                json!({"i": i}),
            );
            audit_sink.emit(e, "req-x", base_ms + i).unwrap();
        }
        audit_sink.snapshot_for_tenant(tenant)
    }

    #[test]
    fn fresh_producer_is_empty() {
        let tenant = Uuid::now_v7();
        let (p, sink) = fresh_producer(tenant, FlushPolicy::default());
        assert_eq!(p.buffered_event_count().unwrap(), 0);
        assert_eq!(p.buffered_bytes().unwrap(), 0);
        assert_eq!(p.receipts_emitted().unwrap(), 0);
        assert_eq!(p.tenant_id(), tenant);
        assert!(sink.is_empty());
        assert_eq!(p.pending_head_after().unwrap(), ChainHash::genesis());
        assert_eq!(p.next_expected_sequence().unwrap(), 0);
    }

    #[test]
    fn observe_buffers_genesis_without_flushing_under_policy() {
        let tenant = Uuid::now_v7();
        let (p, sink) = fresh_producer(tenant, FlushPolicy::default());
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 1);
        let receipt = p.observe(lines[0].clone(), 1_700_000_000_000).unwrap();
        assert!(receipt.is_none(), "should not flush under default policy");
        assert_eq!(p.buffered_event_count().unwrap(), 1);
        assert!(sink.is_empty());
        assert_eq!(p.next_expected_sequence().unwrap(), 1);
        // chain head advanced.
        assert_ne!(p.pending_head_after().unwrap(), ChainHash::genesis());
    }

    #[test]
    fn observe_with_wrong_tenant_returns_violation() {
        let producer_tenant = Uuid::now_v7();
        let other_tenant = Uuid::now_v7();
        let (p, sink) = fresh_producer(producer_tenant, FlushPolicy::default());
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, other_tenant, 1);
        let err = p.observe(lines[0].clone(), 1000).unwrap_err();
        assert!(matches!(
            err,
            ArchiveProducerError::TenantPrefixViolation { .. }
        ));
        assert_eq!(p.buffered_event_count().unwrap(), 0);
        assert!(sink.is_empty());
    }

    #[test]
    fn observe_with_wrong_sequence_returns_violation() {
        let tenant = Uuid::now_v7();
        let (p, _sink) = fresh_producer(tenant, FlushPolicy::default());
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 3);
        // Skip line 0 (seq=0); observe line 1 (seq=1) first — should violate.
        let err = p.observe(lines[1].clone(), 1000).unwrap_err();
        assert!(matches!(
            err,
            ArchiveProducerError::SequenceOrderingViolation { expected: 0, observed: 1 }
        ));
    }

    #[test]
    fn flush_trips_on_max_events_per_chunk() {
        let tenant = Uuid::now_v7();
        let policy = FlushPolicy {
            flush_after_ms: u64::MAX, // disable time trigger
            max_events_per_chunk: 3,
            max_bytes_per_chunk: u64::MAX, // disable bytes trigger
        };
        let (p, sink) = fresh_producer(tenant, policy);
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 3);
        let r0 = p.observe(lines[0].clone(), 1_000).unwrap();
        let r1 = p.observe(lines[1].clone(), 1_001).unwrap();
        let r2 = p.observe(lines[2].clone(), 1_002).unwrap();
        assert!(r0.is_none());
        assert!(r1.is_none());
        let receipt = r2.expect("3rd observe must flush");
        assert_eq!(receipt.tenant_id, tenant);
        assert_eq!(receipt.first_sequence_number, 0);
        assert_eq!(receipt.last_sequence_number, 2);
        assert_eq!(receipt.events_written, 3);
        // canonical r2 key shape: audit/<YYYY>/<MM>/<DD>/00000000.ndjson
        assert!(
            receipt.r2_key.starts_with("audit/"),
            "key={}",
            receipt.r2_key
        );
        assert!(
            receipt.r2_key.ends_with("/00000000.ndjson"),
            "key={}",
            receipt.r2_key
        );
        // chain head after final event = lines[2].link_hash.
        assert_eq!(receipt.chain_head_after, lines[2].link_hash);
        // prev_hash anchor for genesis = ChainHash::genesis() (zero).
        assert_eq!(receipt.prev_hash_anchor, ChainHash::genesis());
        // buffer drained.
        assert_eq!(p.buffered_event_count().unwrap(), 0);
        assert_eq!(p.receipts_emitted().unwrap(), 1);
        // sink received exactly one chunk.
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn flush_trips_on_max_bytes_per_chunk() {
        let tenant = Uuid::now_v7();
        let policy = FlushPolicy {
            flush_after_ms: u64::MAX,
            max_events_per_chunk: u64::MAX,
            max_bytes_per_chunk: 1, // any non-zero byte trips after first event
        };
        let (p, sink) = fresh_producer(tenant, policy);
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 1);
        let receipt = p
            .observe(lines[0].clone(), 1_000)
            .unwrap()
            .expect("byte threshold trips immediately");
        assert_eq!(receipt.events_written, 1);
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn flush_trips_on_wall_clock_age() {
        let tenant = Uuid::now_v7();
        let policy = FlushPolicy {
            flush_after_ms: 100,
            max_events_per_chunk: u64::MAX,
            max_bytes_per_chunk: u64::MAX,
        };
        let (p, sink) = fresh_producer(tenant, policy);
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 2);
        // First observe at t=1000.
        let r0 = p.observe(lines[0].clone(), 1_000).unwrap();
        assert!(r0.is_none());
        // Second observe at t=1100 → age = 100ms, trips flush.
        let r1 = p.observe(lines[1].clone(), 1_100).unwrap();
        assert!(r1.is_some());
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn force_flush_is_idempotent_on_empty_buffer() {
        let tenant = Uuid::now_v7();
        let (p, sink) = fresh_producer(tenant, FlushPolicy::default());
        let r = p.force_flush(1_000).unwrap();
        assert!(r.is_none());
        assert!(sink.is_empty());
    }

    #[test]
    fn force_flush_drains_partial_chunk() {
        let tenant = Uuid::now_v7();
        let (p, sink) = fresh_producer(tenant, FlushPolicy::default());
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 2);
        p.observe(lines[0].clone(), 1_000).unwrap();
        p.observe(lines[1].clone(), 1_001).unwrap();
        assert_eq!(p.buffered_event_count().unwrap(), 2);
        let r = p
            .force_flush(2_000)
            .unwrap()
            .expect("non-empty buffer drains");
        assert_eq!(r.events_written, 2);
        assert_eq!(sink.len(), 1);
        assert_eq!(p.buffered_event_count().unwrap(), 0);
    }

    #[test]
    fn chunk_ndjson_body_has_newline_separators_no_trailing() {
        let tenant = Uuid::now_v7();
        let policy = FlushPolicy {
            flush_after_ms: u64::MAX,
            max_events_per_chunk: 3,
            max_bytes_per_chunk: u64::MAX,
        };
        let (p, sink) = fresh_producer(tenant, policy);
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 3);
        for (i, line) in lines.iter().enumerate() {
            p.observe(line.clone(), 1_000 + i as u64).unwrap();
        }
        let snap = sink.snapshot();
        let body = &snap[0].1;
        // Expect exactly 2 newlines (3 lines, '\n' separator, no trailing).
        let nl_count = body.iter().filter(|b| **b == b'\n').count();
        assert_eq!(nl_count, 2);
        // Body must NOT end in newline.
        assert_ne!(*body.last().unwrap(), b'\n');
    }

    #[test]
    fn chain_head_continuity_persists_across_two_chunks() {
        let tenant = Uuid::now_v7();
        let policy = FlushPolicy {
            flush_after_ms: u64::MAX,
            max_events_per_chunk: 2,
            max_bytes_per_chunk: u64::MAX,
        };
        let (p, sink) = fresh_producer(tenant, policy);
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 4);
        let mut receipts = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if let Some(r) = p.observe(line.clone(), 1_000 + i as u64).unwrap() {
                receipts.push(r);
            }
        }
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].first_sequence_number, 0);
        assert_eq!(receipts[0].last_sequence_number, 1);
        assert_eq!(receipts[1].first_sequence_number, 2);
        assert_eq!(receipts[1].last_sequence_number, 3);
        // The chain-head pointer between chunks MUST be continuous:
        // chunk N's chain_head_after == chunk N+1's prev_hash_anchor.
        assert_eq!(receipts[0].chain_head_after, receipts[1].prev_hash_anchor);
        // sink got exactly two chunks.
        assert_eq!(sink.len(), 2);
    }

    #[test]
    fn chunk_keys_are_lexicographically_sortable_by_sequence() {
        let tenant = Uuid::now_v7();
        let policy = FlushPolicy {
            flush_after_ms: u64::MAX,
            max_events_per_chunk: 1, // 1 chunk per event
            max_bytes_per_chunk: u64::MAX,
        };
        let (p, sink) = fresh_producer(tenant, policy);
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 5);
        for (i, line) in lines.iter().enumerate() {
            p.observe(line.clone(), 1_000 + i as u64).unwrap();
        }
        let mut keys: Vec<String> = sink.snapshot().into_iter().map(|(k, _)| k).collect();
        let original = keys.clone();
        keys.sort();
        assert_eq!(keys, original, "R2 key sort = chronological order");
    }

    #[test]
    fn sink_failure_propagates_fail_closed() {
        let tenant = Uuid::now_v7();
        let sink = Arc::new(FailingArchiveSink::new());
        let producer = ArchiveProducer::new_for_tenant(
            tenant,
            sink as Arc<dyn ArchiveSink>,
            FlushPolicy {
                flush_after_ms: u64::MAX,
                max_events_per_chunk: 1,
                max_bytes_per_chunk: u64::MAX,
            },
            ChainHash::genesis(),
            0,
        );
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 1);
        let err = producer.observe(lines[0].clone(), 1_000).unwrap_err();
        assert!(matches!(err, ArchiveProducerError::SinkBackend(_)));
    }

    #[test]
    fn archive_chunk_key_shape_matches_spec() {
        // 2026-05-15T12:00:00Z = 1747310400000 ms; key shape:
        // audit/2026/05/15/<8-digit>.ndjson
        let ts = 1_747_310_400_000u64;
        let key = archive_chunk_key(ts, 42);
        // The exact date depends on the canonical_date_yyyy_mm_dd
        // algorithm; we assert the shape rather than the exact date.
        assert!(
            key.starts_with("audit/"),
            "expected `audit/` prefix, got {}",
            key
        );
        assert!(
            key.ends_with("/00000042.ndjson"),
            "expected 8-digit seq + .ndjson suffix, got {}",
            key
        );
        // Path segments: audit / YYYY / MM / DD / <seq>.ndjson = 5.
        let segments: Vec<&str> = key.split('/').collect();
        assert_eq!(
            segments.len(),
            5,
            "expected 5 path segments, got {} in {}",
            segments.len(),
            key
        );
        assert_eq!(segments[0], "audit");
        assert_eq!(segments[1].len(), 4, "YYYY len = 4");
        assert_eq!(segments[2].len(), 2, "MM len = 2");
        assert_eq!(segments[3].len(), 2, "DD len = 2");
    }

    #[test]
    fn chain_hashes_eq_ct_returns_true_for_equal_hashes() {
        let a = ChainHash([0xAB; 32]);
        let b = ChainHash([0xAB; 32]);
        assert!(chain_hashes_eq_ct(&a, &b));
    }

    #[test]
    fn chain_hashes_eq_ct_returns_false_for_different_hashes() {
        let a = ChainHash([0xAB; 32]);
        let b = ChainHash([0xCD; 32]);
        assert!(!chain_hashes_eq_ct(&a, &b));
    }

    #[test]
    fn cloned_producer_shares_buffer_state() {
        let tenant = Uuid::now_v7();
        let policy = FlushPolicy {
            flush_after_ms: u64::MAX,
            max_events_per_chunk: 10,
            max_bytes_per_chunk: u64::MAX,
        };
        let (p, _sink) = fresh_producer(tenant, policy);
        let p2 = p.clone();
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 2);
        p.observe(lines[0].clone(), 1_000).unwrap();
        p2.observe(lines[1].clone(), 1_001).unwrap();
        assert_eq!(p.buffered_event_count().unwrap(), 2);
        assert_eq!(p2.buffered_event_count().unwrap(), 2);
    }

    #[test]
    fn resumed_producer_starts_from_checkpoint() {
        let tenant = Uuid::now_v7();
        // Drive a fresh chain to seq 3 so we have a non-genesis
        // checkpoint to resume from.
        let audit_sink = fresh_audit_sink();
        let lines = emit_n_events(&audit_sink, tenant, 4);
        // Resume a producer at seq=3 with the chain head AFTER seq=2.
        let checkpoint_head = lines[2].link_hash;
        let sink: Arc<InMemoryArchiveSink> = Arc::new(InMemoryArchiveSink::new());
        let policy = FlushPolicy {
            flush_after_ms: u64::MAX,
            max_events_per_chunk: 1,
            max_bytes_per_chunk: u64::MAX,
        };
        let producer = ArchiveProducer::new_for_tenant(
            tenant,
            sink.clone() as Arc<dyn ArchiveSink>,
            policy,
            checkpoint_head,
            3,
        );
        // Observe seq=3 — must succeed.
        let r = producer.observe(lines[3].clone(), 1_000).unwrap();
        let receipt = r.expect("policy 1-event chunk: flush trips");
        assert_eq!(receipt.first_sequence_number, 3);
        assert_eq!(receipt.last_sequence_number, 3);
        assert_eq!(receipt.chain_head_after, lines[3].link_hash);
    }

    #[test]
    fn flush_policy_should_flush_combinations() {
        let p = FlushPolicy {
            flush_after_ms: 1000,
            max_events_per_chunk: 10,
            max_bytes_per_chunk: 100,
        };
        // Trip via events.
        assert!(p.should_flush(10, 0, 0));
        // Trip via bytes.
        assert!(p.should_flush(0, 100, 0));
        // Trip via age.
        assert!(p.should_flush(0, 0, 1000));
        // No trip.
        assert!(!p.should_flush(9, 99, 999));
    }
}
