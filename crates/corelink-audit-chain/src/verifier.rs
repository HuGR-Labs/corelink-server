//! Daily-verify routine: walks a per-tenant chain `[start, end]`,
//! recomputes every BLAKE3 link hash, fail-CLOSED on the first
//! mismatch.
//!
//! ## Why per-tenant chain (not per-region)
//!
//! Per WI §2 + §6.1.4 the chain is partitioned per-tenant (the WI
//! §1 uses `audit/{tenant_id}/...` prefix; cross-region split-brain
//! risk eliminated by per-tenant partitioning + cross-region correlation
//! via `trace_id` at the event level). A region-level split would
//! force cross-region chain reconciliation (consensus protocol
//! complexity); per-tenant is the canonical CoreLink approach.
//!
//! ## Daily verifier 24h cycle (sprint contract §6 DoD)
//!
//! Per WI §1 invariant 9 + sprint contract §6 DoD: the verifier runs
//! at UTC 02:00 (low-traffic window); breaks must surface within 24h.
//! Real-time verification per-emit would add ~10ms latency tax (BLAKE3
//! hash + R2 read) — 24h batch amortizes the cost to ~1ms/event over
//! 1M events × 5min execution time.
//!
//! ## Streaming walk + bounded memory
//!
//! Per WI §6.1.5 the production verifier streams via batched R2 list
//! (1 KB/event × 1000 events/batch = 1 MB/batch; well under the DO 32
//! MiB budget). The in-memory fake here exercises the algorithmic core
//! — given an in-order slice of events, recompute the chain link at
//! each position + assert the claimed `prev_hash` matches the
//! recomputed hash. Real R2 binding lands at WI-S09-007 PRR ship gate.

use crate::audit::{
    AuditChainAuditEventType, AuditChainAuditRecord, AuditChainAuditSink,
};
use crate::chain::link_chain_hash;
use crate::error::AuditChainError;
use crate::event::{AuditEvent, ChainHash, GENESIS_PREV_HASH, GENESIS_SEQUENCE_NUMBER};

/// Outcome of a daily-verify run over a slice of chain events.
///
/// `events_verified_count` is the total number of events the verifier
/// successfully checked (everything before the first break, or all
/// events if the chain is intact).
///
/// `last_verified_hash` is the BLAKE3 link hash of the last successfully
/// verified event (i.e. the chain head AFTER the last good event); the
/// production wiring persists this to the durable mirror as the
/// `verify_checkpoint` so the next verify run can resume.
///
/// `first_break_at_seq` is `Some(seq)` if a break was detected (the
/// sequence number of the FIRST event whose recomputed `prev_hash` did
/// not match the claimed `prev_hash`); `None` if the chain walked
/// cleanly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyOutcome {
    /// Total number of events successfully verified.
    pub events_verified_count: u64,
    /// Last verified link hash (the chain head after the last good
    /// event).
    pub last_verified_hash: ChainHash,
    /// `Some(seq)` if a break was detected (first divergence sequence);
    /// `None` if the chain is intact.
    pub first_break_at_seq: Option<u64>,
}

/// Chain verifier — pure-logic primitive over an in-order slice of
/// `AuditEvent`s. Production wiring composes this on top of an R2 list
/// stream + a `verify_checkpoint` D1 mirror.
///
/// The verifier holds the audit-of-audit sink so every verify run
/// (success or break) emits a meta-audit row (per WI §6.1.10
/// `corelink_audit_chain_break_detected_total` SEV-0 alert source).
#[derive(Clone, Debug)]
pub struct ChainVerifier<A>
where
    A: AuditChainAuditSink + 'static,
{
    audit: std::sync::Arc<A>,
}

impl<A> ChainVerifier<A>
where
    A: AuditChainAuditSink + 'static,
{
    /// Construct with explicit audit-of-audit sink.
    pub const fn new(audit: std::sync::Arc<A>) -> Self {
        Self { audit }
    }

    /// Verify the chain represented by `events` (in-order; sequence
    /// numbers must be strictly monotonic increasing by 1 starting from
    /// `expected_start_seq`).
    ///
    /// `expected_start_seq` is the sequence number the FIRST event in
    /// `events` MUST carry (the production wiring sets this to the
    /// `verify_checkpoint` from the last verify run, or `0` for the
    /// canonical fresh-tenant verify).
    ///
    /// `expected_start_prev_hash` is the chain head the first event's
    /// `prev_hash` MUST match (`[0u8; 32]` for the genesis verify;
    /// the persisted last-verified-hash from the durable mirror for a
    /// resumed verify).
    ///
    /// `expected_tenant_id` is the canonical UUID (string-compared) all
    /// events MUST share — defensive guard against caller misuse where
    /// a cross-tenant slice is accidentally passed.
    ///
    /// `created_by_request_id` is the canonical request-id slot; for
    /// daily-verify routine emits this is `"verifier"` per WI §6.1.5.
    ///
    /// `now_ms` is the Unix epoch ms instant of the verify run.
    ///
    /// # Errors
    ///
    /// - [`AuditChainError::ChainBreak`] if any event's recomputed
    ///   `prev_hash` doesn't match the claimed `prev_hash` — this is
    ///   the SEV-0 fail-CLOSED arm; the verifier emits the canonical
    ///   `corelink.audit_chain.chain_break_detected` audit BEFORE
    ///   returning.
    /// - [`AuditChainError::SequenceOrderingViolation`] if the events
    ///   are not strictly monotonic increasing by 1.
    /// - [`AuditChainError::TenantIsolationViolation`] if any event's
    ///   `tenant_id` does not match `expected_tenant_id`.
    /// - [`AuditChainError::Canonicalization`] if JCS canonicalization
    ///   of any event fails.
    /// - [`AuditChainError::Audit`] if the audit-of-audit sink fails.
    pub fn verify_chain(
        &self,
        events: &[AuditEvent],
        expected_start_seq: u64,
        expected_start_prev_hash: ChainHash,
        expected_tenant_id: uuid::Uuid,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<VerifyOutcome, AuditChainError> {
        let mut current_prev_hash = expected_start_prev_hash;
        let mut current_seq = expected_start_seq;
        let mut events_verified_count: u64 = 0;
        let mut last_verified_hash = expected_start_prev_hash;

        for (idx, ev) in events.iter().enumerate() {
            // Tenant isolation guard.
            if ev.tenant_id != expected_tenant_id {
                return Err(AuditChainError::TenantIsolationViolation {
                    chain_tenant: expected_tenant_id.to_string(),
                    event_tenant: ev.tenant_id.to_string(),
                    index: idx,
                });
            }
            // Sequence monotonicity guard.
            if ev.sequence_number != current_seq {
                return Err(AuditChainError::SequenceOrderingViolation {
                    expected_start: current_seq,
                    observed: ev.sequence_number,
                    index: idx,
                });
            }
            // Chain link integrity check. We compare the event's
            // claimed prev_hash to the running expected prev_hash; if
            // they diverge, the chain is broken at THIS event (the
            // FIRST event whose claimed prev_hash doesn't match the
            // running chain head).
            if ev.prev_hash.as_bytes() != current_prev_hash.as_bytes() {
                self.emit_chain_break_audit(
                    ev.subject.subject(),
                    expected_tenant_id,
                    ev.sequence_number,
                    created_by_request_id,
                    now_ms,
                )?;
                return Err(AuditChainError::ChainBreak {
                    at_sequence: ev.sequence_number,
                    tenant_id: expected_tenant_id.to_string(),
                });
            }
            // Recompute the link hash + advance the running chain head.
            let next_link = link_chain_hash(&current_prev_hash, ev)?;
            current_prev_hash = next_link;
            last_verified_hash = next_link;
            current_seq = current_seq.saturating_add(1);
            events_verified_count = events_verified_count.saturating_add(1);
        }

        // Emit chain_verified_ok audit (informational; SLO source per
        // WI §3 SLA `corelink_audit_chain_verify_runs_total{result=ok}`
        // counter).
        self.emit_chain_verified_ok_audit(
            "chain",
            expected_tenant_id,
            current_seq.saturating_sub(1),
            created_by_request_id,
            now_ms,
        )?;

        Ok(VerifyOutcome {
            events_verified_count,
            last_verified_hash,
            first_break_at_seq: None,
        })
    }

    /// Verify a fresh tenant chain starting from the canonical genesis
    /// position (`expected_start_seq = 0`, `expected_start_prev_hash =
    /// [0u8; 32]`). Convenience wrapper around
    /// [`ChainVerifier::verify_chain`] for the common case where the
    /// caller hasn't persisted a verify checkpoint yet.
    ///
    /// # Errors
    ///
    /// Same as [`ChainVerifier::verify_chain`].
    pub fn verify_chain_from_genesis(
        &self,
        events: &[AuditEvent],
        expected_tenant_id: uuid::Uuid,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<VerifyOutcome, AuditChainError> {
        self.verify_chain(
            events,
            GENESIS_SEQUENCE_NUMBER,
            ChainHash(GENESIS_PREV_HASH),
            expected_tenant_id,
            created_by_request_id,
            now_ms,
        )
    }

    fn emit_chain_break_audit(
        &self,
        chain_subject: &'static str,
        tenant_id: uuid::Uuid,
        sequence_number: u64,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<(), AuditChainError> {
        self.audit.emit(AuditChainAuditRecord {
            event_type: AuditChainAuditEventType::ChainBreakDetected,
            chain_subject,
            tenant_id: tenant_id.to_string(),
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            sequence_number,
        })?;
        Ok(())
    }

    fn emit_chain_verified_ok_audit(
        &self,
        chain_subject: &'static str,
        tenant_id: uuid::Uuid,
        last_sequence: u64,
        created_by_request_id: &str,
        now_ms: u64,
    ) -> Result<(), AuditChainError> {
        self.audit.emit(AuditChainAuditRecord {
            event_type: AuditChainAuditEventType::ChainVerifiedOk,
            chain_subject,
            tenant_id: tenant_id.to_string(),
            created_by_request_id: created_by_request_id.to_string(),
            now_ms,
            sequence_number: last_sequence,
        })?;
        Ok(())
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
    use crate::event::AuditEventKind;
    use crate::sink::InMemoryR2AuditSink;
    use corelink_analytics::Region;
    use serde_json::json;
    use uuid::Uuid;

    type Sink = InMemoryR2AuditSink<InMemoryAuditChainAuditSink>;

    fn fresh_setup() -> (
        Sink,
        ChainVerifier<InMemoryAuditChainAuditSink>,
        std::sync::Arc<InMemoryAuditChainAuditSink>,
    ) {
        let audit = std::sync::Arc::new(InMemoryAuditChainAuditSink::new());
        let s = InMemoryR2AuditSink::new(std::sync::Arc::clone(&audit));
        let v = ChainVerifier::new(std::sync::Arc::clone(&audit));
        (s, v, audit)
    }

    fn build_chain(sink: &Sink, tenant: Uuid, n: u64) -> Vec<AuditEvent> {
        let mut chain = Vec::new();
        for i in 0..n {
            let (seq, prev) = sink.next_link_inputs(tenant);
            let ev = AuditEvent::new(
                if i == 0 {
                    AuditEventKind::Tenant
                } else {
                    AuditEventKind::CasPut
                },
                "corelink/region/iad",
                Uuid::now_v7(),
                1_700_000_000_000_u64.saturating_add(i),
                tenant,
                Region::Iad,
                seq,
                prev,
                json!({"i": i}),
            );
            sink.emit(ev.clone(), "req-x", 1_700_000_000_000_u64.saturating_add(i))
                .unwrap();
            chain.push(ev);
        }
        chain
    }

    #[test]
    fn verify_intact_chain_returns_ok_outcome() {
        let (s, v, audit) = fresh_setup();
        let tenant = Uuid::now_v7();
        let chain = build_chain(&s, tenant, 5);
        let outcome = v
            .verify_chain_from_genesis(&chain, tenant, "verifier", 5_000)
            .unwrap();
        assert_eq!(outcome.events_verified_count, 5);
        assert_eq!(outcome.first_break_at_seq, None);
        assert!(audit
            .snapshot_of(AuditChainAuditEventType::ChainVerifiedOk)
            .iter()
            .any(|r| r.tenant_id == tenant.to_string()));
    }

    #[test]
    fn verify_empty_chain_returns_ok_zero_events() {
        let (_s, v, audit) = fresh_setup();
        let tenant = Uuid::now_v7();
        let outcome = v
            .verify_chain_from_genesis(&[], tenant, "verifier", 1)
            .unwrap();
        assert_eq!(outcome.events_verified_count, 0);
        assert_eq!(outcome.first_break_at_seq, None);
        assert_eq!(outcome.last_verified_hash, ChainHash::genesis());
        // Even the empty-chain verify emits the chain_verified_ok
        // audit (informational; the production wiring uses this as the
        // "the verifier ran" heartbeat).
        assert!(!audit.is_empty());
    }

    #[test]
    fn verify_detects_tampered_data_at_break_seq() {
        let (s, v, audit) = fresh_setup();
        let tenant = Uuid::now_v7();
        let mut chain = build_chain(&s, tenant, 5);
        // Tamper: flip a byte in event 2's data payload AFTER the
        // chain was built. The verifier recomputes the link hash from
        // the canonical bytes; the tampered event's link will diverge
        // from event 3's claimed prev_hash.
        chain[2].data = json!({"i": 999_999_999u64, "tampered": true});
        let err = v
            .verify_chain_from_genesis(&chain, tenant, "verifier", 5_000)
            .unwrap_err();
        match err {
            AuditChainError::ChainBreak { at_sequence, .. } => {
                // Tampering event 2 changes its computed link hash;
                // event 3's claimed prev_hash no longer matches —
                // break surfaces at sequence 3 (the FIRST event whose
                // claimed prev_hash diverges from the running chain
                // head).
                assert_eq!(at_sequence, 3);
            }
            other => panic!("unexpected err: {other:?}"),
        }
        // chain_break_detected audit emitted.
        assert_eq!(
            audit
                .snapshot_of(AuditChainAuditEventType::ChainBreakDetected)
                .len(),
            1
        );
    }

    #[test]
    fn verify_detects_tampered_prev_hash_at_break_seq() {
        let (s, v, _audit) = fresh_setup();
        let tenant = Uuid::now_v7();
        let mut chain = build_chain(&s, tenant, 5);
        // Tamper: flip a byte in event 2's prev_hash slot.
        let mut bytes = *chain[2].prev_hash.as_bytes();
        bytes[0] ^= 0xFF;
        chain[2].prev_hash = ChainHash(bytes);
        let err = v
            .verify_chain_from_genesis(&chain, tenant, "verifier", 5_000)
            .unwrap_err();
        // Break is at event 2 itself (its prev_hash doesn't match the
        // running chain head computed from events 0 + 1).
        match err {
            AuditChainError::ChainBreak { at_sequence, .. } => {
                assert_eq!(at_sequence, 2);
            }
            other => panic!("unexpected err: {other:?}"),
        }
    }

    #[test]
    fn verify_detects_tampered_genesis_prev_hash() {
        let (s, v, _audit) = fresh_setup();
        let tenant = Uuid::now_v7();
        let mut chain = build_chain(&s, tenant, 3);
        // Tamper: flip event 0's prev_hash from zero to non-zero.
        chain[0].prev_hash = ChainHash([0xAB; 32]);
        let err = v
            .verify_chain_from_genesis(&chain, tenant, "verifier", 1)
            .unwrap_err();
        match err {
            AuditChainError::ChainBreak { at_sequence, .. } => {
                assert_eq!(at_sequence, 0);
            }
            other => panic!("unexpected err: {other:?}"),
        }
    }

    #[test]
    fn verify_rejects_wrong_tenant_event() {
        let (s, v, _audit) = fresh_setup();
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let mut chain = build_chain(&s, tenant_a, 3);
        // Tamper: rewrite event 1's tenant_id to tenant_b.
        chain[1].tenant_id = tenant_b;
        let err = v
            .verify_chain_from_genesis(&chain, tenant_a, "verifier", 1)
            .unwrap_err();
        assert!(matches!(err, AuditChainError::TenantIsolationViolation { .. }));
    }

    #[test]
    fn verify_rejects_non_monotonic_sequence() {
        let (s, v, _audit) = fresh_setup();
        let tenant = Uuid::now_v7();
        let mut chain = build_chain(&s, tenant, 3);
        // Tamper: rewrite event 1's sequence_number to 5.
        chain[1].sequence_number = 5;
        let err = v
            .verify_chain_from_genesis(&chain, tenant, "verifier", 1)
            .unwrap_err();
        assert!(matches!(err, AuditChainError::SequenceOrderingViolation { .. }));
    }

    #[test]
    fn verify_resume_from_checkpoint_works() {
        let (s, v, _audit) = fresh_setup();
        let tenant = Uuid::now_v7();
        let chain = build_chain(&s, tenant, 6);
        // Verify first half from genesis.
        let head_after_3 = {
            let outcome = v
                .verify_chain_from_genesis(&chain[0..3], tenant, "verifier", 1)
                .unwrap();
            assert_eq!(outcome.events_verified_count, 3);
            outcome.last_verified_hash
        };
        // Verify second half resuming from checkpoint.
        let outcome2 = v
            .verify_chain(&chain[3..6], 3, head_after_3, tenant, "verifier", 2)
            .unwrap();
        assert_eq!(outcome2.events_verified_count, 3);
        assert_eq!(outcome2.first_break_at_seq, None);
    }

    #[test]
    fn verify_outcome_last_hash_matches_sink_chain_head() {
        let (s, v, _audit) = fresh_setup();
        let tenant = Uuid::now_v7();
        let chain = build_chain(&s, tenant, 4);
        let outcome = v
            .verify_chain_from_genesis(&chain, tenant, "verifier", 1)
            .unwrap();
        // The verifier's last_verified_hash must equal the sink's
        // chain head (independent recomputation == producer-side
        // chain head; this is the falsifiability target of the
        // INV-OBS-AUDIT-CHAIN-INTEGRITY invariant).
        assert_eq!(outcome.last_verified_hash, s.chain_head(tenant).unwrap());
    }
}
