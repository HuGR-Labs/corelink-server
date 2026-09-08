/// Production [`QuotaStore`] over the `tenant_quota` D1 table (migration
/// 0066), reached via the async [`crate::storage::d1_http::D1HttpClient`].
///
/// All SQL is parameterised (positional binds); the tenant scope rides
/// in `WHERE tenant_id = ?1` on every statement (INV-TENANT-ISOLATION).
#[derive(Debug)]
#[non_exhaustive]
pub struct D1QuotaStore {
    client: Arc<crate::storage::d1_http::D1HttpClient>,
}

impl D1QuotaStore {
    /// Construct over a shared D1 HTTP client.
    #[must_use]
    pub fn new(client: Arc<crate::storage::d1_http::D1HttpClient>) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl QuotaStore for D1QuotaStore {
    async fn get(&self, tenant_id: &str) -> Result<Option<QuotaState>, String> {
        self.client.tenant_quota_lookup(tenant_id).await
    }

    async fn put(
        &self,
        tenant_id: &str,
        state: QuotaState,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.client
            .tenant_quota_upsert(tenant_id, state, updated_at_ms)
            .await
    }

    async fn accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<(), String> {
        self.client
            .tenant_quota_accrue(tenant_id, delta_micros, seed_anchor_ms, updated_at_ms)
            .await
    }

    /// Production override for F13 — atomic check-and-accrue in ONE D1
    /// statement, eliminating the TOCTOU over-admission window.
    ///
    /// SQL:
    /// ```sql
    /// UPDATE tenant_quota
    ///    SET accrued_usd_micros = accrued_usd_micros + ?2,
    ///        updated_at_ms      = ?3
    ///  WHERE tenant_id = ?1
    ///    AND accrued_usd_micros + ?2 <= monthly_budget_usd_micros
    /// RETURNING accrued_usd_micros
    /// ```
    ///
    /// - **Row returned** → the increment was applied within budget → `Ok(true)`.
    /// - **No row returned** → either the ceiling would be exceeded OR the
    ///   row doesn't exist yet. A missing row is handled by the caller
    ///   (see [`QuotaGuard::check`] for the seed path), so this method
    ///   returns `Ok(false)` in both cases (fail-CLOSED; the caller then
    ///   seeds the row and retries on the `put` path if appropriate).
    /// - **D1 error** → `Err(String)` → caller rejects 503 (fail-CLOSED).
    async fn check_and_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        _seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        let rows = self
            .client
            .query(
                "UPDATE tenant_quota \
                    SET accrued_usd_micros = accrued_usd_micros + ?2, \
                        updated_at_ms      = ?3 \
                  WHERE tenant_id = ?1 \
                    AND accrued_usd_micros + ?2 <= monthly_budget_usd_micros \
                 RETURNING accrued_usd_micros",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::from(delta_micros),
                    serde_json::Value::from(updated_at_ms),
                ],
            )
            .await?;
        // A non-empty result set means the WHERE predicate matched and the
        // increment was applied. An empty set means either the ceiling would
        // be exceeded or no row exists — both are treated as "denied" here;
        // the cycle-roll / fresh-row path uses `put` via `QuotaGuard::check`.
        Ok(!rows.is_empty())
    }

    /// Production override for red-team #6 — atomic seed-and-ceiling-check of a
    /// brand-new tenant's first op in a SINGLE statement.
    ///
    /// SQL:
    /// ```sql
    /// INSERT INTO tenant_quota
    ///     (tenant_id, monthly_budget_usd_micros, accrued_usd_micros,
    ///      cycle_anchor_ms, updated_at_ms)
    /// VALUES (?1, ?4, ?2, ?3, ?5)
    /// ON CONFLICT(tenant_id) DO UPDATE
    ///     SET accrued_usd_micros = accrued_usd_micros + ?2,
    ///         updated_at_ms      = ?5
    ///   WHERE accrued_usd_micros + ?2 <= monthly_budget_usd_micros
    /// RETURNING accrued_usd_micros
    /// ```
    ///
    /// - **INSERT path** (no row yet): the row is created with
    ///   `accrued = delta`; the new row is only returned when
    ///   `delta <= budget` is enforced app-side first (a fresh row carries the
    ///   default tripwire, so we reject a first op above it BEFORE the INSERT
    ///   rather than persist an over-cap row).
    /// - **ON CONFLICT path** (a concurrent first-op already seeded the row):
    ///   the increment applies only under the ceiling — `RETURNING` yields a
    ///   row ⇒ `Ok(true)`, no row ⇒ ceiling exceeded ⇒ `Ok(false)`.
    /// - **D1 error** → `Err(String)` → caller rejects 503 (fail-CLOSED).
    async fn seed_checked_accrue(
        &self,
        tenant_id: &str,
        delta_micros: i64,
        seed_anchor_ms: i64,
        updated_at_ms: i64,
    ) -> Result<bool, String> {
        // The first op of a brand-new tenant must itself fit under the default
        // tripwire that the freshly-INSERTed row will carry. Reject above-cap
        // first ops BEFORE persisting a row, so we never seed an over-ceiling
        // row (the ON CONFLICT WHERE only guards the increment path).
        if delta_micros > DEFAULT_MONTHLY_BUDGET_USD_MICROS {
            return Ok(false);
        }
        let rows = self
            .client
            .query(
                "INSERT INTO tenant_quota \
                     (tenant_id, monthly_budget_usd_micros, accrued_usd_micros, \
                      cycle_anchor_ms, updated_at_ms) \
                 VALUES (?1, ?4, ?2, ?3, ?5) \
                 ON CONFLICT(tenant_id) DO UPDATE \
                     SET accrued_usd_micros = accrued_usd_micros + ?2, \
                         updated_at_ms      = ?5 \
                   WHERE accrued_usd_micros + ?2 <= monthly_budget_usd_micros \
                 RETURNING accrued_usd_micros",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::from(delta_micros),
                    serde_json::Value::from(seed_anchor_ms),
                    serde_json::Value::from(DEFAULT_MONTHLY_BUDGET_USD_MICROS),
                    serde_json::Value::from(updated_at_ms),
                ],
            )
            .await?;
        // A returned row means either the INSERT created the seed row or the
        // ON CONFLICT increment stayed under the ceiling. An empty set means
        // the existing row's increment would breach the ceiling (the INSERT
        // path always returns its new row).
        Ok(!rows.is_empty())
    }

    /// Production override (CAA-360 #5/#20): atomic conditional cycle reset in a
    /// SINGLE statement. Rolls ONLY if the row is still stale; a concurrent op
    /// that already advanced the anchor makes this a no-op (idempotent), so two
    /// boundary ops cannot both reset+absolute-write and lose spend.
    async fn roll_if_stale(&self, tenant_id: &str, now_ms: i64) -> Result<(), String> {
        self.client
            .query(
                "UPDATE tenant_quota \
                    SET accrued_usd_micros = 0, cycle_anchor_ms = ?2, updated_at_ms = ?2 \
                  WHERE tenant_id = ?1 AND cycle_anchor_ms + ?3 <= ?2",
                &[
                    serde_json::Value::String(tenant_id.to_owned()),
                    serde_json::Value::from(now_ms),
                    serde_json::Value::from(CYCLE_LENGTH_MS),
                ],
            )
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    include!("b126_m2_test_1_1.rs");
    include!("b126_m2_test_1_2.rs");

    #[test]
    fn b126_m2_test_fragments_are_wired() {
        let _ = [B126_M2_TEST_1_1_REANCHOR, B126_M2_TEST_1_2_REANCHOR];
    }
}

#[allow(dead_code)]
const B126_M2_IMPL_2_REANCHOR: () = ();
