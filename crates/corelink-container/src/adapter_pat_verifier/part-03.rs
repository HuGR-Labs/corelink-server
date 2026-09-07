impl PatVerifier {
    /// Test-only: shrink the per-tenant LRU map cap so the eviction path can be
    /// driven deterministically (the production cap of 10k is too large to fill
    /// in a unit test). Returns `self` for chaining off a constructor.
    #[cfg(test)]
    #[must_use]
    pub(super) fn with_map_cap(mut self, cap: usize) -> Self {
        // The gate is behind an `Arc`, so rebuild it. Only ever called straight
        // off a constructor, where the map is still empty.
        self.per_tenant = Arc::new(PerTenantGate::new(self.per_tenant.cap, cap));
        self
    }

    /// Test-only: current number of live per-tenant semaphore entries (LRU map
    /// size). Used to assert the map stays bounded under distinct-tenant churn.
    #[cfg(test)]
    pub(super) fn per_tenant_map_len(&self) -> usize {
        self.per_tenant
            .permits
            .lock()
            .map(|map| map.len())
            .unwrap_or_default()
    }

    /// Test-only constructor that overrides the Argon2id concurrency bound so
    /// a unit test can drive the semaphore to exhaustion deterministically
    /// (the production const is too large to fill in a test). Not part of the
    /// public production surface.
    #[cfg(test)]
    #[must_use]
    pub(super) fn with_key_set_and_permits(
        lookup: Arc<dyn PatRowLookup>,
        signing_keys: Vec<PatSigningKey>,
        permits: usize,
    ) -> Self {
        Self::with_key_set_and_permits_per_tenant(
            lookup,
            signing_keys,
            permits,
            ARGON2_PER_TENANT_PERMITS,
        )
    }

    /// Test-only constructor that overrides BOTH the global Argon2id concurrency
    /// bound AND the per-tenant sub-cap, so the fairness test can drive the
    /// two-tier interaction deterministically (e.g. a per-tenant cap small
    /// enough to saturate while global headroom remains for other tenants).
    #[cfg(test)]
    #[must_use]
    pub(super) fn with_key_set_and_permits_per_tenant(
        lookup: Arc<dyn PatRowLookup>,
        signing_keys: Vec<PatSigningKey>,
        permits: usize,
        per_tenant_permits: usize,
    ) -> Self {
        Self {
            lookup,
            signing_keys: Arc::new(signing_keys),
            argon2_permits: Arc::new(tokio::sync::Semaphore::new(permits)),
            per_tenant: Arc::new(PerTenantGate::new(per_tenant_permits, PER_TENANT_MAP_CAP)),
            secret_match_memo: Arc::new(SecretMatchMemo::new(
                SECRET_MATCH_MEMO_CAP,
                SECRET_MATCH_MEMO_TTL,
            )),
            verify_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
            burn_flights: Arc::new(FlightGroup::new(FLIGHT_GROUP_CAP)),
        }
    }

    /// The full Option-B verification pipeline. Returns the PAT's owning
    /// tenant id on success. Thin wrapper over [`Self::verify_capability`]
    /// for callers that don't need the write-capability bit.
    pub async fn verify(&self, pat_plaintext: &str) -> Result<String, VerifyError> {
        self.verify_capability(pat_plaintext)
            .await
            .map(|(tenant, _can_write)| tenant)
    }
}
