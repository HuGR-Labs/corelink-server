use super::*;

/// In-memory sweep phase orchestrator (the canonical pure-logic
/// skeleton). Composes the trait dependencies declared at construction
/// time.
pub struct InMemorySweepPhase<S, C, B, X, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaStore,
    X: AcReferenceIndex,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: SweepClock,
{
    runs: Arc<S>,
    candidates: Arc<C>,
    blob_meta: Arc<B>,
    ac_index: Arc<X>,
    audit: Arc<A>,
    metrics: Arc<M>,
    clock: Arc<K>,
    config: SweepConfig,
}

impl<S, C, B, X, A, M, K> core::fmt::Debug for InMemorySweepPhase<S, C, B, X, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaStore,
    X: AcReferenceIndex,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: SweepClock,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemorySweepPhase")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, C, B, X, A, M, K> InMemorySweepPhase<S, C, B, X, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaStore,
    X: AcReferenceIndex,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: SweepClock,
{
    /// Construct a sweep phase orchestrator with the canonical
    /// defaults.
    pub fn with_defaults(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta: Arc<B>,
        ac_index: Arc<X>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
    ) -> Self {
        Self {
            runs,
            candidates,
            blob_meta,
            ac_index,
            audit,
            metrics,
            clock,
            config: SweepConfig::default(),
        }
    }

    /// Construct with an explicit [`SweepConfig`].
    #[allow(
        clippy::too_many_arguments,
        reason = "ctor wires 7 trait deps + 1 knob; collapsing into a builder \
                  hurts call-site clarity in the in-memory pure-logic tests."
    )]
    pub fn new(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta: Arc<B>,
        ac_index: Arc<X>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
        config: SweepConfig,
    ) -> Self {
        Self {
            runs,
            candidates,
            blob_meta,
            ac_index,
            audit,
            metrics,
            clock,
            config,
        }
    }

    /// The `SweepConfig` snapshot.
    #[must_use]
    pub fn config(&self) -> SweepConfig {
        self.config
    }

    /// Process a single candidate row through the sweep decision
    /// pipeline. Visible for property tests so the decision boundary
    /// can be exercised independently of the phase orchestration.
    ///
    /// # Errors
    ///
    /// Surface as [`SweepError`].
    pub fn step_candidate(
        &self,
        candidate: &GcCandidate,
        region: GcRegion,
    ) -> Result<SweepDecision, SweepError> {
        // Idempotent re-run guard: already-resolved candidates emit
        // AlreadyResolved + no audit. The mark→sweep contract is that
        // sweep only ingests rows whose mark phase wrote `Candidate`.
        if candidate.status != CandidateStatus::Candidate {
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        }
        // INV-GC-004 protect-if->= probe (canonical TLA semantics).
        let witness = self.ac_index.find_re_reference(
            candidate.tenant_id,
            &candidate.digest,
            candidate.mark_started_at_ms,
        )?;
        let now = self.clock.now_ms();
        if let Some(witness) = witness {
            // PROTECT path. Fail-closed audit emit BEFORE flipping the
            // row status — if audit fails the row remains 'candidate'
            // and a subsequent re-run can retry.
            let reason = format!(
                "ac.created_at_ms={} mark_started_at_ms={} action_digest={}",
                witness.created_at_ms, candidate.mark_started_at_ms, witness.action_digest
            );
            self.audit.emit(GcAuditRecord {
                event_type: GcEventType::SweepProtectedReRef,
                run_id: candidate.mark_run_id,
                tenant_id: candidate.tenant_id,
                region,
                status: GcStatus::Running,
                from_phase: Some(GcPhase::Sweep),
                to_phase: Some(GcPhase::Sweep),
                created_by_request_id: "cron".to_owned(),
                reason: "inv_gc_004_protected_re_ref",
                now_ms: now,
            })?;
            // Atomic transition; if the row drifted from 'candidate'
            // mid-flight (concurrent re-run) the boolean returns false
            // and we surface AlreadyResolved on the second observation.
            let fired = self.candidates.transition_status(
                candidate.tenant_id,
                &candidate.digest,
                candidate.mark_run_id,
                CandidateStatus::Candidate,
                CandidateStatus::ProtectedReRef,
                now,
                Some(reason),
            )?;
            if !fired {
                // Concurrent winner; observe + surface as resolved.
                let observed = self
                    .candidates
                    .lookup(
                        candidate.tenant_id,
                        &candidate.digest,
                        candidate.mark_run_id,
                    )?
                    .map_or(CandidateStatus::Candidate, |c| c.status);
                return Ok(SweepDecision::AlreadyResolved {
                    observed_status: observed,
                });
            }
            return Ok(SweepDecision::ProtectedReRef {
                protected_at_ms: now,
                mark_started_at_ms: candidate.mark_started_at_ms,
                witness,
            });
        }
        // SOFT-DELETE path. Read prev-state via a non-mutating lookup,
        // emit audit, then perform the conditional UPDATE. Audit emit
        // failure leaves blob_meta unchanged (fail-closed envelope on
        // the in-memory fake matches production D1 atomic batch
        // ROLLBACK semantics).
        let Some(row) = self
            .blob_meta
            .lookup(candidate.tenant_id, &candidate.digest)?
        else {
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        };
        if row.deleted_at_ms.is_some() {
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        }
        let prev_state = BlobState {
            digest: row.digest.clone(),
            size_bytes: row.size_bytes,
            refcount: row.refcount,
            last_referenced_at_ms: row.last_referenced_at_ms,
            created_at_ms: row.created_at_ms,
        };
        self.audit.emit(GcAuditRecord {
            event_type: GcEventType::SweepSoftDeleted,
            run_id: candidate.mark_run_id,
            tenant_id: candidate.tenant_id,
            region,
            status: GcStatus::Running,
            from_phase: Some(GcPhase::Sweep),
            to_phase: Some(GcPhase::Sweep),
            created_by_request_id: "cron".to_owned(),
            reason: "sweep_soft_deleted",
            now_ms: now,
        })?;
        if self
            .blob_meta
            .soft_delete(candidate.tenant_id, &candidate.digest, now)?
            .is_none()
        {
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        };
        let fired = self.candidates.transition_status(
            candidate.tenant_id,
            &candidate.digest,
            candidate.mark_run_id,
            CandidateStatus::Candidate,
            CandidateStatus::Swept,
            now,
            None,
        )?;
        if !fired {
            let observed = self
                .candidates
                .lookup(
                    candidate.tenant_id,
                    &candidate.digest,
                    candidate.mark_run_id,
                )?
                .map_or(CandidateStatus::Candidate, |c| c.status);
            return Ok(SweepDecision::AlreadyResolved {
                observed_status: observed,
            });
        }
        Ok(SweepDecision::Sweep {
            swept_at_ms: now,
            prev_state,
        })
    }
}

impl<S, C, B, X, A, M, K> SweepPhase for InMemorySweepPhase<S, C, B, X, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaStore,
    X: AcReferenceIndex,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: SweepClock,
{
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<SweepResult, SweepError> {
        // 1. Read gc_run + verify mark anchor + region match.
        let run_row = self
            .runs
            .lookup(run_id, tenant_id)?
            .ok_or(SweepError::RunStore(GcRunStoreError::NotFound(run_id)))?;
        if run_row.region != region {
            return Err(SweepError::RegionMismatch {
                run_region: run_row.region,
                caller_region: region,
            });
        }
        let mark_anchor = run_row
            .mark_started_at_ms
            .ok_or(SweepError::MarkAnchorMissing { run_id })?;

        // 2. Phase budget deadline.
        let phase_start = self.clock.now_ms();
        let deadline_ms = phase_start.saturating_add(self.config.phase_budget_ms);

        // 3. Sweep candidates for the run.
        let candidates = self.candidates.snapshot_for_run(tenant_id, run_id)?;
        let mut blobs_swept_count: u64 = 0;
        let mut blobs_protected_re_ref_count: u64 = 0;
        let mut already_resolved_count: u64 = 0;
        let mut bytes_to_be_reclaimed: u64 = 0;
        let mut audit_events_emitted: u64 = 0;
        let mut candidates_processed: u64 = 0;
        for candidate in &candidates {
            // Phase-budget probe per candidate (cheaper than per
            // mutation; aligns with mark phase batched probe pattern).
            let now_for_probe = self.clock.now_ms();
            if now_for_probe > deadline_ms {
                return Err(SweepError::PhaseBudgetExceeded {
                    duration_ms: now_for_probe.saturating_sub(phase_start),
                    budget_ms: self.config.phase_budget_ms,
                });
            }
            let decision = self.step_candidate(candidate, region)?;
            candidates_processed = candidates_processed.saturating_add(1);
            match decision {
                SweepDecision::Sweep { prev_state, .. } => {
                    blobs_swept_count = blobs_swept_count.saturating_add(1);
                    bytes_to_be_reclaimed =
                        bytes_to_be_reclaimed.saturating_add(prev_state.size_bytes);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                }
                SweepDecision::ProtectedReRef { .. } => {
                    blobs_protected_re_ref_count = blobs_protected_re_ref_count.saturating_add(1);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                }
                SweepDecision::AlreadyResolved { .. } => {
                    already_resolved_count = already_resolved_count.saturating_add(1);
                }
            }
        }

        // 4. Checkpoint counters in gc_run.
        let phase_end = self.clock.now_ms();
        let duration_ms = phase_end.saturating_sub(phase_start);
        self.runs.checkpoint(
            run_id,
            tenant_id,
            phase_end,
            CheckpointDeltas {
                blobs_swept_delta: blobs_swept_count,
                bytes_reclaimed_delta: bytes_to_be_reclaimed,
                ..Default::default()
            },
        )?;

        // 5. Phase duration histogram (canonical metric).
        self.metrics
            .record_phase_duration_ms(GcPhase::Sweep, tenant_id, region, duration_ms)?;

        Ok(SweepResult {
            mark_started_at_ms: mark_anchor,
            candidates_processed,
            blobs_swept_count,
            blobs_protected_re_ref_count,
            already_resolved_count,
            bytes_to_be_reclaimed,
            sweep_duration_ms: duration_ms,
            audit_events_emitted,
        })
    }
}
