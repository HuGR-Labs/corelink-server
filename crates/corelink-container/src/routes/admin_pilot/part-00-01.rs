impl InMemoryPilotStore {
    /// Construct an empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a tenant into the store.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the internal mutex is poisoned.
    pub fn seed(&self, tenant: PilotTenant) -> Result<(), &'static str> {
        let mut guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        guard.insert(tenant.tenant_id, tenant);
        Ok(())
    }

    /// Inject a failure that subsequent calls will return.
    ///
    /// # Errors
    ///
    /// Returns `&'static str` if the internal mutex is poisoned.
    pub fn inject_failure(&self, reason: &'static str) -> Result<(), &'static str> {
        let mut guard = self.fail_with.lock().map_err(|_| "store poisoned")?;
        *guard = Some(reason);
        Ok(())
    }
}

impl PilotStore for InMemoryPilotStore {
    fn list_by_state(
        &self,
        state: PilotState,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<PilotTenant>, &'static str> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(reason) = *g {
                return Err(reason);
            }
        }
        let guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        let mut rows: Vec<PilotTenant> = guard
            .values()
            .filter(|t| t.pilot_state == state)
            .cloned()
            .collect();
        rows.sort_by_key(|t| t.signup_at_ms);
        let end = offset.saturating_add(limit).min(rows.len());
        let start = offset.min(rows.len());
        Ok(rows.get(start..end).map(<[_]>::to_vec).unwrap_or_default())
    }

    fn get(&self, tenant_id: Uuid) -> Result<Option<PilotTenant>, &'static str> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(reason) = *g {
                return Err(reason);
            }
        }
        let guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        Ok(guard.get(&tenant_id).cloned())
    }

    fn apply_grant_tier(
        &self,
        tenant_id: Uuid,
        tier: &str,
        cap_bytes: u64,
        granted_at_ms: u64,
    ) -> Result<PilotTenant, &'static str> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(reason) = *g {
                return Err(reason);
            }
        }
        let mut guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        let tenant = guard.get_mut(&tenant_id).ok_or("tenant not found")?;
        if !matches!(
            tenant.pilot_state,
            PilotState::New | PilotState::Reserved | PilotState::Provisioned
        ) {
            return Err("tenant not in grant-eligible state");
        }
        tenant.tier = tier.to_string();
        tenant.cap_bytes = cap_bytes;
        tenant.pilot_state = PilotState::Active;
        tenant.tier_granted_at_ms = Some(granted_at_ms);
        Ok(tenant.clone())
    }

    fn create(
        &self,
        tenant_id: Uuid,
        slug: &str,
        cap_bytes: u64,
        signup_at_ms: u64,
    ) -> Result<PilotTenant, &'static str> {
        if let Ok(g) = self.fail_with.lock() {
            if let Some(reason) = *g {
                return Err(reason);
            }
        }
        let mut guard = self.by_id.lock().map_err(|_| "store poisoned")?;
        if guard.contains_key(&tenant_id) {
            return Err("tenant already exists");
        }
        let tenant = PilotTenant {
            tenant_id,
            slug: slug.to_string(),
            tier: "free".to_string(),
            cap_bytes,
            pilot_state: PilotState::New,
            signup_at_ms,
            tier_granted_at_ms: None,
            first_blob_at_ms: None,
        };
        guard.insert(tenant_id, tenant.clone());
        Ok(tenant)
    }
}

// -----------------------------------------------------------------------------
// D1-durable pilot-tenant store
// -----------------------------------------------------------------------------
