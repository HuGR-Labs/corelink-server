    /// Test-only: reach the per-tenant fairness gate's semaphore for `tenant`
    /// through the exact get-or-insert path a verify uses, so a test can
    /// saturate (or pin) a bucket. Production code goes through
    /// [`PerTenantGate::acquire`].
    #[cfg(test)]
    pub(super) fn per_tenant_semaphore(&self, tenant: &str) -> Option<Arc<tokio::sync::Semaphore>> {
        self.per_tenant.semaphore(tenant)
    }

    /// The full Option-B verification pipeline, returning the PAT's owning
    /// tenant id **and** whether it carries cache WRITE capability.
    ///
    /// Callers that mint a downstream credential FROM the PAT (the OCI
    /// `/token` Basic→Bearer exchange) use the write bit to downscope the
    /// grant to the PAT's real rights — otherwise a read-only (`cas:r`)
    /// PAT could obtain a `push` registry token. The simpler [`Self::verify`]
    /// discards the bit (per-op write enforcement for the header-scoped
    /// adapters stays at the route from `x-corelink-scope`).
    pub async fn verify_capability(
        &self,
        pat_plaintext: &str,
    ) -> Result<(String, bool), VerifyError> {
        self.verify_capability_full(pat_plaintext)
            .await
            .map(|(tenant_id, can_write, _runner_job)| (tenant_id, can_write))
    }
