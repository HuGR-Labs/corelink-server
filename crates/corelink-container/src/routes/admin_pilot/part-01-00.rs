/// Canonical `SELECT` projection for the `pilot_tenants` table (0065) —
/// every column the [`PilotTenant`] shape needs, in a fixed order.
/// Table + column names are compile-time constants (injection-safe);
/// only values are ever bound as `?n` params.
const PILOT_SELECT_COLS: &str = "tenant_id, slug, tier, cap_bytes, pilot_state, signup_at_ms, \
     tier_granted_at_ms, first_blob_at_ms";

/// Sync row-source seam over D1 — the sync↔async bridge point.
///
/// The [`PilotStore`] trait is synchronous (the pilot-admin handlers
/// call it inside the axum task), but on the native container D1 is
/// reachable only via the **async** [`D1HttpClient`]. The production
/// impl ([`D1HttpPilotDb`]) bridges each call through
/// `tokio::task::block_in_place` + `Handle::current().block_on(…)` —
/// the same single documented bridge as
/// [`crate::customer_d1::D1HttpCustomerDb`]; tests supply a hermetic
/// mock so the SQL/serialization is exercised without a live D1.
pub trait PilotD1: fmt::Debug + Send + Sync {
    /// Run one parameterised statement; return the result rows (empty
    /// for non-`RETURNING` writes).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any D1 transport, HTTP, or decode
    /// failure.
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String>;
}

/// Production [`PilotD1`] over the CF D1 REST API. Single documented
/// sync↔async bridge point (mirrors `customer_d1::D1HttpCustomerDb`).
pub struct D1HttpPilotDb {
    /// Shared D1-over-HTTP client (owns + redacts the CF API token).
    d1: Arc<D1HttpClient>,
}

impl D1HttpPilotDb {
    /// Wire the row source over a shared [`D1HttpClient`].
    #[must_use]
    pub fn new(d1: Arc<D1HttpClient>) -> Self {
        Self { d1 }
    }
}

impl fmt::Debug for D1HttpPilotDb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The client's own Debug is never surfaced here so a leaked
        // Debug can never expose the CF API token.
        f.debug_struct("D1HttpPilotDb")
            .field("d1", &"[D1HttpClient]")
            .finish()
    }
}

impl PilotD1 for D1HttpPilotDb {
    fn query(&self, sql: &str, binds: Vec<Value>) -> Result<Vec<D1Row>, String> {
        let d1 = Arc::clone(&self.d1);
        // The native server is `#[tokio::main]` (multi-thread); we are
        // inside an async task (the axum handler), so `block_in_place`
        // hands the worker thread back to the scheduler while
        // `Handle::current().block_on` drives the D1 round-trip.
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move { d1.query(sql, &binds).await })
        })
    }
}

/// D1-durable [`PilotStore`] over the `pilot_tenants` table (0065).
/// Rows survive container restarts (unlike [`InMemoryPilotStore`]).
pub struct D1PilotStore {
    /// D1 row source (production: [`D1HttpPilotDb`]).
    db: Arc<dyn PilotD1>,
}

impl D1PilotStore {
    /// Construct over a [`PilotD1`] row source.
    #[must_use]
    pub fn new(db: Arc<dyn PilotD1>) -> Self {
        Self { db }
    }

    /// Build the production D1-backed store from process env. `None`
    /// when the D1 config ([`crate::storage::StorageEnv`]) is
    /// absent/invalid — the caller then keeps the InMemory store
    /// (dev/CI), mirroring
    /// [`crate::customer_d1::D1CustomerHandler::from_env`]'s
    /// fail-closed pattern.
    #[must_use]
    pub fn from_env() -> Option<Arc<Self>> {
        let storage_env = crate::storage::StorageEnv::from_env()?;
        let d1 = D1HttpClient::new(&storage_env)
            .map_err(|e| tracing::warn!(error = %e, "admin_pilot: D1 client init failed"))
            .ok()?;
        Some(Arc::new(Self::new(Arc::new(D1HttpPilotDb::new(Arc::new(
            d1,
        ))))))
    }
}

impl fmt::Debug for D1PilotStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Redaction marker only — never surface the inner client Debug.
        f.debug_struct("D1PilotStore")
            .field("db", &"[PilotD1]")
            .finish()
    }
}

/// Map a `pilot_tenants` D1 row into a [`PilotTenant`]. Returns a
/// `&'static str` when a required column is missing / mistyped (the
/// store fails CLOSED rather than fabricating a tenant).
fn pilot_row_to_tenant(row: &D1Row) -> Result<PilotTenant, &'static str> {
    let tenant_id = row
        .get("tenant_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("pilot_tenants: missing/invalid tenant_id")?;
    let slug = row
        .get("slug")
        .and_then(Value::as_str)
        .ok_or("pilot_tenants: missing slug")?
        .to_string();
    let tier = row
        .get("tier")
        .and_then(Value::as_str)
        .ok_or("pilot_tenants: missing tier")?
        .to_string();
    let cap_bytes = row
        .get("cap_bytes")
        .and_then(Value::as_i64)
        .map(|v| u64::try_from(v).unwrap_or(0))
        .ok_or("pilot_tenants: missing cap_bytes")?;
    let pilot_state = row
        .get("pilot_state")
        .and_then(Value::as_str)
        .and_then(PilotState::parse)
        .ok_or("pilot_tenants: missing/invalid pilot_state")?;
    let signup_at_ms = row
        .get("signup_at_ms")
        .and_then(Value::as_i64)
        .map(|v| u64::try_from(v).unwrap_or(0))
        .ok_or("pilot_tenants: missing signup_at_ms")?;
    let tier_granted_at_ms = row
        .get("tier_granted_at_ms")
        .and_then(Value::as_i64)
        .map(|v| u64::try_from(v).unwrap_or(0));
    let first_blob_at_ms = row
        .get("first_blob_at_ms")
        .and_then(Value::as_i64)
        .map(|v| u64::try_from(v).unwrap_or(0));
    Ok(PilotTenant {
        tenant_id,
        slug,
        tier,
        cap_bytes,
        pilot_state,
        signup_at_ms,
        tier_granted_at_ms,
        first_blob_at_ms,
    })
}

impl PilotStore for D1PilotStore {
    fn list_by_state(
        &self,
        state: PilotState,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<PilotTenant>, &'static str> {
        let sql = format!(
            "SELECT {PILOT_SELECT_COLS} FROM pilot_tenants \
             WHERE pilot_state = ?1 ORDER BY signup_at_ms ASC LIMIT ?2 OFFSET ?3"
        );
        let rows = self
            .db
            .query(
                &sql,
                vec![
                    json!(state.as_str()),
                    json!(i64::try_from(limit).unwrap_or(i64::MAX)),
                    json!(i64::try_from(offset).unwrap_or(i64::MAX)),
                ],
            )
            .map_err(|_| "store unavailable")?;
        rows.iter().map(pilot_row_to_tenant).collect()
    }

    fn get(&self, tenant_id: Uuid) -> Result<Option<PilotTenant>, &'static str> {
        let sql =
            format!("SELECT {PILOT_SELECT_COLS} FROM pilot_tenants WHERE tenant_id = ?1 LIMIT 1");
        let rows = self
            .db
            .query(&sql, vec![json!(tenant_id.to_string())])
            .map_err(|_| "store unavailable")?;
        match rows.first() {
            Some(row) => Ok(Some(pilot_row_to_tenant(row)?)),
            None => Ok(None),
        }
    }

    fn apply_grant_tier(
        &self,
        tenant_id: Uuid,
        tier: &str,
        cap_bytes: u64,
        granted_at_ms: u64,
    ) -> Result<PilotTenant, &'static str> {
        // Read-modify-write through the same store seam so the
        // grant-eligible-state invariant matches InMemory exactly.
        let existing = self.get(tenant_id)?.ok_or("tenant not found")?;
        if !matches!(
            existing.pilot_state,
            PilotState::New | PilotState::Reserved | PilotState::Provisioned
        ) {
            return Err("tenant not in grant-eligible state");
        }
        // Tenant-scoped, state-guarded UPDATE: the `pilot_state IN (...)`
        // predicate makes the write idempotent + race-safe (a concurrent
        // grant that already flipped the row to ACTIVE updates 0 rows).
        self.db
            .query(
                "UPDATE pilot_tenants \
                 SET tier = ?1, cap_bytes = ?2, pilot_state = 'ACTIVE', \
                     tier_granted_at_ms = ?3 \
                 WHERE tenant_id = ?4 \
                   AND pilot_state IN ('NEW', 'RESERVED', 'PROVISIONED')",
                vec![
                    json!(tier),
                    json!(i64::try_from(cap_bytes).unwrap_or(i64::MAX)),
                    json!(i64::try_from(granted_at_ms).unwrap_or(i64::MAX)),
                    json!(tenant_id.to_string()),
                ],
            )
            .map_err(|_| "store unavailable")?;
        // Re-read so the returned record reflects the persisted row.
        self.get(tenant_id)?.ok_or("tenant not found")
    }

    fn create(
        &self,
        tenant_id: Uuid,
        slug: &str,
        cap_bytes: u64,
        signup_at_ms: u64,
    ) -> Result<PilotTenant, &'static str> {
        // Pre-check existence: D1 HTTP `success` does not discriminate a
        // PRIMARY-KEY conflict cleanly across the wire, so confirm the id
        // is free before inserting (mirrors `d1_http::tenant_set_tier`'s
        // existence pre-check). A duplicate id is an operator fault → a
        // distinct error the handler maps to 409.
        if self.get(tenant_id)?.is_some() {
            return Err("tenant already exists");
        }
        self.db
            .query(
                "INSERT INTO pilot_tenants \
                 (tenant_id, slug, tier, cap_bytes, pilot_state, signup_at_ms, \
                  tier_granted_at_ms, first_blob_at_ms) \
                 VALUES (?1, ?2, 'free', ?3, 'NEW', ?4, NULL, NULL)",
                vec![
                    json!(tenant_id.to_string()),
                    json!(slug),
                    json!(i64::try_from(cap_bytes).unwrap_or(i64::MAX)),
                    json!(i64::try_from(signup_at_ms).unwrap_or(i64::MAX)),
                ],
            )
            .map_err(|_| "store unavailable")?;
        Ok(PilotTenant {
            tenant_id,
            slug: slug.to_string(),
            tier: "free".to_string(),
            cap_bytes,
            pilot_state: PilotState::New,
            signup_at_ms,
            tier_granted_at_ms: None,
            first_blob_at_ms: None,
        })
    }
}

// -----------------------------------------------------------------------------
// Route state + scope guard
// -----------------------------------------------------------------------------

/// Shared route state — store + audit sink + wall clock.
#[derive(Clone)]
pub struct PilotAdminRouteState {
    /// Tenant store (production: D1-backed; tests: in-memory).
    pub store: Arc<dyn PilotStore>,
    /// Audit sink (production: audit-chain producer; tests:
    /// in-memory).
    pub audit_sink: Arc<dyn PilotAuditSink>,
    /// Wall clock (production: `SystemWallClock`; tests:
    /// `InMemoryFakeWallClock`).
    pub wall_clock: Arc<dyn crate::wall_clock::WallClock>,
    /// Operator-only shared secret for the `x-corelink-internal-auth`
    /// gate (sourced from `CORELINK_INTERNAL_AUTH_KEY`). `None` when the
    /// key is unset at boot → every pilot-admin handler fails CLOSED
    /// (403) (mirrors the `internal_pat` fail-CLOSED posture).
    pub internal_auth_key: Option<Arc<str>>,
}

impl fmt::Debug for PilotAdminRouteState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PilotAdminRouteState")
            .finish_non_exhaustive()
    }
}

/// Parsed admin scope claim — extracted from the
/// `X-Admin-Scope` header by [`require_admin_scope`].
#[derive(Clone, Debug)]
pub struct PilotAdminScope {
    /// Validated admin principal id.
    pub principal: String,
    /// Tenant-scope bound to the admin claim, if any. `None` ==
    /// global pilot-admin (operator). When present, the admin's
    /// authority is limited to the bound tenant — cross-tenant
    /// probes emit `EVENT_TYPE_CROSS_TENANT` BEFORE the 403.
    pub bound_tenant: Option<Uuid>,
}

impl PilotAdminScope {
    /// Returns `true` if this scope is allowed to operate on
    /// `target_tenant`. Global admins (no `bound_tenant`) are
    /// allowed; tenant-scoped admins must match exactly.
    #[must_use]
    pub fn allows_tenant(&self, target_tenant: Uuid) -> bool {
        match self.bound_tenant {
            None => true,
            Some(bound) => bound == target_tenant,
        }
    }
}
