use super::*;

/// In-memory physical-delete phase orchestrator (the canonical
/// pure-logic skeleton). Composes the trait dependencies declared at
/// construction time.
pub struct InMemoryPhysicalDeletePhase<S, C, B, R, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
{
    runs: Arc<S>,
    candidates: Arc<C>,
    blob_meta_purge: Arc<B>,
    r2: Arc<R>,
    audit: Arc<A>,
    metrics: Arc<M>,
    clock: Arc<K>,
    config: PhysicalDeleteConfig,
}

impl<S, C, B, R, A, M, K> core::fmt::Debug for InMemoryPhysicalDeletePhase<S, C, B, R, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryPhysicalDeletePhase")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, C, B, R, A, M, K> InMemoryPhysicalDeletePhase<S, C, B, R, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
{
    /// Construct a physical-delete phase orchestrator with the
    /// canonical defaults.
    pub fn with_defaults(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta_purge: Arc<B>,
        r2: Arc<R>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
    ) -> Self {
        Self {
            runs,
            candidates,
            blob_meta_purge,
            r2,
            audit,
            metrics,
            clock,
            config: PhysicalDeleteConfig::default(),
        }
    }

    /// Construct with an explicit [`PhysicalDeleteConfig`].
    #[allow(
        clippy::too_many_arguments,
        reason = "ctor wires 7 trait deps + 1 knob; collapsing into a builder \
                  hurts call-site clarity in the in-memory pure-logic tests."
    )]
    pub fn new(
        runs: Arc<S>,
        candidates: Arc<C>,
        blob_meta_purge: Arc<B>,
        r2: Arc<R>,
        audit: Arc<A>,
        metrics: Arc<M>,
        clock: Arc<K>,
        config: PhysicalDeleteConfig,
    ) -> Self {
        Self {
            runs,
            candidates,
            blob_meta_purge,
            r2,
            audit,
            metrics,
            clock,
            config,
        }
    }

    /// The [`PhysicalDeleteConfig`] snapshot.
    #[must_use]
    pub fn config(&self) -> PhysicalDeleteConfig {
        self.config
    }

    /// Classify a single candidate against the reclaim gate with **zero
    /// side effects** — no R2 DeleteObject, no D1 purge, no candidate
    /// transition, no audit emit. The only observable effect is one
    /// [`PhysicalDeleteClock::now_ms`] read (when the candidate reaches
    /// the grace gate), exactly as the live path consumes it.
    ///
    /// This is the single source of truth for the reclaim predicate:
    /// [`Self::step_candidate`] (live) routes through it, and the
    /// non-destructive dry-run sweep ([`crate::sweep_runner`]) calls it
    /// directly so a dry-run report provably matches what a live delete
    /// WOULD remove.
    ///
    /// # Errors
    ///
    /// Surface as [`PhysicalDeleteError`] (the `blob_meta` lookup is the
    /// only fallible backend call); fail-closed.
    pub fn classify_candidate(
        &self,
        candidate: &GcCandidate,
    ) -> Result<ReclaimClassification, PhysicalDeleteError> {
        // Idempotent re-run guard: only `Swept` candidates progress;
        // any other status is a no-op (mark→sweep→physical-delete
        // monotone status graph). No clock read on this arm.
        if candidate.status != CandidateStatus::Swept {
            return Ok(ReclaimClassification::NotSwept {
                observed_status: candidate.status,
            });
        }
        // 1. Lookup the soft-deleted blob_meta row's purge state. A
        // missing row means the sweep pre-condition is no longer
        // satisfied (e.g. customer re-uploaded same digest within the
        // grace window — CAP-GC-002 reversibility — and the CAS write
        // handler reset `deleted_at_ms = NULL`). Surface as Resolved.
        let Some(purge_state) = self
            .blob_meta_purge
            .lookup_purge_state(candidate.tenant_id, &candidate.digest)?
        else {
            return Ok(ReclaimClassification::Resolved {
                observed_status: candidate.status,
            });
        };
        let now = self.clock.now_ms();
        let grace_period_ms = self.config.grace_cas_ms;

        // 2. Post-grace gate (strict `>` per WI §6.1.3).
        if now.saturating_sub(purge_state.deleted_at_ms) <= grace_period_ms {
            return Ok(ReclaimClassification::GracePending {
                now_ms: now,
                deleted_at_ms: purge_state.deleted_at_ms,
                grace_period_ms,
            });
        }

        // 3. refcount = 0 guard (Lote 10.6bis P0-4 race protection) —
        // this is the live-blob protection: a re-referenced blob is
        // NEVER classified reclaimable.
        if purge_state.refcount != 0 {
            return Ok(ReclaimClassification::RefcountNonZero {
                refcount: purge_state.refcount,
            });
        }

        Ok(ReclaimClassification::Reclaimable {
            r2_key: purge_state.r2_key,
            size_bytes: purge_state.size_bytes,
            deleted_at_ms: purge_state.deleted_at_ms,
            now_ms: now,
            grace_period_ms,
        })
    }

    /// Process a single candidate row through the physical-delete
    /// decision pipeline. Visible for property tests so the decision
    /// boundary can be exercised independently of the phase
    /// orchestration.
    ///
    /// The reclaim gate is delegated to [`Self::classify_candidate`]
    /// (the single source of truth); only the
    /// [`ReclaimClassification::Reclaimable`] arm performs the mutating
    /// R2 + D1 + transition steps.
    ///
    /// # Errors
    ///
    /// Surface as [`PhysicalDeleteError`].
    pub fn step_candidate(
        &self,
        candidate: &GcCandidate,
        region: GcRegion,
    ) -> Result<PhysicalDeleteDecision, PhysicalDeleteError> {
        let (r2_key, size_bytes, now) = match self.classify_candidate(candidate)? {
            ReclaimClassification::NotSwept { observed_status }
            | ReclaimClassification::Resolved { observed_status } => {
                return Ok(PhysicalDeleteDecision::AlreadyResolved { observed_status });
            }
            ReclaimClassification::GracePending {
                now_ms,
                deleted_at_ms,
                grace_period_ms,
            } => {
                return Ok(PhysicalDeleteDecision::SkippedGracePending {
                    now_ms,
                    deleted_at_ms,
                    grace_period_ms,
                });
            }
            ReclaimClassification::RefcountNonZero { refcount } => {
                return Ok(PhysicalDeleteDecision::SkippedRefcountNonZero { refcount });
            }
            ReclaimClassification::Reclaimable {
                r2_key,
                size_bytes,
                now_ms,
                ..
            } => (r2_key, size_bytes, now_ms),
        };
        let grace_period_ms = self.config.grace_cas_ms;

        // Acquire the durable epoch before touching R2. A resumed
        // `R2Deleted` stage skips the remote call and proceeds directly to
        // the fenced metadata finalization; `None` means another worker owns
        // the lease (or the row was already finalized).
        let stage = self.blob_meta_purge.begin_purge(
            candidate.tenant_id,
            &candidate.digest,
            candidate.mark_run_id,
            now,
            grace_period_ms,
        )?;
        let Some(stage) = stage else {
            return Ok(PhysicalDeleteDecision::AlreadyResolved {
                observed_status: candidate.status,
            });
        };
        let (epoch, r2_outcome) = match stage {
            PurgeStage::Acquired { epoch } => {
                // 4. R2 DeleteObject FIRST (Lote 10.6bis P0-2 ordering;
                // PAT-RETRY-IDEMPOTENT-001 semantics — both `Deleted` and
                // `NotFound` are successes). Record a retry state if the
                // remote call fails, preserving the D1 row for resume.
                let outcome = match self.r2.delete(candidate.tenant_id, region, &r2_key) {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        let physical_error = PhysicalDeleteError::from(error);
                        self.blob_meta_purge.mark_r2_retry(
                            candidate.tenant_id,
                            &candidate.digest,
                            epoch,
                            now,
                            &physical_error.to_string(),
                        )?;
                        return Err(physical_error);
                    }
                };
                self.blob_meta_purge.mark_r2_deleted(
                    candidate.tenant_id,
                    &candidate.digest,
                    epoch,
                    now,
                )?;
                (epoch, outcome)
            }
            PurgeStage::R2Deleted { epoch } => (epoch, R2DeleteOutcome::NotFound),
        };

        // 5. Audit emit BEFORE flipping the row status — fail-closed
        // envelope (mirrors sweep) for OWN D1 mutations. Production
        // wiring atomically rolls back the D1 batch (DELETE blob_meta
        // + DELETE gc_candidate + INSERT audit_outbox) on emit
        // failure; the in-memory fake's lower fidelity is documented
        // in the module-level rustdoc.
        //
        // INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER scope clarification
        // (audit-ordering-high-risk-seal §Escalation 3, 2026-05-27):
        // R2 DeleteObject precedes the audit emit. The INV applies to
        // OWN-state mutations (D1 batch: DELETE blob_meta + DELETE
        // gc_candidate + INSERT audit_outbox); external R2 calls are
        // classified as external-API observability (same pattern as
        // `corelink-byok::byok_*::*::wrap_dek`, `corelink-slack-real`,
        // `corelink-statuspage-real::http`). Reasoning: R2 is
        // idempotent (NotFound is a success), so retry-after-emit
        // would either silently delete (success replay) or duplicate
        // the audit (failure replay). R2-first means the audit row
        // ALWAYS reflects the actual deletion state; the D1 batch
        // (step 6+7 below) atomically rolls back on emit failure,
        // preserving the canonical fail-CLOSED envelope around OWN
        // state.
        self.audit.emit(GcAuditRecord {
            event_type: GcEventType::PhysicalDeleted,
            run_id: candidate.mark_run_id,
            tenant_id: candidate.tenant_id,
            region,
            status: GcStatus::Running,
            from_phase: Some(GcPhase::PhysicalDelete),
            to_phase: Some(GcPhase::PhysicalDelete),
            created_by_request_id: "cron".to_owned(),
            reason: "physical_deleted",
            now_ms: now,
        })?;

        // 6. D1 conditional row purge (re-checks refcount=0 + grace
        // gate atomically per the SQL predicate). The epoch is part of
        // the fence, so a stale worker cannot finalize a newer retry.
        let purged = self.blob_meta_purge.finalize_purge(
            candidate.tenant_id,
            &candidate.digest,
            epoch,
            now,
            grace_period_ms,
        )?;
        if !purged {
            // Race window: the predicate was true at lookup time but
            // false at SQL evaluation time (customer CAS write
            // re-incremented refcount post-lookup, OR clock ran
            // backwards). Surface as a skipped decision so the
            // candidate row is preserved. The classified refcount was 0
            // (we reached the Reclaimable arm); the race re-incremented
            // it at SQL evaluation time.
            return Ok(PhysicalDeleteDecision::SkippedRefcountNonZero { refcount: 0 });
        }

        // 7. Atomic candidate transition; if drift detected (concurrent
        // re-run), observe and surface AlreadyResolved.
        let fired = self.candidates.transition_status(
            candidate.tenant_id,
            &candidate.digest,
            candidate.mark_run_id,
            CandidateStatus::Swept,
            CandidateStatus::PhysicallyDeleted,
            now,
            None,
        )?;
        if !fired {
            // Concurrent winner; observe and surface AlreadyResolved.
            let observed = self
                .candidates
                .lookup(
                    candidate.tenant_id,
                    &candidate.digest,
                    candidate.mark_run_id,
                )?
                .map_or(CandidateStatus::Swept, |c| c.status);
            // The fenced D1 finalization trigger performs this candidate
            // transition in the same transaction as the metadata DELETE.
            // In that production path our explicit transition observes the
            // already-committed terminal state; retain the successful purge
            // accounting instead of misclassifying it as a concurrent no-op.
            if observed == CandidateStatus::PhysicallyDeleted {
                return Ok(PhysicalDeleteDecision::Purged {
                    purged_at_ms: now,
                    bytes_reclaimed: size_bytes,
                    r2_outcome,
                });
            }
            return Ok(PhysicalDeleteDecision::AlreadyResolved {
                observed_status: observed,
            });
        }
        Ok(PhysicalDeleteDecision::Purged {
            purged_at_ms: now,
            bytes_reclaimed: size_bytes,
            r2_outcome,
        })
    }
}

impl<S, C, B, R, A, M, K> PhysicalDeletePhase for InMemoryPhysicalDeletePhase<S, C, B, R, A, M, K>
where
    S: GcRunStore,
    C: GcCandidatesStore,
    B: BlobMetaPurgeStore,
    R: R2Delete,
    A: GcAuditSink,
    M: GcMetricsObserver,
    K: PhysicalDeleteClock,
{
    fn execute(
        &self,
        run_id: RunId,
        tenant_id: Uuid,
        region: GcRegion,
    ) -> Result<PhysicalDeleteResult, PhysicalDeleteError> {
        // 1. Read gc_run + verify region match.
        let run_row = self
            .runs
            .lookup(run_id, tenant_id)?
            .ok_or(PhysicalDeleteError::RunStore(GcRunStoreError::NotFound(
                run_id,
            )))?;
        if run_row.region != region {
            return Err(PhysicalDeleteError::RegionMismatch {
                run_region: run_row.region,
                caller_region: region,
            });
        }

        // 2. Phase budget deadline.
        let phase_start = self.clock.now_ms();
        let deadline_ms = phase_start.saturating_add(self.config.phase_budget_ms);

        // 3. Iterate candidates for the run.
        let candidates = self.candidates.snapshot_for_run(tenant_id, run_id)?;
        let mut blobs_deleted_count: u64 = 0;
        let mut blobs_skipped_grace_pending: u64 = 0;
        let mut blobs_skipped_refcount_non_zero: u64 = 0;
        let mut already_resolved_count: u64 = 0;
        let mut bytes_reclaimed: u64 = 0;
        let mut audit_events_emitted: u64 = 0;
        let mut candidates_processed: u64 = 0;
        for candidate in &candidates {
            // Phase-budget probe per candidate (cheaper than per
            // mutation; aligns with sweep phase pattern).
            let now_for_probe = self.clock.now_ms();
            if now_for_probe > deadline_ms {
                return Err(PhysicalDeleteError::PhaseBudgetExceeded {
                    duration_ms: now_for_probe.saturating_sub(phase_start),
                    budget_ms: self.config.phase_budget_ms,
                });
            }
            let decision = self.step_candidate(candidate, region)?;
            candidates_processed = candidates_processed.saturating_add(1);
            match decision {
                PhysicalDeleteDecision::Purged {
                    bytes_reclaimed: br,
                    ..
                } => {
                    blobs_deleted_count = blobs_deleted_count.saturating_add(1);
                    bytes_reclaimed = bytes_reclaimed.saturating_add(br);
                    audit_events_emitted = audit_events_emitted.saturating_add(1);
                }
                PhysicalDeleteDecision::SkippedGracePending { .. } => {
                    blobs_skipped_grace_pending = blobs_skipped_grace_pending.saturating_add(1);
                }
                PhysicalDeleteDecision::SkippedRefcountNonZero { .. } => {
                    blobs_skipped_refcount_non_zero =
                        blobs_skipped_refcount_non_zero.saturating_add(1);
                }
                PhysicalDeleteDecision::AlreadyResolved { .. } => {
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
                blobs_physically_deleted_delta: blobs_deleted_count,
                bytes_reclaimed_delta: bytes_reclaimed,
                ..Default::default()
            },
        )?;

        // 5. Phase duration histogram (canonical metric).
        self.metrics.record_phase_duration_ms(
            GcPhase::PhysicalDelete,
            tenant_id,
            region,
            duration_ms,
        )?;

        Ok(PhysicalDeleteResult {
            candidates_processed,
            blobs_deleted_count,
            blobs_skipped_grace_pending,
            blobs_skipped_refcount_non_zero,
            already_resolved_count,
            bytes_reclaimed,
            phase_duration_ms: duration_ms,
            audit_events_emitted,
        })
    }
}
