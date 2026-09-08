impl CustomerOverviewHandler for D1CustomerHandler {
    fn overview(&self, req: OverviewRequest) -> Result<OverviewResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::OverviewDenied,
            &req.principal,
            "",
        )?;
        // Attempted audit BEFORE any lookup (fail-CLOSED ordering).
        self.emit_audit(
            AuditEventKind::OverviewAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;

        let tenant = self
            .tenant_row(&req.caller_tenant)?
            .ok_or_else(|| self.tenant_not_found(&req.caller_tenant))?;
        let (cas_bytes, quota_bytes) = self.storage_usage(&req.caller_tenant)?;
        let billing = self.billing_row(&req.caller_tenant)?;
        let byok = self.byok_status(&req.caller_tenant, Some(&tenant))?;
        // BE-3: the newest few control-plane events for the snapshot feed
        // (honestly empty for a brand-new tenant — never a fabricated row).
        let recent = self.recent_activity(&req.caller_tenant, 8)?;

        let plan = col_opt_str(&tenant, "tier").unwrap_or_else(|| "free".to_owned());
        let next_invoice_at = billing
            .as_ref()
            .and_then(|b| col_opt_i64(b, "current_period_end_ms"))
            .map(ms_to_iso8601)
            .unwrap_or_default();
        let billing_status = map_billing_status(
            billing
                .as_ref()
                .and_then(|b| col_opt_str(b, "status"))
                .as_deref(),
        );

        let resp = OverviewResponse::new(
            req.caller_tenant.clone(),
            // HONEST v1: no display-name column exists; the tenant id IS
            // the name (never a fabricated company name).
            req.caller_tenant.clone(),
            plan,
            OverviewUsage::new(
                period_from_ms(self.clock.now_ms()),
                cas_bytes,
                quota_bytes,
                // reads/writes are not tracked per-tenant yet: honest 0.
                0,
                0,
            ),
            // amount_due_cents is not materialized in D1: honest 0.
            OverviewBilling::new(billing_status, next_invoice_at, 0, "usd"),
            byok,
            recent,
        );

        self.emit_audit(
            AuditEventKind::OverviewServed,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;
        self.emit_sli(false);
        Ok(resp)
    }
}

impl CustomerUsageHandler for D1CustomerHandler {
    fn usage(&self, req: UsageRequest) -> Result<UsageResponse, CustomerHandlerError> {
        self.reject_cross_tenant(
            req.requested_tenant.as_deref(),
            &req.caller_tenant,
            AuditEventKind::UsageDenied,
            &req.principal,
            "",
        )?;
        self.emit_audit(
            AuditEventKind::UsageAttempted,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;

        if self.tenant_row(&req.caller_tenant)?.is_none() {
            return Err(self.tenant_not_found(&req.caller_tenant));
        }
        let (cas_bytes, quota_bytes) = self.storage_usage(&req.caller_tenant)?;

        let current_period = period_from_ms(self.clock.now_ms());
        let requested = req.period.clone().unwrap_or_else(|| current_period.clone());
        // HONEST: only the CURRENT period's running counter exists in
        // `tenant_storage_state`; a historical period has no retained
        // data and reports 0 bytes rather than mislabeling today's.
        let period_bytes = if requested == current_period {
            cas_bytes
        } else {
            0
        };

        // Billable request count for the period (monthly_request_counts, 0071):
        // a real usage-vs-quota signal. The quota gate already increments this
        // per request, so surfacing it is a READ — no hot-path write added.
        let request_count = self.monthly_request_count(&req.caller_tenant, &requested)?;

        // BE-1 + BE-2: real reads/writes/daily + cache hit-rate + estimated
        // build-time / $ saved from usage_daily (0089). DISPLAY telemetry — a
        // READ-ONLY period scan, never a hot-path write, fail-CLOSED on transport.
        let rollup = self.usage_daily_rollup(&req.caller_tenant, &requested)?;

        // hit_rate: fraction 0.0..=1.0; None when there were no cache reads at
        // all (hits + misses == 0) — an honest "no data", NEVER a fabricated rate.
        let cache_reads = rollup.hits.saturating_add(rollup.misses);
        let hit_rate = if cache_reads == 0 {
            None
        } else {
            Some(rollup.hits as f64 / cache_reads as f64)
        };

        // Estimated build-time / compute-cost saved (DISPLAYED AS AN ESTIMATE):
        //   time_saved_seconds   = hits * SECONDS_SAVED_PER_HIT
        //   dollars_saved_cents  = round(time_saved_seconds * USD_PER_COMPUTE_SECOND * 100)
        let seconds_saved = rollup.hits as f64 * SECONDS_SAVED_PER_HIT;
        let time_saved_seconds = seconds_saved as u64;
        let dollars_saved_cents = (seconds_saved * USD_PER_COMPUTE_SECOND * 100.0).round() as u64;

        let resp = UsageResponse::new(
            requested,
            period_bytes,
            rollup.reads,
            rollup.writes,
            quota_bytes,
            rollup.daily,
            request_count,
            hit_rate,
            time_saved_seconds,
            dollars_saved_cents,
        );

        self.emit_audit(
            AuditEventKind::UsageServed,
            &req.caller_tenant,
            &req.principal,
            "",
        )?;
        self.emit_sli(false);
        Ok(resp)
    }
}
