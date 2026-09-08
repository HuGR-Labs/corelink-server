// ─── D1CustomerHandler ────────────────────────────────────────────────────────

/// Self-serve PAT TTL: 90 days (the canonical rotation cadence from
/// migration 0037's `pat.expires_ms` contract).
const SELF_SERVE_PAT_TTL: Duration = Duration::from_secs(90 * 86_400);

/// Max customer-facing audit rows returned by `GET /v1/customer/audit`
/// (newest-first). Bounds the row source (migration 0077) so a long-lived
/// tenant's activity log can never return an unbounded page.
const AUDIT_QUERY_LIMIT: i64 = 100;

/// Estimated wall-clock seconds a single cache hit saves — a hit avoids
/// re-executing ~one build action. Feeds the DISPLAYED build-time-saved
/// estimate on the customer usage surface; NEVER used to bill or gate.
const SECONDS_SAVED_PER_HIT: f64 = 15.0;

/// Estimated USD per compute-second (~$0.04/vCPU-hour) — deliberately
/// conservative. Feeds the DISPLAYED $-saved estimate; NEVER used to bill.
const USD_PER_COMPUTE_SECOND: f64 = 0.000_011_1;

/// Aggregate of one period's `usage_daily` rows (migration 0089): the summed
/// counters plus the per-day `{day, reads, writes}` buckets. DISPLAY telemetry
/// only.
#[derive(Debug, Default)]
struct UsageRollup {
    /// Summed cache reads across the period.
    reads: u64,
    /// Summed cache writes across the period.
    writes: u64,
    /// Summed cache read HITS across the period.
    hits: u64,
    /// Summed cache read MISSES across the period.
    misses: u64,
    /// Per-day buckets, oldest-first (`cas_bytes` always 0 — no per-day byte
    /// history exists).
    daily: Vec<DailyUsageBucket>,
}

/// Production D1-backed customer handler. Implements all 6
/// `corelink-handler-customer` traits over the [`CustomerD1`] seam.
/// See the module docs for the per-endpoint HONEST-v1 matrix.
#[non_exhaustive]
pub struct D1CustomerHandler {
    /// D1 row source (production: [`D1HttpCustomerDb`]).
    db: Arc<dyn CustomerD1>,
    /// Stripe billing-portal creator; `None` when `STRIPE_SECRET_KEY`
    /// is absent (portal requests then fail CLOSED with 500, never a
    /// fabricated URL).
    portal: Option<Arc<dyn PortalSessions>>,
    /// `return_url` for portal sessions (the dashboard billing page).
    portal_return_url: String,
    /// PAT signing key + key generation; `None` when `PAT_SIGNING_KEY`
    /// is absent (key creation then fails CLOSED with 500).
    signing: Option<(Arc<PatSigningKey>, u32)>,
    /// Audit sink collaborator (fail-CLOSED ordering per trait docs).
    audit: Arc<dyn AuditSink>,
    /// SLI observer collaborator (emit on EVERY return path).
    sli: Arc<dyn SliObserver>,
    /// Wall clock — the routes pass `at_unix_ms = 0`, so every real
    /// timestamp comes from here.
    clock: Arc<dyn WallClock>,
}

impl core::fmt::Debug for D1CustomerHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Redaction marker only: the signing key + Stripe bearer must
        // never surface through a leaked Debug.
        f.debug_struct("D1CustomerHandler")
            .field("db", &"[CustomerD1]")
            .field("portal", &self.portal.is_some())
            .field("signing", &self.signing.is_some())
            .finish_non_exhaustive()
    }
}

impl D1CustomerHandler {
    /// Construct from explicit collaborators (production wiring + tests).
    #[must_use]
    pub fn new(
        db: Arc<dyn CustomerD1>,
        portal: Option<Arc<dyn PortalSessions>>,
        portal_return_url: String,
        signing: Option<(Arc<PatSigningKey>, u32)>,
        audit: Arc<dyn AuditSink>,
        sli: Arc<dyn SliObserver>,
        clock: Arc<dyn WallClock>,
    ) -> Self {
        Self {
            db,
            portal,
            portal_return_url,
            signing,
            audit,
            sli,
            clock,
        }
    }

    /// Build the production handler from process env. `None` when the
    /// D1 config ([`crate::storage::StorageEnv`]) is absent/invalid —
    /// the caller then keeps the InMemory handler (dev/CI), mirroring
    /// [`crate::adapter_pat::PatVerifier::from_env`]'s fail-closed
    /// pattern. The Stripe client and PAT signing key are OPTIONAL
    /// per-capability inputs: when absent, only billing-portal /
    /// key-creation fail CLOSED (500) while every read surface stays
    /// real.
    #[must_use]
    pub fn from_env() -> Option<Arc<Self>> {
        let storage_env = crate::storage::StorageEnv::from_env()?;
        let d1 = D1HttpClient::new(&storage_env)
            .map_err(|e| tracing::warn!(error = %e, "customer_d1: D1 client init failed"))
            .ok()?;

        // PAT signing key — optional capability (key creation).
        let signing = crate::storage::non_empty_env("PAT_SIGNING_KEY")
            .and_then(|hex_str| {
                hex::decode(hex_str.trim())
                    .map_err(|_| {
                        tracing::warn!(
                            "customer_d1: PAT_SIGNING_KEY is not valid hex; \
                             self-serve key creation will fail CLOSED (500)"
                        );
                    })
                    .ok()
            })
            .and_then(|bytes| {
                PatSigningKey::from_bytes(bytes)
                    .map_err(|e| {
                        tracing::warn!(
                            error = %e,
                            "customer_d1: PAT_SIGNING_KEY invalid; \
                             self-serve key creation will fail CLOSED (500)"
                        );
                    })
                    .ok()
            })
            .map(|key| (Arc::new(key), 1u32));
        if signing.is_none() {
            tracing::warn!(
                "customer_d1: PAT_SIGNING_KEY unset/invalid; \
                 POST /v1/customer/keys will fail CLOSED (500)"
            );
        }

        // Stripe client — optional capability (billing portal).
        let portal: Option<Arc<dyn PortalSessions>> =
            match corelink_stripe_real::StripeRealClient::from_env() {
                Ok(stripe) => Some(Arc::new(StripePortalSessions::new(Arc::new(stripe)))),
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "customer_d1: Stripe client init failed; \
                         POST /v1/customer/billing/portal will fail CLOSED (500)"
                    );
                    None
                }
            };

        let portal_return_url = std::env::var("CORELINK_PORTAL_RETURN_URL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "https://humangr.com/corelink/en/customer/billing".to_owned());

        Some(Arc::new(Self::new(
            Arc::new(D1HttpCustomerDb::new(Arc::new(d1))),
            portal,
            portal_return_url,
            signing,
            Arc::new(TracingCustomerAuditSink::new()),
            Arc::new(TracingCustomerSliObserver::new()),
            crate::wall_clock::default_wall_clock(),
        )))
    }

    // ── Shared plumbing ───────────────────────────────────────────────────────

    /// Emit one `Sli::AvailControlPlane` observation.
    fn emit_sli(&self, is_error: bool) {
        self.sli
            .observe(SliObservation::new(Sli::AvailControlPlane, is_error, 0));
    }

    /// Emit an audit event (wall-clock timestamped — NEVER the request's
    /// `at_unix_ms`, which the routes pin to 0); `AuditFailed` +
    /// error-SLI on sink failure (fail-CLOSED, BEFORE any lookup).
    fn emit_audit(
        &self,
        kind: AuditEventKind,
        tenant: &str,
        principal: &str,
        resource: &str,
    ) -> Result<(), CustomerHandlerError> {
        self.audit
            .emit(AuditEvent::new(
                kind,
                tenant,
                principal,
                resource,
                self.clock.now_ms(),
            ))
            .map_err(|e| {
                self.emit_sli(true);
                CustomerHandlerError::AuditFailed(e)
            })
    }

    /// Enforce the customer route tenant boundary before any D1 read/write.
    /// The `*Denied` audit is the linearization point for a cross-tenant
    /// rejection: only after it succeeds may `CrossTenantDenied` be returned.
    pub(crate) fn reject_cross_tenant(
        &self,
        requested_tenant: Option<&str>,
        caller_tenant: &str,
        kind: AuditEventKind,
        principal: &str,
        resource: &str,
    ) -> Result<(), CustomerHandlerError> {
        let requested = requested_tenant.unwrap_or(caller_tenant);
        if requested == caller_tenant {
            return Ok(());
        }
        self.emit_audit(kind, requested, principal, resource)?;
        self.emit_sli(true);
        Err(CustomerHandlerError::CrossTenantDenied {
            caller: caller_tenant.to_owned(),
            requested_tenant: requested.to_owned(),
        })
    }

    /// Run one parameterised D1 statement; fail-CLOSED: any transport /
    /// decode error maps to `Internal` (→ 500) + error SLI. NEVER
    /// degraded to fabricated empty data.
    fn run(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, CustomerHandlerError> {
        self.db.query(sql, binds).map_err(|e| {
            self.emit_sli(true);
            CustomerHandlerError::Internal(format!("customer_d1: {e}"))
        })
    }

    /// Preserve fail-CLOSED error semantics for the typed D1 mutation batch.
    fn atomic_error(&self, error: CustomerAtomicError) -> CustomerHandlerError {
        self.emit_sli(true);
        match error {
            CustomerAtomicError::Audit(e) | CustomerAtomicError::Transport(e) => {
                tracing::error!(error = %e, "customer_d1: atomic audit transaction failed");
                CustomerHandlerError::AuditFailed(format!("customer audit transaction failed: {e}"))
            }
            CustomerAtomicError::Mutation(e) => {
                tracing::error!(error = %e, "customer_d1: atomic handler transaction failed");
                CustomerHandlerError::Internal(format!("customer_d1: {e}"))
            }
            CustomerAtomicError::Unsupported(e) => {
                tracing::error!(error = %e, "customer_d1: typed atomic operation unavailable");
                CustomerHandlerError::Internal(format!("customer_d1: {e}"))
            }
        }
    }

    /// Fetch the tenant row (`tier` / `clerk_user_id` / `byok_status` /
    /// `created_at_ms`); `Ok(None)` when the tenant does not exist.
    fn tenant_row(&self, tenant_id: &str) -> Result<Option<D1Row>, CustomerHandlerError> {
        let rows = self.run(
            "SELECT tenant_id, tier, clerk_user_id, byok_status, created_at_ms \
             FROM tenant WHERE tenant_id = ?1 LIMIT 1",
            vec![json!(tenant_id)],
        )?;
        Ok(rows.into_iter().next())
    }

    /// Tenant-not-found error (the canonical 404 shape).
    fn tenant_not_found(&self, tenant_id: &str) -> CustomerHandlerError {
        self.emit_sli(true);
        CustomerHandlerError::NotFound {
            what: format!("tenant={tenant_id}"),
        }
    }

    /// Real storage usage: `SUM(bytes_used)` across the tenant's
    /// per-region rows + the `MAX(bytes_quota)` snapshot (the quota is
    /// denormalized per region row; `MAX` avoids multiplying it by the
    /// region count). Zero rows → honest (0, 0).
    fn storage_usage(&self, tenant_id: &str) -> Result<(u64, u64), CustomerHandlerError> {
        let rows = self.run(
            "SELECT COALESCE(SUM(bytes_used), 0) AS bytes_used, \
                    COALESCE(MAX(bytes_quota), 0) AS bytes_quota \
             FROM tenant_storage_state WHERE tenant_id = ?1",
            vec![json!(tenant_id)],
        )?;
        let row = rows.into_iter().next().unwrap_or_default();
        let used = col_opt_i64(&row, "bytes_used").unwrap_or(0);
        let quota = col_opt_i64(&row, "bytes_quota").unwrap_or(0);
        Ok((
            u64::try_from(used).unwrap_or(0),
            u64::try_from(quota).unwrap_or(0),
        ))
    }

    /// `tenant_billing` row (0055), if any.
    fn billing_row(&self, tenant_id: &str) -> Result<Option<D1Row>, CustomerHandlerError> {
        let rows = self.run(
            "SELECT status, plan, stripe_customer_id, current_period_end_ms \
             FROM tenant_billing WHERE tenant_id = ?1 LIMIT 1",
            vec![json!(tenant_id)],
        )?;
        Ok(rows.into_iter().next())
    }

    /// BYOK status for the dashboard. HONEST mapping: a tenant with NO
    /// `byok_envelope` row has no BYOK configured → `"none"` — the
    /// `tenant.byok_status` column (0031) defaults to `'active'` for
    /// every tenant because it is the kill-switch state, NOT a
    /// "customer configured BYOK" flag, so it is only surfaced once an
    /// envelope exists.
    fn byok_status(
        &self,
        tenant_id: &str,
        tenant: Option<&D1Row>,
    ) -> Result<ByokStatus, CustomerHandlerError> {
        let envelope = self.run(
            "SELECT 1 AS present FROM byok_envelope WHERE tenant_id = ?1 LIMIT 1",
            vec![json!(tenant_id)],
        )?;
        if envelope.is_empty() {
            return Ok(ByokStatus::new("none", None, None));
        }
        let status = tenant
            .and_then(|row| col_opt_str(row, "byok_status"))
            .unwrap_or_else(|| "active".to_owned());
        Ok(ByokStatus::new(status, None, None))
    }

    /// Newest-first customer activity for the overview snapshot (dashboard-revival
    /// BE-3). Reads the same `customer_audit_events` (0077) surface the audit
    /// endpoint serves — written UNSKIPPABLE / fail-CLOSED by the control-plane
    /// mutations (`keys create` → `pat.created`, `team invite` → `team.invited`) — bounded
    /// to `limit`, tenant-scoped, newest-first. Fail-CLOSED on transport (never a
    /// fabricated row; an honestly-empty feed stays empty).
    fn recent_activity(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<CustomerAuditEventRow>, CustomerHandlerError> {
        let rows = self.run(
            "SELECT id, event_type, actor, target, ts_ms, detail \
             FROM customer_audit_events WHERE tenant_id = ?1 \
             ORDER BY ts_ms DESC LIMIT ?2",
            vec![json!(tenant_id), json!(limit)],
        )?;
        Ok(rows
            .iter()
            .map(|row| {
                CustomerAuditEventRow::new(
                    col_opt_i64(row, "id")
                        .map(|i| i.to_string())
                        .unwrap_or_default(),
                    col_opt_i64(row, "ts_ms")
                        .map(ms_to_iso8601)
                        .unwrap_or_default(),
                    col_opt_str(row, "event_type").unwrap_or_default(),
                    // No per-event severity column — informational activity only.
                    "info",
                    col_opt_str(row, "actor").unwrap_or_default(),
                    col_opt_str(row, "detail").unwrap_or_default(),
                )
            })
            .collect())
    }

    /// The billable request count for `(tenant, year_month)` from
    /// `monthly_request_counts` (migration 0071) — the running counter the quota
    /// gate already increments per request. READ-ONLY (never writes the hot-path
    /// counter); `0` when no row exists (a period with no requests yet). The
    /// `year_month` key format (`YYYY-MM`, UTC) matches [`period_from_ms`] and
    /// the writer's `request_count::year_month_utc`, so a period lookup aligns.
    fn monthly_request_count(
        &self,
        tenant_id: &str,
        year_month: &str,
    ) -> Result<u64, CustomerHandlerError> {
        let rows = self.run(
            "SELECT request_count FROM monthly_request_counts \
             WHERE tenant_id = ?1 AND year_month = ?2 LIMIT 1",
            vec![json!(tenant_id), json!(year_month)],
        )?;
        Ok(rows
            .first()
            .and_then(|r| col_opt_i64(r, "request_count"))
            .and_then(|c| u64::try_from(c).ok())
            .unwrap_or(0))
    }

    /// Per-period usage rollup for `(tenant, year_month)` from `usage_daily`
    /// (migration 0089) — the DISPLAY telemetry feeding the dashboard ROI
    /// surface (BE-1 reads/writes/daily + BE-2 hit-rate / $-saved). A READ-ONLY
    /// period scan (`day LIKE 'YYYY-MM-%'`, `ORDER BY day`); it NEVER writes the
    /// hot-path counters. Fail-CLOSED on transport like
    /// [`Self::monthly_request_count`]: a transport/decode error propagates
    /// (→ 500) rather than degrading to a fabricated empty rollup. An honestly
    /// empty result set (no rows for the period) is a zeroed rollup with an
    /// empty daily series. `cas_bytes` is 0 in every bucket — no per-day byte
    /// history exists.
    fn usage_daily_rollup(
        &self,
        tenant_id: &str,
        year_month: &str,
    ) -> Result<UsageRollup, CustomerHandlerError> {
        // Escape LIKE metacharacters even though the period normally comes
        // from our YYYY-MM formatter. This helper is also reachable from the
        // customer API query surface, so a crafted period cannot turn the
        // bounded month scan into a wildcard/table-wide scan.
        let escaped_period = year_month
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let rows = self.run(
            "SELECT day, reads, writes, hits, misses FROM usage_daily \
             WHERE tenant_id = ?1 AND day LIKE ?2 ESCAPE '\\' ORDER BY day",
            vec![json!(tenant_id), json!(format!("{escaped_period}-%"))],
        )?;
        let mut roll = UsageRollup::default();
        for row in &rows {
            let reads = col_u64(row, "reads");
            let writes = col_u64(row, "writes");
            roll.reads = roll.reads.saturating_add(reads);
            roll.writes = roll.writes.saturating_add(writes);
            roll.hits = roll.hits.saturating_add(col_u64(row, "hits"));
            roll.misses = roll.misses.saturating_add(col_u64(row, "misses"));
            let day = col_opt_str(row, "day").unwrap_or_default();
            // cas_bytes = 0: no per-day byte history exists (0089 has no byte col).
            roll.daily
                .push(DailyUsageBucket::new(day, reads, writes, 0));
        }
        Ok(roll)
    }
}

// ─── Trait impls ──────────────────────────────────────────────────────────────
