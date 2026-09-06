use super::*;

impl PilotHarness {
    // -------------------------------------------------------------------
    // Stage 4 — DSR erasure
    // -------------------------------------------------------------------

    /// Submit a DSR erasure request for `tenant`. The request is
    /// accepted immediately; completion is gated on the 7-day SLA
    /// window via [`PilotHarness::finalise_dsr_erasure`].
    ///
    /// # Errors
    ///
    /// - [`DsrErasureError::TenantNotProvisioned`] if signup never ran.
    /// - [`DsrErasureError::AlreadyRequested`] if a request was
    ///   already submitted (idempotent rejection).
    pub fn request_dsr_erasure(
        &self,
        tenant: &PilotTenant,
        request_id: &str,
    ) -> Result<DsrErasureReceipt, PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(DsrErasureError::TenantNotProvisioned(tenant.slug.clone()).into());
            }
            if state.erasure_requests.contains(request_id) {
                return Err(DsrErasureError::AlreadyRequested(request_id.to_owned()).into());
            }

            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::DsrErasureRequested,
                FIXED_NOW_MS,
                serde_json::json!({"request_id": request_id}),
            )?;
            state.erasure_requests.insert(request_id.to_owned());
            state.erasure_requested_at_ms = Some(FIXED_NOW_MS);

            Ok(DsrErasureReceipt {
                request_id: request_id.to_owned(),
                sla_deadline_ms: FIXED_NOW_MS.saturating_add(DSR_ERASURE_WINDOW_MS),
            })
        })
    }

    /// Finalise a previously-accepted DSR erasure at wall-clock
    /// `now_ms`. The harness enforces that the 7-day SLA window has
    /// elapsed and then drains every CAS blob for the tenant and
    /// flushes the audit chain down to the genesis row plus the
    /// closing `DsrErasureCompleted` event.
    ///
    /// # Errors
    ///
    /// - [`DsrErasureError::TenantNotProvisioned`] if signup never ran.
    /// - [`DsrErasureError::SlaWindowOpen`] if `now_ms` is before the
    ///   request's SLA deadline.
    pub fn finalise_dsr_erasure(
        &self,
        tenant: &PilotTenant,
        now_ms: u64,
    ) -> Result<(), PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(DsrErasureError::TenantNotProvisioned(tenant.slug.clone()).into());
            }
            let requested_at = match state.erasure_requested_at_ms {
                Some(v) => v,
                None => {
                    return Err(DsrErasureError::TenantNotProvisioned(tenant.slug.clone()).into())
                }
            };
            let deadline = requested_at.saturating_add(DSR_ERASURE_WINDOW_MS);
            if now_ms < deadline {
                return Err(DsrErasureError::SlaWindowOpen {
                    remaining_ms: deadline.saturating_sub(now_ms),
                }
                .into());
            }

            // Emit the completion audit BEFORE the drain so the audit
            // chain itself contains the explicit completion marker
            // before being wiped.
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::DsrErasureCompleted,
                now_ms,
                serde_json::json!({
                    "cas_blobs_erased": state.cas.len(),
                    "audit_rows_redacted": state.audit_rows.len(),
                }),
            )?;

            // Drain CAS.
            state.cas.clear();
            // Audit chain is reduced to a single tombstone row: the
            // DsrErasureCompleted event. Per CTRL-PRIV-ERASURE the
            // event MUST survive for compliance evidence, but every
            // prior data-plane row is redacted.
            let last = state.audit_rows.pop();
            state.audit_rows.clear();
            if let Some(mut tombstone) = last {
                // Re-anchor the tombstone to the zero prev-hash so the
                // single-row chain remains internally consistent.
                let genesis = "0".repeat(64);
                let canon = serde_json::to_vec(&serde_json::json!({
                    "tenant_id": tombstone.tenant_id,
                    "seq": 0u64,
                    "ts_ms": tombstone.ts_ms,
                    "kind": tombstone.kind,
                    "detail": tombstone.detail,
                }))
                .map_err(|_| PilotHarnessError::Invariant("tombstone canon serialize"))?;
                let prev_bytes = hex::decode(&genesis)
                    .map_err(|_| PilotHarnessError::Invariant("tombstone prev hex"))?;
                let mut hasher = blake3::Hasher::new();
                hasher.update(&prev_bytes);
                hasher.update(&canon);
                tombstone.seq = 0;
                tombstone.prev_hash_hex = genesis;
                tombstone.row_hash_hex = hasher.finalize().to_hex().to_string();
                state.audit_rows.push(tombstone);
            }

            Ok(())
        })
    }

    // -------------------------------------------------------------------
    // Stage 5 — offboarding
    // -------------------------------------------------------------------

    /// Cancel the subscription for `tenant`. Pre-condition: the
    /// subscription must be currently `Active`.
    ///
    /// # Errors
    ///
    /// - [`OffboardingError::TenantNotProvisioned`] if signup never ran.
    /// - [`OffboardingError::SubscriptionNotCancelled`] is never
    ///   returned here — this is the *cancelling* path.
    pub fn cancel_subscription(&self, tenant: &PilotTenant) -> Result<(), PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(OffboardingError::TenantNotProvisioned(tenant.slug.clone()).into());
            }
            // Audit BEFORE state flip.
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::SubscriptionCancelled,
                FIXED_NOW_MS,
                serde_json::json!({"subscription_id": tenant.subscription_id}),
            )?;
            state.subscription = SubscriptionState::Cancelled;
            state.cancelled_at_ms = Some(FIXED_NOW_MS);
            state.lifecycle = Some(TenantLifecycleState::CancelledInGrace {
                grace_ends_at_ms: FIXED_NOW_MS.saturating_add(OFFBOARDING_GRACE_MS),
            });
            Ok(())
        })
    }

    /// Complete offboarding at wall-clock `now_ms`. Enforces the
    /// 30-day grace window and then performs the final hard-delete +
    /// erasure verification: every CAS blob and every audit row MUST
    /// be empty.
    ///
    /// # Errors
    ///
    /// See variants of [`OffboardingError`].
    pub fn complete_offboarding(
        &self,
        tenant: &PilotTenant,
        now_ms: u64,
    ) -> Result<OffboardingReceipt, PilotHarnessError> {
        self.with_tenant(&tenant.slug, |state| {
            if state.lifecycle.is_none() {
                return Err(OffboardingError::TenantNotProvisioned(tenant.slug.clone()).into());
            }
            let cancelled_at = match state.cancelled_at_ms {
                Some(v) => v,
                None => {
                    return Err(
                        OffboardingError::SubscriptionNotCancelled(tenant.slug.clone()).into(),
                    )
                }
            };
            let deadline = cancelled_at.saturating_add(OFFBOARDING_GRACE_MS);
            if now_ms < deadline {
                return Err(OffboardingError::GraceNotElapsed {
                    remaining_ms: deadline.saturating_sub(now_ms),
                }
                .into());
            }

            // The hard-delete event is emitted BEFORE the wipe
            // (fail-CLOSED) so a downstream auditor can prove the
            // deletion happened — but it is NOT counted toward the
            // post-wipe residual.
            Self::emit_audit(
                state,
                &tenant.tenant_id,
                AuditEventKind::TenantHardDeleted,
                now_ms,
                serde_json::json!({
                    "cas_blobs_at_delete": state.cas.len(),
                    "audit_rows_at_delete": state.audit_rows.len(),
                }),
            )?;
            state.cas.clear();
            state.audit_rows.clear();
            state.lifecycle = Some(TenantLifecycleState::HardDeleted);

            // INV-OFFBOARDING-CLEAN: post-wipe residual MUST be zero
            // across both planes. The hard-delete event is wiped too —
            // its evidentiary copy lives in the external audit sink
            // (mirrored at the emission point above).
            let residual_cas = state.cas.len();
            let residual_audit_rows = state.audit_rows.len();
            if residual_cas != 0 {
                return Err(OffboardingError::ResidualCasData(residual_cas).into());
            }
            if residual_audit_rows != 0 {
                return Err(OffboardingError::ResidualAuditRows(residual_audit_rows).into());
            }

            Ok(OffboardingReceipt {
                tenant_id: tenant.tenant_id.clone(),
                deleted_at_ms: now_ms,
                residual_cas_blobs: 0,
                residual_audit_rows: 0,
            })
        })
    }
}
