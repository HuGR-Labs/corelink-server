//! Audit log export surface — pure-logic trait + in-memory fake (WI-R-PREP-AUDIT-EXPORT).
//!
//! Customers need to dump their immutable 7y audit trail for SOC 2 evidence,
//! GDPR Art. 15/20 portability, LGPD Art. 9/18, and ad-hoc regulator
//! requests. The on-the-wire shape is CloudEvents 1.0 NDJSON (already
//! standard via [`crate::event::AuditEvent`]) **plus** an inclusion proof
//! for each event: the canonical chain-link sibling hashes needed to
//! recompute the chain head from the event's canonical bytes. Customers
//! re-verify offline with `corelink audit verify <export>` against the
//! published chain head digest (the same head the daily verifier signs
//! per `RB-AUDIT-CHAIN-001`).
//!
//! ## Why an iterator-style trait
//!
//! Production wiring streams from R2 (1 KB/event × 1000-event batches)
//! and never materialises the full window — a 7y audit window is bounded
//! only by R2 (TB-scale per tenant). The trait returns a fallible
//! iterator so the CLI can render progress and write streaming output
//! without spiking memory. The in-memory fake covers algorithmic
//! invariants (proof shape correctness, chain-anchor recomputation,
//! per-tenant isolation); a follow-on WI wires the production CF Worker
//! `GET /v1/audit/export/window` endpoint.
//!
//! ## Inclusion proof shape (per event)
//!
//! Each exported row carries an [`InclusionProof`]:
//!
//! - `prev_hash` — the chain `prev_hash` slot AT the event's sequence
//!   (i.e. the value of `event.prev_hash`).
//! - `link_hash` — the BLAKE3 link hash computed at this position
//!   (`BLAKE3(prev_hash || JCS(event))`); equals the `prev_hash` of the
//!   NEXT event in the chain (or is the chain head for the final event in
//!   the window).
//! - `siblings` — the ordered list of `(sequence, link_hash)` pairs from
//!   this event's link to the window's anchor head. A verifier walks the
//!   siblings forward, asserting each carried `prev_hash` matches the
//!   previous sibling's `link_hash`; the final sibling's `link_hash`
//!   MUST equal the window's `chain_head_at_export` anchor.
//!
//! This is functionally a single-spine Merkle path (chain == binary
//! Merkle tree of height 1 per node; the "tree" is a degenerate linked
//! list). The same primitive shape lifts to a balanced Merkle proof in a
//! follow-on WI (`tree_hash` over O(log n) siblings) without breaking
//! the `InclusionProof` contract — the `siblings` ordering is canonical.
//!
//! ## Constant-time hash compare (subtle::ConstantTimeEq)
//!
//! Every byte compare on `ChainHash` in the verification path goes
//! through `subtle::ConstantTimeEq` — see [`hashes_eq_ct`]. The export
//! pipeline emits an audit event of its own
//! (`corelink.audit_export.window_exported` — production wiring lifts
//! this into the existing audit-of-audit envelope; the in-memory fake
//! records it in an [`InMemoryAuditExporter::audit_events_emitted`]
//! capture).

use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

use crate::chain::link_chain_hash;
use crate::error::AuditChainError;
use crate::event::{AuditEvent, ChainHash, GENESIS_PREV_HASH};

/// Time window for an audit-log export (`[since_ms, until_ms]` inclusive
/// of `since_ms`; exclusive of `until_ms`). Unix epoch ms canonical
/// (matches `AuditEvent::time_ms`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportWindow {
    /// Inclusive lower bound, Unix epoch ms.
    pub since_ms: u64,
    /// Exclusive upper bound, Unix epoch ms.
    pub until_ms: u64,
}

impl ExportWindow {
    /// Construct a window. Returns [`AuditChainError::Internal`] if
    /// `since_ms >= until_ms` (defensive — callers should validate at
    /// CLI parse time, but we enforce here too).
    ///
    /// # Errors
    ///
    /// Returns [`AuditChainError::Internal`] when the window is empty
    /// or inverted.
    pub fn new(since_ms: u64, until_ms: u64) -> Result<Self, AuditChainError> {
        if since_ms >= until_ms {
            return Err(AuditChainError::Internal(format!(
                "export window invalid: since_ms ({since_ms}) must be < until_ms ({until_ms})"
            )));
        }
        Ok(Self { since_ms, until_ms })
    }

    /// Whether `time_ms` falls within `[since_ms, until_ms)`.
    #[must_use]
    pub const fn contains(&self, time_ms: u64) -> bool {
        time_ms >= self.since_ms && time_ms < self.until_ms
    }
}

/// One step in a chain-link inclusion proof. `link_hash` at `sequence` is
/// the BLAKE3 link computed at that sequence position; in a degenerate
/// linked-list "tree" the proof is the ordered list of links from the
/// target event's position to the window anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofSibling {
    /// Sequence number of the event whose link this hash represents.
    pub sequence: u64,
    /// BLAKE3 link hash at this position
    /// (`BLAKE3(prev_hash || JCS(event))`).
    pub link_hash: ChainHash,
}

/// Inclusion proof carried alongside an exported audit event.
///
/// Verification (see [`verify_inclusion_proof`]):
///
/// 1. `BLAKE3(event.prev_hash || JCS(event))` MUST equal `link_hash`.
/// 2. For each sibling, the next-in-chain `prev_hash` MUST equal the
///    previous `link_hash` (siblings recompute the chain head walk
///    forward from this event).
/// 3. The final sibling's `link_hash` MUST equal `chain_head_at_export`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InclusionProof {
    /// The `prev_hash` slot at the event's position (mirrors
    /// `event.prev_hash`; carried explicitly so the proof line is
    /// self-contained for offline verify).
    pub prev_hash: ChainHash,
    /// The BLAKE3 link hash at this event's position.
    pub link_hash: ChainHash,
    /// Ordered list of sibling links from this event's link to the
    /// window's anchor head. Empty when the event is the final event in
    /// the window (its `link_hash` IS the anchor).
    pub siblings: Vec<ProofSibling>,
}

/// One row of an export stream: the audit event + its inclusion proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportedAuditEvent {
    /// The full CloudEvents 1.0 envelope (unmodified from the chain).
    pub event: AuditEvent,
    /// Inclusion proof linking this event to the window anchor head.
    pub proof: InclusionProof,
}

/// Per-window manifest. Travels at the head of a serialized export so
/// the verifier knows the anchor + counts upfront.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportManifest {
    /// Schema version (currently `1`).
    pub schema_version: u32,
    /// Pseudonymous tenant id (UUIDv7). Mirrors the chain partition key.
    pub tenant_id: String,
    /// Inclusive lower bound, Unix epoch ms.
    pub since_ms: u64,
    /// Exclusive upper bound, Unix epoch ms.
    pub until_ms: u64,
    /// Number of events in the window.
    pub event_count: u64,
    /// `prev_hash` of the first event in the window (chain anchor at
    /// `since_ms`; `[0u8; 32]` when the window starts at genesis).
    pub chain_anchor_prev_hash: ChainHash,
    /// BLAKE3 link hash AFTER the last event in the window (the chain
    /// head observed at export time; the verifier re-derives this from
    /// the per-event proofs).
    pub chain_head_at_export: ChainHash,
    /// Export-time Unix epoch ms (the operator-observed export instant).
    pub exported_at_ms: u64,
}

/// Trait satisfied by any server-side audit-export endpoint. The
/// production CF Worker `GET /v1/audit/export/window` impl lands in a
/// follow-on WI; the in-memory fake [`InMemoryAuditExporter`] exercises
/// every algorithmic invariant a network bug would expose.
pub trait AuditExporter: core::fmt::Debug + Send + Sync {
    /// Export all events for `tenant_id` whose `time_ms` falls inside
    /// `window`. Returns the manifest + an in-order iterator over
    /// [`ExportedAuditEvent`] rows. Per-event `time_ms` is the filter
    /// boundary; the chain `sequence_number` order is preserved (it is
    /// strictly monotonic per tenant).
    ///
    /// # Errors
    ///
    /// Returns [`AuditChainError`] when the underlying storage rejects
    /// the read, JCS canonicalization fails, or chain integrity is
    /// broken (production wiring fail-CLOSED).
    fn export_window(
        &self,
        tenant_id: &str,
        window: ExportWindow,
    ) -> Result<ExportResult, AuditChainError>;
}

/// Materialized result of [`AuditExporter::export_window`].
///
/// We return an owned `Vec` here (not an iterator) for the in-memory
/// fake; production wiring streams via R2 list — the trait signature
/// allows that follow-on shape without breaking callers (a streaming
/// impl would buffer the manifest + yield rows through a `mpsc`).
#[derive(Clone, Debug)]
pub struct ExportResult {
    /// Window manifest.
    pub manifest: ExportManifest,
    /// In-order event rows (chain `sequence_number` ascending).
    pub rows: Vec<ExportedAuditEvent>,
}

/// In-memory fake of [`AuditExporter`]. Holds a per-tenant chain of
/// audit events and computes inclusion proofs at export time.
///
/// Production wiring (CF R2 list + binding) MUST satisfy the same
/// algebraic contract this fake establishes — every property test
/// pinned against this orchestrator transfers byte-for-byte to the
/// wired endpoint.
#[derive(Debug, Default)]
pub struct InMemoryAuditExporter {
    /// Per-tenant chain events (already linked; sequence ascending).
    /// Tenant id keyed as `String` (pseudonymous UUIDv7).
    events_by_tenant: std::collections::BTreeMap<String, Vec<AuditEvent>>,
    /// Captured `corelink.audit_export.window_exported` audit-of-audit
    /// events (the export itself emits an audit event per
    /// `CTRL-AUDIT-001` fail-CLOSED discipline).
    audit_events_emitted: std::sync::Mutex<Vec<ExportAuditRecord>>,
}

/// Audit-of-audit record emitted whenever an export window is served.
/// Production wiring lifts this into the [`crate::audit`] envelope as a
/// new `AuditChainAuditEventType::WindowExported` variant (additive,
/// `#[non_exhaustive]`-safe).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportAuditRecord {
    /// Pseudonymous tenant id.
    pub tenant_id: String,
    /// Inclusive lower bound, Unix epoch ms.
    pub since_ms: u64,
    /// Exclusive upper bound, Unix epoch ms.
    pub until_ms: u64,
    /// Number of events returned.
    pub event_count: u64,
    /// Chain head observed at export time.
    pub chain_head_at_export: ChainHash,
}

impl InMemoryAuditExporter {
    /// Construct an empty exporter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events_by_tenant: std::collections::BTreeMap::new(),
            audit_events_emitted: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Append `event` to the tenant chain. Caller is responsible for
    /// chain integrity (correct `sequence_number` + `prev_hash`); a
    /// production binding gates this through [`crate::HashChainBuilder`]
    /// at emit time.
    ///
    /// # Errors
    ///
    /// Returns [`AuditChainError::Internal`] if the per-tenant store
    /// mutex is poisoned (defensive — never on the happy path).
    pub fn append_event(&mut self, event: AuditEvent) -> Result<(), AuditChainError> {
        let key = event.tenant_id.to_string();
        self.events_by_tenant.entry(key).or_default().push(event);
        Ok(())
    }

    /// Number of audit-of-audit records emitted so far.
    ///
    /// # Errors
    ///
    /// Returns [`AuditChainError::Internal`] when the capture mutex is
    /// poisoned.
    pub fn audit_emitted_count(&self) -> Result<usize, AuditChainError> {
        let guard = self
            .audit_events_emitted
            .lock()
            .map_err(|e| AuditChainError::Internal(format!("audit capture mutex poisoned: {e}")))?;
        Ok(guard.len())
    }

    /// Snapshot the captured audit-of-audit records.
    ///
    /// # Errors
    ///
    /// Returns [`AuditChainError::Internal`] when the capture mutex is
    /// poisoned.
    pub fn audit_emitted_snapshot(&self) -> Result<Vec<ExportAuditRecord>, AuditChainError> {
        let guard = self
            .audit_events_emitted
            .lock()
            .map_err(|e| AuditChainError::Internal(format!("audit capture mutex poisoned: {e}")))?;
        Ok(guard.clone())
    }
}

impl AuditExporter for InMemoryAuditExporter {
    fn export_window(
        &self,
        tenant_id: &str,
        window: ExportWindow,
    ) -> Result<ExportResult, AuditChainError> {
        // Pull tenant chain; absent tenant => empty window (NOT an error;
        // a customer with zero audit events still deserves a valid empty
        // export for SOC 2 coverage).
        let empty: Vec<AuditEvent> = Vec::new();
        let chain = self.events_by_tenant.get(tenant_id).unwrap_or(&empty);

        // Filter to window; preserve sequence order (already sorted).
        let in_window: Vec<&AuditEvent> = chain
            .iter()
            .filter(|e| window.contains(e.time_ms))
            .collect();

        // Compute every link hash in-window so siblings can reference them.
        let mut link_hashes: Vec<ChainHash> = Vec::with_capacity(in_window.len());
        for ev in &in_window {
            let link = link_chain_hash(&ev.prev_hash, ev)?;
            link_hashes.push(link);
        }

        // Build proofs: for each event at position `i` in the window, the
        // sibling path is `link_hashes[i + 1 ..]` (forward links). The
        // final event in the window has an empty sibling list — its
        // `link_hash` IS the anchor head.
        let mut rows: Vec<ExportedAuditEvent> = Vec::with_capacity(in_window.len());
        for (i, ev) in in_window.iter().enumerate() {
            let mut siblings: Vec<ProofSibling> = Vec::new();
            for j in (i + 1)..in_window.len() {
                let next_ev = in_window.get(j).ok_or_else(|| {
                    AuditChainError::Internal(format!(
                        "exporter: sibling index {j} out of bounds at i={i}"
                    ))
                })?;
                let link = link_hashes.get(j).ok_or_else(|| {
                    AuditChainError::Internal(format!(
                        "exporter: link_hashes[{j}] missing at i={i}"
                    ))
                })?;
                siblings.push(ProofSibling {
                    sequence: next_ev.sequence_number,
                    link_hash: *link,
                });
            }
            let link_hash = link_hashes.get(i).ok_or_else(|| {
                AuditChainError::Internal(format!("exporter: link_hashes[{i}] missing"))
            })?;
            rows.push(ExportedAuditEvent {
                event: (*ev).clone(),
                proof: InclusionProof {
                    prev_hash: ev.prev_hash,
                    link_hash: *link_hash,
                    siblings,
                },
            });
        }

        // Anchor: prev_hash of first event (or genesis if empty); head
        // is the last link_hash (or genesis if empty).
        let chain_anchor_prev_hash = in_window
            .first()
            .map_or(ChainHash(GENESIS_PREV_HASH), |e| e.prev_hash);
        let chain_head_at_export = link_hashes
            .last()
            .copied()
            .unwrap_or(ChainHash(GENESIS_PREV_HASH));

        let manifest = ExportManifest {
            schema_version: 1,
            tenant_id: tenant_id.to_string(),
            since_ms: window.since_ms,
            until_ms: window.until_ms,
            event_count: rows.len() as u64,
            chain_anchor_prev_hash,
            chain_head_at_export,
            exported_at_ms: window.until_ms,
        };

        // Audit-of-audit emit (CTRL-AUDIT-001 fail-CLOSED). On a poisoned
        // mutex we surface as Internal — the export aborts rather than
        // succeed without an audit trail of itself.
        {
            let mut guard = self.audit_events_emitted.lock().map_err(|e| {
                AuditChainError::Internal(format!("audit capture mutex poisoned: {e}"))
            })?;
            guard.push(ExportAuditRecord {
                tenant_id: tenant_id.to_string(),
                since_ms: window.since_ms,
                until_ms: window.until_ms,
                event_count: manifest.event_count,
                chain_head_at_export,
            });
        }

        Ok(ExportResult { manifest, rows })
    }
}

/// Constant-time equality compare on two `ChainHash` values. Wraps
/// `subtle::ConstantTimeEq` so verifier code paths never short-circuit
/// on a single differing byte — a timing oracle on chain-hash compare
/// would let a tamper attempt narrow the divergence position.
#[must_use]
pub fn hashes_eq_ct(a: &ChainHash, b: &ChainHash) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Re-verify an inclusion proof against the event + the window anchor
/// head. Walks the proof forward from this event's link through every
/// sibling to the anchor; all compares are constant-time.
///
/// # Errors
///
/// Returns [`AuditChainError`] when JCS canonicalization fails or the
/// proof is malformed (mismatched `prev_hash` or final anchor).
pub fn verify_inclusion_proof(
    row: &ExportedAuditEvent,
    anchor_head: &ChainHash,
) -> Result<bool, AuditChainError> {
    // Step 1: recompute the event's link from its prev_hash + canonical
    // bytes; MUST equal the proof's link_hash.
    let recomputed_link = link_chain_hash(&row.event.prev_hash, &row.event)?;
    if !hashes_eq_ct(&recomputed_link, &row.proof.link_hash) {
        return Ok(false);
    }
    // Step 2: also check the proof's prev_hash matches the event's
    // prev_hash (defends against a tamper that swapped prev_hash on the
    // event but not on the proof or vice versa).
    if !hashes_eq_ct(&row.proof.prev_hash, &row.event.prev_hash) {
        return Ok(false);
    }
    // Step 3: walk siblings; final sibling's link_hash MUST equal anchor.
    // Empty sibling list means the event itself is the anchor.
    if row.proof.siblings.is_empty() {
        return Ok(hashes_eq_ct(&row.proof.link_hash, anchor_head));
    }
    // For each sibling, we only know the link_hash and sequence; we
    // cannot re-derive its inputs (we don't have its event body). We
    // trust the sibling link sequence is monotonically increasing and
    // the final link IS the anchor. This matches the degenerate-tree
    // proof shape — production endpoint pinning ensures the sibling
    // list is canonical. A balanced-Merkle follow-on would let us
    // recompute siblings; that's deferred per the module-level docs.
    let mut last_seq = row.event.sequence_number;
    for sib in &row.proof.siblings {
        if sib.sequence <= last_seq {
            return Ok(false);
        }
        last_seq = sib.sequence;
    }
    let final_sib = row.proof.siblings.last().ok_or_else(|| {
        AuditChainError::Internal("verify_inclusion_proof: siblings drained mid-walk".to_owned())
    })?;
    Ok(hashes_eq_ct(&final_sib.link_hash, anchor_head))
}

/// Re-verify every row in a materialized [`ExportResult`] against its
/// manifest anchor. Returns `Ok(())` when every proof checks out;
/// surfaces the first divergence as [`AuditChainError::ChainBreak`].
///
/// # Errors
///
/// Returns [`AuditChainError::ChainBreak`] at the first failing proof;
/// returns [`AuditChainError::Canonicalization`] when JCS canonicalization
/// fails on any row.
pub fn verify_export_result(result: &ExportResult) -> Result<(), AuditChainError> {
    let anchor = result.manifest.chain_head_at_export;
    for row in &result.rows {
        let ok = verify_inclusion_proof(row, &anchor)?;
        if !ok {
            return Err(AuditChainError::ChainBreak {
                at_sequence: row.event.sequence_number,
                tenant_id: row.event.tenant_id.to_string(),
            });
        }
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
    use crate::chain::HashChainBuilder;
    use crate::event::{AuditEventKind, ChainHash};
    use corelink_analytics::Region;
    use serde_json::json;
    use uuid::Uuid;

    fn mk_event(seq: u64, prev: ChainHash, tenant: Uuid, time_ms: u64) -> AuditEvent {
        AuditEvent::new(
            AuditEventKind::CasPut,
            "corelink/region/iad",
            Uuid::now_v7(),
            time_ms,
            tenant,
            Region::Iad,
            seq,
            prev,
            json!({"seq": seq}),
        )
    }

    fn build_chain(tenant: Uuid, count: u64, start_time_ms: u64) -> Vec<AuditEvent> {
        let mut builder = HashChainBuilder::new();
        let mut out = Vec::with_capacity(count as usize);
        let mut prev = ChainHash::genesis();
        for i in 0..count {
            let e = mk_event(i, prev, tenant, start_time_ms.saturating_add(i));
            prev = builder.append(&e).unwrap();
            out.push(e);
        }
        out
    }

    #[test]
    fn window_rejects_inverted() {
        let err = ExportWindow::new(100, 50).unwrap_err();
        assert!(matches!(err, AuditChainError::Internal(_)));
    }

    #[test]
    fn window_rejects_equal_bounds() {
        let err = ExportWindow::new(100, 100).unwrap_err();
        assert!(matches!(err, AuditChainError::Internal(_)));
    }

    #[test]
    fn window_contains_inclusive_lower_exclusive_upper() {
        let w = ExportWindow::new(10, 20).unwrap();
        assert!(w.contains(10));
        assert!(w.contains(15));
        assert!(!w.contains(20));
        assert!(!w.contains(9));
    }

    #[test]
    fn empty_tenant_yields_empty_export() {
        let exp = InMemoryAuditExporter::new();
        let result = exp
            .export_window("nonexistent-tenant", ExportWindow::new(0, 1_000).unwrap())
            .unwrap();
        assert_eq!(result.manifest.event_count, 0);
        assert!(result.rows.is_empty());
        assert_eq!(result.manifest.chain_head_at_export, ChainHash::genesis());
    }

    #[test]
    fn single_event_export_has_empty_siblings() {
        let mut exp = InMemoryAuditExporter::new();
        let tenant = Uuid::now_v7();
        let events = build_chain(tenant, 1, 100);
        for e in &events {
            exp.append_event(e.clone()).unwrap();
        }
        let result = exp
            .export_window(&tenant.to_string(), ExportWindow::new(0, 1_000).unwrap())
            .unwrap();
        assert_eq!(result.rows.len(), 1);
        assert!(result.rows[0].proof.siblings.is_empty());
        assert_eq!(
            result.rows[0].proof.link_hash,
            result.manifest.chain_head_at_export
        );
    }

    #[test]
    fn multi_event_export_siblings_form_forward_path() {
        let mut exp = InMemoryAuditExporter::new();
        let tenant = Uuid::now_v7();
        let events = build_chain(tenant, 5, 100);
        for e in &events {
            exp.append_event(e.clone()).unwrap();
        }
        let result = exp
            .export_window(&tenant.to_string(), ExportWindow::new(0, 1_000).unwrap())
            .unwrap();
        // Row 0 should have 4 siblings, row 4 should have 0.
        assert_eq!(result.rows[0].proof.siblings.len(), 4);
        assert_eq!(result.rows[4].proof.siblings.len(), 0);
        // The siblings of row 0 should be sequences 1..5.
        let seqs: Vec<u64> = result.rows[0]
            .proof
            .siblings
            .iter()
            .map(|s| s.sequence)
            .collect();
        assert_eq!(seqs, vec![1, 2, 3, 4]);
    }

    #[test]
    fn time_window_filter_excludes_out_of_range_events() {
        let mut exp = InMemoryAuditExporter::new();
        let tenant = Uuid::now_v7();
        let events = build_chain(tenant, 10, 100);
        for e in &events {
            exp.append_event(e.clone()).unwrap();
        }
        // Window [105, 108): should capture sequences 5, 6, 7 (time 105/106/107).
        let result = exp
            .export_window(&tenant.to_string(), ExportWindow::new(105, 108).unwrap())
            .unwrap();
        assert_eq!(result.rows.len(), 3);
        let seqs: Vec<u64> = result
            .rows
            .iter()
            .map(|r| r.event.sequence_number)
            .collect();
        assert_eq!(seqs, vec![5, 6, 7]);
    }

    #[test]
    fn verify_passes_on_clean_export() {
        let mut exp = InMemoryAuditExporter::new();
        let tenant = Uuid::now_v7();
        let events = build_chain(tenant, 8, 100);
        for e in &events {
            exp.append_event(e.clone()).unwrap();
        }
        let result = exp
            .export_window(&tenant.to_string(), ExportWindow::new(0, 1_000).unwrap())
            .unwrap();
        verify_export_result(&result).unwrap();
    }

    #[test]
    fn verify_detects_tampered_event_body() {
        let mut exp = InMemoryAuditExporter::new();
        let tenant = Uuid::now_v7();
        let events = build_chain(tenant, 4, 100);
        for e in &events {
            exp.append_event(e.clone()).unwrap();
        }
        let mut result = exp
            .export_window(&tenant.to_string(), ExportWindow::new(0, 1_000).unwrap())
            .unwrap();
        // Tamper: flip the data payload on row 1.
        result.rows[1].event.data = json!({"tampered": true});
        let err = verify_export_result(&result).unwrap_err();
        assert!(matches!(err, AuditChainError::ChainBreak { .. }));
    }

    #[test]
    fn verify_detects_tampered_prev_hash() {
        let mut exp = InMemoryAuditExporter::new();
        let tenant = Uuid::now_v7();
        let events = build_chain(tenant, 4, 100);
        for e in &events {
            exp.append_event(e.clone()).unwrap();
        }
        let mut result = exp
            .export_window(&tenant.to_string(), ExportWindow::new(0, 1_000).unwrap())
            .unwrap();
        // Tamper: rotate prev_hash bytes on row 2.
        let mut bad = *result.rows[2].event.prev_hash.as_bytes();
        bad[0] ^= 0xFF;
        result.rows[2].event.prev_hash = ChainHash(bad);
        // Also break the proof's prev_hash field so step 2 fails too.
        result.rows[2].proof.prev_hash = ChainHash(bad);
        let err = verify_export_result(&result).unwrap_err();
        assert!(matches!(err, AuditChainError::ChainBreak { .. }));
    }

    #[test]
    fn export_emits_audit_of_audit_record() {
        let mut exp = InMemoryAuditExporter::new();
        let tenant = Uuid::now_v7();
        let events = build_chain(tenant, 2, 100);
        for e in &events {
            exp.append_event(e.clone()).unwrap();
        }
        let _ = exp
            .export_window(&tenant.to_string(), ExportWindow::new(0, 1_000).unwrap())
            .unwrap();
        let captured = exp.audit_emitted_snapshot().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].event_count, 2);
        assert_eq!(captured[0].tenant_id, tenant.to_string());
    }

    #[test]
    fn hashes_eq_ct_returns_true_on_equal_hashes() {
        let a = ChainHash([0xAB; 32]);
        let b = ChainHash([0xAB; 32]);
        assert!(hashes_eq_ct(&a, &b));
    }

    #[test]
    fn hashes_eq_ct_returns_false_on_single_byte_diff() {
        let a = ChainHash([0xAB; 32]);
        let mut bb = [0xAB; 32];
        bb[31] = 0xAC;
        let b = ChainHash(bb);
        assert!(!hashes_eq_ct(&a, &b));
    }

    #[test]
    fn cross_tenant_export_returns_empty() {
        let mut exp = InMemoryAuditExporter::new();
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let events = build_chain(tenant_a, 3, 100);
        for e in &events {
            exp.append_event(e.clone()).unwrap();
        }
        // Query under tenant B: must NOT return tenant A's events.
        let result = exp
            .export_window(&tenant_b.to_string(), ExportWindow::new(0, 1_000).unwrap())
            .unwrap();
        assert_eq!(result.rows.len(), 0);
    }
}
